//! Servidor MCP de ARGUS (herramientas que usa el binario `argus-mcp`).
//!
//! Lee la misma base de datos que la app, así que funciona con la app cerrada.
//! El token del bot nunca sale de aquí: ni en respuestas, ni en logs.

use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{Implementation, ServerCapabilities, ServerConfig},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::commands::{self, sincronizar_guilds, token_del_bot_activo};
use crate::state::{AppState, Channel, Guild, Member, Role};

/// Respuesta de cada herramienta cuando no hay ningún bot activo.
pub const SIN_BOT: &str = "No hay bot activo en ARGUS: añade o elige uno en la app";

/// Respuesta cuando una herramienta no recibe `guild_id` y no hay servidor activo.
pub const SIN_SERVIDOR: &str =
    "No hay servidor activo en ARGUS: indica `guild_id` o elige uno con `elegir_servidor`";

/// Mensajes que devuelve `leer_mensajes` como máximo.
const MENSAJES_MAX: usize = 50;
/// Miembros por defecto y máximo de `listar_miembros`.
const MIEMBROS_DEFECTO: usize = 25;
const MIEMBROS_MAX: usize = 100;

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ServidorParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ListarMiembrosParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Texto a buscar en el usuario, el apodo o el ID (sin distinguir mayúsculas).
    pub buscar: Option<String>,
    /// Máximo de miembros a devolver (por defecto 25, máximo 100).
    pub limite: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InfoMiembroParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID (snowflake) del usuario.
    pub user_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct LeerMensajesParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// Cuántos de los últimos mensajes devolver (por defecto y máximo 50).
    pub limite: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ElegirServidorParams {
    /// ID (snowflake) del servidor que pasa a ser el servidor activo. Debe ser
    /// uno de los que devuelve `listar_servidores`.
    pub guild_id: String,
}

#[derive(Clone)]
pub struct ArgusMcp {
    pool: SqlitePool,
    state: Arc<AppState>,
    /// URL base de Discord; solo los tests la cambian (Discord falso).
    base_url: Option<String>,
}

impl ArgusMcp {
    /// Abre el servidor sobre una base ya abierta y carga el bot activo. Si hay
    /// un bot activo comprueba que su token se pueda descifrar (sin mostrarlo).
    pub async fn nuevo(pool: SqlitePool, base_url: Option<String>) -> Result<Self, String> {
        let servidor = Self {
            pool,
            state: Arc::new(AppState::new()),
            base_url,
        };
        servidor.recargar().await?;
        if servidor.state.get_active_bot().is_some() {
            token_del_bot_activo(&servidor.state)?;
        }
        Ok(servidor)
    }

    /// Relee la configuración de la base: la app puede haber cambiado de bot o
    /// de servidor mientras el MCP seguía abierto.
    async fn recargar(&self) -> Result<(), String> {
        self.state
            .load_config_from_pool(&self.pool)
            .await
            .map_err(|e| format!("No se pudo leer la configuración de ARGUS: {e}"))
    }

    /// Recarga y exige un bot activo.
    async fn preparar(&self) -> Result<(), String> {
        self.recargar().await?;
        if self.state.get_active_bot().is_none() {
            return Err(SIN_BOT.to_string());
        }
        Ok(())
    }

    /// Quita el token de un mensaje de error por si Discord lo repitiera.
    fn limpiar(&self, mensaje: String) -> String {
        match token_del_bot_activo(&self.state) {
            Ok(token) if !token.is_empty() => mensaje.replace(&token, "***"),
            _ => mensaje,
        }
    }

    /// Servidores del bot activo: los pide a Discord y, si falla, usa los
    /// guardados en la base.
    async fn servidores(&self) -> Result<Vec<Guild>, String> {
        match sincronizar_guilds(&self.pool, &self.state, self.base_url.as_deref()).await {
            Ok(guilds) => Ok(guilds),
            Err(e) => {
                let cache = crate::db::list_guilds_cached(&self.pool)
                    .await
                    .unwrap_or_default();
                if cache.is_empty() {
                    Err(e)
                } else {
                    Ok(cache)
                }
            }
        }
    }

    pub async fn estado_json(&self) -> Result<String, String> {
        self.preparar().await?;
        let bot = self.state.get_active_bot().ok_or(SIN_BOT)?;
        let activo = self.state.get_active_guild();
        let servidor = match activo {
            Some(id) => {
                let nombre = crate::db::list_guilds_cached(&self.pool)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .find(|g| g.id == id)
                    .map(|g| g.name);
                json!({ "id": id, "nombre": nombre })
            }
            None => serde_json::Value::Null,
        };
        let registrados = self.state.config.lock().unwrap().bots.len();
        Ok(json!({
            "bot_activo": { "id": bot.id, "nombre": bot.name, "avatar": bot.avatar },
            "servidor_activo": servidor,
            "bots_registrados": registrados,
        })
        .to_string())
    }

    pub async fn listar_servidores_json(&self) -> Result<String, String> {
        self.preparar().await?;
        let activo = self.state.get_active_guild();
        let guilds = self.servidores().await.map_err(|e| self.limpiar(e))?;
        let lista: Vec<_> = guilds
            .iter()
            .map(|g| json!({ "id": g.id, "nombre": g.name, "activo": activo.as_deref() == Some(&g.id) }))
            .collect();
        Ok(json!({ "servidores": lista }).to_string())
    }

    pub async fn elegir_servidor_json(&self, guild_id: &str) -> Result<String, String> {
        self.preparar().await?;
        let guild_id = guild_id.trim();
        let guilds = self.servidores().await.map_err(|e| self.limpiar(e))?;
        let guild = guilds.iter().find(|g| g.id == guild_id).ok_or_else(|| {
            format!("El bot activo no está en el servidor {guild_id}: usa `listar_servidores`")
        })?;
        self.state.set_active_guild(&guild.id);
        self.state
            .save_config_to_pool(&self.pool)
            .await
            .map_err(|e| format!("No se pudo guardar el servidor activo: {e}"))?;
        Ok(json!({ "servidor_activo": { "id": guild.id, "nombre": guild.name } }).to_string())
    }

    /// Recarga, exige bot activo y resuelve el servidor: el indicado o el activo.
    async fn servidor_de(&self, guild_id: Option<&str>) -> Result<String, String> {
        self.preparar().await?;
        match guild_id.map(str::trim).filter(|g| !g.is_empty()) {
            Some(g) => Ok(g.to_string()),
            None => self
                .state
                .get_active_guild()
                .ok_or_else(|| SIN_SERVIDOR.to_string()),
        }
    }

    fn url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// Roles del servidor para poner nombre a los IDs (vacío si no se pueden leer).
    async fn roles_con_nombre(&self, gid: &str) -> Vec<Role> {
        commands::leer_roles(&self.state, gid, self.url())
            .await
            .unwrap_or_default()
    }

    pub async fn mapa_servidor_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let guild = commands::informacion_del_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let roles = commands::leer_roles(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        // Sin el intent de miembros Discord rechaza la lista: el mapa sigue siendo útil.
        let (miembros, aviso) = match commands::leer_miembros(&self.state, &gid, self.url()).await {
            Ok(m) => (Some(m), None),
            Err(e) => (None, Some(self.limpiar(e))),
        };
        let visibles: Vec<&Channel> = canales
            .iter()
            .filter(|c| !matches!(c.channel_type, 10..=12))
            .collect();
        let hijos = |padre: Option<&str>| {
            let mut v: Vec<&&Channel> = visibles
                .iter()
                .filter(|c| c.channel_type != 4 && c.parent_id.as_deref() == padre)
                .collect();
            v.sort_by_key(|c| c.position);
            v.into_iter().map(|c| canal_json(c)).collect::<Vec<_>>()
        };
        let mut categorias: Vec<&&Channel> =
            visibles.iter().filter(|c| c.channel_type == 4).collect();
        categorias.sort_by_key(|c| c.position);
        let categorias: Vec<_> = categorias
            .iter()
            .map(|c| json!({ "id": c.id, "nombre": c.name, "canales": hijos(Some(&c.id)) }))
            .collect();
        let mut roles_ord: Vec<&Role> = roles.iter().collect();
        roles_ord.sort_by_key(|r| std::cmp::Reverse(r.position));
        let roles_json: Vec<_> = roles_ord
            .iter()
            .map(|r| {
                let mut v = json!({ "id": r.id, "nombre": r.name, "posicion": r.position });
                if let Some(m) = &miembros {
                    v["miembros"] = json!(m.iter().filter(|x| x.roles.contains(&r.id)).count());
                }
                v
            })
            .collect();
        let mut salida = json!({
            "servidor": { "id": guild.id, "nombre": guild.name, "miembros": guild.member_count },
            "categorias": categorias,
            "sin_categoria": hijos(None),
            "roles": roles_json,
        });
        if let Some(m) = &miembros {
            salida["miembros_cargados"] = json!(m.len());
        }
        if let Some(a) = aviso {
            salida["aviso_miembros"] = json!(a);
        }
        Ok(salida.to_string())
    }

    pub async fn listar_canales_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let lista: Vec<_> = canales
            .iter()
            .map(|c| {
                let categoria = c
                    .parent_id
                    .as_ref()
                    .and_then(|p| canales.iter().find(|x| &x.id == p))
                    .map(|p| p.name.clone());
                let mut v = canal_json(c);
                v["categoria_id"] = json!(c.parent_id);
                v["categoria"] = json!(categoria);
                if let Some(o) = c.overwrites.as_ref().filter(|o| !o.is_empty()) {
                    v["overwrites"] = json!(o
                        .iter()
                        .map(|o| json!({
                            "id": o.id,
                            "tipo": if o.type_ == 0 { "rol" } else { "miembro" },
                            "permitir": o.allow,
                            "denegar": o.deny,
                        }))
                        .collect::<Vec<_>>());
                }
                v
            })
            .collect();
        Ok(json!({ "canales": lista }).to_string())
    }

    pub async fn listar_roles_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let mut roles = commands::leer_roles(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        roles.sort_by_key(|r| std::cmp::Reverse(r.position));
        let lista: Vec<_> = roles
            .iter()
            .map(|r| {
                let bits = r.permissions.parse::<u64>().unwrap_or(0);
                json!({
                    "id": r.id,
                    "nombre": r.name,
                    "posicion": r.position,
                    "color": format!("#{:06x}", r.color),
                    "separado": r.hoist,
                    "mencionable": r.mentionable,
                    "permisos_bits": r.permissions,
                    "permisos": crate::discord::nombres_de_permisos(bits),
                })
            })
            .collect();
        Ok(json!({ "roles": lista }).to_string())
    }

    pub async fn listar_miembros_json(
        &self,
        guild_id: Option<&str>,
        buscar: Option<&str>,
        limite: Option<u32>,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let limite = (limite.unwrap_or(MIEMBROS_DEFECTO as u32) as usize).clamp(1, MIEMBROS_MAX);
        let miembros = commands::leer_miembros(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let roles = self.roles_con_nombre(&gid).await;
        let buscar = buscar
            .map(|b| b.trim().to_lowercase())
            .filter(|b| !b.is_empty());
        let coinciden: Vec<&Member> = miembros
            .iter()
            .filter(|m| match &buscar {
                None => true,
                Some(b) => {
                    m.username.to_lowercase().contains(b)
                        || m.user_id == *b
                        || m.nickname
                            .as_deref()
                            .is_some_and(|n| n.to_lowercase().contains(b))
                }
            })
            .collect();
        let lista: Vec<_> = coinciden
            .iter()
            .take(limite)
            .map(|m| miembro_json(m, &roles))
            .collect();
        Ok(json!({
            "total_servidor": miembros.len(),
            "coincidencias": coinciden.len(),
            "devueltos": lista.len(),
            "miembros": lista,
        })
        .to_string())
    }

    pub async fn info_miembro_json(
        &self,
        guild_id: Option<&str>,
        user_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let miembro = commands::leer_miembro(&self.state, &gid, user_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let roles = self.roles_con_nombre(&gid).await;
        let mut v = miembro_json(&miembro, &roles);
        v["unido"] = json!(miembro.joined_at);
        Ok(v.to_string())
    }

    pub async fn permisos_bot_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let bot = self.state.get_active_bot().ok_or(SIN_BOT)?;
        let permisos = commands::permisos_del_bot(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(
            json!({ "bot": { "id": bot.id, "nombre": bot.name }, "permisos": permisos })
                .to_string(),
        )
    }

    pub async fn info_servidor_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let g = commands::informacion_del_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({
            "id": g.id,
            "nombre": g.name,
            "icono": g.icon,
            "miembros_aprox": g.member_count,
        })
        .to_string())
    }

    pub async fn ajustes_servidor_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let a = commands::leer_ajustes_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .unwrap_or_default();
        let canal = |id: &Option<String>| {
            id.as_ref().map(|id| {
                json!({
                    "id": id,
                    "nombre": canales.iter().find(|c| &c.id == id).map(|c| c.name.clone()),
                })
            })
        };
        Ok(json!({
            "nombre": a.name,
            "descripcion": a.description,
            "nivel_verificacion": a.verification_level,
            "filtro_contenido_explicito": a.explicit_content_filter,
            "notificaciones_por_defecto": a.default_message_notifications,
            "canal_del_sistema": canal(&a.system_channel_id),
            "canal_afk": canal(&a.afk_channel_id),
            "afk_segundos": a.afk_timeout,
        })
        .to_string())
    }

    pub async fn registro_auditoria_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let entradas = commands::leer_auditoria(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "entradas": entradas }).to_string())
    }

    pub async fn leer_mensajes_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
        limite: Option<u32>,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let limite = (limite.unwrap_or(MENSAJES_MAX as u32) as usize).clamp(1, MENSAJES_MAX);
        let mensajes = commands::leer_mensajes(&self.state, &gid, channel_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        // Discord los entrega del más nuevo al más antiguo.
        let lista: Vec<_> = mensajes.into_iter().take(limite).collect();
        Ok(json!({ "canal_id": channel_id.trim(), "mensajes": lista }).to_string())
    }

    pub async fn listar_baneos_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let bans = commands::listar_bans(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "baneos": bans }).to_string())
    }

    pub async fn listar_invitaciones_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let invitaciones = commands::leer_invitaciones(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "invitaciones": invitaciones }).to_string())
    }

    /// Los webhooks nunca llevan su URL (contiene el token): solo se da al crearlos.
    pub async fn listar_webhooks_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let webhooks = commands::leer_webhooks(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .unwrap_or_default();
        let lista: Vec<_> = webhooks
            .iter()
            .map(|w| {
                let canal = w
                    .channel_id
                    .as_ref()
                    .and_then(|id| canales.iter().find(|c| &c.id == id))
                    .map(|c| c.name.clone());
                json!({ "id": w.id, "nombre": w.name, "canal_id": w.channel_id, "canal": canal })
            })
            .collect();
        Ok(json!({ "webhooks": lista }).to_string())
    }
}

fn tipo_de_canal(t: u8) -> &'static str {
    match t {
        0 => "texto",
        2 => "voz",
        4 => "categoría",
        5 => "anuncios",
        10..=12 => "hilo",
        13 => "escenario",
        15 => "foro",
        16 => "multimedia",
        _ => "otro",
    }
}

fn canal_json(c: &Channel) -> serde_json::Value {
    json!({ "id": c.id, "nombre": c.name, "tipo": tipo_de_canal(c.channel_type), "posicion": c.position })
}

fn miembro_json(m: &Member, roles: &[Role]) -> serde_json::Value {
    let con_nombre: Vec<_> = m
        .roles
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "nombre": roles.iter().find(|r| &r.id == id).map(|r| r.name.clone()),
            })
        })
        .collect();
    json!({
        "id": m.user_id,
        "usuario": m.username,
        "apodo": m.nickname,
        "bot": m.bot,
        "roles": con_nombre,
    })
}

#[tool_router]
impl ArgusMcp {
    #[tool(
        name = "estado",
        description = "Muestra el bot activo de ARGUS (con su nombre oficial de Discord) y el servidor activo, que es el que usan por defecto las demás herramientas."
    )]
    async fn estado(&self) -> Result<String, String> {
        self.estado_json().await
    }

    #[tool(
        name = "listar_servidores",
        description = "Lista los servidores de Discord donde está el bot activo (id y nombre) e indica cuál es el servidor activo."
    )]
    async fn listar_servidores(&self) -> Result<String, String> {
        self.listar_servidores_json().await
    }

    #[tool(
        name = "elegir_servidor",
        description = "Cambia el servidor activo (se guarda en la base de datos de ARGUS, igual que al elegirlo en la app). El servidor debe ser uno de los del bot activo."
    )]
    async fn elegir_servidor(
        &self,
        Parameters(p): Parameters<ElegirServidorParams>,
    ) -> Result<String, String> {
        self.elegir_servidor_json(&p.guild_id).await
    }

    #[tool(
        name = "mapa_servidor",
        description = "Mapa del servidor: categorías con sus canales, canales sin categoría y roles (con nº de miembros). Solo lectura. `guild_id` es opcional (por defecto, el servidor activo)."
    )]
    async fn mapa_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.mapa_servidor_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "listar_canales",
        description = "Lista los canales y categorías del servidor (id, nombre, tipo, categoría, posición y permisos específicos). Solo lectura. `guild_id` es opcional."
    )]
    async fn listar_canales(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.listar_canales_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "listar_roles",
        description = "Lista los roles del servidor, del más alto al más bajo, con color y permisos legibles. Solo lectura. `guild_id` es opcional."
    )]
    async fn listar_roles(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.listar_roles_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "listar_miembros",
        description = "Lista miembros del servidor con sus roles. Admite `buscar` (usuario, apodo o ID) y `limite` (por defecto 25, máximo 100). Solo lectura. `guild_id` es opcional."
    )]
    async fn listar_miembros(
        &self,
        Parameters(p): Parameters<ListarMiembrosParams>,
    ) -> Result<String, String> {
        self.listar_miembros_json(p.guild_id.as_deref(), p.buscar.as_deref(), p.limite)
            .await
    }

    #[tool(
        name = "info_miembro",
        description = "Datos de un miembro (usuario, apodo, roles con nombre, fecha de ingreso). Solo lectura. `guild_id` es opcional."
    )]
    async fn info_miembro(
        &self,
        Parameters(p): Parameters<InfoMiembroParams>,
    ) -> Result<String, String> {
        self.info_miembro_json(p.guild_id.as_deref(), &p.user_id)
            .await
    }

    #[tool(
        name = "permisos_bot",
        description = "Permisos efectivos del bot activo en el servidor, con nombres en español. Solo lectura. `guild_id` es opcional."
    )]
    async fn permisos_bot(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.permisos_bot_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "info_servidor",
        description = "Nombre, icono y número aproximado de miembros del servidor. Solo lectura. `guild_id` es opcional."
    )]
    async fn info_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.info_servidor_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "ajustes_servidor",
        description = "Ajustes generales del servidor (descripción, verificación, filtro de contenido, notificaciones, canales de sistema y AFK). Requiere «Gestionar servidor». Solo lectura. `guild_id` es opcional."
    )]
    async fn ajustes_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.ajustes_servidor_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "registro_auditoria",
        description = "Últimas 50 entradas del registro de auditoría con texto legible. Requiere «Ver registro de auditoría». Solo lectura. `guild_id` es opcional."
    )]
    async fn registro_auditoria(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.registro_auditoria_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "leer_mensajes",
        description = "Lee los últimos mensajes de un canal (`limite` de 1 a 50, del más nuevo al más antiguo). Requiere «Leer el historial de mensajes». Solo lectura. `guild_id` es opcional."
    )]
    async fn leer_mensajes(
        &self,
        Parameters(p): Parameters<LeerMensajesParams>,
    ) -> Result<String, String> {
        self.leer_mensajes_json(p.guild_id.as_deref(), &p.channel_id, p.limite)
            .await
    }

    #[tool(
        name = "listar_baneos",
        description = "Lista los usuarios baneados con su motivo. Requiere «Banear miembros». Solo lectura. `guild_id` es opcional."
    )]
    async fn listar_baneos(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.listar_baneos_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "listar_invitaciones",
        description = "Lista las invitaciones del servidor (código, enlace, canal, usos). Requiere «Gestionar servidor». Solo lectura. `guild_id` es opcional."
    )]
    async fn listar_invitaciones(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.listar_invitaciones_json(p.guild_id.as_deref()).await
    }

    #[tool(
        name = "listar_webhooks",
        description = "Lista los webhooks del servidor (id, nombre, canal) sin token ni URL: la URL completa solo se da al crearlos. Requiere «Gestionar webhooks». Solo lectura. `guild_id` es opcional."
    )]
    async fn listar_webhooks(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        self.listar_webhooks_json(p.guild_id.as_deref()).await
    }
}

#[tool_handler]
impl ServerHandler for ArgusMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("argus-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Gestiona Discord con los bots registrados en ARGUS. Empieza con `estado` \
                 para ver el bot y el servidor activos.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use crate::db::{self, BotConfig};
    use chrono::Utc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TOKEN: &str = "token-secreto-de-prueba.XYZ";

    async fn bd_temporal() -> (tempfile_dir::Dir, SqlitePool) {
        let dir = tempfile_dir::Dir::nuevo();
        let pool = db::abrir_bd(&dir.0.join("argus.db")).await.unwrap();
        (dir, pool)
    }

    /// Carpeta temporal que se borra al soltarla (sin dependencias nuevas).
    mod tempfile_dir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn nuevo() -> Self {
                let p = std::env::temp_dir().join(format!("argus-mcp-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    async fn guardar_bot(pool: &SqlitePool) {
        let now = Utc::now().to_rfc3339();
        db::upsert_bot(
            pool,
            &BotConfig {
                id: "123456789".into(),
                name: "Centinela".into(),
                token_ciphertext: crypto::encrypt(TOKEN).unwrap(),
                avatar: None,
                application_id: None,
                allowed_guilds: Vec::new(),
                created_at: now.clone(),
                updated_at: now,
            },
        )
        .await
        .unwrap();
    }

    async fn discord_falso() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me/guilds"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": "555", "name": "Taller", "icon": null, "owner": false,
                 "permissions": "8", "features": []},
                {"id": "556", "name": "Casa", "icon": null, "owner": false,
                 "permissions": "8", "features": []}
            ])))
            .mount(&server)
            .await;
        server
    }

    async fn servidor(pool: SqlitePool, discord: &MockServer) -> ArgusMcp {
        ArgusMcp::nuevo(pool, Some(discord.address().to_string()))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn sin_bot_activo_todas_las_herramientas_lo_dicen() {
        let (_dir, pool) = bd_temporal().await;
        let discord = discord_falso().await;
        let mcp = servidor(pool, &discord).await;
        assert_eq!(mcp.estado_json().await.unwrap_err(), SIN_BOT);
        assert_eq!(mcp.listar_servidores_json().await.unwrap_err(), SIN_BOT);
        assert_eq!(mcp.elegir_servidor_json("555").await.unwrap_err(), SIN_BOT);
    }

    #[tokio::test]
    async fn estado_muestra_bot_y_servidor_sin_token() {
        let (_dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let discord = discord_falso().await;
        let mcp = servidor(pool, &discord).await;
        let v: serde_json::Value = serde_json::from_str(&mcp.estado_json().await.unwrap()).unwrap();
        assert_eq!(v["bot_activo"]["nombre"], "Centinela");
        assert!(v["servidor_activo"].is_null());
        assert!(!v.to_string().contains(TOKEN));
        assert!(!v.to_string().contains("token"));
    }

    #[tokio::test]
    async fn listar_y_elegir_servidor_persiste_en_la_bd() {
        let (_dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let discord = discord_falso().await;
        let mcp = servidor(pool.clone(), &discord).await;

        let lista: serde_json::Value =
            serde_json::from_str(&mcp.listar_servidores_json().await.unwrap()).unwrap();
        assert_eq!(lista["servidores"].as_array().unwrap().len(), 2);
        assert_eq!(lista["servidores"][0]["nombre"], "Taller");
        assert_eq!(lista["servidores"][0]["activo"], false);

        let elegido = mcp.elegir_servidor_json("556").await.unwrap();
        assert!(elegido.contains("Casa"));

        // Otra instancia (otro proceso) ve el cambio: está en la base.
        let otra = ArgusMcp::nuevo(pool, Some(discord.address().to_string()))
            .await
            .unwrap();
        let est: serde_json::Value =
            serde_json::from_str(&otra.estado_json().await.unwrap()).unwrap();
        assert_eq!(est["servidor_activo"]["id"], "556");
        assert_eq!(est["servidor_activo"]["nombre"], "Casa");
        let lista: serde_json::Value =
            serde_json::from_str(&otra.listar_servidores_json().await.unwrap()).unwrap();
        assert_eq!(lista["servidores"][1]["activo"], true);
    }

    #[tokio::test]
    async fn elegir_servidor_desconocido_falla_y_no_cambia_nada() {
        let (_dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let discord = discord_falso().await;
        let mcp = servidor(pool, &discord).await;
        let err = mcp.elegir_servidor_json("999").await.unwrap_err();
        assert!(err.contains("999"));
        let v: serde_json::Value = serde_json::from_str(&mcp.estado_json().await.unwrap()).unwrap();
        assert!(v["servidor_activo"].is_null());
    }

    #[tokio::test]
    async fn las_herramientas_tienen_descripcion_y_esquema() {
        let tools = ArgusMcp::tool_router().list_all();
        let nombres: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        for n in [
            "estado",
            "listar_servidores",
            "elegir_servidor",
            "mapa_servidor",
            "listar_canales",
            "listar_roles",
            "listar_miembros",
            "info_miembro",
            "permisos_bot",
            "info_servidor",
            "ajustes_servidor",
            "registro_auditoria",
            "leer_mensajes",
            "listar_baneos",
            "listar_invitaciones",
            "listar_webhooks",
        ] {
            assert!(nombres.contains(&n.to_string()), "falta {n}");
        }
        for t in &tools {
            assert!(t.description.as_deref().is_some_and(|d| !d.is_empty()));
            assert_eq!(t.input_schema["type"], "object");
        }
        let elegir = tools.iter().find(|t| t.name == "elegir_servidor").unwrap();
        assert!(elegir.input_schema["properties"]["guild_id"].is_object());
        // `guild_id` es opcional en las herramientas de lectura.
        let canales = tools.iter().find(|t| t.name == "listar_canales").unwrap();
        assert!(canales.input_schema["properties"]["guild_id"].is_object());
        let requeridos = canales.input_schema.get("required");
        assert!(requeridos.is_none_or(|r| !r.to_string().contains("guild_id")));
    }

    // ---------------------------------------------------- herramientas de lectura

    const WEBHOOK_TOKEN: &str = "wh-token-secreto";

    fn rol(id: &str, nombre: &str, pos: u32, bits: u64) -> serde_json::Value {
        json!({"id": id, "name": nombre, "color": 255, "hoist": false, "flags": 0,
               "position": pos, "permissions": bits.to_string(), "managed": false,
               "mentionable": false})
    }

    fn miembro(id: &str, nombre: &str, nick: Option<&str>, roles: &[&str]) -> serde_json::Value {
        json!({"user": {"id": id, "username": nombre, "discriminator": "0",
                        "avatar": null, "bot": id == "123456789"},
               "nick": nick, "roles": roles,
               "joined_at": "2024-01-02T03:04:05.000000+00:00",
               "deaf": false, "mute": false, "flags": 0})
    }

    async fn get_json(server: &MockServer, ruta: &str, cuerpo: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(ruta))
            .respond_with(ResponseTemplate::new(200).set_body_json(cuerpo))
            .mount(server)
            .await;
    }

    /// Servidor 555 completo; el bot (rol 600) tiene `bits` de permisos.
    async fn servidor_555(server: &MockServer, bits: u64) {
        get_json(
            server,
            "/api/v10/guilds/555",
            json!({
                "id": "555", "name": "Taller", "owner_id": "9", "description": "Una comunidad",
                "icon": null, "approximate_member_count": 3,
                "roles": [], "emojis": [], "features": [], "afk_timeout": 300,
                "afk_channel_id": "12", "system_channel_id": "11",
                "default_message_notifications": 1, "explicit_content_filter": 2,
                "verification_level": 3, "mfa_level": 0, "nsfw_level": 0,
                "premium_tier": 0, "system_channel_flags": 0
            }),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/roles",
            json!([
                rol("555", "@everyone", 0, 0),
                rol("600", "Bot", 5, bits),
                rol("700", "Moderadores", 3, 8192)
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/channels",
            json!([
                {"id": "10", "type": 4, "name": "Texto", "position": 0,
                 "permission_overwrites": []},
                {"id": "11", "type": 0, "name": "general", "position": 1, "parent_id": "10",
                 "permission_overwrites": [{"id": "700", "type": 0, "allow": "1024", "deny": "0"}]},
                {"id": "12", "type": 2, "name": "Voz", "position": 2, "parent_id": "10",
                 "permission_overwrites": []},
                {"id": "13", "type": 0, "name": "suelto", "position": 3,
                 "permission_overwrites": []}
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/members",
            json!([
                miembro("123456789", "centinela", None, &["600"]),
                miembro("1001", "ana", Some("Anita"), &["700"]),
                miembro("1002", "beto", None, &[])
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/members/123456789",
            miembro("123456789", "centinela", None, &["600"]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/members/1001",
            miembro("1001", "ana", Some("Anita"), &["700"]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/bans",
            json!([{"reason": "spam", "user": {"id": "1004", "username": "malo",
                    "discriminator": "0", "avatar": null}}]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/audit-logs",
            json!({
                "audit_log_entries": [{"id": "1", "user_id": "7", "target_id": "88",
                                       "action_type": 12, "changes": []}],
                "users": [{"id": "7", "username": "mod", "discriminator": "0", "avatar": null}],
                "webhooks": [], "integrations": [], "threads": [],
                "application_commands": [], "auto_moderation_rules": [],
                "guild_scheduled_events": []
            }),
        )
        .await;
        get_json(
            server,
            "/api/v10/channels/11/messages",
            json!([
                {"id": "302", "channel_id": "11", "content": "segundo", "pinned": false,
                 "timestamp": "2026-10-27T10:00:00.000000+00:00",
                 "author": {"id": "1001", "username": "ana", "discriminator": "0",
                            "avatar": null}},
                {"id": "301", "channel_id": "11", "content": "primero", "pinned": false,
                 "timestamp": "2026-10-27T09:00:00.000000+00:00",
                 "author": {"id": "1002", "username": "beto", "discriminator": "0",
                            "avatar": null}}
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/invites",
            json!([{"code": "abc123", "uses": 2, "max_uses": 0, "max_age": 0,
                    "channel": {"id": "11", "name": "general"},
                    "inviter": {"id": "1001", "username": "ana"}}]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/webhooks",
            json!([{"id": "900", "name": "Avisos", "channel_id": "11",
                    "token": WEBHOOK_TOKEN}]),
        )
        .await;
    }

    const TODO: u64 = 8; // Administrator

    async fn mcp_con_servidor(
        bits: u64,
        activo: bool,
    ) -> (tempfile_dir::Dir, ArgusMcp, MockServer) {
        let (dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let discord = discord_falso().await;
        servidor_555(&discord, bits).await;
        let mcp = servidor(pool, &discord).await;
        if activo {
            mcp.elegir_servidor_json("555").await.unwrap();
        }
        (dir, mcp, discord)
    }

    fn v(texto: String) -> serde_json::Value {
        serde_json::from_str(&texto).unwrap()
    }

    #[tokio::test]
    async fn mapa_y_listas_usan_el_servidor_activo_con_nombres() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        let mapa = v(mcp.mapa_servidor_json(None).await.unwrap());
        assert_eq!(mapa["servidor"]["nombre"], "Taller");
        assert_eq!(mapa["categorias"][0]["nombre"], "Texto");
        assert_eq!(mapa["categorias"][0]["canales"][0]["nombre"], "general");
        assert_eq!(mapa["categorias"][0]["canales"][1]["tipo"], "voz");
        assert_eq!(mapa["sin_categoria"][0]["nombre"], "suelto");
        assert_eq!(mapa["miembros_cargados"], 3);
        let mods = mapa["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "700")
            .unwrap();
        assert_eq!(mods["miembros"], 1);

        let canales = v(mcp.listar_canales_json(None).await.unwrap());
        let general = &canales["canales"][1];
        assert_eq!(general["categoria"], "Texto");
        assert_eq!(general["overwrites"][0]["tipo"], "rol");

        let roles = v(mcp.listar_roles_json(None).await.unwrap());
        assert_eq!(roles["roles"][0]["nombre"], "Bot");
        assert_eq!(roles["roles"][0]["color"], "#0000ff");
        assert_eq!(roles["roles"][1]["permisos"][0], "Gestionar mensajes");

        let info = v(mcp.info_servidor_json(None).await.unwrap());
        assert_eq!(
            (info["nombre"].as_str(), info["miembros_aprox"].as_u64()),
            (Some("Taller"), Some(3))
        );
    }

    #[tokio::test]
    async fn miembros_con_busqueda_y_limite_e_info_miembro() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        let todos = v(mcp.listar_miembros_json(None, None, None).await.unwrap());
        assert_eq!(todos["total_servidor"], 3);
        assert_eq!(todos["devueltos"], 3);

        let limitado = v(mcp.listar_miembros_json(None, None, Some(1)).await.unwrap());
        assert_eq!(
            (
                limitado["devueltos"].as_u64(),
                limitado["coincidencias"].as_u64()
            ),
            (Some(1), Some(3))
        );

        let por_apodo = v(mcp
            .listar_miembros_json(None, Some("ANIT"), None)
            .await
            .unwrap());
        assert_eq!(por_apodo["coincidencias"], 1);
        assert_eq!(por_apodo["miembros"][0]["usuario"], "ana");
        assert_eq!(
            por_apodo["miembros"][0]["roles"][0]["nombre"],
            "Moderadores"
        );

        let por_id = v(mcp
            .listar_miembros_json(None, Some("1002"), None)
            .await
            .unwrap());
        assert_eq!(por_id["miembros"][0]["usuario"], "beto");

        let uno = v(mcp.info_miembro_json(None, "1001").await.unwrap());
        assert_eq!(uno["apodo"], "Anita");
        assert_eq!(uno["roles"][0]["id"], "700");
        assert_eq!(uno["roles"][0]["nombre"], "Moderadores");
        assert!(mcp.info_miembro_json(None, "abc").await.is_err());
    }

    #[tokio::test]
    async fn permisos_ajustes_y_auditoria() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        let permisos = v(mcp.permisos_bot_json(None).await.unwrap());
        assert_eq!(permisos["bot"]["nombre"], "Centinela");
        assert!(permisos["permisos"].to_string().contains("Banear miembros"));

        let ajustes = v(mcp.ajustes_servidor_json(None).await.unwrap());
        assert_eq!(ajustes["nombre"], "Taller");
        assert_eq!(ajustes["canal_del_sistema"]["nombre"], "general");
        assert_eq!(ajustes["canal_afk"]["nombre"], "Voz");
        assert_eq!(ajustes["afk_segundos"], 300);

        let auditoria = v(mcp.registro_auditoria_json(None).await.unwrap());
        assert_eq!(auditoria["entradas"][0]["accion"], "Canal eliminado");
        assert_eq!(auditoria["entradas"][0]["autor"], "mod");
    }

    #[tokio::test]
    async fn mensajes_baneos_invitaciones_y_webhooks_sin_token() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        let todos = v(mcp.leer_mensajes_json(None, "11", None).await.unwrap());
        assert_eq!(todos["mensajes"].as_array().unwrap().len(), 2);
        assert_eq!(todos["mensajes"][0]["author_name"], "ana");
        let uno = v(mcp.leer_mensajes_json(None, "11", Some(1)).await.unwrap());
        assert_eq!(uno["mensajes"].as_array().unwrap().len(), 1);
        assert_eq!(uno["mensajes"][0]["content"], "segundo");
        // Más de 50 se recorta a 50 (Discord entrega 2 aquí) y 0 sube a 1.
        let cero = v(mcp.leer_mensajes_json(None, "11", Some(0)).await.unwrap());
        assert_eq!(cero["mensajes"].as_array().unwrap().len(), 1);
        assert_eq!(
            v(mcp.leer_mensajes_json(None, "11", Some(500)).await.unwrap())["mensajes"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        let bans = v(mcp.listar_baneos_json(None).await.unwrap());
        assert_eq!(bans["baneos"][0]["username"], "malo");
        assert_eq!(bans["baneos"][0]["reason"], "spam");

        let inv = v(mcp.listar_invitaciones_json(None).await.unwrap());
        assert_eq!(inv["invitaciones"][0]["code"], "abc123");
        assert_eq!(inv["invitaciones"][0]["channel_name"], "general");

        let whs = mcp.listar_webhooks_json(None).await.unwrap();
        assert!(!whs.contains(WEBHOOK_TOKEN));
        assert!(!whs.contains("discord.com/api/webhooks"));
        let whs = v(whs);
        assert_eq!(whs["webhooks"][0]["nombre"], "Avisos");
        assert_eq!(whs["webhooks"][0]["canal"], "general");
    }

    #[tokio::test]
    async fn guild_id_explicito_funciona_sin_servidor_activo() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, false).await;
        let roles = v(mcp.listar_roles_json(Some("555")).await.unwrap());
        assert_eq!(roles["roles"].as_array().unwrap().len(), 3);
        let info = v(mcp.info_servidor_json(Some(" 555 ")).await.unwrap());
        assert_eq!(info["nombre"], "Taller");
    }

    #[tokio::test]
    async fn sin_servidor_activo_las_herramientas_lo_dicen() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, false).await;
        let resultados = [
            mcp.mapa_servidor_json(None).await,
            mcp.listar_canales_json(None).await,
            mcp.listar_roles_json(None).await,
            mcp.listar_miembros_json(None, None, None).await,
            mcp.info_miembro_json(None, "1001").await,
            mcp.permisos_bot_json(None).await,
            mcp.info_servidor_json(None).await,
            mcp.ajustes_servidor_json(None).await,
            mcp.registro_auditoria_json(None).await,
            mcp.leer_mensajes_json(None, "11", None).await,
            mcp.listar_baneos_json(None).await,
            mcp.listar_invitaciones_json(None).await,
            mcp.listar_webhooks_json(None).await,
        ];
        for r in resultados {
            assert_eq!(r.unwrap_err(), SIN_SERVIDOR);
        }
    }

    #[tokio::test]
    async fn sin_bot_activo_las_herramientas_de_lectura_lo_dicen() {
        let (_dir, pool) = bd_temporal().await;
        let discord = discord_falso().await;
        let mcp = servidor(pool, &discord).await;
        assert_eq!(
            mcp.mapa_servidor_json(Some("555")).await.unwrap_err(),
            SIN_BOT
        );
        assert_eq!(mcp.listar_canales_json(None).await.unwrap_err(), SIN_BOT);
        assert_eq!(
            mcp.leer_mensajes_json(None, "11", None).await.unwrap_err(),
            SIN_BOT
        );
        assert_eq!(mcp.listar_webhooks_json(None).await.unwrap_err(), SIN_BOT);
    }

    #[tokio::test]
    async fn falta_de_permiso_se_informa_como_en_la_app() {
        let (_d, mcp, _s) = mcp_con_servidor(0, true).await;
        assert_eq!(
            mcp.listar_baneos_json(None).await.unwrap_err(),
            "Al bot le falta el permiso «Banear miembros»"
        );
        assert_eq!(
            mcp.registro_auditoria_json(None).await.unwrap_err(),
            "Al bot le falta el permiso «Ver registro de auditoría»"
        );
        assert_eq!(
            mcp.listar_webhooks_json(None).await.unwrap_err(),
            "Al bot le falta el permiso «Gestionar webhooks»"
        );
        assert_eq!(
            mcp.leer_mensajes_json(None, "11", None).await.unwrap_err(),
            "Al bot le falta el permiso «Leer el historial de mensajes»"
        );
    }
}
