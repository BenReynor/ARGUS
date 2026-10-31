//! Servidor MCP de ARGUS (herramientas que usa el binario `argus-mcp`).
//!
//! Lee la misma base de datos que la app, así que funciona con la app cerrada.
//! El token del bot nunca sale de aquí: ni en respuestas, ni en logs.

use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::discord::{DatosCanal, DatosRol};
use crate::i18n;
use crate::propuesta::{
    ordenar_propuesta, EstadoAccion, Nombres, Propuesta, ResultadoAccion, ResumenPropuesta,
};
use crate::state::{AppState, Channel, Guild, Member, Role};

/// Respuesta de cada herramienta cuando no hay ningún bot activo.
pub const SIN_BOT: &str = "No active bot in ARGUS: add or choose one in the app";

/// Respuesta cuando una herramienta no recibe `guild_id` y no hay servidor activo.
pub const SIN_SERVIDOR: &str =
    "No active server in ARGUS: pass `guild_id` or choose one with `elegir_servidor`";

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

/// Permisos como texto de bits (`"8192"`) o como lista de nombres legibles
/// (`["ManageMessages","ViewChannel"]` o `["Gestionar mensajes"]`).
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(untagged)]
pub enum PermisosParam {
    Bits(String),
    Nombres(Vec<String>),
}

impl PermisosParam {
    /// Texto de bits que espera Discord.
    pub fn a_bits(&self) -> Result<String, String> {
        match self {
            Self::Bits(t) => t
                .trim()
                .parse::<u64>()
                .map(|b| b.to_string())
                .map_err(|_| "Permissions must be a bits number or a list of names".to_string()),
            Self::Nombres(n) => crate::discord::bits_desde_nombres(n),
        }
    }
}

fn bits_opcionales(p: &Option<PermisosParam>) -> Result<Option<String>, String> {
    p.as_ref().map(PermisosParam::a_bits).transpose()
}

/// Color de un rol: número RGB (`5793266`) o texto hexadecimal (`"#5865F2"`),
/// el mismo formato que devuelve `listar_roles`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(untagged)]
pub enum ColorParam {
    Numero(u32),
    Hex(String),
}

impl ColorParam {
    pub fn a_u32(&self) -> Result<u32, String> {
        let valor = match self {
            ColorParam::Numero(n) => *n,
            ColorParam::Hex(t) => {
                let hex = t.trim().trim_start_matches('#');
                u32::from_str_radix(hex, 16)
                    .map_err(|_| format!("Invalid color: «{t}» (use #RRGGBB or an RGB number)"))?
            }
        };
        if valor > 0xFF_FFFF {
            return Err(format!("Color out of range: {valor} (maximum #FFFFFF)"));
        }
        Ok(valor)
    }
}

fn color_opcional(c: &Option<ColorParam>) -> Result<Option<u32>, String> {
    c.as_ref().map(ColorParam::a_u32).transpose()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EnviarMensajeParams {
    /// ID del canal de texto.
    pub channel_id: String,
    /// Texto del mensaje (hasta 2000 caracteres).
    pub contenido: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EnviarDmParams {
    /// ID del usuario que recibe el mensaje directo.
    pub user_id: String,
    /// Texto del mensaje (hasta 2000 caracteres).
    pub contenido: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct MensajeParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// ID del mensaje.
    pub message_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BorrarMensajesParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// IDs de los mensajes: entre 2 y 100, todos de menos de 14 días.
    pub message_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearCanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Nombre del canal (1–100 caracteres).
    pub nombre: String,
    /// «texto» (por defecto), «voz», «categoria», «anuncios» o «foro».
    pub tipo: Option<String>,
    /// ID de la categoría donde se crea.
    pub categoria_id: Option<String>,
    /// Tema del canal.
    pub tema: Option<String>,
    /// Marcar el canal como NSFW.
    pub nsfw: Option<bool>,
    /// Modo lento en segundos (0–21600).
    pub slowmode_segundos: Option<u16>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarCanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// Nombre nuevo.
    pub nombre: Option<String>,
    /// ID de la categoría nueva; texto vacío para sacarlo de su categoría.
    pub categoria_id: Option<String>,
    /// Tema nuevo.
    pub tema: Option<String>,
    /// Marcar o desmarcar NSFW.
    pub nsfw: Option<bool>,
    /// Modo lento en segundos (0–21600).
    pub slowmode_segundos: Option<u16>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct Posicion {
    /// ID del canal o del rol.
    pub id: String,
    /// Posición nueva.
    pub posicion: u64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct OrdenarParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Pares (id, posición) a aplicar.
    pub posiciones: Vec<Posicion>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearRolParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Nombre del rol (1–100 caracteres).
    pub nombre: String,
    /// Color: "#RRGGBB" o número RGB (p. ej. 255 = azul).
    pub color: Option<ColorParam>,
    /// Mostrar a los miembros del rol por separado.
    pub separado: Option<bool>,
    /// Permitir que cualquiera mencione el rol.
    pub mencionable: Option<bool>,
    /// Permisos: texto de bits o lista de nombres (p. ej. ["ManageMessages","ViewChannel"]).
    pub permisos: Option<PermisosParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarRolParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del rol.
    pub role_id: String,
    /// Nombre nuevo.
    pub nombre: Option<String>,
    /// Color nuevo: "#RRGGBB" o número RGB.
    pub color: Option<ColorParam>,
    /// Mostrar por separado.
    pub separado: Option<bool>,
    /// Mencionable.
    pub mencionable: Option<bool>,
    /// Permisos nuevos (sustituyen a los actuales): bits o lista de nombres.
    pub permisos: Option<PermisosParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RolParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del rol.
    pub role_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RolMiembroParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario.
    pub user_id: String,
    /// ID del rol.
    pub role_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PermisoCanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// ID del rol o del miembro al que se aplica.
    pub target_id: String,
    /// «rol» o «miembro».
    pub tipo: String,
    /// Permisos que se conceden (bits o lista de nombres).
    pub permitir: Option<PermisosParam>,
    /// Permisos que se deniegan (bits o lista de nombres).
    pub denegar: Option<PermisosParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct QuitarPermisoCanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// ID del rol o del miembro.
    pub target_id: String,
    /// «rol» o «miembro».
    pub tipo: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ExpulsarParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario.
    pub user_id: String,
    /// Motivo; queda en el registro de auditoría.
    pub motivo: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BanearParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario (puede no estar en el servidor).
    pub user_id: String,
    /// Motivo; queda en el registro de auditoría.
    pub motivo: Option<String>,
    /// Días de mensajes a borrar, de 0 a 7 (por defecto 0).
    pub dias_mensajes: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SilenciarParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario.
    pub user_id: String,
    /// Duración en minutos (máximo 40320 = 28 días); 0 quita el silencio.
    pub minutos: u32,
    /// Motivo; queda en el registro de auditoría.
    pub motivo: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CambiarApodoParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario.
    pub user_id: String,
    /// Apodo nuevo (hasta 32 caracteres); vacío u omitido lo borra.
    pub apodo: Option<String>,
    /// Motivo; queda en el registro de auditoría.
    pub motivo: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarServidorParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Nombre nuevo (2–100 caracteres).
    pub nombre: Option<String>,
    /// Descripción nueva (hasta 120 caracteres); texto vacío la borra.
    pub descripcion: Option<String>,
    /// Nivel de verificación (0–4).
    pub nivel_verificacion: Option<u8>,
    /// Filtro de contenido explícito (0–2).
    pub filtro_contenido_explicito: Option<u8>,
    /// Notificaciones por defecto (0 = todos los mensajes, 1 = solo menciones).
    pub notificaciones_por_defecto: Option<u8>,
    /// ID del canal de sistema; texto vacío lo quita.
    pub canal_del_sistema_id: Option<String>,
    /// ID del canal AFK; texto vacío lo quita.
    pub canal_afk_id: Option<String>,
    /// Inactividad AFK en segundos (60, 300, 900, 1800 o 3600).
    pub afk_segundos: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearInvitacionParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal al que lleva la invitación.
    pub channel_id: String,
    /// Caducidad en segundos (0 = no caduca; máximo 604800; por defecto 86400).
    pub caducidad_segundos: Option<u32>,
    /// Usos máximos (0 = ilimitados; máximo 100; por defecto 0).
    pub usos_maximos: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BorrarInvitacionParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Código de la invitación (la parte final del enlace).
    pub codigo: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearWebhookParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal donde publicará el webhook.
    pub channel_id: String,
    /// Nombre del webhook (1–80 caracteres).
    pub nombre: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BorrarWebhookParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del webhook.
    pub webhook_id: String,
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
            .map_err(|e| format!("Could not read the ARGUS configuration: {e}"))
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
            format!("The active bot is not in server {guild_id}: use `listar_servidores`")
        })?;
        self.state.set_active_guild(&guild.id);
        self.state
            .save_config_to_pool(&self.pool)
            .await
            .map_err(|e| format!("Could not save the active server: {e}"))?;
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
            Err(e) => (None, Some(self.tr(&self.limpiar(e)))),
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

    // ------------------------------------------- escritura: mensajes, canales, roles

    fn mensaje(&self, texto: String) -> Result<String, String> {
        Ok(json!({ "mensaje": self.tr(&texto) }).to_string())
    }

    /// Idioma guardado en la config de la base (inglés por defecto).
    fn idioma(&self) -> String {
        i18n::idioma(&self.state)
    }

    fn tr(&self, texto: &str) -> String {
        i18n::tr(&self.idioma(), texto)
    }

    /// Salida de una herramienta: el error sale en el idioma de la config.
    fn salida(&self, r: Result<String, String>) -> Result<String, String> {
        r.map_err(|e| self.tr(&e))
    }

    pub async fn enviar_mensaje_json(
        &self,
        channel_id: &str,
        contenido: &str,
    ) -> Result<String, String> {
        self.preparar().await?;
        let texto = commands::enviar_a_canal(&self.state, channel_id.trim(), contenido, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn enviar_dm_json(&self, user_id: &str, contenido: &str) -> Result<String, String> {
        self.preparar().await?;
        let texto = commands::enviar_dm(&self.state, user_id.trim(), contenido, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn borrar_mensaje_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
        message_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_mensaje(
            &self.state,
            &gid,
            channel_id.trim(),
            message_id.trim(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn borrar_mensajes_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
        message_ids: &[String],
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_mensajes_en_masa(
            &self.state,
            &gid,
            channel_id.trim(),
            message_ids,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn fijar_mensaje_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
        message_id: &str,
        fijar: bool,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::fijar_mensaje(
            &self.state,
            &gid,
            channel_id.trim(),
            message_id.trim(),
            fijar,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn crear_canal_json(&self, p: &CrearCanalParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let tipo = match p.tipo.as_deref().map(normalizar) {
            None => 0,
            Some(t) => match t.as_str() {
                "texto" | "text" => 0,
                "voz" | "voice" => 2,
                "categoria" | "category" => 4,
                "anuncios" | "announcement" => 5,
                "foro" | "forum" => 15,
                _ => {
                    return Err(
                        "Unsupported channel type: use texto, voz, categoria, anuncios or foro"
                            .to_string(),
                    )
                }
            },
        };
        let datos = DatosCanal {
            name: Some(p.nombre.clone()),
            channel_type: Some(tipo),
            parent_id: p.categoria_id.clone(),
            topic: p.tema.clone(),
            nsfw: p.nsfw,
            rate_limit_per_user: p.slowmode_segundos,
        };
        let canal = commands::crear_canal(&self.state, &gid, &datos, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "canal": canal_json(&canal) }).to_string())
    }

    pub async fn editar_canal_json(&self, p: &EditarCanalParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let datos = DatosCanal {
            name: p.nombre.clone(),
            channel_type: None,
            parent_id: p.categoria_id.clone(),
            topic: p.tema.clone(),
            nsfw: p.nsfw,
            rate_limit_per_user: p.slowmode_segundos,
        };
        let canal =
            commands::editar_canal(&self.state, &gid, p.channel_id.trim(), &datos, self.url())
                .await
                .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "canal": canal_json(&canal) }).to_string())
    }

    pub async fn borrar_canal_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_canal(&self.state, &gid, channel_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn ordenar_canales_json(
        &self,
        guild_id: Option<&str>,
        posiciones: &[Posicion],
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let pares: Vec<(String, u64)> = posiciones
            .iter()
            .map(|p| (p.id.trim().to_string(), p.posicion))
            .collect();
        let texto = commands::reordenar_canales(&self.state, &gid, &pares, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn crear_rol_json(&self, p: &CrearRolParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let datos = DatosRol {
            name: Some(p.nombre.clone()),
            color: color_opcional(&p.color)?,
            hoist: p.separado,
            mentionable: p.mencionable,
            permissions: bits_opcionales(&p.permisos)?,
        };
        let rol = commands::crear_rol(&self.state, &gid, &datos, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "rol": rol_json(&rol) }).to_string())
    }

    pub async fn editar_rol_json(&self, p: &EditarRolParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let datos = DatosRol {
            name: p.nombre.clone(),
            color: color_opcional(&p.color)?,
            hoist: p.separado,
            mentionable: p.mencionable,
            permissions: bits_opcionales(&p.permisos)?,
        };
        let rol = commands::editar_rol(&self.state, &gid, p.role_id.trim(), &datos, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "rol": rol_json(&rol) }).to_string())
    }

    pub async fn borrar_rol_json(
        &self,
        guild_id: Option<&str>,
        role_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_rol(&self.state, &gid, role_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn ordenar_roles_json(
        &self,
        guild_id: Option<&str>,
        posiciones: &[Posicion],
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let pares: Vec<(String, u64)> = posiciones
            .iter()
            .map(|p| (p.id.trim().to_string(), p.posicion))
            .collect();
        let texto = commands::reordenar_roles(&self.state, &gid, &pares, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn cambiar_rol_json(
        &self,
        guild_id: Option<&str>,
        user_id: &str,
        role_id: &str,
        anadir: bool,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::cambiar_rol(
            &self.state,
            &gid,
            user_id.trim(),
            role_id.trim(),
            anadir,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn permiso_canal_json(&self, p: &PermisoCanalParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let tipo = tipo_de_overwrite(&p.tipo)?;
        let permitir = bits_opcionales(&p.permitir)?.unwrap_or_else(|| "0".to_string());
        let denegar = bits_opcionales(&p.denegar)?.unwrap_or_else(|| "0".to_string());
        let texto = commands::fijar_permiso_canal(
            &self.state,
            &gid,
            p.channel_id.trim(),
            p.target_id.trim(),
            tipo,
            &permitir,
            &denegar,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn quitar_permiso_canal_json(
        &self,
        p: &QuitarPermisoCanalParams,
    ) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let tipo = tipo_de_overwrite(&p.tipo)?;
        let texto = commands::quitar_permiso_canal(
            &self.state,
            &gid,
            p.channel_id.trim(),
            p.target_id.trim(),
            tipo,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    // ------------------------------- escritura: moderación, servidor, invitaciones, webhooks

    pub async fn expulsar_json(&self, p: &ExpulsarParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let texto = commands::expulsar_miembro(
            &self.state,
            &gid,
            p.user_id.trim(),
            p.motivo.as_deref(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn banear_json(&self, p: &BanearParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        // Fuera de 0–7 (incluso si no cabe en u8) se rechaza con el mismo mensaje de la app.
        let dias = u8::try_from(p.dias_mensajes.unwrap_or(0)).unwrap_or(u8::MAX);
        let texto = commands::banear_miembro(
            &self.state,
            &gid,
            p.user_id.trim(),
            p.motivo.as_deref(),
            dias,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn desbanear_json(
        &self,
        guild_id: Option<&str>,
        user_id: &str,
        motivo: Option<&str>,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::quitar_ban(&self.state, &gid, user_id.trim(), motivo, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn silenciar_json(&self, p: &SilenciarParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let texto = commands::silenciar_miembro(
            &self.state,
            &gid,
            p.user_id.trim(),
            p.minutos,
            p.motivo.as_deref(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn cambiar_apodo_json(&self, p: &CambiarApodoParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let texto = commands::cambiar_apodo(
            &self.state,
            &gid,
            p.user_id.trim(),
            p.apodo.as_deref(),
            p.motivo.as_deref(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// Lee los ajustes actuales y cambia solo los campos indicados; el guardado
    /// es el de la app (`PATCH /guilds/{id}` con todos los campos).
    pub async fn editar_servidor_json(&self, p: &EditarServidorParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let mut a = commands::leer_ajustes_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        if let Some(n) = &p.nombre {
            a.name = n.clone();
        }
        if let Some(d) = &p.descripcion {
            a.description = Some(d.clone());
        }
        if let Some(n) = p.nivel_verificacion {
            a.verification_level = n;
        }
        if let Some(n) = p.filtro_contenido_explicito {
            a.explicit_content_filter = n;
        }
        if let Some(n) = p.notificaciones_por_defecto {
            a.default_message_notifications = n;
        }
        if let Some(c) = &p.canal_del_sistema_id {
            a.system_channel_id = Some(c.clone());
        }
        if let Some(c) = &p.canal_afk_id {
            a.afk_channel_id = Some(c.clone());
        }
        if let Some(s) = p.afk_segundos {
            a.afk_timeout = s;
        }
        let nuevo = commands::guardar_ajustes_servidor(&self.state, &gid, &a, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({
            "servidor": {
                "nombre": nuevo.name,
                "descripcion": nuevo.description,
                "nivel_verificacion": nuevo.verification_level,
                "filtro_contenido_explicito": nuevo.explicit_content_filter,
                "notificaciones_por_defecto": nuevo.default_message_notifications,
                "canal_del_sistema_id": nuevo.system_channel_id,
                "canal_afk_id": nuevo.afk_channel_id,
                "afk_segundos": nuevo.afk_timeout,
            }
        })
        .to_string())
    }

    pub async fn crear_invitacion_json(&self, p: &CrearInvitacionParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let invitacion = commands::crear_invitacion(
            &self.state,
            &gid,
            p.channel_id.trim(),
            p.caducidad_segundos.unwrap_or(86_400),
            p.usos_maximos.unwrap_or(0),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "invitacion": invitacion }).to_string())
    }

    pub async fn borrar_invitacion_json(
        &self,
        guild_id: Option<&str>,
        codigo: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_invitacion(&self.state, &gid, codigo.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// Única respuesta que incluye la URL completa del webhook (lleva su token).
    pub async fn crear_webhook_json(&self, p: &CrearWebhookParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let w = commands::crear_webhook(
            &self.state,
            &gid,
            p.channel_id.trim(),
            &p.nombre,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        Ok(json!({
            "webhook": { "id": w.id, "nombre": w.name, "canal_id": w.channel_id, "url": w.url }
        })
        .to_string())
    }

    pub async fn borrar_webhook_json(
        &self,
        guild_id: Option<&str>,
        webhook_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_webhook(&self.state, &gid, webhook_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }
}

/// Minúsculas y sin tildes para comparar textos sueltos («categoría» = «categoria»).
fn normalizar(t: &str) -> String {
    t.trim()
        .to_lowercase()
        .replace('í', "i")
        .replace('á', "a")
        .replace('é', "e")
}

/// 0 = rol, 1 = miembro (igual que `Overwrite::type_`).
fn tipo_de_overwrite(t: &str) -> Result<u8, String> {
    match normalizar(t).as_str() {
        "rol" | "role" | "0" => Ok(0),
        "miembro" | "member" | "usuario" | "1" => Ok(1),
        _ => Err("The type must be «rol» or «miembro»".to_string()),
    }
}

fn rol_json(r: &Role) -> serde_json::Value {
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

/// Herramientas que solo leen (no cambian nada en Discord ni en la BD). Son las
/// únicas que se le dan a un agente de IA; el resto va por propuesta aprobada.
pub const HERRAMIENTAS_LECTURA: &[&str] = &[
    "estado",
    "listar_servidores",
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
];

/// Herramientas que modifican algo (Discord o el servidor activo guardado).
pub const HERRAMIENTAS_ESCRITURA: &[&str] = &[
    "elegir_servidor",
    "enviar_mensaje",
    "enviar_dm",
    "borrar_mensaje",
    "borrar_mensajes",
    "fijar_mensaje",
    "desfijar_mensaje",
    "crear_canal",
    "editar_canal",
    "borrar_canal",
    "ordenar_canales",
    "crear_rol",
    "editar_rol",
    "borrar_rol",
    "ordenar_roles",
    "dar_rol",
    "quitar_rol",
    "permiso_canal",
    "quitar_permiso_canal",
    "expulsar",
    "banear",
    "desbanear",
    "silenciar",
    "cambiar_apodo",
    "editar_servidor",
    "crear_invitacion",
    "borrar_invitacion",
    "crear_webhook",
    "borrar_webhook",
];

fn args_de<T: serde::de::DeserializeOwned>(
    nombre: &str,
    args: serde_json::Value,
) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("Invalid arguments for «{nombre}»: {e}"))
}

/// Rechaza lo que no sea una herramienta de escritura (desconocida o de lectura).
fn comprobar_escritura(nombre: &str) -> Result<(), String> {
    if HERRAMIENTAS_ESCRITURA.contains(&nombre) {
        return Ok(());
    }
    Err(if HERRAMIENTAS_LECTURA.contains(&nombre) {
        format!("«{nombre}» is a read-only tool, not an action")
    } else {
        format!("Unknown action: «{nombre}»")
    })
}

/// Comprueba, sin ejecutar nada, que `nombre` es una herramienta de escritura y
/// que `args` se deserializa en sus parámetros.
pub fn validar_accion(nombre: &str, args: &serde_json::Value) -> Result<(), String> {
    comprobar_escritura(nombre)?;
    fn ok<T: serde::de::DeserializeOwned>(n: &str, a: &serde_json::Value) -> Result<(), String> {
        args_de::<T>(n, a.clone()).map(|_| ())
    }
    match nombre {
        "elegir_servidor" => ok::<ElegirServidorParams>(nombre, args),
        "enviar_mensaje" => ok::<EnviarMensajeParams>(nombre, args),
        "enviar_dm" => ok::<EnviarDmParams>(nombre, args),
        "borrar_mensaje" | "fijar_mensaje" | "desfijar_mensaje" => {
            ok::<MensajeParams>(nombre, args)
        }
        "borrar_mensajes" => ok::<BorrarMensajesParams>(nombre, args),
        "crear_canal" => ok::<CrearCanalParams>(nombre, args),
        "editar_canal" => ok::<EditarCanalParams>(nombre, args),
        "borrar_canal" => ok::<CanalParams>(nombre, args),
        "ordenar_canales" | "ordenar_roles" => ok::<OrdenarParams>(nombre, args),
        "crear_rol" => ok::<CrearRolParams>(nombre, args),
        "editar_rol" => ok::<EditarRolParams>(nombre, args),
        "borrar_rol" => ok::<RolParams>(nombre, args),
        "dar_rol" | "quitar_rol" => ok::<RolMiembroParams>(nombre, args),
        "permiso_canal" => ok::<PermisoCanalParams>(nombre, args),
        "quitar_permiso_canal" => ok::<QuitarPermisoCanalParams>(nombre, args),
        "expulsar" | "desbanear" => ok::<ExpulsarParams>(nombre, args),
        "banear" => ok::<BanearParams>(nombre, args),
        "silenciar" => ok::<SilenciarParams>(nombre, args),
        "cambiar_apodo" => ok::<CambiarApodoParams>(nombre, args),
        "editar_servidor" => ok::<EditarServidorParams>(nombre, args),
        "crear_invitacion" => ok::<CrearInvitacionParams>(nombre, args),
        "borrar_invitacion" => ok::<BorrarInvitacionParams>(nombre, args),
        "crear_webhook" => ok::<CrearWebhookParams>(nombre, args),
        "borrar_webhook" => ok::<BorrarWebhookParams>(nombre, args),
        _ => Err(format!("Unknown action: «{nombre}»")),
    }
}

/// Esquema JSON de los argumentos de una herramienta de escritura (el mismo tipo
/// que valida `validar_args`), o `None` si no es de escritura.
pub fn esquema_args(nombre: &str) -> Option<serde_json::Value> {
    fn de<T: schemars::JsonSchema>() -> Option<serde_json::Value> {
        serde_json::to_value(schemars::schema_for!(T)).ok()
    }
    match nombre {
        "elegir_servidor" => de::<ElegirServidorParams>(),
        "enviar_mensaje" => de::<EnviarMensajeParams>(),
        "enviar_dm" => de::<EnviarDmParams>(),
        "borrar_mensaje" | "fijar_mensaje" | "desfijar_mensaje" => de::<MensajeParams>(),
        "borrar_mensajes" => de::<BorrarMensajesParams>(),
        "crear_canal" => de::<CrearCanalParams>(),
        "editar_canal" => de::<EditarCanalParams>(),
        "borrar_canal" => de::<CanalParams>(),
        "ordenar_canales" | "ordenar_roles" => de::<OrdenarParams>(),
        "crear_rol" => de::<CrearRolParams>(),
        "editar_rol" => de::<EditarRolParams>(),
        "borrar_rol" => de::<RolParams>(),
        "dar_rol" | "quitar_rol" => de::<RolMiembroParams>(),
        "permiso_canal" => de::<PermisoCanalParams>(),
        "quitar_permiso_canal" => de::<QuitarPermisoCanalParams>(),
        "expulsar" | "desbanear" => de::<ExpulsarParams>(),
        "banear" => de::<BanearParams>(),
        "silenciar" => de::<SilenciarParams>(),
        "cambiar_apodo" => de::<CambiarApodoParams>(),
        "editar_servidor" => de::<EditarServidorParams>(),
        "crear_invitacion" => de::<CrearInvitacionParams>(),
        "borrar_invitacion" => de::<BorrarInvitacionParams>(),
        "crear_webhook" => de::<CrearWebhookParams>(),
        "borrar_webhook" => de::<BorrarWebhookParams>(),
        _ => None,
    }
}

/// Una línea por herramienta de escritura con sus argumentos (nombre, tipo,
/// obligatorio u opcional y su descripción), sacada del esquema de sus params:
/// así el asistente no adivina argumentos (antes solo se documentaban cuatro).
/// `guild_id` se omite: siempre se usa el servidor activo.
pub fn firmas_escritura() -> String {
    fn tipo(v: &serde_json::Value) -> String {
        match v.get("type") {
            Some(serde_json::Value::String(t)) if t == "array" => {
                let item = v.get("items").map(tipo).unwrap_or_else(|| "any".into());
                format!("list of {item}")
            }
            Some(serde_json::Value::String(t)) => t.clone(),
            Some(serde_json::Value::Array(ts)) => ts
                .iter()
                .filter_map(|t| t.as_str())
                .filter(|t| *t != "null")
                .collect::<Vec<_>>()
                .join("|"),
            _ => "any".into(),
        }
    }
    let mut lineas = Vec::new();
    for h in HERRAMIENTAS_ESCRITURA {
        if *h == "elegir_servidor" {
            continue;
        }
        let Some(esquema) = esquema_args(h) else {
            continue;
        };
        let requeridos: Vec<&str> = esquema["required"]
            .as_array()
            .map(|r| r.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        let mut args = Vec::new();
        if let Some(props) = esquema["properties"].as_object() {
            for (campo, def) in props {
                if campo == "guild_id" {
                    continue;
                }
                let oblig = if requeridos.contains(&campo.as_str()) {
                    "required"
                } else {
                    "optional"
                };
                let mut texto = format!("{campo} ({}, {oblig})", tipo(def));
                if let Some(d) = def.get("description").and_then(|d| d.as_str()) {
                    let d: String = d.split_whitespace().collect::<Vec<_>>().join(" ");
                    texto.push_str(&format!(": {d}"));
                }
                args.push(texto);
            }
        }
        lineas.push(format!(
            "- `{h}`: {}",
            if args.is_empty() {
                "no arguments".into()
            } else {
                args.join("; ")
            }
        ));
    }
    lineas.join("\n")
}

impl ArgusMcp {
    /// Ejecuta una herramienta de escritura por nombre con la misma lógica que
    /// el MCP (permisos, jerarquía y mensajes). Rechaza nombres desconocidos,
    /// herramientas de lectura y argumentos inválidos.
    pub async fn ejecutar_accion(
        &self,
        nombre: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        let r = self.despachar_accion(nombre, args).await;
        self.salida(r)
    }

    /// Ejecuta una propuesta en orden seguro. Los nombres de roles, categorías y
    /// canales se resuelven a IDs con lo que ya existe o se ha creado antes. Una
    /// acción que falla no detiene las demás; una cuya referencia no se resuelve
    /// se marca como omitida.
    pub async fn ejecutar_propuesta(
        &self,
        propuesta: &Propuesta,
    ) -> Result<Vec<ResultadoAccion>, String> {
        let cancelar = AtomicBool::new(false);
        Ok(self
            .ejecutar_propuesta_con(propuesta, &cancelar, &mut |_, _| {})
            .await?
            .resultados)
    }

    /// Como `ejecutar_propuesta`, avisando de cada acción en cuanto termina
    /// (`avance(total, resultado)`) y parando antes de la siguiente si `cancelar`
    /// se activa.
    pub async fn ejecutar_propuesta_con(
        &self,
        propuesta: &Propuesta,
        cancelar: &AtomicBool,
        avance: &mut (dyn FnMut(usize, &ResultadoAccion) + Send),
    ) -> Result<ResumenPropuesta, String> {
        // El servidor de la propuesta (la app pone su `guild_id` en cada
        // acción); si no lo trae, el servidor activo del MCP.
        let de_la_propuesta = propuesta
            .acciones
            .iter()
            .find_map(|a| a.args.get("guild_id").and_then(serde_json::Value::as_str));
        let gid = self.servidor_de(de_la_propuesta).await?;
        let roles = commands::leer_roles(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let mut nombres = Nombres::desde(&roles, &canales);
        let ordenada = ordenar_propuesta(propuesta);
        let total = ordenada.acciones.len();
        let mut resultados = Vec::new();
        for (indice, a) in ordenada.acciones.iter().enumerate() {
            if cancelar.load(Ordering::SeqCst) {
                break;
            }
            let (estado, mensaje) = match nombres.resolver_args(&a.herramienta, &a.args) {
                Err(e) => (EstadoAccion::Omitida, e),
                Ok(args) => match self.ejecutar_accion(&a.herramienta, args.clone()).await {
                    Ok(salida) => {
                        nombres.registrar_creado(&a.herramienta, &args, &salida);
                        (EstadoAccion::Ok, salida)
                    }
                    Err(e) => (EstadoAccion::Error, e),
                },
            };
            let r = ResultadoAccion {
                indice,
                herramienta: a.herramienta.clone(),
                descripcion: a.descripcion.clone(),
                estado,
                mensaje,
            };
            avance(total, &r);
            resultados.push(r);
        }
        Ok(ResumenPropuesta::de(total, resultados))
    }

    async fn despachar_accion(
        &self,
        nombre: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        comprobar_escritura(nombre)?;
        match nombre {
            "elegir_servidor" => {
                let p: ElegirServidorParams = args_de(nombre, args)?;
                self.elegir_servidor_json(&p.guild_id).await
            }
            "enviar_mensaje" => {
                let p: EnviarMensajeParams = args_de(nombre, args)?;
                self.enviar_mensaje_json(&p.channel_id, &p.contenido).await
            }
            "enviar_dm" => {
                let p: EnviarDmParams = args_de(nombre, args)?;
                self.enviar_dm_json(&p.user_id, &p.contenido).await
            }
            "borrar_mensaje" => {
                let p: MensajeParams = args_de(nombre, args)?;
                self.borrar_mensaje_json(p.guild_id.as_deref(), &p.channel_id, &p.message_id)
                    .await
            }
            "borrar_mensajes" => {
                let p: BorrarMensajesParams = args_de(nombre, args)?;
                self.borrar_mensajes_json(p.guild_id.as_deref(), &p.channel_id, &p.message_ids)
                    .await
            }
            "fijar_mensaje" | "desfijar_mensaje" => {
                let p: MensajeParams = args_de(nombre, args)?;
                self.fijar_mensaje_json(
                    p.guild_id.as_deref(),
                    &p.channel_id,
                    &p.message_id,
                    nombre == "fijar_mensaje",
                )
                .await
            }
            "crear_canal" => self.crear_canal_json(&args_de(nombre, args)?).await,
            "editar_canal" => self.editar_canal_json(&args_de(nombre, args)?).await,
            "borrar_canal" => {
                let p: CanalParams = args_de(nombre, args)?;
                self.borrar_canal_json(p.guild_id.as_deref(), &p.channel_id)
                    .await
            }
            "ordenar_canales" => {
                let p: OrdenarParams = args_de(nombre, args)?;
                self.ordenar_canales_json(p.guild_id.as_deref(), &p.posiciones)
                    .await
            }
            "crear_rol" => self.crear_rol_json(&args_de(nombre, args)?).await,
            "editar_rol" => self.editar_rol_json(&args_de(nombre, args)?).await,
            "borrar_rol" => {
                let p: RolParams = args_de(nombre, args)?;
                self.borrar_rol_json(p.guild_id.as_deref(), &p.role_id)
                    .await
            }
            "ordenar_roles" => {
                let p: OrdenarParams = args_de(nombre, args)?;
                self.ordenar_roles_json(p.guild_id.as_deref(), &p.posiciones)
                    .await
            }
            "dar_rol" | "quitar_rol" => {
                let p: RolMiembroParams = args_de(nombre, args)?;
                self.cambiar_rol_json(
                    p.guild_id.as_deref(),
                    &p.user_id,
                    &p.role_id,
                    nombre == "dar_rol",
                )
                .await
            }
            "permiso_canal" => self.permiso_canal_json(&args_de(nombre, args)?).await,
            "quitar_permiso_canal" => {
                let p: QuitarPermisoCanalParams = args_de(nombre, args)?;
                self.quitar_permiso_canal_json(&p).await
            }
            "expulsar" => self.expulsar_json(&args_de(nombre, args)?).await,
            "banear" => self.banear_json(&args_de(nombre, args)?).await,
            "desbanear" => {
                let p: ExpulsarParams = args_de(nombre, args)?;
                self.desbanear_json(p.guild_id.as_deref(), &p.user_id, p.motivo.as_deref())
                    .await
            }
            "silenciar" => self.silenciar_json(&args_de(nombre, args)?).await,
            "cambiar_apodo" => self.cambiar_apodo_json(&args_de(nombre, args)?).await,
            "editar_servidor" => self.editar_servidor_json(&args_de(nombre, args)?).await,
            "crear_invitacion" => self.crear_invitacion_json(&args_de(nombre, args)?).await,
            "borrar_invitacion" => {
                let p: BorrarInvitacionParams = args_de(nombre, args)?;
                self.borrar_invitacion_json(p.guild_id.as_deref(), &p.codigo)
                    .await
            }
            "crear_webhook" => self.crear_webhook_json(&args_de(nombre, args)?).await,
            "borrar_webhook" => {
                let p: BorrarWebhookParams = args_de(nombre, args)?;
                self.borrar_webhook_json(p.guild_id.as_deref(), &p.webhook_id)
                    .await
            }
            _ => Err(format!("Unknown action: «{nombre}»")),
        }
    }
}

#[tool_router]
impl ArgusMcp {
    #[tool(
        name = "estado",
        description = "Shows the active ARGUS bot (with its official Discord name) and the active server, which the other tools use by default."
    )]
    async fn estado(&self) -> Result<String, String> {
        let r = self.estado_json().await;
        self.salida(r)
    }

    #[tool(
        name = "listar_servidores",
        description = "Lists the Discord servers the active bot is in (id and name) and marks which one is the active server."
    )]
    async fn listar_servidores(&self) -> Result<String, String> {
        let r = self.listar_servidores_json().await;
        self.salida(r)
    }

    #[tool(
        name = "elegir_servidor",
        description = "Changes the active server (saved in the ARGUS database, same as choosing it in the app). It must be one of the active bot's servers."
    )]
    async fn elegir_servidor(
        &self,
        Parameters(p): Parameters<ElegirServidorParams>,
    ) -> Result<String, String> {
        let r = self.elegir_servidor_json(&p.guild_id).await;
        self.salida(r)
    }

    #[tool(
        name = "mapa_servidor",
        description = "Server map: categories with their channels, channels without a category and roles (with member counts). Read-only. `guild_id` is optional (defaults to the active server)."
    )]
    async fn mapa_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.mapa_servidor_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_canales",
        description = "Lists the server's channels and categories (id, name, type, category, position and specific permissions). Read-only. `guild_id` is optional."
    )]
    async fn listar_canales(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_canales_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_roles",
        description = "Lists the server's roles, highest to lowest, with color and readable permissions. Read-only. `guild_id` is optional."
    )]
    async fn listar_roles(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_roles_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_miembros",
        description = "Lists server members with their roles. Supports `buscar` (user, nickname or ID) and `limite` (default 25, maximum 100). Read-only. `guild_id` is optional."
    )]
    async fn listar_miembros(
        &self,
        Parameters(p): Parameters<ListarMiembrosParams>,
    ) -> Result<String, String> {
        let r = self
            .listar_miembros_json(p.guild_id.as_deref(), p.buscar.as_deref(), p.limite)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "info_miembro",
        description = "Details of one member (user, nickname, roles with names, join date). Read-only. `guild_id` is optional."
    )]
    async fn info_miembro(
        &self,
        Parameters(p): Parameters<InfoMiembroParams>,
    ) -> Result<String, String> {
        let r = self
            .info_miembro_json(p.guild_id.as_deref(), &p.user_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "permisos_bot",
        description = "Effective permissions of the active bot on the server, with their official names. Read-only. `guild_id` is optional."
    )]
    async fn permisos_bot(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.permisos_bot_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "info_servidor",
        description = "Name, icon and approximate member count of the server. Read-only. `guild_id` is optional."
    )]
    async fn info_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.info_servidor_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "ajustes_servidor",
        description = "General server settings (description, verification, content filter, notifications, system and AFK channels). Requires «Manage Server». Read-only. `guild_id` is optional."
    )]
    async fn ajustes_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.ajustes_servidor_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "registro_auditoria",
        description = "Latest 50 audit log entries as readable text. Requires «View Audit Log». Read-only. `guild_id` is optional."
    )]
    async fn registro_auditoria(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.registro_auditoria_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "leer_mensajes",
        description = "Reads the latest messages of a channel (`limite` from 1 to 50, newest first). Requires «Read Message History». Read-only. `guild_id` is optional."
    )]
    async fn leer_mensajes(
        &self,
        Parameters(p): Parameters<LeerMensajesParams>,
    ) -> Result<String, String> {
        let r = self
            .leer_mensajes_json(p.guild_id.as_deref(), &p.channel_id, p.limite)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "listar_baneos",
        description = "Lists banned users with their reason. Requires «Ban Members». Read-only. `guild_id` is optional."
    )]
    async fn listar_baneos(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_baneos_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_invitaciones",
        description = "Lists the server's invites (code, link, channel, uses). Requires «Manage Server». Read-only. `guild_id` is optional."
    )]
    async fn listar_invitaciones(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_invitaciones_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_webhooks",
        description = "Lists the server's webhooks (id, name, channel) without token or URL: the full URL is only given when creating one. Requires «Manage Webhooks». Read-only. `guild_id` is optional."
    )]
    async fn listar_webhooks(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_webhooks_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "enviar_mensaje",
        description = "Sends a text message (up to 2000 characters) to a channel as the active bot. Requires permission to send messages in that channel."
    )]
    async fn enviar_mensaje(
        &self,
        Parameters(p): Parameters<EnviarMensajeParams>,
    ) -> Result<String, String> {
        let r = self.enviar_mensaje_json(&p.channel_id, &p.contenido).await;
        self.salida(r)
    }

    #[tool(
        name = "enviar_dm",
        description = "Sends a direct message (up to 2000 characters) to a user as the active bot. The user must allow direct messages."
    )]
    async fn enviar_dm(&self, Parameters(p): Parameters<EnviarDmParams>) -> Result<String, String> {
        let r = self.enviar_dm_json(&p.user_id, &p.contenido).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_mensaje",
        description = "Deletes a message from a channel. Requires «Manage Messages». `guild_id` is optional (defaults to the active server)."
    )]
    async fn borrar_mensaje(
        &self,
        Parameters(p): Parameters<MensajeParams>,
    ) -> Result<String, String> {
        let r = self
            .borrar_mensaje_json(p.guild_id.as_deref(), &p.channel_id, &p.message_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_mensajes",
        description = "Deletes several messages from a channel at once (between 2 and 100, all less than 14 days old). Requires «Manage Messages». `guild_id` is optional."
    )]
    async fn borrar_mensajes(
        &self,
        Parameters(p): Parameters<BorrarMensajesParams>,
    ) -> Result<String, String> {
        let r = self
            .borrar_mensajes_json(p.guild_id.as_deref(), &p.channel_id, &p.message_ids)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "fijar_mensaje",
        description = "Pins a message in its channel. Requires «Manage Messages». `guild_id` is optional."
    )]
    async fn fijar_mensaje(
        &self,
        Parameters(p): Parameters<MensajeParams>,
    ) -> Result<String, String> {
        let r = self
            .fijar_mensaje_json(p.guild_id.as_deref(), &p.channel_id, &p.message_id, true)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "desfijar_mensaje",
        description = "Unpins a message from its channel. Requires «Manage Messages». `guild_id` is optional."
    )]
    async fn desfijar_mensaje(
        &self,
        Parameters(p): Parameters<MensajeParams>,
    ) -> Result<String, String> {
        let r = self
            .fijar_mensaje_json(p.guild_id.as_deref(), &p.channel_id, &p.message_id, false)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "crear_canal",
        description = "Creates a channel or category (`tipo`: texto, voz, categoria, anuncios or foro). Requires «Manage Channels». `guild_id` is optional."
    )]
    async fn crear_canal(
        &self,
        Parameters(p): Parameters<CrearCanalParams>,
    ) -> Result<String, String> {
        let r = self.crear_canal_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "editar_canal",
        description = "Edits a channel's name, category, topic, NSFW flag or slowmode; only the given fields change. Requires «Manage Channels». `guild_id` is optional."
    )]
    async fn editar_canal(
        &self,
        Parameters(p): Parameters<EditarCanalParams>,
    ) -> Result<String, String> {
        let r = self.editar_canal_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_canal",
        description = "Deletes a channel or category (irreversible). Requires «Manage Channels». `guild_id` is optional."
    )]
    async fn borrar_canal(&self, Parameters(p): Parameters<CanalParams>) -> Result<String, String> {
        let r = self
            .borrar_canal_json(p.guild_id.as_deref(), &p.channel_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "ordenar_canales",
        description = "Changes the position of several channels at once with a list of {id, posicion}. Requires «Manage Channels». `guild_id` is optional."
    )]
    async fn ordenar_canales(
        &self,
        Parameters(p): Parameters<OrdenarParams>,
    ) -> Result<String, String> {
        let r = self
            .ordenar_canales_json(p.guild_id.as_deref(), &p.posiciones)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "crear_rol",
        description = "Creates a role. `permisos` accepts the bits text or a list of names (e.g. [\"ManageMessages\",\"ViewChannel\"]). Requires «Manage Roles». `guild_id` is optional."
    )]
    async fn crear_rol(&self, Parameters(p): Parameters<CrearRolParams>) -> Result<String, String> {
        let r = self.crear_rol_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "editar_rol",
        description = "Edits a role; only the given fields change. `permisos` (bits or list of names) replaces the current ones. Requires «Manage Roles» and the role must be below the bot's role. `guild_id` is optional."
    )]
    async fn editar_rol(
        &self,
        Parameters(p): Parameters<EditarRolParams>,
    ) -> Result<String, String> {
        let r = self.editar_rol_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_rol",
        description = "Deletes a role (irreversible). Requires «Manage Roles» and the role must be below the bot's role. `guild_id` is optional."
    )]
    async fn borrar_rol(&self, Parameters(p): Parameters<RolParams>) -> Result<String, String> {
        let r = self
            .borrar_rol_json(p.guild_id.as_deref(), &p.role_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "ordenar_roles",
        description = "Changes the position of several roles with a list of {id, posicion}. Source and target must stay below the bot's role. Requires «Manage Roles». `guild_id` is optional."
    )]
    async fn ordenar_roles(
        &self,
        Parameters(p): Parameters<OrdenarParams>,
    ) -> Result<String, String> {
        let r = self
            .ordenar_roles_json(p.guild_id.as_deref(), &p.posiciones)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "dar_rol",
        description = "Adds a role to a member. Discord requires «Manage Roles» and the role must be below the bot's role. `guild_id` is optional."
    )]
    async fn dar_rol(&self, Parameters(p): Parameters<RolMiembroParams>) -> Result<String, String> {
        let r = self
            .cambiar_rol_json(p.guild_id.as_deref(), &p.user_id, &p.role_id, true)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "quitar_rol",
        description = "Removes a role from a member. Discord requires «Manage Roles» and the role must be below the bot's role. `guild_id` is optional."
    )]
    async fn quitar_rol(
        &self,
        Parameters(p): Parameters<RolMiembroParams>,
    ) -> Result<String, String> {
        let r = self
            .cambiar_rol_json(p.guild_id.as_deref(), &p.user_id, &p.role_id, false)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "permiso_canal",
        description = "Sets the specific permissions of a role or member in a channel (`tipo`: rol or miembro; `permitir` and `denegar` as bits or a list of names). Requires «Manage Roles». `guild_id` is optional."
    )]
    async fn permiso_canal(
        &self,
        Parameters(p): Parameters<PermisoCanalParams>,
    ) -> Result<String, String> {
        let r = self.permiso_canal_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "quitar_permiso_canal",
        description = "Removes the specific permissions of a role or member in a channel (`tipo`: rol or miembro). Requires «Manage Roles». `guild_id` is optional."
    )]
    async fn quitar_permiso_canal(
        &self,
        Parameters(p): Parameters<QuitarPermisoCanalParams>,
    ) -> Result<String, String> {
        let r = self.quitar_permiso_canal_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "expulsar",
        description = "Kicks a member from the server. The `motivo` goes to the audit log. Requires «Kick Members»; the owner and anyone with a role equal to or above the bot's are rejected. `guild_id` is optional."
    )]
    async fn expulsar(&self, Parameters(p): Parameters<ExpulsarParams>) -> Result<String, String> {
        let r = self.expulsar_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "banear",
        description = "Bans a user (even if not in the server), with a `motivo` for the audit log and `dias_mensajes` (0–7) of messages to delete. Requires «Ban Members»; the owner and anyone with a role equal to or above the bot's are rejected. `guild_id` is optional."
    )]
    async fn banear(&self, Parameters(p): Parameters<BanearParams>) -> Result<String, String> {
        let r = self.banear_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "desbanear",
        description = "Unbans a user, with an optional `motivo` for the audit log. Requires «Ban Members». `guild_id` is optional."
    )]
    async fn desbanear(&self, Parameters(p): Parameters<ExpulsarParams>) -> Result<String, String> {
        let r = self
            .desbanear_json(p.guild_id.as_deref(), &p.user_id, p.motivo.as_deref())
            .await;
        self.salida(r)
    }

    #[tool(
        name = "silenciar",
        description = "Times out a member for `minutos` (maximum 40320 = 28 days); 0 removes the timeout. The `motivo` goes to the audit log. Requires «Timeout Members»; the owner and anyone with a role equal to or above the bot's are rejected. `guild_id` is optional."
    )]
    async fn silenciar(
        &self,
        Parameters(p): Parameters<SilenciarParams>,
    ) -> Result<String, String> {
        let r = self.silenciar_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "cambiar_apodo",
        description = "Changes a member's nickname on the server (up to 32 characters); without `apodo` it is cleared. The `motivo` goes to the audit log. Requires «Manage Nicknames»; the owner and anyone with a role equal to or above the bot's are rejected (the bot itself always can). `guild_id` is optional."
    )]
    async fn cambiar_apodo(
        &self,
        Parameters(p): Parameters<CambiarApodoParams>,
    ) -> Result<String, String> {
        let r = self.cambiar_apodo_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "editar_servidor",
        description = "Edits the general server settings (name, description, verification level, content filter, default notifications, system channel, AFK channel and AFK timeout); only the given fields change and an empty description or channel clears it. Requires «Manage Server». `guild_id` is optional."
    )]
    async fn editar_servidor(
        &self,
        Parameters(p): Parameters<EditarServidorParams>,
    ) -> Result<String, String> {
        let r = self.editar_servidor_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "crear_invitacion",
        description = "Creates an invite to a channel (`caducidad_segundos` 0–604800, default 86400; `usos_maximos` 0–100, default unlimited). Requires «Create Invite». `guild_id` is optional."
    )]
    async fn crear_invitacion(
        &self,
        Parameters(p): Parameters<CrearInvitacionParams>,
    ) -> Result<String, String> {
        let r = self.crear_invitacion_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_invitacion",
        description = "Deletes an invite by its code. Requires «Manage Server». `guild_id` is optional."
    )]
    async fn borrar_invitacion(
        &self,
        Parameters(p): Parameters<BorrarInvitacionParams>,
    ) -> Result<String, String> {
        let r = self
            .borrar_invitacion_json(p.guild_id.as_deref(), &p.codigo)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "crear_webhook",
        description = "Creates a webhook in a channel and returns its full URL (shown only this once: it contains the token, keep it safe). Requires «Manage Webhooks». `guild_id` is optional."
    )]
    async fn crear_webhook(
        &self,
        Parameters(p): Parameters<CrearWebhookParams>,
    ) -> Result<String, String> {
        let r = self.crear_webhook_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_webhook",
        description = "Deletes a webhook by its ID (irreversible). Requires «Manage Webhooks». `guild_id` is optional."
    )]
    async fn borrar_webhook(
        &self,
        Parameters(p): Parameters<BorrarWebhookParams>,
    ) -> Result<String, String> {
        let r = self
            .borrar_webhook_json(p.guild_id.as_deref(), &p.webhook_id)
            .await;
        self.salida(r)
    }
}

#[tool_handler]
impl ServerHandler for ArgusMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("argus-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Manage Discord with the bots registered in ARGUS. Start with `estado` \
                 to see the active bot and server.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use crate::db::{self, BotConfig};
    use chrono::Utc;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TOKEN: &str = "token-secreto-de-prueba.XYZ";

    #[test]
    fn color_acepta_hex_como_lo_devuelve_listar_roles_y_numero() {
        let de = |v: serde_json::Value| serde_json::from_value::<ColorParam>(v).unwrap();
        assert_eq!(de(json!("#5865F2")).a_u32().unwrap(), 0x5865F2);
        assert_eq!(de(json!("5865f2")).a_u32().unwrap(), 0x5865F2);
        assert_eq!(de(json!(255)).a_u32().unwrap(), 255);
        assert!(de(json!("azul")).a_u32().is_err());
        assert!(de(json!(0x0100_0000)).a_u32().is_err());
    }

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
    async fn los_mensajes_salen_en_el_idioma_de_la_config() {
        let (_dir, pool) = bd_temporal().await;
        let discord = discord_falso().await;
        let mcp = servidor(pool.clone(), &discord).await;

        // Inglés por defecto.
        assert_eq!(
            mcp.estado().await.unwrap_err(),
            "No active bot in ARGUS: add or choose one in the app"
        );

        // Español con `ui_language = "es"` guardado en la base.
        mcp.state.config.lock().unwrap().ui_language = "es".to_string();
        mcp.state.save_config_to_pool(&pool).await.unwrap();
        assert_eq!(
            mcp.estado().await.unwrap_err(),
            "No hay bot activo en ARGUS: añade o elige uno en la app"
        );
        let err = mcp
            .elegir_servidor(Parameters(ElegirServidorParams {
                guild_id: "555".into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "No hay bot activo en ARGUS: añade o elige uno en la app"
        );

        // Con bot activo: el servidor activo falta y el mensaje también se traduce.
        guardar_bot(&pool).await;
        assert_eq!(
            mcp.listar_canales(Parameters(ServidorParams::default()))
                .await
                .unwrap_err(),
            "No hay servidor activo en ARGUS: indica `guild_id` o elige uno con `elegir_servidor`"
        );
        mcp.state.config.lock().unwrap().ui_language = "en".to_string();
        mcp.state.save_config_to_pool(&pool).await.unwrap();
        assert_eq!(
            mcp.listar_canales(Parameters(ServidorParams::default()))
                .await
                .unwrap_err(),
            "No active server in ARGUS: pass `guild_id` or choose one with `elegir_servidor`"
        );
    }

    #[test]
    fn las_descripciones_de_las_herramientas_estan_en_ingles() {
        for t in ArgusMcp::tool_router().list_all() {
            let d = t.description.as_deref().unwrap_or_default();
            assert!(
                !d.contains("Requiere") && !d.contains("opcional."),
                "{}",
                t.name
            );
            assert!(!d.contains("Solo lectura"), "{}", t.name);
        }
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
        // Crear un canal dentro de una categoría lee antes sus permisos para
        // copiarlos: respuesta genérica (sin permisos propios) de baja prioridad.
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex(r"^/api/v10/channels/\d+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "10", "type": 4, "guild_id": "555", "name": "Categoría",
                "position": 0, "permission_overwrites": []
            })))
            .with_priority(10)
            .mount(&discord)
            .await;
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
            "The bot is missing the permission «Banear miembros»"
        );
        assert_eq!(
            mcp.registro_auditoria_json(None).await.unwrap_err(),
            "The bot is missing the permission «Ver registro de auditoría»"
        );
        assert_eq!(
            mcp.listar_webhooks_json(None).await.unwrap_err(),
            "The bot is missing the permission «Gestionar webhooks»"
        );
        assert_eq!(
            mcp.leer_mensajes_json(None, "11", None).await.unwrap_err(),
            "The bot is missing the permission «Leer el historial de mensajes»"
        );
    }

    // ------------------------------------------- escritura: mensajes, canales, roles

    const GESTIONAR_MENSAJES: u64 = 1 << 13;
    const GESTIONAR_CANALES: u64 = 1 << 4;
    const GESTIONAR_ROLES: u64 = 1 << 28;
    const JERARQUIA: &str =
        "That role is above the bot's role: move it up in Server settings → Roles";

    fn mensaje_discord(id: &str, canal: &str) -> serde_json::Value {
        json!({
            "id": id, "channel_id": canal, "content": "hola",
            "timestamp": "2026-10-27T10:00:00.000000+00:00",
            "edited_timestamp": null, "tts": false, "mention_everyone": false,
            "mentions": [], "mention_roles": [], "attachments": [], "embeds": [],
            "pinned": false, "type": 0,
            "author": {"id": "1", "username": "bot", "discriminator": "0", "avatar": null}
        })
    }

    /// Acepta una petición de cambio exactamente `veces` veces.
    async fn aceptar(
        server: &MockServer,
        metodo: &str,
        ruta: &str,
        estado: u16,
        cuerpo: Option<serde_json::Value>,
        veces: u64,
    ) {
        let mut r = ResponseTemplate::new(estado);
        if let Some(c) = cuerpo {
            r = r.set_body_json(c);
        }
        Mock::given(method(metodo))
            .and(path(ruta))
            .respond_with(r)
            .expect(veces)
            .mount(server)
            .await;
    }

    /// Ninguna petición de cambio debe llegar al Discord falso.
    async fn prohibir_cambios(server: &MockServer) {
        for metodo in ["POST", "PATCH", "PUT", "DELETE"] {
            Mock::given(method(metodo))
                .respond_with(ResponseTemplate::new(200))
                .expect(0)
                .mount(server)
                .await;
        }
    }

    /// Añade al servidor falso un rol «Alto» por encima del rol del bot.
    async fn con_rol_alto(server: &MockServer, bits: u64) {
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                rol("555", "@everyone", 0, 0),
                rol("600", "Bot", 5, bits),
                rol("700", "Moderadores", 3, 8192),
                rol("900", "Alto", 9, 0)
            ])))
            .with_priority(1)
            .mount(server)
            .await;
    }

    fn canal_nuevo(nombre: &str, tipo: Option<&str>) -> CrearCanalParams {
        CrearCanalParams {
            guild_id: None,
            nombre: nombre.into(),
            tipo: tipo.map(str::to_string),
            categoria_id: None,
            tema: None,
            nsfw: None,
            slowmode_segundos: None,
        }
    }

    fn editar_canal_nombre(id: &str, nombre: &str) -> EditarCanalParams {
        EditarCanalParams {
            guild_id: None,
            channel_id: id.into(),
            nombre: Some(nombre.into()),
            categoria_id: None,
            tema: None,
            nsfw: None,
            slowmode_segundos: None,
        }
    }

    fn rol_nuevo(nombre: &str) -> CrearRolParams {
        CrearRolParams {
            guild_id: None,
            nombre: nombre.into(),
            color: None,
            separado: None,
            mencionable: None,
            permisos: None,
        }
    }

    fn editar_rol_nombre(id: &str, nombre: &str) -> EditarRolParams {
        EditarRolParams {
            guild_id: None,
            role_id: id.into(),
            nombre: Some(nombre.into()),
            color: None,
            separado: None,
            mencionable: None,
            permisos: None,
        }
    }

    fn pos(id: &str, p: u64) -> Posicion {
        Posicion {
            id: id.into(),
            posicion: p,
        }
    }

    fn permiso(target: &str, tipo: &str) -> PermisoCanalParams {
        PermisoCanalParams {
            guild_id: None,
            channel_id: "11".into(),
            target_id: target.into(),
            tipo: tipo.into(),
            permitir: None,
            denegar: None,
        }
    }

    fn quitar_permiso(target: &str, tipo: &str) -> QuitarPermisoCanalParams {
        QuitarPermisoCanalParams {
            guild_id: None,
            channel_id: "11".into(),
            target_id: target.into(),
            tipo: tipo.into(),
        }
    }

    fn snowflake_hace_dias(dias: i64) -> String {
        let ms = chrono::Utc::now().timestamp_millis() - dias * 86_400_000;
        (((ms - 1_420_070_400_000) as u64) << 22).to_string()
    }

    #[tokio::test]
    async fn envia_mensaje_y_dm() {
        let (_d, mcp, s) = mcp_con_servidor(TODO, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/11/messages"))
            .and(wiremock::matchers::body_partial_json(
                json!({"content": "hola"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(mensaje_discord("900", "11")))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "POST",
            "/api/v10/users/@me/channels",
            200,
            Some(json!({"id": "4242", "type": 1})),
            1,
        )
        .await;
        aceptar(
            &s,
            "POST",
            "/api/v10/channels/4242/messages",
            200,
            Some(mensaje_discord("901", "4242")),
            1,
        )
        .await;
        let r = v(mcp.enviar_mensaje_json("11", "hola").await.unwrap());
        assert!(r["mensaje"].as_str().unwrap().contains("900"));
        let r = v(mcp.enviar_dm_json("1001", "hola").await.unwrap());
        assert!(r["mensaje"].as_str().unwrap().contains("901"));
        // Mismos errores que la app: vacío o demasiado largo no llegan a Discord.
        let largo = "a".repeat(2001);
        let e = mcp.enviar_mensaje_json("11", &largo).await.unwrap_err();
        assert!(e.contains("too long"), "{e}");
        assert!(mcp.enviar_dm_json("1001", "  ").await.is_err());
    }

    #[tokio::test]
    async fn borra_y_fija_mensajes() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/channels/11/messages/301",
            204,
            None,
            1,
        )
        .await;
        aceptar(&s, "PUT", "/api/v10/channels/11/pins/301", 204, None, 1).await;
        aceptar(&s, "DELETE", "/api/v10/channels/11/pins/301", 204, None, 1).await;
        let r = mcp.borrar_mensaje_json(None, "11", "301").await.unwrap();
        assert!(r.contains("deleted"));
        let r = mcp
            .fijar_mensaje_json(None, "11", "301", true)
            .await
            .unwrap();
        assert!(r.contains("pinned"));
        let r = mcp
            .fijar_mensaje_json(None, "11", "301", false)
            .await
            .unwrap();
        assert!(r.contains("unpinned"));
    }

    #[tokio::test]
    async fn borrado_en_masa_valida_cantidad_y_antiguedad() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        let (a, b) = (snowflake_hace_dias(1), snowflake_hace_dias(2));
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/11/messages/bulk-delete"))
            .and(wiremock::matchers::body_partial_json(
                json!({"messages": [a, b]}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        let ok = mcp.borrar_mensajes_json(None, "11", &[a.clone(), b]).await;
        assert!(ok.unwrap().contains("2 messages deleted"));
        // Una sola, o una de más de 14 días: error de la app, sin llamar a Discord.
        let e = mcp
            .borrar_mensajes_json(None, "11", &[a.clone()])
            .await
            .unwrap_err();
        assert!(e.contains("between 2 and 100"), "{e}");
        let e = mcp
            .borrar_mensajes_json(None, "11", &[a, snowflake_hace_dias(20)])
            .await
            .unwrap_err();
        assert!(e.contains("14 days"), "{e}");
    }

    #[tokio::test]
    async fn mensajes_sin_permiso_no_llegan_a_discord() {
        let (_d, mcp, s) = mcp_con_servidor(1 << 10, true).await;
        prohibir_cambios(&s).await;
        // `borrar_mensaje` sí llama a Discord sin el permiso: el bot puede borrar
        // los suyos (ver tests_mensajes.rs).
        let falta = "The bot is missing the permission «Gestionar mensajes»";
        let e = mcp.fijar_mensaje_json(None, "11", "301", true).await;
        assert_eq!(e.unwrap_err(), falta);
        let ids = [snowflake_hace_dias(1), snowflake_hace_dias(2)];
        let e = mcp.borrar_mensajes_json(None, "11", &ids).await;
        assert_eq!(e.unwrap_err(), falta);
    }

    #[tokio::test]
    async fn crea_edita_borra_y_ordena_canales() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES, true).await;
        let canal = |id: &str, nombre: &str, tipo: u8| {
            json!({"id": id, "type": tipo, "guild_id": "555", "name": nombre,
                   "position": 4, "parent_id": "10"})
        };
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "avisos", "type": 0, "parent_id": "10", "topic": "t"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(canal("20", "avisos", 0)))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/channels/20"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "noticias"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(canal("20", "noticias", 0)))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_json(json!([
                {"id": "20", "position": 1}, {"id": "11", "position": 0}
            ])))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/channels/20",
            200,
            Some(canal("20", "x", 0)),
            1,
        )
        .await;

        let mut p = canal_nuevo("avisos", Some("texto"));
        p.categoria_id = Some("10".into());
        p.tema = Some("t".into());
        let creado = v(mcp.crear_canal_json(&p).await.unwrap());
        assert_eq!(creado["canal"]["id"], "20");
        let editado = v(mcp
            .editar_canal_json(&editar_canal_nombre("20", "noticias"))
            .await
            .unwrap());
        assert_eq!(editado["canal"]["nombre"], "noticias");
        let r = mcp
            .ordenar_canales_json(None, &[pos("20", 1), pos("11", 0)])
            .await
            .unwrap();
        assert!(r.contains("2 channels reordered"));
        let r = mcp.borrar_canal_json(None, "20").await.unwrap();
        assert!(r.contains("Channel 20 deleted"));
        // Tipo desconocido: error propio, sin llamar a Discord.
        let e = mcp
            .crear_canal_json(&canal_nuevo("x", Some("holograma")))
            .await
            .unwrap_err();
        assert!(e.contains("Unsupported channel type"));
    }

    #[tokio::test]
    async fn canales_sin_permiso_no_llegan_a_discord() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let falta = "The bot is missing the permission «Gestionar canales»";
        let e = mcp.crear_canal_json(&canal_nuevo("x", None)).await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp.borrar_canal_json(None, "11").await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp.ordenar_canales_json(None, &[pos("11", 0)]).await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp.editar_canal_json(&editar_canal_nombre("11", "y")).await;
        assert_eq!(e.unwrap_err(), falta);
    }

    #[test]
    fn permisos_como_bits_o_como_nombres() {
        let lista = PermisosParam::Nombres(vec!["ManageMessages".into(), "ViewChannel".into()]);
        let esperado = ((1u64 << 13) | (1 << 10)).to_string();
        assert_eq!(lista.a_bits().unwrap(), esperado);
        // Mayúsculas con guion bajo y nombres en español.
        let otra = PermisosParam::Nombres(vec!["MANAGE_MESSAGES".into(), "Ver canales".into()]);
        assert_eq!(otra.a_bits().unwrap(), esperado);
        assert_eq!(
            PermisosParam::Bits(" 8192 ".into()).a_bits().unwrap(),
            "8192"
        );
        assert!(PermisosParam::Bits("abc".into()).a_bits().is_err());
        let e = PermisosParam::Nombres(vec!["Volar".into()])
            .a_bits()
            .unwrap_err();
        assert!(e.contains("Volar"));
        // Se leen ambas formas desde JSON.
        let a: PermisosParam = serde_json::from_value(json!("8")).unwrap();
        let b: PermisosParam = serde_json::from_value(json!(["Administrator"])).unwrap();
        assert_eq!(a.a_bits().unwrap(), b.a_bits().unwrap());
    }

    #[tokio::test]
    async fn crea_edita_borra_y_ordena_roles() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_ROLES, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(json!({
                "name": "Mods", "color": 255, "hoist": true, "permissions": "9216"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1000", "Mods", 1, 9216)))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/roles/700"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Mods2", "permissions": "8192"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("700", "Mods2", 3, 8192)))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(&s, "DELETE", "/api/v10/guilds/555/roles/700", 204, None, 1).await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .expect(1)
            .mount(&s)
            .await;

        let mut p = rol_nuevo("Mods");
        p.color = Some(ColorParam::Numero(255));
        p.separado = Some(true);
        p.permisos = Some(PermisosParam::Nombres(vec![
            "ManageMessages".into(),
            "ViewChannel".into(),
        ]));
        let creado = v(mcp.crear_rol_json(&p).await.unwrap());
        assert_eq!(creado["rol"]["id"], "1000");
        assert_eq!(creado["rol"]["permisos"][0], "Ver canales");
        let mut p = editar_rol_nombre("700", "Mods2");
        p.permisos = Some(PermisosParam::Bits("8192".into()));
        let editado = v(mcp.editar_rol_json(&p).await.unwrap());
        assert_eq!(editado["rol"]["nombre"], "Mods2");
        let r = mcp
            .ordenar_roles_json(None, &[pos("700", 2)])
            .await
            .unwrap();
        assert!(r.contains("1 roles reordered"));
        let r = mcp.borrar_rol_json(None, "700").await.unwrap();
        assert!(r.contains("Role 700 deleted"));
    }

    #[tokio::test]
    async fn roles_sin_permiso_o_sobre_la_jerarquia_no_llegan_a_discord() {
        // Sin «Gestionar roles».
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let falta = "The bot is missing the permission «Gestionar roles»";
        assert_eq!(mcp.borrar_rol_json(None, "700").await.unwrap_err(), falta);
        let e = mcp.crear_rol_json(&rol_nuevo("x")).await;
        assert_eq!(e.unwrap_err(), falta);

        // Con permiso, pero con un rol por encima del bot.
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_ROLES, true).await;
        con_rol_alto(&s, GESTIONAR_ROLES).await;
        prohibir_cambios(&s).await;
        assert_eq!(
            mcp.borrar_rol_json(None, "900").await.unwrap_err(),
            JERARQUIA
        );
        let e = mcp.editar_rol_json(&editar_rol_nombre("900", "y")).await;
        assert_eq!(e.unwrap_err(), JERARQUIA);
        let e = mcp.ordenar_roles_json(None, &[pos("700", 9)]).await;
        assert_eq!(e.unwrap_err(), JERARQUIA);
        let e = mcp.permiso_canal_json(&permiso("900", "rol")).await;
        assert_eq!(e.unwrap_err(), JERARQUIA);
        let e = mcp
            .quitar_permiso_canal_json(&quitar_permiso("900", "rol"))
            .await;
        assert_eq!(e.unwrap_err(), JERARQUIA);
    }

    #[tokio::test]
    async fn da_y_quita_roles() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_ROLES, true).await;
        aceptar(
            &s,
            "PUT",
            "/api/v10/guilds/555/members/1001/roles/700",
            204,
            None,
            1,
        )
        .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/guilds/555/members/1001/roles/700",
            204,
            None,
            1,
        )
        .await;
        let r = mcp
            .cambiar_rol_json(None, "1001", "700", true)
            .await
            .unwrap();
        assert!(r.contains("added"));
        let r = mcp
            .cambiar_rol_json(None, "1001", "700", false)
            .await
            .unwrap();
        assert!(r.contains("removed"));
    }

    #[tokio::test]
    async fn fija_y_quita_permisos_de_canal() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_ROLES, true).await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/11/permissions/700"))
            .and(wiremock::matchers::body_partial_json(
                json!({"allow": "9216", "deny": "2048", "type": 0}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/11/permissions/1001"))
            .and(wiremock::matchers::body_partial_json(
                json!({"allow": "0", "deny": "2048", "type": 1}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/channels/11/permissions/700",
            204,
            None,
            1,
        )
        .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/channels/11/permissions/1001",
            204,
            None,
            1,
        )
        .await;

        let mut p = permiso("700", "rol");
        p.permitir = Some(PermisosParam::Nombres(vec![
            "ManageMessages".into(),
            "ViewChannel".into(),
        ]));
        p.denegar = Some(PermisosParam::Bits("2048".into()));
        let r = mcp.permiso_canal_json(&p).await.unwrap();
        assert!(r.contains("updated"));
        let mut p = permiso("1001", "miembro");
        p.denegar = Some(PermisosParam::Nombres(vec!["SendMessages".into()]));
        mcp.permiso_canal_json(&p).await.unwrap();
        for (objetivo, tipo) in [("700", "rol"), ("1001", "miembro")] {
            let r = mcp
                .quitar_permiso_canal_json(&quitar_permiso(objetivo, tipo))
                .await
                .unwrap();
            assert!(r.contains("removed"));
        }
        // Tipo inválido y permiso desconocido: errores propios.
        let e = mcp
            .quitar_permiso_canal_json(&quitar_permiso("700", "cosa"))
            .await
            .unwrap_err();
        assert!(e.contains("«rol» or «miembro»"));
        let mut p = permiso("700", "rol");
        p.permitir = Some(PermisosParam::Nombres(vec!["Volar".into()]));
        let e = mcp.permiso_canal_json(&p).await.unwrap_err();
        assert!(e.contains("Volar"));
    }

    #[tokio::test]
    async fn permisos_de_canal_sin_permiso_no_llegan_a_discord() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let falta = "The bot is missing the permission «Gestionar roles»";
        let e = mcp.permiso_canal_json(&permiso("700", "rol")).await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp
            .quitar_permiso_canal_json(&quitar_permiso("700", "rol"))
            .await;
        assert_eq!(e.unwrap_err(), falta);
    }

    #[tokio::test]
    async fn escritura_sin_servidor_o_sin_bot_lo_dice() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, false).await;
        let e = mcp.borrar_canal_json(None, "11").await;
        assert_eq!(e.unwrap_err(), SIN_SERVIDOR);
        let e = mcp.cambiar_rol_json(None, "1", "2", true).await;
        assert_eq!(e.unwrap_err(), SIN_SERVIDOR);
        let (_dir, pool) = bd_temporal().await;
        let discord = discord_falso().await;
        let sin_bot = servidor(pool, &discord).await;
        let e = sin_bot.enviar_mensaje_json("11", "hola").await;
        assert_eq!(e.unwrap_err(), SIN_BOT);
        let e = sin_bot.enviar_dm_json("1", "hola").await;
        assert_eq!(e.unwrap_err(), SIN_BOT);
        let e = sin_bot.borrar_rol_json(Some("555"), "7").await;
        assert_eq!(e.unwrap_err(), SIN_BOT);
    }

    #[tokio::test]
    async fn las_herramientas_de_escritura_estan_registradas() {
        let tools = ArgusMcp::tool_router().list_all();
        let nombres: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        for n in [
            "enviar_mensaje",
            "enviar_dm",
            "borrar_mensaje",
            "borrar_mensajes",
            "fijar_mensaje",
            "desfijar_mensaje",
            "crear_canal",
            "editar_canal",
            "borrar_canal",
            "ordenar_canales",
            "crear_rol",
            "editar_rol",
            "borrar_rol",
            "ordenar_roles",
            "dar_rol",
            "quitar_rol",
            "permiso_canal",
            "quitar_permiso_canal",
        ] {
            assert!(nombres.contains(&n.to_string()), "falta {n}");
        }
        let crear = tools.iter().find(|t| t.name == "crear_rol").unwrap();
        assert!(crear.input_schema["properties"]["permisos"].is_object());
    }

    #[test]
    fn cada_herramienta_esta_en_exactamente_una_lista() {
        let registradas: Vec<String> = ArgusMcp::tool_router()
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        for n in &registradas {
            let veces = HERRAMIENTAS_LECTURA.iter().filter(|x| **x == n).count()
                + HERRAMIENTAS_ESCRITURA.iter().filter(|x| **x == n).count();
            assert_eq!(veces, 1, "{n} debe estar en exactamente una lista");
        }
        for n in HERRAMIENTAS_LECTURA.iter().chain(HERRAMIENTAS_ESCRITURA) {
            assert!(registradas.iter().any(|r| r == n), "{n} no está registrada");
        }
    }

    #[test]
    fn las_herramientas_de_lectura_no_modifican_nada() {
        const PERMITIDAS: [&str; 5] = [
            "estado",
            "mapa_servidor",
            "permisos_bot",
            "ajustes_servidor",
            "registro_auditoria",
        ];
        for n in HERRAMIENTAS_LECTURA {
            let ok = PERMITIDAS.contains(n)
                || *n == "listar_servidores"
                || *n == "leer_mensajes"
                || n.starts_with("listar_")
                || n.starts_with("info_");
            assert!(ok, "{n} no es una herramienta de lectura conocida");
            assert!(
                !HERRAMIENTAS_ESCRITURA.contains(n),
                "{n} está en ambas listas"
            );
        }
        for n in HERRAMIENTAS_ESCRITURA {
            assert!(!HERRAMIENTAS_LECTURA.contains(n));
        }
    }

    // ------------------------- moderación, servidor, invitaciones y webhooks

    const KICK: u64 = 1 << 1;
    const BAN: u64 = 1 << 2;
    const GESTIONAR_SERVIDOR: u64 = 1 << 5;
    const CREAR_INVITACION: u64 = 1;
    const GESTIONAR_APODOS: u64 = 1 << 27;
    const GESTIONAR_WEBHOOKS: u64 = 1 << 29;
    const MODERAR: u64 = 1 << 40;
    const DUENO: &str = "That cannot be done to the server owner";
    const JERARQUIA_MIEMBRO: &str = "That member has a role equal to or above the bot's: move the bot's role up in Server settings → Roles";

    fn expulsar_a(user: &str) -> ExpulsarParams {
        ExpulsarParams {
            guild_id: None,
            user_id: user.into(),
            motivo: None,
        }
    }

    fn banear_a(user: &str, dias: Option<u32>) -> BanearParams {
        BanearParams {
            guild_id: None,
            user_id: user.into(),
            motivo: None,
            dias_mensajes: dias,
        }
    }

    fn silenciar_a(user: &str, minutos: u32) -> SilenciarParams {
        SilenciarParams {
            guild_id: None,
            user_id: user.into(),
            minutos,
            motivo: None,
        }
    }

    fn apodo_de(user: &str, apodo: Option<&str>) -> CambiarApodoParams {
        CambiarApodoParams {
            guild_id: None,
            user_id: user.into(),
            apodo: apodo.map(Into::into),
            motivo: None,
        }
    }

    fn servidor_vacio() -> EditarServidorParams {
        EditarServidorParams {
            guild_id: None,
            nombre: None,
            descripcion: None,
            nivel_verificacion: None,
            filtro_contenido_explicito: None,
            notificaciones_por_defecto: None,
            canal_del_sistema_id: None,
            canal_afk_id: None,
            afk_segundos: None,
        }
    }

    #[tokio::test]
    async fn modera_con_el_motivo_en_la_auditoria() {
        let (_d, mcp, s) = mcp_con_servidor(KICK | BAN | MODERAR | GESTIONAR_APODOS, true).await;
        Mock::given(method("DELETE"))
            .and(path("/api/v10/guilds/555/members/1001"))
            .and(header("x-audit-log-reason", "spam"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/guilds/555/bans/1001"))
            .and(header("x-audit-log-reason", "insultos"))
            .and(wiremock::matchers::body_partial_json(
                json!({"delete_message_seconds": 2 * 86400}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/v10/guilds/555/bans/1004"))
            .and(header("x-audit-log-reason", "perdonado"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        let ana = miembro("1001", "ana", Some("Anita"), &["700"]);
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/members/1001"))
            .and(header("x-audit-log-reason", "calma"))
            .and(wiremock::matchers::body_partial_json(
                json!({"communication_disabled_until": null}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(ana.clone()))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/members/1001"))
            .and(header("x-audit-log-reason", "normas"))
            .and(wiremock::matchers::body_partial_json(
                json!({"nick": "Nuevo"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(ana))
            .expect(1)
            .mount(&s)
            .await;

        let mut p = expulsar_a("1001");
        p.motivo = Some("spam".into());
        assert!(mcp.expulsar_json(&p).await.unwrap().contains("kicked"));
        let mut p = banear_a("1001", Some(2));
        p.motivo = Some("insultos".into());
        assert!(mcp.banear_json(&p).await.unwrap().contains("banned"));
        let r = mcp
            .desbanear_json(None, "1004", Some("perdonado"))
            .await
            .unwrap();
        assert!(r.contains("Ban of 1004 removed"));
        let mut p = silenciar_a("1001", 0);
        p.motivo = Some("calma".into());
        assert!(mcp.silenciar_json(&p).await.unwrap().contains("removed"));
        let mut p = apodo_de("1001", Some("Nuevo"));
        p.motivo = Some("normas".into());
        assert!(mcp
            .cambiar_apodo_json(&p)
            .await
            .unwrap()
            .contains("Nickname"));
    }

    #[tokio::test]
    async fn silencia_con_duracion() {
        let (_d, mcp, s) = mcp_con_servidor(MODERAR, true).await;
        let ana = miembro("1001", "ana", Some("Anita"), &["700"]);
        aceptar(
            &s,
            "PATCH",
            "/api/v10/guilds/555/members/1001",
            200,
            Some(ana),
            1,
        )
        .await;
        let r = mcp.silenciar_json(&silenciar_a("1001", 10)).await.unwrap();
        assert!(r.contains("timed out for 10 minutes"), "{r}");
        let peticiones = s.received_requests().await.unwrap();
        let patch = peticiones
            .iter()
            .find(|r| r.method.as_str() == "PATCH")
            .unwrap();
        let cuerpo: serde_json::Value = serde_json::from_slice(&patch.body).unwrap();
        assert!(cuerpo["communication_disabled_until"].is_string());
        // Más de 28 días: error de la app, sin llamar a Discord.
        let e = mcp
            .silenciar_json(&silenciar_a("1001", 40321))
            .await
            .unwrap_err();
        assert!(e.contains("28 days"), "{e}");
    }

    #[tokio::test]
    async fn moderacion_rechaza_dueno_y_jerarquia_con_el_mensaje_de_la_app() {
        let (_d, mcp, s) = mcp_con_servidor(KICK | BAN | MODERAR | GESTIONAR_APODOS, true).await;
        con_rol_alto(&s, KICK | BAN | MODERAR | GESTIONAR_APODOS).await;
        get_json(
            &s,
            "/api/v10/guilds/555/members/1002",
            miembro("1002", "beto", None, &["900"]),
        )
        .await;
        prohibir_cambios(&s).await;
        // El dueño es «9».
        assert_eq!(
            mcp.expulsar_json(&expulsar_a("9")).await.unwrap_err(),
            DUENO
        );
        assert_eq!(
            mcp.banear_json(&banear_a("9", None)).await.unwrap_err(),
            DUENO
        );
        assert_eq!(
            mcp.silenciar_json(&silenciar_a("9", 5)).await.unwrap_err(),
            DUENO
        );
        let e = mcp.cambiar_apodo_json(&apodo_de("9", Some("x"))).await;
        assert_eq!(e.unwrap_err(), DUENO);
        // «1002» tiene un rol por encima del bot.
        assert_eq!(
            mcp.expulsar_json(&expulsar_a("1002")).await.unwrap_err(),
            JERARQUIA_MIEMBRO
        );
        assert_eq!(
            mcp.banear_json(&banear_a("1002", None)).await.unwrap_err(),
            JERARQUIA_MIEMBRO
        );
        assert_eq!(
            mcp.silenciar_json(&silenciar_a("1002", 5))
                .await
                .unwrap_err(),
            JERARQUIA_MIEMBRO
        );
        let e = mcp.cambiar_apodo_json(&apodo_de("1002", Some("x"))).await;
        assert_eq!(e.unwrap_err(), JERARQUIA_MIEMBRO);
        // Días de mensajes fuera de 0–7.
        let e = mcp.banear_json(&banear_a("1001", Some(8))).await;
        assert_eq!(
            e.unwrap_err(),
            "The days of messages to delete must be between 0 and 7"
        );
        let e = mcp.banear_json(&banear_a("1001", Some(300))).await;
        assert!(e.unwrap_err().contains("between 0 and 7"));
    }

    #[tokio::test]
    async fn moderacion_sin_permiso_no_llega_a_discord() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let falta = |p: &str| format!("The bot is missing the permission «{p}»");
        let e = mcp.expulsar_json(&expulsar_a("1001")).await;
        assert_eq!(e.unwrap_err(), falta("Expulsar miembros"));
        let e = mcp.banear_json(&banear_a("1001", None)).await;
        assert_eq!(e.unwrap_err(), falta("Banear miembros"));
        let e = mcp.desbanear_json(None, "1004", None).await;
        assert_eq!(e.unwrap_err(), falta("Banear miembros"));
        let e = mcp.silenciar_json(&silenciar_a("1001", 5)).await;
        assert_eq!(e.unwrap_err(), falta("Moderar miembros"));
        let e = mcp.cambiar_apodo_json(&apodo_de("1001", Some("x"))).await;
        assert_eq!(e.unwrap_err(), falta("Gestionar apodos"));
        let e = mcp.editar_servidor_json(&servidor_vacio()).await;
        assert_eq!(e.unwrap_err(), falta("Gestionar servidor"));
        let p = CrearInvitacionParams {
            guild_id: None,
            channel_id: "11".into(),
            caducidad_segundos: None,
            usos_maximos: None,
        };
        let e = mcp.crear_invitacion_json(&p).await;
        assert_eq!(e.unwrap_err(), falta("Crear invitaciones"));
        let e = mcp.borrar_invitacion_json(None, "abc123").await;
        assert_eq!(e.unwrap_err(), falta("Gestionar servidor"));
        let p = CrearWebhookParams {
            guild_id: None,
            channel_id: "11".into(),
            nombre: "Avisos".into(),
        };
        let e = mcp.crear_webhook_json(&p).await;
        assert_eq!(e.unwrap_err(), falta("Gestionar webhooks"));
        let e = mcp.borrar_webhook_json(None, "900").await;
        assert_eq!(e.unwrap_err(), falta("Gestionar webhooks"));
    }

    #[tokio::test]
    async fn edita_el_servidor_cambiando_solo_los_campos_indicados() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_SERVIDOR, true).await;
        let guild = json!({
            "id": "555", "name": "Taller 2", "owner_id": "9", "description": null,
            "roles": [], "emojis": [], "features": [], "afk_timeout": 300,
            "afk_channel_id": null, "system_channel_id": "11",
            "default_message_notifications": 1, "explicit_content_filter": 2,
            "verification_level": 1, "mfa_level": 0, "nsfw_level": 0,
            "premium_tier": 0, "system_channel_flags": 0
        });
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555"))
            .and(wiremock::matchers::body_partial_json(json!({
                "name": "Taller 2", "verification_level": 1, "afk_channel_id": null,
                "system_channel_id": "11", "explicit_content_filter": 2
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(guild))
            .expect(1)
            .mount(&s)
            .await;
        let mut p = servidor_vacio();
        p.nombre = Some("Taller 2".into());
        p.nivel_verificacion = Some(1);
        p.canal_afk_id = Some(String::new());
        let r = v(mcp.editar_servidor_json(&p).await.unwrap());
        assert_eq!(r["servidor"]["nombre"], "Taller 2");
        assert_eq!(r["servidor"]["nivel_verificacion"], 1);
        assert!(r["servidor"]["canal_afk_id"].is_null());
        // Validaciones de la app: sin llamar a Discord.
        let mut p = servidor_vacio();
        p.nivel_verificacion = Some(9);
        assert!(mcp.editar_servidor_json(&p).await.is_err());
        let mut p = servidor_vacio();
        p.nombre = Some("x".into());
        assert!(mcp.editar_servidor_json(&p).await.is_err());
    }

    #[tokio::test]
    async fn crea_y_borra_invitaciones() {
        let (_d, mcp, s) = mcp_con_servidor(CREAR_INVITACION | GESTIONAR_SERVIDOR, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/11/invites"))
            .and(wiremock::matchers::body_partial_json(
                json!({"max_age": 3600, "max_uses": 5}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"code": "abc123", "uses": 0, "max_uses": 5, "max_age": 3600,
                       "channel": {"id": "11", "name": "general"}}),
            ))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/invites/abc123",
            200,
            Some(json!({"code": "abc123"})),
            1,
        )
        .await;
        let p = CrearInvitacionParams {
            guild_id: None,
            channel_id: "11".into(),
            caducidad_segundos: Some(3600),
            usos_maximos: Some(5),
        };
        let r = v(mcp.crear_invitacion_json(&p).await.unwrap());
        assert_eq!(r["invitacion"]["code"], "abc123");
        assert_eq!(r["invitacion"]["url"], "https://discord.gg/abc123");
        let r = mcp.borrar_invitacion_json(None, "abc123").await.unwrap();
        assert!(r.contains("abc123"));
        // Más de 7 días: error de la app.
        let p = CrearInvitacionParams {
            caducidad_segundos: Some(604_801),
            ..p
        };
        let e = mcp.crear_invitacion_json(&p).await.unwrap_err();
        assert!(e.contains("7 days"), "{e}");
    }

    #[tokio::test]
    async fn crea_y_borra_webhooks_y_solo_al_crear_da_la_url() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_WEBHOOKS, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/11/webhooks"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Avisos"}),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id": "901", "name": "Avisos", "channel_id": "11",
                       "token": WEBHOOK_TOKEN, "type": 1})),
            )
            .expect(1)
            .mount(&s)
            .await;
        aceptar(&s, "DELETE", "/api/v10/webhooks/900", 204, None, 1).await;
        let p = CrearWebhookParams {
            guild_id: None,
            channel_id: "11".into(),
            nombre: "Avisos".into(),
        };
        let r = v(mcp.crear_webhook_json(&p).await.unwrap());
        assert_eq!(r["webhook"]["id"], "901");
        assert_eq!(
            r["webhook"]["url"],
            format!("https://discord.com/api/webhooks/901/{WEBHOOK_TOKEN}")
        );
        let r = mcp.borrar_webhook_json(None, "900").await.unwrap();
        assert!(r.contains("900"));
        // Nombre vacío: error de la app.
        let vacio = CrearWebhookParams {
            nombre: "  ".into(),
            ..p
        };
        assert!(mcp.crear_webhook_json(&vacio).await.is_err());
        // El listado sigue sin token.
        let lista = mcp.listar_webhooks_json(None).await.unwrap();
        assert!(!lista.contains(WEBHOOK_TOKEN));
    }

    #[tokio::test]
    async fn las_herramientas_de_moderacion_estan_registradas() {
        let tools = ArgusMcp::tool_router().list_all();
        let nombres: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        for n in [
            "expulsar",
            "banear",
            "desbanear",
            "silenciar",
            "cambiar_apodo",
            "editar_servidor",
            "crear_invitacion",
            "borrar_invitacion",
            "crear_webhook",
            "borrar_webhook",
        ] {
            assert!(nombres.contains(&n.to_string()), "falta {n}");
        }
        let banear = tools.iter().find(|t| t.name == "banear").unwrap();
        assert!(banear.input_schema["properties"]["dias_mensajes"].is_object());
        assert!(banear.input_schema["properties"]["motivo"].is_object());
        let silenciar = tools.iter().find(|t| t.name == "silenciar").unwrap();
        assert!(silenciar.input_schema["properties"]["minutos"].is_object());
    }

    #[tokio::test]
    async fn ejecutar_accion_usa_la_logica_del_mcp() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES | GESTIONAR_ROLES, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Mods", "permissions": "9216"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1000", "Mods", 1, 9216)))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "avisos", "type": 0, "topic": "t"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id": "20", "type": 0, "guild_id": "555", "name": "avisos", "position": 4}),
            ))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/11/permissions/700"))
            .and(wiremock::matchers::body_partial_json(
                json!({"allow": "9216", "deny": "2048", "type": 0}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;

        let r = v(mcp
            .ejecutar_accion(
                "crear_rol",
                json!({"nombre": "Mods", "permisos": ["ManageMessages", "ViewChannel"]}),
            )
            .await
            .unwrap());
        assert_eq!(r["rol"]["id"], "1000");
        let r = v(mcp
            .ejecutar_accion("crear_canal", json!({"nombre": "avisos", "tema": "t"}))
            .await
            .unwrap());
        assert_eq!(r["canal"]["id"], "20");
        let r = mcp
            .ejecutar_accion(
                "permiso_canal",
                json!({"channel_id": "11", "target_id": "700", "tipo": "rol",
                       "permitir": ["ManageMessages", "ViewChannel"], "denegar": "2048"}),
            )
            .await
            .unwrap();
        assert!(r.contains("updated"));

        // Nombre inventado, herramienta de lectura y argumentos inválidos.
        let e = mcp.ejecutar_accion("inventada", json!({})).await;
        assert!(e.unwrap_err().contains("Unknown action"));
        let e = mcp.ejecutar_accion("listar_roles", json!({})).await;
        assert!(e.unwrap_err().contains("read-only"));
        let e = mcp.ejecutar_accion("crear_rol", json!({"color": 1})).await;
        assert!(e.unwrap_err().contains("Invalid arguments"));
    }

    #[tokio::test]
    async fn ejecutar_propuesta_resuelve_nombres_a_ids() {
        // Como en la app: el MCP sin servidor activo; la app pone su guild_id.
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES | GESTIONAR_ROLES, false).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Nueva", "type": 4}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id": "30", "type": 4, "guild_id": "555", "name": "Nueva", "position": 5}),
            ))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "avisos", "type": 0, "parent_id": "30"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id": "31", "type": 0, "guild_id": "555", "name": "avisos", "position": 6,
                       "parent_id": "30"}),
            ))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/31/permissions/700"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;

        let acc = |h: &str, args: serde_json::Value| crate::propuesta::Accion {
            herramienta: h.to_string(),
            args,
            descripcion: h.to_string(),
        };
        // Desordenada: el permiso y el canal van antes que su categoría.
        let p = Propuesta {
            resumen: "x".to_string(),
            acciones: vec![
                acc(
                    "permiso_canal",
                    json!({"channel_id": "avisos", "target_id": "Moderadores", "tipo": "rol",
                           "permitir": "1024"}),
                ),
                acc("dar_rol", json!({"user_id": "1001", "role_id": "Fantasma"})),
                acc(
                    "crear_canal",
                    json!({"nombre": "avisos", "parent": "Nueva"}),
                ),
                acc(
                    "crear_canal",
                    json!({"nombre": "Nueva", "tipo": "categoria"}),
                ),
            ],
        };
        let p = crate::propuesta::con_servidor(&p, "555");
        let r = mcp.ejecutar_propuesta(&p).await.unwrap();
        let h: Vec<_> = r.iter().map(|x| x.herramienta.as_str()).collect();
        assert_eq!(
            h,
            ["crear_canal", "crear_canal", "permiso_canal", "dar_rol"]
        );
        assert_eq!(r[0].descripcion, "crear_canal");
        assert!(r[..3].iter().all(|x| x.estado == EstadoAccion::Ok), "{r:?}");
        // La referencia que no se resuelve se omite, sin tocar Discord.
        assert_eq!(r[3].estado, EstadoAccion::Omitida);
        assert!(r[3].mensaje.contains("«Fantasma»"));
    }

    /// El Discord falso responde a `GET roles` con los roles de siempre más estos
    /// (los que la propuesta acaba de crear).
    async fn roles_con_nuevos(s: &MockServer, nuevos: &[serde_json::Value]) {
        let mut roles = vec![
            rol("555", "@everyone", 0, 0),
            rol("600", "Bot", 5, TODO),
            rol("700", "Moderadores", 3, 8192),
        ];
        roles.extend(nuevos.iter().cloned());
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(roles))
            .with_priority(1)
            .mount(s)
            .await;
    }

    fn accion(h: &str, args: serde_json::Value) -> crate::propuesta::Accion {
        crate::propuesta::Accion {
            herramienta: h.to_string(),
            descripcion: format!("{h} {args}"),
            args,
        }
    }

    #[tokio::test]
    async fn aplicar_propuesta_crea_en_orden_con_avance() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES | GESTIONAR_ROLES, true).await;
        for (id, nombre) in [("1100", "Staff"), ("1101", "Miembro")] {
            Mock::given(method("POST"))
                .and(path("/api/v10/guilds/555/roles"))
                .and(wiremock::matchers::body_partial_json(
                    json!({"name": nombre}),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(rol(id, nombre, 1, 0)))
                .expect(1)
                .mount(&s)
                .await;
        }
        let canal = |id: &str, tipo: u8, nombre: &str, padre: Option<&str>| {
            let mut c = json!({"id": id, "type": tipo, "guild_id": "555", "name": nombre,
                               "position": 9});
            if let Some(p) = padre {
                c["parent_id"] = json!(p);
            }
            c
        };
        let creados = [
            canal("40", 4, "Comunidad", None),
            canal("41", 0, "bienvenida", Some("40")),
            canal("42", 0, "normas", Some("40")),
            canal("43", 0, "charla", Some("40")),
        ];
        for c in creados {
            let mut cuerpo = json!({"name": c["name"], "type": c["type"]});
            if c["type"] == 0 {
                cuerpo["parent_id"] = json!("40");
            }
            Mock::given(method("POST"))
                .and(path("/api/v10/guilds/555/channels"))
                .and(wiremock::matchers::body_partial_json(cuerpo))
                .respond_with(ResponseTemplate::new(200).set_body_json(c))
                .expect(1)
                .mount(&s)
                .await;
        }
        roles_con_nuevos(
            &s,
            &[rol("1100", "Staff", 1, 0), rol("1101", "Miembro", 1, 0)],
        )
        .await;
        for (canal, rol) in [("41", "1100"), ("42", "1101")] {
            Mock::given(method("PUT"))
                .and(path(format!("/api/v10/channels/{canal}/permissions/{rol}")))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&s)
                .await;
        }

        // Desordenada a propósito: permisos y canales antes que sus roles/categoría.
        let p = Propuesta {
            resumen: "x".to_string(),
            acciones: vec![
                accion(
                    "permiso_canal",
                    json!({"channel_id": "bienvenida", "target_id": "Staff", "tipo": "rol",
                           "permitir": "1024"}),
                ),
                accion(
                    "permiso_canal",
                    json!({"channel_id": "normas", "target_id": "Miembro", "tipo": "rol",
                           "denegar": "2048"}),
                ),
                accion(
                    "crear_canal",
                    json!({"nombre": "bienvenida", "parent": "Comunidad"}),
                ),
                accion(
                    "crear_canal",
                    json!({"nombre": "normas", "parent": "Comunidad"}),
                ),
                accion(
                    "crear_canal",
                    json!({"nombre": "charla", "parent": "Comunidad"}),
                ),
                accion(
                    "crear_canal",
                    json!({"nombre": "Comunidad", "tipo": "categoria"}),
                ),
                accion("crear_rol", json!({"nombre": "Staff"})),
                accion("crear_rol", json!({"nombre": "Miembro"})),
            ],
        };
        let cancelar = AtomicBool::new(false);
        let mut avisos = Vec::new();
        let resumen = mcp
            .ejecutar_propuesta_con(&p, &cancelar, &mut |total, r| {
                avisos.push((total, r.indice, r.estado.clone()))
            })
            .await
            .unwrap();
        assert_eq!(
            (
                resumen.total,
                resumen.hechas,
                resumen.fallidas,
                resumen.omitidas
            ),
            (8, 8, 0, 0),
            "{resumen:?}"
        );
        assert_eq!(avisos.len(), 8);
        assert!(avisos
            .iter()
            .enumerate()
            .all(|(i, (t, idx, e))| *t == 8 && *idx == i && *e == EstadoAccion::Ok));
        // Orden en Discord: roles, categoría, canales, permisos (los `expect`
        // de los mocks comprueban parent_id y los IDs de los roles creados).
        let orden: Vec<String> = s
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method.as_str() != "GET")
            .map(|r| format!("{} {}", r.method, r.url.path()))
            .collect();
        let dest = |i: usize| orden[i].as_str();
        assert_eq!(dest(0), "POST /api/v10/guilds/555/roles");
        assert_eq!(dest(1), "POST /api/v10/guilds/555/roles");
        assert!(orden[2..6]
            .iter()
            .all(|o| o == "POST /api/v10/guilds/555/channels"));
        assert_eq!(dest(6), "PUT /api/v10/channels/41/permissions/1100");
        assert_eq!(dest(7), "PUT /api/v10/channels/42/permissions/1101");
    }

    #[tokio::test]
    async fn aplicar_propuesta_sigue_tras_un_fallo_omite_dependientes_y_cancela() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES | GESTIONAR_ROLES, true).await;
        // Crear el rol «Roto» falla; «Bueno» y el canal se crean.
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Roto"}),
            ))
            .respond_with(ResponseTemplate::new(500))
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Bueno"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1200", "Bueno", 1, 0)))
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id": "50", "type": 0, "guild_id": "555", "name": "sala", "position": 9}),
            ))
            .mount(&s)
            .await;
        roles_con_nuevos(&s, &[rol("1200", "Bueno", 1, 0)]).await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/50/permissions/1200"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&s)
            .await;
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![
                accion("crear_rol", json!({"nombre": "Roto"})),
                accion("crear_rol", json!({"nombre": "Bueno"})),
                accion("crear_canal", json!({"nombre": "sala"})),
                accion(
                    "permiso_canal",
                    json!({"channel_id": "sala", "target_id": "Roto", "tipo": "rol",
                           "permitir": "1024"}),
                ),
                accion(
                    "permiso_canal",
                    json!({"channel_id": "sala", "target_id": "Bueno", "tipo": "rol",
                           "permitir": "1024"}),
                ),
            ],
        };
        let cancelar = AtomicBool::new(false);
        let r = mcp
            .ejecutar_propuesta_con(&p, &cancelar, &mut |_, _| {})
            .await
            .unwrap();
        let e: Vec<_> = r.resultados.iter().map(|x| x.estado.clone()).collect();
        assert_eq!(
            e,
            [
                EstadoAccion::Error,
                EstadoAccion::Ok,
                EstadoAccion::Ok,
                EstadoAccion::Omitida,
                EstadoAccion::Ok
            ]
        );
        assert_eq!(
            (r.hechas, r.fallidas, r.omitidas, r.canceladas),
            (3, 1, 1, 0)
        );

        // Cancelar tras la primera acción: el resto no se intenta.
        let cancelar = AtomicBool::new(false);
        let r = mcp
            .ejecutar_propuesta_con(&p, &cancelar, &mut |_, _| {
                cancelar.store(true, Ordering::SeqCst)
            })
            .await
            .unwrap();
        assert_eq!((r.resultados.len(), r.canceladas, r.total), (1, 4, 5));
    }

    #[tokio::test]
    async fn ejecutar_accion_respeta_los_permisos() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let e = mcp
            .ejecutar_accion("crear_canal", json!({"nombre": "x"}))
            .await;
        assert_eq!(
            e.unwrap_err(),
            "The bot is missing the permission «Manage Channels»"
        );
    }
}

#[cfg(test)]
mod tests_firmas {
    use super::*;

    #[test]
    fn firmas_cubren_todas_las_de_escritura_con_sus_obligatorios() {
        let f = firmas_escritura();
        println!("{f}");
        for h in HERRAMIENTAS_ESCRITURA
            .iter()
            .filter(|h| **h != "elegir_servidor")
        {
            assert!(f.contains(&format!("- `{h}`:")), "falta {h}");
            assert!(esquema_args(h).is_some(), "sin esquema {h}");
        }
        // La que falló en la prueba real: `posiciones` es obligatorio.
        let ordenar = f
            .lines()
            .find(|l| l.starts_with("- `ordenar_roles`"))
            .unwrap();
        assert!(
            ordenar.contains("posiciones (") && ordenar.contains("required"),
            "{ordenar}"
        );
        let canal = f
            .lines()
            .find(|l| l.starts_with("- `crear_canal`"))
            .unwrap();
        assert!(canal.contains("nombre (string, required)"), "{canal}");
        assert!(!f.contains("guild_id"));
    }
}
