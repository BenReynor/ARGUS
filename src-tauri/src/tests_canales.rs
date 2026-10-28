//! Tests de los comandos de canales contra el Discord falso (wiremock).

use chrono::Utc;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::commands::{borrar_canal, crear_canal, editar_canal, reordenar_canales};
use crate::crypto;
use crate::discord::DatosCanal;
use crate::state::{AppState, BotConfig, Guild, ServerMap};

const GESTIONAR_CANALES: u64 = 1 << 4;
const ADMINISTRADOR: u64 = 1 << 3;
const ESPERADO_SIN_PERMISO: &str = "Al bot le falta el permiso «Gestionar canales»";

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

fn canal_json(id: &str, nombre: &str, tipo: u8, padre: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": id, "type": tipo, "guild_id": "555", "name": nombre,
        "position": 0, "parent_id": padre
    })
}

/// Simula los permisos del bot: un rol con los bits dados.
async fn mock_permisos(server: &MockServer, bits: u64) {
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
            {"id": "555", "name": "@everyone", "color": 0, "hoist": false, "flags": 0,
             "position": 0, "permissions": "0", "managed": false, "mentionable": false},
            {"id": "600", "name": "Bot", "color": 0, "hoist": false, "flags": 0,
             "position": 1, "permissions": bits.to_string(), "managed": false,
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

/// Lectura de canales que hace el refresco del mapa tras cada cambio.
async fn mock_lista(server: &MockServer, canales: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/channels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(canales))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn crea_canal_con_todos_los_campos_y_actualiza_el_mapa() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_CANALES).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/guilds/555/channels"))
        .and(body_json(serde_json::json!({
            "name": "novedades", "type": 5, "parent_id": "5",
            "topic": "Avisos", "nsfw": true, "rate_limit_per_user": 30
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(canal_json(
            "10",
            "novedades",
            5,
            Some("5"),
        )))
        .expect(1)
        .mount(&server)
        .await;
    mock_lista(
        &server,
        serde_json::json!([
            canal_json("5", "Info", 4, None),
            canal_json("10", "novedades", 5, Some("5"))
        ]),
    )
    .await;

    let state = estado("token-cc");
    let addr = server.address().to_string();
    let datos = DatosCanal {
        name: Some("novedades".into()),
        channel_type: Some(5),
        parent_id: Some("5".into()),
        topic: Some("Avisos".into()),
        nsfw: Some(true),
        rate_limit_per_user: Some(30),
    };
    let canal = crear_canal(&state, "555", &datos, Some(&addr))
        .await
        .unwrap();
    assert_eq!((canal.id.as_str(), canal.channel_type), ("10", 5));
    let mapa = state.get_map("555").unwrap();
    assert_eq!(mapa.channels.len(), 2);
    assert_eq!(mapa.categories["5"][0].name, "novedades");
}

#[tokio::test]
async fn crea_categoria_con_administrador() {
    let server = MockServer::start().await;
    mock_permisos(&server, ADMINISTRADOR).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/guilds/555/channels"))
        .and(body_json(serde_json::json!({"name": "Nueva", "type": 4})))
        .respond_with(ResponseTemplate::new(201).set_body_json(canal_json("7", "Nueva", 4, None)))
        .expect(1)
        .mount(&server)
        .await;
    mock_lista(
        &server,
        serde_json::json!([canal_json("7", "Nueva", 4, None)]),
    )
    .await;
    let state = estado("token-cat");
    let addr = server.address().to_string();
    let datos = DatosCanal {
        name: Some("Nueva".into()),
        channel_type: Some(4),
        ..Default::default()
    };
    let canal = crear_canal(&state, "555", &datos, Some(&addr))
        .await
        .unwrap();
    assert_eq!(canal.channel_type, 4);
}

#[tokio::test]
async fn edita_canal_y_lo_mueve_de_categoria() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_CANALES).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/channels/10"))
        .and(body_json(serde_json::json!({
            "name": "nuevo", "parent_id": "6", "topic": "t", "rate_limit_per_user": 5
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(canal_json(
            "10",
            "nuevo",
            0,
            Some("6"),
        )))
        .expect(1)
        .mount(&server)
        .await;
    mock_lista(
        &server,
        serde_json::json!([
            canal_json("6", "Otra", 4, None),
            canal_json("10", "nuevo", 0, Some("6"))
        ]),
    )
    .await;
    let state = estado("token-up");
    let addr = server.address().to_string();
    let datos = DatosCanal {
        name: Some("nuevo".into()),
        parent_id: Some("6".into()),
        topic: Some("t".into()),
        rate_limit_per_user: Some(5),
        ..Default::default()
    };
    let canal = editar_canal(&state, "555", "10", &datos, Some(&addr))
        .await
        .unwrap();
    assert_eq!(canal.parent_id.as_deref(), Some("6"));
    assert_eq!(state.get_map("555").unwrap().categories["6"].len(), 1);
}

#[tokio::test]
async fn borra_canal_y_lo_quita_del_mapa() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_CANALES).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/channels/10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(canal_json("10", "x", 0, None)))
        .expect(1)
        .mount(&server)
        .await;
    mock_lista(&server, serde_json::json!([])).await;
    let state = estado("token-del");
    let addr = server.address().to_string();
    borrar_canal(&state, "555", "10", Some(&addr))
        .await
        .unwrap();
    assert!(state.get_map("555").unwrap().channels.is_empty());
}

#[tokio::test]
async fn reordena_canales_y_actualiza_posiciones() {
    let server = MockServer::start().await;
    mock_permisos(&server, GESTIONAR_CANALES).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/555/channels"))
        .and(body_json(serde_json::json!([
            {"id": "10", "position": 1}, {"id": "11", "position": 0}
        ])))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let mut a = canal_json("10", "a", 0, None);
    a["position"] = 1.into();
    mock_lista(
        &server,
        serde_json::json!([a, canal_json("11", "b", 0, None)]),
    )
    .await;
    let state = estado("token-re");
    let addr = server.address().to_string();
    let posiciones = vec![("10".to_string(), 1), ("11".to_string(), 0)];
    reordenar_canales(&state, "555", &posiciones, Some(&addr))
        .await
        .unwrap();
    let mapa = state.get_map("555").unwrap();
    assert_eq!(
        mapa.channels
            .iter()
            .find(|c| c.id == "10")
            .unwrap()
            .position,
        1
    );
}

#[tokio::test]
async fn sin_gestionar_canales_no_llama_a_discord() {
    let server = MockServer::start().await;
    mock_permisos(&server, 1 << 10).await;
    // Ninguna acción sobre canales debe llegar al Discord falso.
    for metodo in ["POST", "PATCH", "DELETE"] {
        Mock::given(method(metodo))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
    }
    let state = estado("token-np");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    let datos = DatosCanal {
        name: Some("x".into()),
        ..Default::default()
    };
    assert_eq!(
        crear_canal(&state, "555", &datos, b).await.unwrap_err(),
        ESPERADO_SIN_PERMISO
    );
    assert_eq!(
        editar_canal(&state, "555", "10", &datos, b)
            .await
            .unwrap_err(),
        ESPERADO_SIN_PERMISO
    );
    assert_eq!(
        borrar_canal(&state, "555", "10", b).await.unwrap_err(),
        ESPERADO_SIN_PERMISO
    );
    let pos = vec![("10".to_string(), 0)];
    assert_eq!(
        reordenar_canales(&state, "555", &pos, b).await.unwrap_err(),
        ESPERADO_SIN_PERMISO
    );
}

#[tokio::test]
async fn datos_invalidos_se_rechazan_sin_llamar_a_discord() {
    let server = MockServer::start().await;
    mock_permisos(&server, ADMINISTRADOR).await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let state = estado("token-val");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    let sin_nombre = DatosCanal {
        name: Some("  ".into()),
        ..Default::default()
    };
    let e = crear_canal(&state, "555", &sin_nombre, b)
        .await
        .unwrap_err();
    assert!(e.contains("nombre"), "{e}");
    let lento = DatosCanal {
        name: Some("a".into()),
        rate_limit_per_user: Some(30000),
        ..Default::default()
    };
    let e = crear_canal(&state, "555", &lento, b).await.unwrap_err();
    assert!(e.contains("modo lento"), "{e}");
}
