//! Tests de los overwrites de canal contra el Discord falso (wiremock).

use chrono::Utc;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::commands::{fijar_permiso_canal, quitar_permiso_canal};
use crate::crypto;
use crate::state::{AppState, BotConfig, Guild, ServerMap};

const GESTIONAR_ROLES: u64 = 1 << 28;
const ADMINISTRADOR: u64 = 1 << 3;
const SIN_PERMISO: &str = "The bot is missing the permission «Gestionar roles»";
const JERARQUIA: &str = "That role is above the bot's role: move it up in Server settings → Roles";

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
    state.set_map(
        "555".to_string(),
        ServerMap {
            guild: Guild {
                id: "555".to_string(),
                name: "S".to_string(),
                icon: None,
                member_count: 0,
                joined_at: None,
            },
            channels: Vec::new(),
            members: Vec::new(),
            roles: Vec::new(),
            categories: std::collections::HashMap::new(),
            last_updated: String::new(),
        },
    );
    state
}

fn rol_json(id: &str, nombre: &str, posicion: u32, bits: u64) -> serde_json::Value {
    serde_json::json!({
        "id": id, "name": nombre, "color": 0, "hoist": false, "flags": 0,
        "position": posicion, "permissions": bits.to_string(), "managed": false,
        "mentionable": false
    })
}

/// Bot con rol "600" (posición 5); "700" por debajo, "900" por encima.
/// El canal "10" devuelve los `overwrites` indicados.
async fn mock_servidor(server: &MockServer, bits: u64, overwrites: serde_json::Value) {
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
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/channels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": "10", "type": 0, "name": "general", "position": 0,
             "permission_overwrites": overwrites}
        ])))
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
async fn fija_overwrite_de_rol_y_el_mapa_lo_refleja() {
    let server = MockServer::start().await;
    mock_servidor(
        &server,
        GESTIONAR_ROLES,
        serde_json::json!([{"id": "700", "type": 0, "allow": "1024", "deny": "2048"}]),
    )
    .await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/channels/10/permissions/700"))
        .and(body_partial_json(
            serde_json::json!({"allow": "1024", "deny": "2048", "type": 0}),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-ow1");
    let addr = server.address().to_string();
    fijar_permiso_canal(&state, "555", "10", "700", 0, "1024", "2048", Some(&addr))
        .await
        .unwrap();
    let mapa = state.get_map("555").unwrap();
    let ow = mapa.channels[0].overwrites.as_ref().unwrap();
    assert_eq!(ow.len(), 1);
    assert_eq!(
        (
            ow[0].id.as_str(),
            ow[0].type_,
            ow[0].allow.as_str(),
            ow[0].deny.as_str()
        ),
        ("700", 0, "1024", "2048")
    );
}

#[tokio::test]
async fn fija_overwrite_de_miembro_sin_exigir_jerarquia() {
    let server = MockServer::start().await;
    mock_servidor(
        &server,
        GESTIONAR_ROLES,
        serde_json::json!([{"id": "4242", "type": 1, "allow": "0", "deny": "2048"}]),
    )
    .await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/channels/10/permissions/4242"))
        .and(body_partial_json(
            serde_json::json!({"type": 1, "deny": "2048"}),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-ow2");
    let addr = server.address().to_string();
    fijar_permiso_canal(&state, "555", "10", "4242", 1, "0", "2048", Some(&addr))
        .await
        .unwrap();
    let mapa = state.get_map("555").unwrap();
    assert_eq!(mapa.channels[0].overwrites.as_ref().unwrap()[0].type_, 1);
}

#[tokio::test]
async fn administrador_basta_y_borra_overwrite() {
    let server = MockServer::start().await;
    mock_servidor(&server, ADMINISTRADOR, serde_json::json!([])).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/channels/10/permissions/700"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/channels/10/permissions/4242"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-del");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    quitar_permiso_canal(&state, "555", "10", "700", 0, b)
        .await
        .unwrap();
    quitar_permiso_canal(&state, "555", "10", "4242", 1, b)
        .await
        .unwrap();
    let mapa = state.get_map("555").unwrap();
    assert!(mapa.channels[0].overwrites.as_ref().unwrap().is_empty());
}

#[tokio::test]
async fn overwrite_de_rol_alto_da_error_de_jerarquia() {
    let server = MockServer::start().await;
    mock_servidor(&server, ADMINISTRADOR, serde_json::json!([])).await;
    prohibir_cambios(&server).await;
    let state = estado("token-jq");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    for rol in ["600", "900"] {
        assert_eq!(
            fijar_permiso_canal(&state, "555", "10", rol, 0, "1024", "0", b)
                .await
                .unwrap_err(),
            JERARQUIA
        );
        assert_eq!(
            quitar_permiso_canal(&state, "555", "10", rol, 0, b)
                .await
                .unwrap_err(),
            JERARQUIA
        );
    }
}

#[tokio::test]
async fn sin_gestionar_roles_no_llama_a_discord() {
    let server = MockServer::start().await;
    mock_servidor(&server, 1 << 10, serde_json::json!([])).await;
    prohibir_cambios(&server).await;
    let state = estado("token-np");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    assert_eq!(
        fijar_permiso_canal(&state, "555", "10", "700", 0, "1", "0", b)
            .await
            .unwrap_err(),
        SIN_PERMISO
    );
    assert_eq!(
        quitar_permiso_canal(&state, "555", "10", "4242", 1, b)
            .await
            .unwrap_err(),
        SIN_PERMISO
    );
}

#[tokio::test]
async fn datos_invalidos_se_rechazan_sin_llamar_a_discord() {
    let server = MockServer::start().await;
    mock_servidor(&server, ADMINISTRADOR, serde_json::json!([])).await;
    prohibir_cambios(&server).await;
    let state = estado("token-val");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    // Mismo bit permitido y denegado, bits no numéricos y tipo desconocido.
    assert!(
        fijar_permiso_canal(&state, "555", "10", "4242", 1, "1024", "1024", b)
            .await
            .is_err()
    );
    assert!(
        fijar_permiso_canal(&state, "555", "10", "4242", 1, "abc", "0", b)
            .await
            .is_err()
    );
    assert!(
        fijar_permiso_canal(&state, "555", "10", "4242", 2, "1", "0", b)
            .await
            .is_err()
    );
}
