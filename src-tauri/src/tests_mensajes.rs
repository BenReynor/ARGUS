//! Tests de mensajes, invitaciones y webhooks contra el Discord falso.

use chrono::Utc;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::commands::{
    borrar_invitacion, borrar_mensaje, borrar_mensajes_en_masa, borrar_webhook, crear_invitacion,
    crear_webhook, fijar_mensaje, leer_invitaciones, leer_mensajes, leer_webhooks,
};
use crate::crypto;
use crate::state::{AppState, BotConfig};

const CREAR_INVITACIONES: u64 = 1;
const GESTIONAR_SERVIDOR: u64 = 1 << 5;
const GESTIONAR_MENSAJES: u64 = 1 << 13;
const HISTORIAL: u64 = 1 << 16;
const GESTIONAR_WEBHOOKS: u64 = 1 << 29;

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

/// El bot tiene un rol con `bits` de permisos.
async fn mock_permisos(server: &MockServer, bits: u64) {
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "555", "name": "S", "owner_id": "9", "roles": [], "emojis": [],
            "features": [], "afk_timeout": 300, "default_message_notifications": 0,
            "explicit_content_filter": 0, "verification_level": 0, "mfa_level": 0,
            "nsfw_level": 0, "premium_tier": 0, "system_channel_flags": 0
        })))
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

/// Ningún método que modifique debe llegar al Discord falso.
async fn prohibir_escrituras(server: &MockServer) {
    for metodo in ["PATCH", "PUT", "POST", "DELETE"] {
        Mock::given(method(metodo))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(server)
            .await;
    }
}

fn snowflake(ms_unix: i64) -> String {
    (((ms_unix - 1_420_070_400_000) as u64) << 22).to_string()
}

fn hace_dias(dias: i64) -> String {
    snowflake(Utc::now().timestamp_millis() - dias * 86_400_000)
}

#[tokio::test]
async fn lista_mensajes_con_autor_y_fecha() {
    let server = MockServer::start().await;
    mock_permisos(&server, HISTORIAL).await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/10/messages"))
        .and(wiremock::matchers::query_param("limit", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "301", "channel_id": "10", "content": "hola", "pinned": true,
             "timestamp": "2026-10-27T10:00:00.000000+00:00", "edited_timestamp": null,
             "tts": false, "mention_everyone": false, "mentions": [], "mention_roles": [],
             "attachments": [], "embeds": [], "type": 0,
             "author": {"id": "7", "username": "ana", "global_name": "Ana",
                        "discriminator": "0", "avatar": null}},
            {"id": "300", "channel_id": "10", "content": "beep", "pinned": false,
             "timestamp": "2026-10-27T09:00:00.000000+00:00",
             "author": {"id": "8", "username": "robot", "discriminator": "0",
                        "avatar": null, "bot": true}}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-m1");
    let addr = server.address().to_string();
    let m = leer_mensajes(&state, "555", "10", Some(&addr))
        .await
        .unwrap();
    assert_eq!(m.len(), 2);
    assert_eq!(m[0].author_name, "Ana");
    assert_eq!(m[0].timestamp, "2026-10-27T10:00:00.000000+00:00");
    assert!(m[0].pinned);
    assert_eq!(m[1].author_name, "robot");
    assert!(m[1].author_bot);
}

#[tokio::test]
async fn borra_un_mensaje_y_fija_o_desfija() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_MENSAJES).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/channels/10/messages/301"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/channels/10/pins/301"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/channels/10/pins/301"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-m2");
    let addr = server.address().to_string();
    borrar_mensaje(&state, "555", "10", "301", Some(&addr))
        .await
        .unwrap();
    fijar_mensaje(&state, "555", "10", "301", true, Some(&addr))
        .await
        .unwrap();
    fijar_mensaje(&state, "555", "10", "301", false, Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn borrado_en_masa_de_mensajes_recientes() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_MENSAJES).await;
    let a = hace_dias(1);
    let b = hace_dias(3);
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/10/messages/bulk-delete"))
        .and(body_partial_json(serde_json::json!({"messages": [a, b]})))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-m3");
    let addr = server.address().to_string();
    let r = borrar_mensajes_en_masa(&state, "555", "10", &[a, b], Some(&addr))
        .await
        .unwrap();
    assert_eq!(r, "2 mensajes borrados");
}

#[tokio::test]
async fn borrado_en_masa_rechaza_antiguos_y_cantidades_invalidas() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_MENSAJES).await;
    prohibir_escrituras(&server).await;
    let state = estado("token-m4");
    let addr = server.address().to_string();
    let err = borrar_mensajes_en_masa(
        &state,
        "555",
        "10",
        &[hace_dias(1), hace_dias(20)],
        Some(&addr),
    )
    .await
    .unwrap_err();
    assert!(err.contains("más de 14 días"), "{err}");
    assert!(err.contains("1 de 2"), "{err}");
    let err = borrar_mensajes_en_masa(&state, "555", "10", &[hace_dias(1)], Some(&addr))
        .await
        .unwrap_err();
    assert_eq!(err, "El borrado en masa necesita entre 2 y 100 mensajes");
    let muchos: Vec<String> = (0..101).map(|i| (1_000_000 + i).to_string()).collect();
    assert!(
        borrar_mensajes_en_masa(&state, "555", "10", &muchos, Some(&addr))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn mensajes_sin_permiso_no_llaman_a_discord() {
    let server = MockServer::start().await;
    mock_permisos(&server, CREAR_INVITACIONES).await;
    prohibir_escrituras(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/10/messages"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let state = estado("token-m5");
    let addr = server.address().to_string();
    let m = "Al bot le falta el permiso «Gestionar mensajes»";
    assert_eq!(
        fijar_mensaje(&state, "555", "10", "301", true, Some(&addr))
            .await
            .unwrap_err(),
        m
    );
    assert_eq!(
        borrar_mensajes_en_masa(
            &state,
            "555",
            "10",
            &[hace_dias(1), hace_dias(2)],
            Some(&addr)
        )
        .await
        .unwrap_err(),
        m
    );
    assert_eq!(
        leer_mensajes(&state, "555", "10", Some(&addr))
            .await
            .unwrap_err(),
        "Al bot le falta el permiso «Leer el historial de mensajes»"
    );
}

#[tokio::test]
async fn sin_gestionar_mensajes_el_bot_borra_los_suyos() {
    // Discord deja borrar los mensajes propios sin «Gestionar mensajes».
    let server = MockServer::start().await;
    mock_permisos(&server, CREAR_INVITACIONES).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/channels/10/messages/301"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-propio");
    let addr = server.address().to_string();
    borrar_mensaje(&state, "555", "10", "301", Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn sin_gestionar_mensajes_un_mensaje_ajeno_explica_el_permiso() {
    let server = MockServer::start().await;
    mock_permisos(&server, CREAR_INVITACIONES).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/channels/10/messages/302"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "code": 50013, "message": "Missing Permissions"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-ajeno");
    let addr = server.address().to_string();
    assert_eq!(
        borrar_mensaje(&state, "555", "10", "302", Some(&addr))
            .await
            .unwrap_err(),
        "Al bot le falta el permiso «Gestionar mensajes»"
    );
}

#[tokio::test]
async fn crea_lista_y_borra_invitaciones() {
    let server = MockServer::start().await;
    mock_permisos(&server, CREAR_INVITACIONES | GESTIONAR_SERVIDOR).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/10/invites"))
        .and(body_partial_json(
            serde_json::json!({"max_age": 3600, "max_uses": 5}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": "abc123", "uses": 0, "max_uses": 5, "max_age": 3600,
            "channel": {"id": "10", "name": "general"},
            "inviter": {"id": "123456789", "username": "bot"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/invites"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"code": "abc123", "uses": 2, "max_uses": 5, "max_age": 3600,
             "channel": {"id": "10", "name": "general"},
             "expires_at": "2026-10-28T20:00:00+00:00"}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/invites/abc123"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"code": "abc123"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-i1");
    let addr = server.address().to_string();
    let i = crear_invitacion(&state, "555", "10", 3600, 5, Some(&addr))
        .await
        .unwrap();
    assert_eq!(
        (i.code.as_str(), i.max_age, i.max_uses),
        ("abc123", 3600, 5)
    );
    assert_eq!(i.url, "https://discord.gg/abc123");
    let l = leer_invitaciones(&state, "555", Some(&addr)).await.unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].uses, 2);
    assert_eq!(l[0].channel_name.as_deref(), Some("general"));
    borrar_invitacion(&state, "555", "abc123", Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn invitaciones_validan_y_exigen_permiso() {
    let server = MockServer::start().await;
    mock_permisos(&server, CREAR_INVITACIONES).await;
    prohibir_escrituras(&server).await;
    let state = estado("token-i2");
    let addr = server.address().to_string();
    assert!(
        crear_invitacion(&state, "555", "10", 999_999, 0, Some(&addr))
            .await
            .is_err()
    );
    assert!(crear_invitacion(&state, "555", "10", 0, 101, Some(&addr))
        .await
        .is_err());
    let m = "Al bot le falta el permiso «Gestionar servidor»";
    assert_eq!(
        leer_invitaciones(&state, "555", Some(&addr))
            .await
            .unwrap_err(),
        m
    );
    assert_eq!(
        borrar_invitacion(&state, "555", "abc123", Some(&addr))
            .await
            .unwrap_err(),
        m
    );
}

#[tokio::test]
async fn crea_lista_y_borra_webhooks() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_WEBHOOKS).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/10/webhooks"))
        .and(body_partial_json(serde_json::json!({"name": "Avisos"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "900", "type": 1, "name": "Avisos", "channel_id": "10",
            "guild_id": "555", "token": "tok"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/webhooks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "900", "name": "Avisos", "channel_id": "10"}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/webhooks/900"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-w1");
    let addr = server.address().to_string();
    let w = crear_webhook(&state, "555", "10", "Avisos", Some(&addr))
        .await
        .unwrap();
    assert_eq!(w.id, "900");
    assert_eq!(
        w.url.as_deref(),
        Some("https://discord.com/api/webhooks/900/tok")
    );
    let l = leer_webhooks(&state, "555", Some(&addr)).await.unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].url, None);
    borrar_webhook(&state, "555", "900", Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn webhooks_sin_permiso_o_nombre_invalido() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_SERVIDOR).await;
    prohibir_escrituras(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/webhooks"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let state = estado("token-w2");
    let addr = server.address().to_string();
    let m = "Al bot le falta el permiso «Gestionar webhooks»";
    assert_eq!(
        leer_webhooks(&state, "555", Some(&addr)).await.unwrap_err(),
        m
    );
    assert_eq!(
        crear_webhook(&state, "555", "10", "x", Some(&addr))
            .await
            .unwrap_err(),
        m
    );
    assert_eq!(
        borrar_webhook(&state, "555", "900", Some(&addr))
            .await
            .unwrap_err(),
        m
    );
    // Con permiso pero nombre vacío: se rechaza sin llamar a Discord.
    let server2 = MockServer::start().await;
    mock_permisos(&server2, GESTIONAR_WEBHOOKS).await;
    prohibir_escrituras(&server2).await;
    let addr2 = server2.address().to_string();
    assert!(crear_webhook(&state, "555", "10", "  ", Some(&addr2))
        .await
        .is_err());
}
