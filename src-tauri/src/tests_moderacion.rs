//! Tests de moderación de miembros contra el Discord falso (wiremock).

use chrono::Utc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::commands::{
    banear_miembro, cambiar_apodo, expulsar_miembro, listar_bans, quitar_ban, silenciar_miembro,
};
use crate::crypto;
use crate::state::{AppState, BotConfig};

const KICK: u64 = 1 << 1;
const BAN: u64 = 1 << 2;
const APODOS: u64 = 1 << 27;
const DUENO: &str = "No se puede hacer eso al dueño del servidor";
const JERARQUIA: &str = "Ese miembro tiene un rol igual o superior al del bot: sube el rol del bot en Ajustes del servidor → Roles";

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

fn rol_json(id: &str, nombre: &str, posicion: u32, bits: u64) -> serde_json::Value {
    serde_json::json!({
        "id": id, "name": nombre, "color": 0, "hoist": false, "flags": 0,
        "position": posicion, "permissions": bits.to_string(), "managed": false,
        "mentionable": false
    })
}

fn miembro_json(id: &str, roles: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "user": {"id": id, "username": format!("u{id}"), "discriminator": "0",
                 "avatar": null, "bot": id == "123456789"},
        "nick": null, "roles": roles,
        "joined_at": "2024-01-02T03:04:05.000000+00:00",
        "deaf": false, "mute": false, "flags": 0
    })
}

/// Dueño "9"; bot con rol "600" (pos. 5); "1001" con rol bajo, "1002" con rol
/// alto, "1003" con el mismo rol que el bot; "1004" no está en el servidor.
async fn mock_servidor(server: &MockServer, bits: u64) {
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "555", "name": "S", "owner_id": "9",
            "roles": [], "emojis": [], "features": [], "afk_timeout": 300,
            "default_message_notifications": 0, "explicit_content_filter": 0,
            "mfa_level": 0, "nsfw_level": 0, "premium_tier": 0,
            "system_channel_flags": 0, "verification_level": 0
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/roles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            rol_json("555", "@everyone", 0, 0),
            rol_json("600", "Bot", 5, bits),
            rol_json("700", "Bajo", 2, 0),
            rol_json("900", "Alto", 9, 0),
        ])))
        .mount(server)
        .await;
    for (id, roles) in [
        ("123456789", vec!["600"]),
        ("9", vec![]),
        ("1001", vec!["700"]),
        ("1002", vec!["900"]),
        ("1003", vec!["600"]),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v10/guilds/555/members/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(miembro_json(id, &roles)))
            .mount(server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/members/1004"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"code": 10007, "message": "Unknown Member"})),
        )
        .mount(server)
        .await;
}

async fn prohibir_cambios(server: &MockServer) {
    for metodo in ["POST", "PATCH", "PUT", "DELETE"] {
        Mock::given(method(metodo))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(server)
            .await;
    }
}

#[tokio::test]
async fn expulsa_con_motivo_en_la_auditoria() {
    let server = MockServer::start().await;
    mock_servidor(&server, KICK).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/guilds/555/members/1001"))
        .and(header("x-audit-log-reason", "spam"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-mod1");
    let addr = server.address().to_string();
    expulsar_miembro(&state, "555", "1001", Some("spam"), Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn banea_con_motivo_y_dias_de_mensajes() {
    let server = MockServer::start().await;
    mock_servidor(&server, BAN).await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/guilds/555/bans/1001"))
        .and(header("x-audit-log-reason", "insultos"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"delete_message_seconds": 3 * 86400}),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-mod2");
    let addr = server.address().to_string();
    banear_miembro(&state, "555", "1001", Some("insultos"), 3, Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn banea_por_id_a_alguien_que_no_esta_en_el_servidor() {
    let server = MockServer::start().await;
    mock_servidor(&server, BAN).await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/guilds/555/bans/1004"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-mod3");
    let addr = server.address().to_string();
    banear_miembro(&state, "555", "1004", None, 0, Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn dias_de_mensajes_fuera_de_rango_no_llaman_a_discord() {
    let server = MockServer::start().await;
    prohibir_cambios(&server).await;
    let state = estado("token-mod4");
    let addr = server.address().to_string();
    let e = banear_miembro(&state, "555", "1001", None, 8, Some(&addr))
        .await
        .unwrap_err();
    assert_eq!(e, "Los días de mensajes a borrar deben estar entre 0 y 7");
}

#[tokio::test]
async fn quita_ban_con_motivo_y_lista_bans() {
    let server = MockServer::start().await;
    mock_servidor(&server, BAN).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/guilds/555/bans/1004"))
        .and(header("x-audit-log-reason", "perdonado"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/bans"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"reason": "spam", "user": {"id": "1004", "username": "malo",
             "discriminator": "0", "avatar": null}}
        ])))
        .mount(&server)
        .await;
    let state = estado("token-mod5");
    let addr = server.address().to_string();
    let bans = listar_bans(&state, "555", Some(&addr)).await.unwrap();
    assert_eq!(bans.len(), 1);
    assert_eq!(
        (bans[0].user_id.as_str(), bans[0].username.as_str()),
        ("1004", "malo")
    );
    assert_eq!(bans[0].reason.as_deref(), Some("spam"));
    quitar_ban(&state, "555", "1004", Some("perdonado"), Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn cambia_y_borra_apodo_con_motivo() {
    let server = MockServer::start().await;
    mock_servidor(&server, APODOS).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/555/members/1001"))
        .and(header("x-audit-log-reason", "normas"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"nick": "Nuevo"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(miembro_json("1001", &["700"])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/555/members/1001"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"nick": null}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(miembro_json("1001", &["700"])))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-mod6");
    let addr = server.address().to_string();
    cambiar_apodo(
        &state,
        "555",
        "1001",
        Some("Nuevo"),
        Some("normas"),
        Some(&addr),
    )
    .await
    .unwrap();
    cambiar_apodo(&state, "555", "1001", None, None, Some(&addr))
        .await
        .unwrap();
}

#[tokio::test]
async fn rechaza_al_dueno_en_todas_las_acciones() {
    let server = MockServer::start().await;
    mock_servidor(&server, KICK | BAN | APODOS).await;
    prohibir_cambios(&server).await;
    let state = estado("token-mod7");
    let addr = server.address().to_string();
    let a = Some(addr.as_str());
    assert_eq!(
        expulsar_miembro(&state, "555", "9", None, a)
            .await
            .unwrap_err(),
        DUENO
    );
    assert_eq!(
        banear_miembro(&state, "555", "9", None, 0, a)
            .await
            .unwrap_err(),
        DUENO
    );
    assert_eq!(
        cambiar_apodo(&state, "555", "9", Some("x"), None, a)
            .await
            .unwrap_err(),
        DUENO
    );
}

#[tokio::test]
async fn rechaza_rol_igual_o_superior_al_del_bot() {
    let server = MockServer::start().await;
    mock_servidor(&server, KICK | BAN | APODOS).await;
    prohibir_cambios(&server).await;
    let state = estado("token-mod8");
    let addr = server.address().to_string();
    let a = Some(addr.as_str());
    for objetivo in ["1002", "1003"] {
        assert_eq!(
            expulsar_miembro(&state, "555", objetivo, None, a)
                .await
                .unwrap_err(),
            JERARQUIA
        );
        assert_eq!(
            banear_miembro(&state, "555", objetivo, None, 0, a)
                .await
                .unwrap_err(),
            JERARQUIA
        );
        assert_eq!(
            cambiar_apodo(&state, "555", objetivo, Some("x"), None, a)
                .await
                .unwrap_err(),
            JERARQUIA
        );
    }
}

#[tokio::test]
async fn sin_permiso_no_llama_a_discord() {
    let server = MockServer::start().await;
    mock_servidor(&server, 0).await;
    prohibir_cambios(&server).await;
    let state = estado("token-mod9");
    let addr = server.address().to_string();
    let a = Some(addr.as_str());
    assert_eq!(
        expulsar_miembro(&state, "555", "1001", None, a)
            .await
            .unwrap_err(),
        "Al bot le falta el permiso «Expulsar miembros»"
    );
    assert_eq!(
        banear_miembro(&state, "555", "1001", None, 0, a)
            .await
            .unwrap_err(),
        "Al bot le falta el permiso «Banear miembros»"
    );
    assert_eq!(
        quitar_ban(&state, "555", "1001", None, a)
            .await
            .unwrap_err(),
        "Al bot le falta el permiso «Banear miembros»"
    );
    assert_eq!(
        listar_bans(&state, "555", a).await.unwrap_err(),
        "Al bot le falta el permiso «Banear miembros»"
    );
    assert_eq!(
        cambiar_apodo(&state, "555", "1001", Some("x"), None, a)
            .await
            .unwrap_err(),
        "Al bot le falta el permiso «Gestionar apodos»"
    );
}

#[tokio::test]
async fn apodo_demasiado_largo_no_llama_a_discord() {
    let server = MockServer::start().await;
    mock_servidor(&server, APODOS).await;
    prohibir_cambios(&server).await;
    let state = estado("token-mod10");
    let addr = server.address().to_string();
    let e = cambiar_apodo(
        &state,
        "555",
        "1001",
        Some(&"a".repeat(33)),
        None,
        Some(&addr),
    )
    .await
    .unwrap_err();
    assert_eq!(e, "El apodo no puede superar los 32 caracteres");
}

const MODERAR: u64 = 1 << 40;

#[tokio::test]
async fn silencia_y_quita_timeout_con_motivo() {
    let server = MockServer::start().await;
    mock_servidor(&server, MODERAR).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/555/members/1001"))
        .and(header("x-audit-log-reason", "calma"))
        .respond_with(ResponseTemplate::new(200).set_body_json(miembro_json("1001", &["700"])))
        .expect(2)
        .mount(&server)
        .await;
    let state = estado("token-mod11");
    let addr = server.address().to_string();
    let r = silenciar_miembro(&state, "555", "1001", 10, Some("calma"), Some(&addr))
        .await
        .unwrap();
    assert!(r.contains("silenciado"), "{r}");
    let r = silenciar_miembro(&state, "555", "1001", 0, Some("calma"), Some(&addr))
        .await
        .unwrap();
    assert!(r.contains("quitado"), "{r}");
    let reqs: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "PATCH")
        .collect();
    let b1: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
    assert!(b1["communication_disabled_until"].is_string(), "{b1}");
    let b2: serde_json::Value = serde_json::from_slice(&reqs[1].body).unwrap();
    assert!(b2["communication_disabled_until"].is_null(), "{b2}");
}

#[tokio::test]
async fn silencio_rechaza_dueno_jerarquia_permiso_y_exceso() {
    let server = MockServer::start().await;
    mock_servidor(&server, MODERAR).await;
    prohibir_cambios(&server).await;
    let state = estado("token-mod12");
    let addr = server.address().to_string();
    let a = Some(addr.as_str());
    assert_eq!(
        silenciar_miembro(&state, "555", "9", 5, None, a)
            .await
            .unwrap_err(),
        DUENO
    );
    assert_eq!(
        silenciar_miembro(&state, "555", "1002", 5, None, a)
            .await
            .unwrap_err(),
        JERARQUIA
    );
    assert!(silenciar_miembro(&state, "555", "1001", 40321, None, a)
        .await
        .unwrap_err()
        .contains("28 días"));

    let server = MockServer::start().await;
    mock_servidor(&server, 0).await;
    prohibir_cambios(&server).await;
    let addr = server.address().to_string();
    assert_eq!(
        silenciar_miembro(&state, "555", "1001", 5, None, Some(&addr))
            .await
            .unwrap_err(),
        "Al bot le falta el permiso «Moderar miembros»"
    );
}
