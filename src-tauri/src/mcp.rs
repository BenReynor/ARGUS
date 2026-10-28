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

use crate::commands::{sincronizar_guilds, token_del_bot_activo};
use crate::state::{AppState, Guild};

/// Respuesta de cada herramienta cuando no hay ningún bot activo.
pub const SIN_BOT: &str = "No hay bot activo en ARGUS: añade o elige uno en la app";

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
        for n in ["estado", "listar_servidores", "elegir_servidor"] {
            assert!(nombres.contains(&n.to_string()), "falta {n}");
        }
        for t in &tools {
            assert!(t.description.as_deref().is_some_and(|d| !d.is_empty()));
            assert_eq!(t.input_schema["type"], "object");
        }
        let elegir = tools.iter().find(|t| t.name == "elegir_servidor").unwrap();
        assert!(elegir.input_schema["properties"]["guild_id"].is_object());
    }
}
