//! Tests de los comandos de roles y la jerarquía contra el Discord falso (wiremock).

use chrono::Utc;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::commands::{borrar_rol, crear_rol, editar_rol, reordenar_roles};
use crate::crypto;
use crate::discord::DatosRol;
use crate::state::{AppState, BotConfig, Guild, ServerMap};

const GESTIONAR_ROLES: u64 = 1 << 28;
const ADMINISTRADOR: u64 = 1 << 3;
const SIN_PERMISO: &str = "Al bot le falta el permiso «Gestionar roles»";
const JERARQUIA: &str =
    "Ese rol está por encima del rol del bot: súbelo en Ajustes del servidor → Roles";

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

/// Servidor con: @everyone (0), rol del bot "600" (posición 5, con `bits`),
/// "700" bajo el bot (2), "500" y "800" en la misma posición que el bot (5) y
/// "900" por encima (9). Con la misma posición Discord pone arriba el ID menor:
/// "500" está por encima del bot y "800" por debajo.
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
            rol_json("500", "IgualAntiguo", 5, 0),
            rol_json("800", "IgualNuevo", 5, 0),
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
}

/// Ninguna petición de cambio debe llegar al Discord falso.
async fn prohibir_cambios(server: &MockServer) {
    for metodo in ["POST", "PATCH", "DELETE"] {
        Mock::given(method(metodo))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(server)
            .await;
    }
}

#[tokio::test]
async fn crea_rol_con_todos_los_campos_y_actualiza_el_mapa() {
    let server = MockServer::start().await;
    mock_servidor(&server, GESTIONAR_ROLES).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/guilds/555/roles"))
        .and(body_partial_json(serde_json::json!({
            "name": "Mods", "color": 255, "hoist": true, "mentionable": true,
            "permissions": "8"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(rol_json("1000", "Mods", 1, 8)))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-cr");
    let addr = server.address().to_string();
    let datos = DatosRol {
        name: Some("Mods".into()),
        color: Some(255),
        hoist: Some(true),
        mentionable: Some(true),
        permissions: Some("8".into()),
    };
    let rol = crear_rol(&state, "555", &datos, Some(&addr)).await.unwrap();
    assert_eq!((rol.id.as_str(), rol.permissions.as_str()), ("1000", "8"));
    assert_eq!(state.get_map("555").unwrap().roles.len(), 6);
}

#[tokio::test]
async fn edita_rol_por_debajo_del_bot() {
    let server = MockServer::start().await;
    mock_servidor(&server, GESTIONAR_ROLES).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/555/roles/700"))
        .and(body_partial_json(serde_json::json!({
            "name": "Nuevo", "color": 16, "hoist": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(rol_json("700", "Nuevo", 2, 0)))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-up");
    let addr = server.address().to_string();
    let datos = DatosRol {
        name: Some("Nuevo".into()),
        color: Some(16),
        hoist: Some(true),
        ..Default::default()
    };
    let rol = editar_rol(&state, "555", "700", &datos, Some(&addr))
        .await
        .unwrap();
    assert_eq!(rol.name, "Nuevo");
}

#[tokio::test]
async fn borra_rol_por_debajo_del_bot() {
    let server = MockServer::start().await;
    mock_servidor(&server, GESTIONAR_ROLES).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/guilds/555/roles/700"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-del");
    let addr = server.address().to_string();
    borrar_rol(&state, "555", "700", Some(&addr)).await.unwrap();
}

#[tokio::test]
async fn rol_en_la_misma_posicion_pero_mas_nuevo_queda_bajo_el_bot() {
    // Pasa al crear un rol: Discord le da la misma posición que el del bot.
    let server = MockServer::start().await;
    mock_servidor(&server, GESTIONAR_ROLES).await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/guilds/555/roles/800"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-empate");
    let addr = server.address().to_string();
    borrar_rol(&state, "555", "800", Some(&addr)).await.unwrap();
}

#[tokio::test]
async fn reordena_roles_bajo_el_bot() {
    let server = MockServer::start().await;
    mock_servidor(&server, GESTIONAR_ROLES).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/555/roles"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([rol_json("700", "Bajo", 3, 0)])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let state = estado("token-re");
    let addr = server.address().to_string();
    let pos = vec![("700".to_string(), 3)];
    reordenar_roles(&state, "555", &pos, Some(&addr))
        .await
        .unwrap();
    let peticiones = server.received_requests().await.unwrap();
    let patch = peticiones
        .iter()
        .find(|p| p.method.as_str() == "PATCH")
        .unwrap();
    let cuerpo: serde_json::Value = serde_json::from_slice(&patch.body).unwrap();
    assert_eq!(cuerpo[0]["id"], "700");
    assert_eq!(cuerpo[0]["position"], 3);
}

#[tokio::test]
async fn rol_igual_o_superior_al_del_bot_da_error_de_jerarquia() {
    let server = MockServer::start().await;
    // Administrador no salta la jerarquía: solo el dueño, y actúa el bot.
    mock_servidor(&server, ADMINISTRADOR).await;
    prohibir_cambios(&server).await;
    let state = estado("token-jq");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    let datos = DatosRol {
        name: Some("x".into()),
        ..Default::default()
    };
    for rol in ["600", "500", "900"] {
        assert_eq!(
            editar_rol(&state, "555", rol, &datos, b).await.unwrap_err(),
            JERARQUIA,
            "editar {rol}"
        );
        assert_eq!(
            borrar_rol(&state, "555", rol, b).await.unwrap_err(),
            JERARQUIA,
            "borrar {rol}"
        );
    }
    // Mover un rol permitido a una posición igual o mayor que la del bot.
    let pos = vec![("700".to_string(), 5)];
    assert_eq!(
        reordenar_roles(&state, "555", &pos, b).await.unwrap_err(),
        JERARQUIA
    );
    // Mover un rol que ya está por encima.
    let pos = vec![("900".to_string(), 1)];
    assert_eq!(
        reordenar_roles(&state, "555", &pos, b).await.unwrap_err(),
        JERARQUIA
    );
}

#[tokio::test]
async fn sin_gestionar_roles_no_llama_a_discord() {
    let server = MockServer::start().await;
    mock_servidor(&server, 1 << 10).await;
    prohibir_cambios(&server).await;
    let state = estado("token-np");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    let datos = DatosRol {
        name: Some("x".into()),
        ..Default::default()
    };
    assert_eq!(
        crear_rol(&state, "555", &datos, b).await.unwrap_err(),
        SIN_PERMISO
    );
    assert_eq!(
        editar_rol(&state, "555", "700", &datos, b)
            .await
            .unwrap_err(),
        SIN_PERMISO
    );
    assert_eq!(
        borrar_rol(&state, "555", "700", b).await.unwrap_err(),
        SIN_PERMISO
    );
    let pos = vec![("700".to_string(), 1)];
    assert_eq!(
        reordenar_roles(&state, "555", &pos, b).await.unwrap_err(),
        SIN_PERMISO
    );
}

#[tokio::test]
async fn datos_invalidos_se_rechazan_sin_llamar_a_discord() {
    let server = MockServer::start().await;
    mock_servidor(&server, ADMINISTRADOR).await;
    prohibir_cambios(&server).await;
    let state = estado("token-val");
    let addr = server.address().to_string();
    let b = Some(addr.as_str());
    let sin_nombre = DatosRol {
        name: Some("  ".into()),
        ..Default::default()
    };
    let e = crear_rol(&state, "555", &sin_nombre, b).await.unwrap_err();
    assert!(e.contains("nombre"), "{e}");
    let permisos_malos = DatosRol {
        name: Some("a".into()),
        permissions: Some("abc".into()),
        ..Default::default()
    };
    let e = crear_rol(&state, "555", &permisos_malos, b)
        .await
        .unwrap_err();
    assert!(e.contains("permisos"), "{e}");
}
