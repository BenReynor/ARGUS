//! Sincronización completa del servidor activo al conectar: guild, canales,
//! roles y miembros por REST, informando del progreso paso a paso.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::discord;
use crate::mapa::reconstruir_categorias;
use crate::state::ServerMap;

/// Nombre del evento Tauri con el progreso de la sincronización.
pub const EVENTO_PROGRESO: &str = "sync-progress";

/// Número de pasos de la sincronización: guild, canales, roles y miembros.
pub const TOTAL_PASOS: u32 = 4;

/// Carga útil del evento `sync-progress`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SyncProgress {
    pub guild_id: String,
    pub step: u32,
    pub total: u32,
    pub message: String,
}

/// Emite `sync-progress` hacia la UI.
pub fn emitir_progreso(app: &AppHandle, progreso: &SyncProgress) {
    if let Err(e) = app.emit(EVENTO_PROGRESO, progreso) {
        tracing::warn!("No se pudo emitir sync-progress: {e}");
    }
}

/// Carga guild + canales + roles + miembros del servidor por REST. Llama a
/// `emitir` al empezar cada paso (1..=4 de 4) y devuelve el mapa completo.
pub async fn sincronizar_servidor(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    emitir: impl Fn(SyncProgress),
) -> Result<ServerMap, String> {
    let progreso = |step: u32, message: &str| {
        emitir(SyncProgress {
            guild_id: guild_id.to_string(),
            step,
            total: TOTAL_PASOS,
            message: message.to_string(),
        })
    };

    progreso(1, "Cargando datos del servidor");
    let guild = discord::fetch_guild_info(token, base_url, guild_id).await?;
    progreso(2, "Cargando canales");
    let channels = discord::fetch_guild_channels(token, base_url, guild_id).await?;
    progreso(3, "Cargando roles");
    let roles = discord::fetch_guild_roles(token, base_url, guild_id).await?;
    progreso(4, "Cargando miembros");
    let members = discord::fetch_guild_members(token, base_url, guild_id).await?;

    let mut mapa = ServerMap {
        guild,
        channels,
        members,
        roles,
        categories: Default::default(),
        last_updated: chrono::Utc::now().to_rfc3339(),
    };
    reconstruir_categorias(&mut mapa);
    Ok(mapa)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mock_json(server: &MockServer, ruta: &str, cuerpo: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(ruta))
            .respond_with(ResponseTemplate::new(200).set_body_json(cuerpo))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn sincroniza_guild_canales_roles_y_miembros() {
        let server = MockServer::start().await;
        mock_json(
            &server,
            "/api/v10/guilds/555",
            serde_json::json!({"id":"555","name":"Servidor de prueba","icon":null,
                "owner_id":"1","approximate_member_count":1}),
        )
        .await;
        mock_json(
            &server,
            "/api/v10/guilds/555/channels",
            serde_json::json!([
                {"id":"10","type":4,"name":"Texto","position":0,"permission_overwrites":[]},
                {"id":"11","type":0,"name":"general","position":1,"parent_id":"10",
                 "permission_overwrites":[]}
            ]),
        )
        .await;
        mock_json(
            &server,
            "/api/v10/guilds/555/roles",
            serde_json::json!([{"id":"555","name":"@everyone","color":0,"hoist":false,"flags":0,
                "position":0,"permissions":"1024","managed":false,"mentionable":false}]),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/members"))
            .and(query_param("limit", "1000"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "user":{"id":"1001","username":"ana","discriminator":"0","avatar":null},
                    "roles":[],"joined_at":"2024-01-01T00:00:00.000000+00:00",
                    "deaf":false,"mute":false,"flags":0
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;

        let addr = server.address().to_string();
        let pasos = Mutex::new(Vec::new());
        let mapa = sincronizar_servidor("token-falso", Some(&addr), "555", |p| {
            pasos.lock().unwrap().push(p)
        })
        .await
        .expect("sincronización");

        assert_eq!(mapa.guild.name, "Servidor de prueba");
        assert_eq!(mapa.channels.len(), 2);
        assert_eq!(mapa.categories["10"].len(), 1);
        assert_eq!(mapa.roles.len(), 1);
        assert_eq!(mapa.members.len(), 1);
        assert_eq!(mapa.members[0].username, "ana");

        let pasos = pasos.into_inner().unwrap();
        assert_eq!(
            pasos.iter().map(|p| (p.step, p.total)).collect::<Vec<_>>(),
            vec![(1, 4), (2, 4), (3, 4), (4, 4)]
        );
        assert!(pasos.iter().all(|p| p.guild_id == "555"));
    }

    #[tokio::test]
    async fn un_error_de_discord_corta_la_sincronizacion() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({"code":10004,"message":"Unknown Guild"})),
            )
            .mount(&server)
            .await;
        let addr = server.address().to_string();
        let pasos = Mutex::new(0u32);
        let err = sincronizar_servidor("token-falso", Some(&addr), "555", |_| {
            *pasos.lock().unwrap() += 1
        })
        .await
        .expect_err("debía fallar");
        assert!(err.starts_with("No encontrado"), "{err}");
        assert_eq!(*pasos.lock().unwrap(), 1);
    }
}
