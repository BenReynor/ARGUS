use super::*;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
fn fixture() -> snapshots::ServerSnapshot {
    serde_json::from_value(json!({"version":1,"guild_id":"555","name":"Snapshot","created_at":"2026-01-01T00:00:00Z","guild":{"id":"555","name":"Fixture","owner_id":"1"},"roles":[{"id":"555","position":0,"permissions":"0"},{"id":"10","position":10,"permissions":"2"},{"id":"20","position":2,"permissions":"8"},{"id":"30","position":11,"permissions":"0"}],"members":[{"id":"1","name":"Owner","roles":[]},{"id":"2","name":"Bot","roles":[{"id":"10"}]},{"id":"3","name":"Actor","roles":[]},{"id":"4","name":"Admin","roles":[{"id":"20"}]},{"id":"5","name":"Above","roles":[{"id":"30"}]},{"id":"6","name":"Member","roles":[]}],"channels":[],"emojis":[],"stickers":[],"webhooks":[],"bans":[]})).unwrap()
}
#[test]
fn exclusions_owner_bot_actor_admin_and_hierarchy() {
    let p = plan(&fixture(), "2", "3", None).unwrap();
    assert_eq!(p["targets"], json!([{"id":"6","name":"Member"}]));
    assert_eq!(p["skipped"][0]["id"], "5");
    assert_eq!(
        plan(&fixture(), "2", "3", Some(&[])).unwrap()["targets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(plan(&fixture(), "2", "2", None).is_err());
    assert!(plan(&fixture(), "2", "999", None).is_err());
    assert!(
        plan(&fixture(), "2", "3", Some(&["555".into()])).unwrap()["targets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn fake_discord_stop_after_first_request() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/guilds/555/members/6"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/guilds/555/members/7"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/api/v10/guilds/555/members/6")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"user":{"id":"6","username":"Fixture","discriminator":"0","avatar":null},"roles":[],"joined_at":"2026-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0}))).mount(&server).await;
    let stop = AtomicBool::new(false);
    let mut results = vec![];
    execute(
        "fake-token",
        Some(&server.address().to_string()),
        "555",
        &[json!({"id":"6"}), json!({"id":"7"})],
        &stop,
        &[],
        |v| {
            results.push(v);
            stop.store(true, Ordering::SeqCst);
        },
    )
    .await
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["done"], 1);
    assert_eq!(results[0]["ok"], true);
}
#[tokio::test]
async fn fake_discord_already_stopped_makes_no_request() {
    let server = MockServer::start().await;
    execute(
        "fake-token",
        Some(&server.address().to_string()),
        "555",
        &[json!({"id":"6"})],
        &AtomicBool::new(true),
        &[],
        |_| panic!("must not advance"),
    )
    .await
    .unwrap();
    assert!(server.received_requests().await.unwrap().is_empty());
}
#[test]
fn app_only_not_registered_with_mcp_orders_or_ai() {
    // Contrato: el módulo privado solo tiene un punto de entrada en Tauri.
    for source in [
        include_str!("../mcp.rs"),
        include_str!("../chat.rs"),
        include_str!("../asistente.rs"),
        include_str!("../propuesta.rs"),
        include_str!("../responder.rs"),
    ] {
        assert!(!source
            .split("#[cfg(test)]")
            .next()
            .unwrap()
            .contains("app_mass_kick"));
        assert!(!source.contains("mass_kick::"));
    }
    assert!(include_str!("../lib.rs").contains("mass_kick::app_mass_kick"));
}

#[test]
fn mcp_ai_and_discord_validation_deny_bulk_removal() {
    let name = "app_mass_kick";
    assert!(!crate::mcp::HERRAMIENTAS_LECTURA.contains(&name));
    assert!(!crate::mcp::HERRAMIENTAS_ESCRITURA.contains(&name));
    assert!(crate::mcp::validar_accion(name, &json!({})).is_err());
    let proposal = crate::propuesta::Propuesta {
        resumen: String::new(),
        acciones: vec![crate::propuesta::Accion {
            herramienta: name.into(),
            args: json!({}),
            descripcion: String::new(),
        }],
    };
    let permissions = crate::db::PermisosOrdenes {
        estructura: true,
        borrar_canales_roles: true,
        silenciar: true,
        expulsar: true,
        banear: true,
        borrar_mensajes: true,
        avisos: true,
    };
    assert!(proposal.validar_desde_discord(&permissions).is_err());
    assert!(
        !crate::propuesta::validar_acciones(&proposal, &crate::propuesta::Nombres::default())
            .errores
            .is_empty()
    );
}

#[tokio::test]
async fn fake_discord_new_excluded_role_is_not_kicked() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/api/v10/guilds/555/members/6")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"user":{"id":"6","username":"Fixture","discriminator":"0","avatar":null},"roles":["20"],"joined_at":"2026-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0}))).mount(&server).await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;
    let mut results = vec![];
    execute(
        "fake-token",
        Some(&server.address().to_string()),
        "555",
        &[json!({"id":"6"})],
        &AtomicBool::new(false),
        &["20".into()],
        |v| results.push(v),
    )
    .await
    .unwrap();
    assert_eq!(results[0]["ok"], false);
    assert_eq!(results[0]["error"], "Member now has an excluded role");
}

async fn capture_fixture(server: &MockServer) {
    let s = fixture();
    let mut roles = s.roles.clone();
    for r in &mut roles {
        r["name"] = json!(format!("Role {}", r["id"]));
    }
    let members: Vec<Value> = s.members.iter().map(|m| json!({"user":{"id":m["id"],"username":m["name"]},"roles":m["roles"].as_array().unwrap().iter().map(|r| r["id"].clone()).collect::<Vec<_>>()})).collect();
    for (suffix, body) in [
        ("", s.guild),
        ("/roles", json!(roles)),
        ("/members", json!(members)),
        ("/channels", json!([])),
        ("/emojis", json!([])),
        ("/stickers", json!([])),
        ("/webhooks", json!([])),
        ("/bans", json!([])),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v10/guilds/555{suffix}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(server)
        .await;
}
#[tokio::test]
async fn fake_discord_exclusions_and_automatic_snapshot_barrier() {
    let server = MockServer::start().await;
    capture_fixture(&server).await;
    let base = server.address().to_string();
    let s = snapshots::capture("fake-token", Some(&base), "555")
        .await
        .unwrap();
    let preview = plan(&s, "2", "3", None).unwrap();
    assert_eq!(preview["targets"], json!([{"id":"6","name":"Member"}]));
    assert_eq!(preview["skipped"][0]["id"], "5");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::db::run_migrations(&pool).await.unwrap();
    let id = automatic_snapshot(
        &pool,
        "fake-token",
        Some(&base),
        "2",
        "555",
        "3",
        &["20".into()],
        &preview,
    )
    .await
    .unwrap();
    let automatic: bool = sqlx::query_scalar("SELECT automatic FROM server_snapshots WHERE id=?")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(automatic);
    let saved = snapshots::load(&pool, "2", "555", &id).await.unwrap();
    assert_eq!(saved.members.len(), 6);
    let mut changed = preview.clone();
    changed["guild_name"] = json!("Wrong confirmation");
    assert!(automatic_snapshot(
        &pool,
        "fake-token",
        Some(&base),
        "2",
        "555",
        "3",
        &["20".into()],
        &changed
    )
    .await
    .is_err());
    sqlx::query("DROP TABLE server_snapshots")
        .execute(&pool)
        .await
        .unwrap();
    assert!(automatic_snapshot(
        &pool,
        "fake-token",
        Some(&base),
        "2",
        "555",
        "3",
        &["20".into()],
        &preview
    )
    .await
    .is_err());
}

#[test]
fn start_requires_review_key_exact_name_count_and_ready_state() {
    let mut j = Job {
        key: "review-key".into(),
        actor: "3".into(),
        excluded: vec![],
        preview: plan(&fixture(), "2", "3", None).unwrap(),
        status: json!({"phase":"ready"}),
        stop: Arc::new(AtomicBool::new(false)),
    };
    let request = || Request {
        op: "start".into(),
        actor: None,
        identity_key: None,
        identity_code: None,
        excluded_roles: None,
        key: Some("review-key".into()),
        confirmation: Some("Fixture".into()),
        count: Some(1),
    };
    assert!(validate_start(&j, &request(), "3").is_ok());
    assert!(validate_start(&j, &request(), "6").is_err());
    let mut r = request();
    r.count = None;
    assert!(validate_start(&j, &r, "3").is_err());
    let mut r = request();
    r.count = Some(2);
    assert!(validate_start(&j, &r, "3").is_err());
    let mut r = request();
    r.confirmation = Some("fixture".into());
    assert!(validate_start(&j, &r, "3").is_err());
    let mut r = request();
    r.key = None;
    assert!(validate_start(&j, &r, "3").is_err());
    j.status["phase"] = json!("running");
    assert!(validate_start(&j, &request(), "3").is_err());
}

#[tokio::test]
async fn fake_discord_mcp_dispatch_refuses_app_action_without_network() {
    let server = MockServer::start().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::db::run_migrations(&pool).await.unwrap();
    let mcp = crate::mcp::ArgusMcp::nuevo(pool, Some(server.address().to_string()))
        .await
        .unwrap();
    let error = mcp
        .ejecutar_accion("app_mass_kick", json!({"op":"start","guild_id":"555"}))
        .await
        .unwrap_err();
    assert!(error.contains("Unknown action"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

fn identity_request(key: &str, code: &str) -> Request {
    Request {
        op: "prepare".into(),
        actor: None,
        identity_key: Some(key.into()),
        identity_code: Some(code.into()),
        excluded_roles: None,
        key: Some(key.into()),
        confirmation: Some(code.into()),
        count: None,
    }
}

async fn mock_identity_member(server: &MockServer, bot: bool) {
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/members/3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user":{"id":"3","username":"Actor","discriminator":"0","avatar":null,"bot":bot},
            "roles":[],"joined_at":"2026-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn fake_discord_identity_requires_private_code_and_protects_verified_actor() {
    let server = MockServer::start().await;
    mock_identity_member(&server, false).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/users/@me/channels"))
        .and(wiremock::matchers::body_partial_json(
            json!({"recipient_id":"3"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"42","type":1})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/42/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id":"90","channel_id":"42","content":"verification","timestamp":"2026-01-01T00:00:00+00:00",
            "edited_timestamp":null,"tts":false,"mention_everyone":false,"mentions":[],"mention_roles":[],
            "attachments":[],"embeds":[],"pinned":false,"type":0,
            "author":{"id":"2","username":"Bot","discriminator":"0","avatar":null}
        })))
        .expect(1).mount(&server).await;
    let identity = identify(
        "fake-token",
        Some(&server.address().to_string()),
        "2",
        "555",
        "3",
        "en",
    )
    .await
    .unwrap();
    let requests = server.received_requests().await.unwrap();
    let message: Value = serde_json::from_slice(
        &requests
            .iter()
            .find(|r| r.url.path().ends_with("/messages"))
            .unwrap()
            .body,
    )
    .unwrap();
    let code = message["content"]
        .as_str()
        .unwrap()
        .strip_prefix("ARGUS member removal verification code: ")
        .unwrap()
        .split('.')
        .next()
        .unwrap();
    let key = identity.key.clone();
    // La referencia que recibe la app no revela el secreto enviado por Discord.
    assert_ne!(key, code);
    let scope = ("2".into(), "555".into());
    let mut identities = HashMap::from([(scope.clone(), identity)]);
    let request = identity_request(&key, code);
    let actor = verified_actor(&identities, &scope, &request).unwrap();
    assert_eq!(actor, "3");
    assert!(
        !plan(&fixture(), "2", &actor, Some(&[])).unwrap()["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == actor)
    );
    // Ni un ID válido ni la referencia pública sustituyen la prueba privada.
    let mut spoof = identity_request(&key, code);
    spoof.actor = Some("6".into());
    assert!(verified_actor(&identities, &scope, &spoof).is_err());
    assert!(verified_actor(&identities, &scope, &identity_request(&key, "3")).is_err());
    assert!(verified_actor(&identities, &scope, &identity_request(&key, &key)).is_err());
    assert!(verified_actor(&identities, &scope, &identity_request("other-key", code)).is_err());
    assert!(verified_actor(&identities, &("2".into(), "556".into()), &request).is_err());
    assert!(verified_actor(&identities, &("8".into(), "555".into()), &request).is_err());
    identities.get_mut(&scope).unwrap().expires = Instant::now() - Duration::from_secs(1);
    assert!(verified_actor(&identities, &scope, &request).is_err());
    assert!(verified_actor(&HashMap::new(), &scope, &request).is_err());
}

#[tokio::test]
async fn fake_discord_failed_dm_or_bot_account_cannot_establish_identity() {
    let server = MockServer::start().await;
    mock_identity_member(&server, false).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/users/@me/channels"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50007,"message":"Cannot send messages to this user"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(identify(
        "fake-token",
        Some(&server.address().to_string()),
        "2",
        "555",
        "3",
        "en"
    )
    .await
    .is_err());
    let bot_server = MockServer::start().await;
    mock_identity_member(&bot_server, true).await;
    assert!(identify(
        "fake-token",
        Some(&bot_server.address().to_string()),
        "2",
        "555",
        "3",
        "en"
    )
    .await
    .is_err());
    assert!(bot_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method == "GET"));
}
