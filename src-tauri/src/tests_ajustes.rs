//! Tests de ajustes del servidor y registro de auditoría contra el Discord falso.

use chrono::Utc;
use wiremock::matchers::{body_partial_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::commands::{guardar_ajustes_servidor, leer_ajustes_servidor, leer_auditoria};
use crate::crypto;
use crate::discord::AjustesServidor;
use crate::state::{AppState, BotConfig};

const GESTIONAR_SERVIDOR: u64 = 1 << 5;
const VER_AUDITORIA: u64 = 1 << 7;

fn estado(token: &str) -> AppState {
    let state = AppState::new();
    let now = Utc::now().to_rfc3339();
    state.config.lock().unwrap().bots.push(BotConfig {
        id: "123456789".to_string(),
        name: "bot".to_string(),
        token_ciphertext: crypto::encrypt(token).unwrap(),
        avatar: None,
        application_id: None,
        allowed_guilds: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    });
    state.set_active_bot("123456789");
    state
}

fn guild_json(nombre: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "555", "name": nombre, "owner_id": "9", "description": "Una comunidad",
        "roles": [], "emojis": [], "features": [], "afk_timeout": 300,
        "afk_channel_id": "20", "system_channel_id": "10",
        "default_message_notifications": 1, "explicit_content_filter": 2,
        "verification_level": 3, "mfa_level": 0, "nsfw_level": 0, "premium_tier": 0,
        "system_channel_flags": 0
    })
}

/// El bot tiene un rol con `bits` de permisos.
async fn mock_permisos(server: &MockServer, bits: u64) {
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555"))
        .respond_with(ResponseTemplate::new(200).set_body_json(guild_json("Original")))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/roles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "555", "name": "@everyone", "color": 0, "hoist": false, "flags": 0,
             "position": 0, "permissions": "0", "managed": false, "mentionable": false},
            {"id": "600", "name": "Bot", "color": 0, "hoist": false, "flags": 0,
             "position": 5, "permissions": bits.to_string(), "managed": false,
             "mentionable": false}
        ])))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/members/123456789"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user": {"id": "123456789", "username": "bot", "discriminator": "0",
                     "avatar": null, "bot": true},
            "nick": null, "roles": ["600"],
            "joined_at": "2024-01-02T03:04:05.000000+00:00",
            "deaf": false, "mute": false, "flags": 0
        })))
        .mount(server)
        .await;
}

fn ajustes() -> AjustesServidor {
    AjustesServidor {
        name: "Nuevo nombre".to_string(),
        description: Some("Descripción nueva".to_string()),
        verification_level: 2,
        explicit_content_filter: 1,
        default_message_notifications: 0,
        system_channel_id: Some("10".to_string()),
        afk_channel_id: None,
        afk_timeout: 900,
    }
}

#[tokio::test]
async fn lee_los_ajustes_del_servidor() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_SERVIDOR).await;
    let state = estado("token-aj1");
    let addr = server.address().to_string();
    let a = leer_ajustes_servidor(&state, "555", Some(&addr))
        .await
        .unwrap();
    assert_eq!(a.name, "Original");
    assert_eq!(a.description.as_deref(), Some("Una comunidad"));
    assert_eq!((a.verification_level, a.explicit_content_filter), (3, 2));
    assert_eq!(a.default_message_notifications, 1);
    assert_eq!(a.system_channel_id.as_deref(), Some("10"));
    assert_eq!(a.afk_channel_id.as_deref(), Some("20"));
    assert_eq!(a.afk_timeout, 300);
}

#[tokio::test]
async fn guarda_los_ajustes_con_patch() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_SERVIDOR).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/555"))
        .and(body_partial_json(serde_json::json!({
            "name": "Nuevo nombre", "description": "Descripción nueva",
            "verification_level": 2, "explicit_content_filter": 1,
            "default_message_notifications": 0, "system_channel_id": "10",
            "afk_channel_id": null, "afk_timeout": 900
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(guild_json("Nuevo nombre")))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-aj2");
    let addr = server.address().to_string();
    let r = guardar_ajustes_servidor(&state, "555", &ajustes(), Some(&addr))
        .await
        .unwrap();
    assert_eq!(r.name, "Nuevo nombre");
}

#[tokio::test]
async fn sin_gestionar_servidor_no_lee_ni_guarda() {
    let server = MockServer::start().await;
    mock_permisos(&server, VER_AUDITORIA).await;
    for metodo in ["PATCH", "PUT", "POST", "DELETE"] {
        Mock::given(method(metodo))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
    }
    let state = estado("token-aj3");
    let addr = server.address().to_string();
    let esperado = "The bot is missing the permission «Gestionar servidor»";
    assert_eq!(
        leer_ajustes_servidor(&state, "555", Some(&addr))
            .await
            .unwrap_err(),
        esperado
    );
    assert_eq!(
        guardar_ajustes_servidor(&state, "555", &ajustes(), Some(&addr))
            .await
            .unwrap_err(),
        esperado
    );
}

#[tokio::test]
async fn valida_los_ajustes_antes_de_llamar_a_discord() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_SERVIDOR).await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let state = estado("token-aj4");
    let addr = server.address().to_string();
    let mut corto = ajustes();
    corto.name = "a".to_string();
    assert_eq!(
        guardar_ajustes_servidor(&state, "555", &corto, Some(&addr))
            .await
            .unwrap_err(),
        "The server name must be between 2 and 100 characters"
    );
    let mut afk = ajustes();
    afk.afk_timeout = 123;
    assert_eq!(
        guardar_ajustes_servidor(&state, "555", &afk, Some(&addr))
            .await
            .unwrap_err(),
        "The AFK timeout must be 60, 300, 900, 1800 or 3600 seconds"
    );
    let mut nivel = ajustes();
    nivel.verification_level = 5;
    assert!(guardar_ajustes_servidor(&state, "555", &nivel, Some(&addr))
        .await
        .is_err());
}

fn snowflake(ms_unix: i64) -> String {
    (((ms_unix - 1_420_070_400_000) as u64) << 22).to_string()
}

#[tokio::test]
async fn auditoria_devuelve_entradas_legibles_en_espanol() {
    let server = MockServer::start().await;
    mock_permisos(&server, VER_AUDITORIA).await;
    // 2024-01-02 03:04:00 UTC
    let id = snowflake(1_704_164_640_000);
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/audit-logs"))
        .and(query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "audit_log_entries": [
                {"id": id, "user_id": "7", "target_id": "1001", "action_type": 22,
                 "reason": "spam", "changes": []},
                {"id": id, "user_id": "7", "target_id": "88", "action_type": 12,
                 "changes": []}
            ],
            "users": [
                {"id": "7", "username": "mod", "global_name": "Moderadora",
                 "discriminator": "0", "avatar": null},
                {"id": "1001", "username": "spammer", "discriminator": "0", "avatar": null}
            ],
            "webhooks": [], "integrations": [], "threads": [],
            "application_commands": [], "auto_moderation_rules": [],
            "guild_scheduled_events": []
        })))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-aj5");
    let addr = server.address().to_string();
    let e = leer_auditoria(&state, "555", Some(&addr)).await.unwrap();
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].accion, "Miembro baneado");
    assert_eq!(e[0].autor, "Moderadora");
    assert_eq!(e[0].objetivo, "spammer");
    assert_eq!(e[0].reason.as_deref(), Some("spam"));
    assert_eq!(e[0].fecha, "02/01/2024 03:04 UTC");
    assert_eq!(e[1].accion, "Canal eliminado");
    assert_eq!(e[1].objetivo, "88");
    assert_eq!(e[1].reason, None);
}

#[tokio::test]
async fn auditoria_sin_permiso_no_llama_al_registro() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_SERVIDOR).await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/audit-logs"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let state = estado("token-aj6");
    let addr = server.address().to_string();
    assert_eq!(
        leer_auditoria(&state, "555", Some(&addr))
            .await
            .unwrap_err(),
        "The bot is missing the permission «Ver registro de auditoría»"
    );
}
