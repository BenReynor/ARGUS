use super::*;
use serde::de::DeserializeSeed;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn event(kind: &str, data: Value) -> Event {
    let raw = json!({"op":0,"s":1,"t":kind,"d":data}).to_string();
    let seed = twilight_model::gateway::event::GatewayEventDeserializer::from_json(&raw).unwrap();
    seed.deserialize(&mut serde_json::Deserializer::from_str(&raw))
        .unwrap()
        .into()
}
fn channel(id: &str, parent: Option<&str>, kind: u8) -> Value {
    json!({"id":id,"guild_id":"555","name":format!("Channel {id}"),"type":kind,"parent_id":parent,"position":1,"permission_overwrites":[{"id":"7","type":0,"allow":"1024","deny":"2048"}]})
}
fn role(id: &str) -> Value {
    json!({"id":id,"name":format!("Role {id}"),"color":0,"hoist":false,"managed":false,"mentionable":false,"permissions":"1024","position":1,"flags":0})
}
fn map() -> ServerMap {
    let mut map: ServerMap = serde_json::from_value(json!({"guild":{"id":"555","name":"Fixture server","member_count":1,"icon":null,"joined_at":null},"channels":[],"roles":[],"members":[{"user_id":"9","username":"Fixture member","nickname":null,"roles":["7"],"bot":false,"joined_at":null,"avatar":null,"flags":null}],"categories":{},"last_updated":""})).unwrap();
    for r in ["555", "7", "8"] {
        crate::mapa::aplicar_evento(
            &mut map,
            &event(
                "GUILD_ROLE_CREATE",
                json!({"guild_id":"555","role":role(r)}),
            ),
        );
    }
    for (id, parent, kind) in [
        ("10", None, 4),
        ("11", Some("10"), 0),
        ("12", None, 0),
        ("13", None, 0),
        ("14", None, 0),
    ] {
        crate::mapa::aplicar_evento(
            &mut map,
            &event("CHANNEL_CREATE", channel(id, parent, kind)),
        );
    }
    map
}
fn delete(id: &str) -> Event {
    event("CHANNEL_DELETE", channel(id, None, 0))
}
fn step(
    detector: &mut Detector,
    map: &mut ServerMap,
    ev: Event,
    settings: &Settings,
    time: i64,
) -> Option<Burst> {
    let burst = detector.observe(map, &ev, settings, time);
    crate::mapa::aplicar_evento(map, &ev);
    burst
}
fn burst(time: i64, remove_roles: bool) -> Burst {
    let mut d = Detector::default();
    let mut m = map();
    let settings = Settings {
        remove_roles,
        ..Default::default()
    };
    for id in ["10", "11", "12", "13"] {
        assert!(step(&mut d, &mut m, delete(id), &settings, time).is_none());
    }
    let b = step(&mut d, &mut m, delete("14"), &settings, time).unwrap();
    assert!(m.channels.is_empty());
    b
}
#[test]
fn five_gateway_deletes_preserve_map_before_first_deletion() {
    let b = burst(1000, false);
    assert_eq!(b.before.channels.len(), 5);
    assert_eq!(
        b.before
            .channels
            .iter()
            .find(|c| c.id == "11")
            .unwrap()
            .parent_id
            .as_deref(),
        Some("10")
    );
    let s = from_map(&b.before);
    assert_eq!(s.channels[1]["permission_overwrites"][0]["allow"], "1024");
    assert_eq!(s.members[0]["roles"][0]["id"], "7");
    assert_eq!(b.deletions.len(), 5);
    assert!(!b.remove_roles);
}
#[test]
fn mixed_roles_and_channels_count_together() {
    let mut d = Detector::default();
    let mut m = map();
    let c = Settings {
        threshold: 2,
        ..Default::default()
    };
    assert!(step(&mut d, &mut m, delete("10"), &c, 0).is_none());
    let b = step(
        &mut d,
        &mut m,
        event("GUILD_ROLE_DELETE", json!({"guild_id":"555","role_id":"7"})),
        &c,
        60_000,
    )
    .unwrap();
    assert_eq!(b.deletions[1].action, 32);
    assert!(b.before.roles.iter().any(|r| r.id == "7"));
    assert_eq!(b.before.members[0].roles, vec!["7"]);
}
#[test]
fn rolling_window_expires_old_events_and_resets_baseline_after_idle() {
    let mut d = Detector::default();
    let mut m = map();
    let c = Settings {
        threshold: 3,
        ..Default::default()
    };
    assert!(step(&mut d, &mut m, delete("10"), &c, 0).is_none());
    let expected = serde_json::to_value(&m).unwrap();
    assert!(step(&mut d, &mut m, delete("11"), &c, 59_000).is_none());
    assert!(step(&mut d, &mut m, delete("12"), &c, 61_000).is_none());
    let b = step(&mut d, &mut m, delete("13"), &c, 62_000).unwrap();
    assert_eq!(serde_json::to_value(&b.before).unwrap(), expected);
    assert_eq!(
        b.deletions
            .iter()
            .map(|d| d.target.as_str())
            .collect::<Vec<_>>(),
        ["11", "12", "13"]
    );
    let mut d = Detector::default();
    let mut m = map();
    let c = Settings {
        threshold: 2,
        ..Default::default()
    };
    step(&mut d, &mut m, delete("10"), &c, 0);
    assert!(step(&mut d, &mut m, delete("11"), &c, 61_000).is_none());
    let b = step(&mut d, &mut m, delete("12"), &c, 62_000).unwrap();
    assert_eq!(b.before.channels.len(), 4);
}
#[test]
fn partial_expiry_preserves_the_map_before_the_first_live_delete() {
    let mut d = Detector::default();
    let mut m = map();
    let c = Settings {
        threshold: 3,
        ..Default::default()
    };
    assert!(step(&mut d, &mut m, delete("10"), &c, 0).is_none());
    assert!(step(
        &mut d,
        &mut m,
        event("GUILD_ROLE_DELETE", json!({"guild_id":"555","role_id":"7"})),
        &c,
        1_000,
    )
    .is_none());

    // Cambios legítimos entre los borrados caducados y el primero vigente.
    let mut updated = channel("14", None, 0);
    updated["name"] = json!("Renamed before burst");
    updated["permission_overwrites"] = json!([]);
    assert!(step(
        &mut d,
        &mut m,
        event("CHANNEL_UPDATE", updated.clone()),
        &c,
        30_000,
    )
    .is_none());
    let expected = serde_json::to_value(&m).unwrap();
    assert!(step(
        &mut d,
        &mut m,
        event("GUILD_ROLE_DELETE", json!({"guild_id":"555","role_id":"8"})),
        &c,
        60_001,
    )
    .is_none());

    // El snapshot debe conservar el instante anterior al primer borrado
    // vigente, aunque después también haya actualizaciones del Gateway.
    updated["name"] = json!("Renamed during burst");
    step(&mut d, &mut m, event("CHANNEL_UPDATE", updated), &c, 60_500);
    assert!(step(&mut d, &mut m, delete("11"), &c, 61_001).is_none());
    let b = step(&mut d, &mut m, delete("12"), &c, 62_000).unwrap();
    assert_eq!(serde_json::to_value(&b.before).unwrap(), expected);
    assert_eq!(
        b.deletions
            .iter()
            .map(|d| d.target.as_str())
            .collect::<Vec<_>>(),
        ["8", "11", "12"]
    );
    let snapshot = from_map(&b.before);
    assert!(!snapshot.channels.iter().any(|c| c["id"] == "10"));
    assert!(!snapshot.roles.iter().any(|r| r["id"] == "7"));
    assert_eq!(snapshot.members[0]["roles"], json!([]));
    assert!(snapshot.roles.iter().any(|r| r["id"] == "8"));
}
#[test]
fn ignores_duplicates_other_guilds_threads_and_disabled_detector() {
    let mut d = Detector::default();
    let mut m = map();
    let c = Settings {
        threshold: 2,
        ..Default::default()
    };
    assert!(d
        .observe(
            &m,
            &event(
                "CHANNEL_DELETE",
                json!({"id":"10","guild_id":"556","type":0})
            ),
            &c,
            0
        )
        .is_none());
    assert!(d
        .observe(
            &m,
            &event(
                "CHANNEL_DELETE",
                json!({"id":"11","guild_id":"555","type":11})
            ),
            &c,
            0
        )
        .is_none());
    assert!(d.observe(&m, &Event::GatewayHeartbeatAck, &c, 0).is_none());
    assert!(step(&mut d, &mut m, delete("10"), &c, 0).is_none());
    assert!(step(&mut d, &mut m, delete("10"), &c, 0).is_none());
    let off = Settings {
        enabled: false,
        ..c.clone()
    };
    assert!(step(&mut d, &mut m, delete("11"), &off, 0).is_none());
    assert!(step(&mut d, &mut m, delete("12"), &c, 0).is_none());
    assert!(step(&mut d, &mut m, delete("13"), &c, 0).is_some());
}
fn entry(target: &str, action: u16, actor: &str, time: i64) -> Value {
    json!({"id":(((time - 1_420_070_400_000) as u64)<<22).to_string(),"target_id":target,"action_type":action,"user_id":actor})
}
#[test]
fn audit_requires_matching_target_action_and_recent_timestamp() {
    let now = chrono::Utc::now().timestamp_millis();
    let b = burst(now, false);
    let log = json!({"audit_log_entries":[entry("10",12,"9",now),entry("11",12,"8",now),entry("12",11,"77",now),entry("13",12,"66",now-60000),entry("99",12,"44",now)],"users":[{"id":"9","username":"Actor A"},{"id":"8","username":"Actor B"}]});
    let (ids, names) = actors(&log, &b.deletions);
    assert_eq!(ids, ["9", "8"]);
    assert_eq!(names, ["Actor A (9)", "Actor B (8)"]);
}
async fn database() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db::run_migrations(&pool).await.unwrap();
    pool
}
async fn mock_get(server: &MockServer, route: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}
#[tokio::test]
async fn settings_default_safe_persisted_and_normalized_per_bot_guild() {
    let old: db::GuildSettings = serde_json::from_value(json!({"notes":"old"})).unwrap();
    assert!(old.destruction.enabled);
    assert!(!old.destruction.remove_roles);
    assert_eq!(old.destruction.threshold, 5);
    assert_eq!(old.destruction.window_seconds, 60);
    let p = database().await;
    let mut s = old;
    s.destruction = Settings {
        threshold: 0,
        window_seconds: 0,
        remove_roles: true,
        enabled: true,
    };
    db::save_guild_settings(&p, "bot-a", "555", &s)
        .await
        .unwrap();
    let s = db::get_guild_settings(&p, "bot-a", "555").await.unwrap();
    assert_eq!(s.destruction.threshold, 2);
    assert_eq!(s.destruction.window_seconds, 1);
    assert!(s.destruction.remove_roles);
    assert!(
        !db::get_guild_settings(&p, "bot-b", "555")
            .await
            .unwrap()
            .destruction
            .remove_roles
    );
}
#[tokio::test]
async fn audit_failure_still_saves_complete_emergency_map_and_durable_alert() {
    let p = database().await;
    let server = MockServer::start().await;
    let base = server.address().to_string();
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/audit-logs"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let a = protect(
        &p,
        "bot",
        "fake",
        Some(&base),
        burst(chrono::Utc::now().timestamp_millis(), false),
        |_| {},
    )
    .await
    .unwrap();
    assert!(a.actors.is_empty());
    assert!(!a.detail.is_empty());
    let data: String =
        sqlx::query_scalar("SELECT data FROM server_snapshots WHERE id=? AND automatic=0")
            .bind(&a.snapshot_id)
            .fetch_one(&p)
            .await
            .unwrap();
    let s: snapshots::ServerSnapshot = serde_json::from_str(&data).unwrap();
    assert_eq!(s.channels.len(), 5);
    assert_eq!(s.members[0]["roles"][0]["id"], "7");
    let rows = db::list_incidents(&p, Some("555"), 10).await.unwrap();
    let stored: Alert = serde_json::from_str(&rows[0].payload).unwrap();
    assert_eq!(stored.snapshot_id, a.snapshot_id);
    assert!(!stored.detail.is_empty());
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method == "GET"));
}
#[tokio::test]
async fn emergency_restore_uses_existing_preview_without_writes() {
    let p = database().await;
    let server = MockServer::start().await;
    let base = server.address().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    mock_get(&server,"/api/v10/guilds/555/audit-logs",json!({"audit_log_entries":[entry("10",12,"9",now)],"users":[{"id":"9","username":"Actor"}]})).await;
    let a = protect(&p, "bot", "fake", Some(&base), burst(now, false), |_| {})
        .await
        .unwrap();
    assert_eq!(a.actors, ["Actor (9)"]);
    mock_get(
        &server,
        "/api/v10/guilds/555",
        json!({"id":"555","name":"Fixture server"}),
    )
    .await;
    mock_get(&server, "/api/v10/guilds/555/roles", json!([role("555")])).await;
    mock_get(&server, "/api/v10/guilds/555/channels", json!([])).await;
    mock_get(
        &server,
        "/api/v10/guilds/555/members",
        json!([{"user":{"id":"9"},"roles":[]}]),
    )
    .await;
    let preview = snapshots::restore::preview(
        &p,
        "bot",
        "555",
        &a.snapshot_id,
        "fake",
        Some(&base),
        false,
        false,
    )
    .await
    .unwrap();
    let v = serde_json::to_value(preview).unwrap();
    let steps = v["steps"].as_array().unwrap();
    assert_eq!(
        steps
            .iter()
            .filter(|s| s["kind"] == "channel_create")
            .count(),
        5
    );
    assert_eq!(
        steps.iter().filter(|s| s["kind"] == "role_create").count(),
        2
    );
    assert!(steps.iter().any(|s| s["kind"] == "member_add"));
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method == "GET"));
}
#[tokio::test]
async fn opt_in_removes_only_correlated_actor_editable_roles() {
    let p = database().await;
    let server = MockServer::start().await;
    let base = server.address().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    mock_get(
        &server,
        "/api/v10/guilds/555/audit-logs",
        json!({"audit_log_entries":[entry("10",12,"9",now),entry("99",12,"99",now)],"users":[]}),
    )
    .await;
    mock_get(&server,"/api/v10/guilds/555/members/9",json!({"user":{"id":"9","username":"Actor","discriminator":"0","avatar":null},"roles":["7","8"],"joined_at":"2026-01-01T00:00:00.000000+00:00","deaf":false,"mute":false,"flags":0})).await;
    mock_get(&server, "/api/v10/guilds/555/members/100", json!({"user":{"id":"100","username":"Fixture bot","discriminator":"0","avatar":null,"bot":true},"roles":["6"],"joined_at":"2026-01-01T00:00:00.000000+00:00","deaf":false,"mute":false,"flags":0})).await;
    let mut bot_role = role("6");
    bot_role["permissions"] = json!("268435456");
    let mut managed = role("8");
    managed["managed"] = json!(true);
    mock_get(
        &server,
        "/api/v10/guilds/555/roles",
        json!([bot_role, role("7"), managed]),
    )
    .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/guilds/555/members/9/roles/7"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let a = protect(&p, "100", "fake", Some(&base), burst(now, true), |_| {})
        .await
        .unwrap();
    assert!(a.detail.iter().any(|s| s == "Roles removed from 9"));
    let writes: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method != "GET")
        .collect();
    assert_eq!(writes.len(), 1);
    assert!(writes[0].url.path().ends_with("/9/roles/7"));
}
