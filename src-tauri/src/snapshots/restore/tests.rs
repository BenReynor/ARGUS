use super::*;
use wiremock::{matchers::any, Mock, MockServer, Request, Respond, ResponseTemplate};

struct Fake {
    guild: Value,
    roles: Vec<Value>,
    channels: Vec<Value>,
    members: Vec<Value>,
    next: u64,
    writes: usize,
}
#[derive(Clone)]
struct Discord(Arc<Mutex<Fake>>);
fn role_defaults(r: &mut Value) {
    if r["icon"].as_str().is_some_and(|s| s.starts_with("data:")) {
        r["icon"] = Value::Null;
    }
    for (k, v) in [
        ("flags", json!(0)),
        ("managed", json!(false)),
        ("hoist", json!(false)),
        ("mentionable", json!(false)),
        ("color", json!(0)),
    ] {
        if r.get(k).is_none() {
            r[k] = v;
        }
    }
}
fn member(user: &str, roles: Value) -> Value {
    json!({"user":{"id":user,"username":"Fixture member","discriminator":"0","avatar":null},"roles":roles,"joined_at":"2026-01-01T00:00:00.000000+00:00","deaf":false,"mute":false,"flags":0})
}
impl Respond for Discord {
    fn respond(&self, r: &Request) -> ResponseTemplate {
        let mut f = self.0.lock().unwrap();
        let parts: Vec<_> = r
            .url
            .path()
            .trim_start_matches("/api/v10/")
            .split('/')
            .collect();
        let method = r.method.as_str();
        if method == "GET" {
            let body = match parts.as_slice() {
                ["guilds", _] => f.guild.clone(),
                ["guilds", _, "roles"] => json!(f.roles),
                ["guilds", _, "channels"] => json!(f.channels),
                ["guilds", _, "members"] => json!(f.members),
                ["guilds", _, "members", u] => f
                    .members
                    .iter()
                    .find(|m| m["user"]["id"] == *u)
                    .cloned()
                    .unwrap_or(Value::Null),
                _ => return ResponseTemplate::new(404),
            };
            return ResponseTemplate::new(200).set_body_json(body);
        }
        f.writes += 1;
        let body: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
        match (method, parts.as_slice()) {
            ("POST", ["guilds", _, collection @ ("roles" | "channels")]) => {
                f.next += 1;
                let mut v = body;
                v["id"] = json!(f.next.to_string());
                if *collection == "roles" {
                    role_defaults(&mut v);
                    v["position"] = json!(1);
                    f.roles.push(v.clone());
                } else {
                    f.channels.push(v.clone());
                }
                ResponseTemplate::new(200).set_body_json(v)
            }
            ("PATCH", ["guilds", _, collection @ ("roles" | "channels")]) => {
                let list = if *collection == "roles" {
                    &mut f.roles
                } else {
                    &mut f.channels
                };
                for position in body.as_array().unwrap() {
                    let v = list.iter_mut().find(|v| v["id"] == position["id"]).unwrap();
                    v["position"] = position["position"].clone();
                }
                ResponseTemplate::new(200).set_body_json(json!(list))
            }
            ("PATCH", ["guilds", _, "roles", target]) => {
                let v = f.roles.iter_mut().find(|v| id(v) == *target).unwrap();
                v.as_object_mut()
                    .unwrap()
                    .extend(body.as_object().unwrap().clone());
                ResponseTemplate::new(200).set_body_json(v)
            }
            ("PATCH", ["channels", target]) => {
                let v = f.channels.iter_mut().find(|v| id(v) == *target).unwrap();
                v.as_object_mut()
                    .unwrap()
                    .extend(body.as_object().unwrap().clone());
                ResponseTemplate::new(200).set_body_json(v)
            }
            ("PATCH", ["guilds", _]) => {
                f.guild
                    .as_object_mut()
                    .unwrap()
                    .extend(body.as_object().unwrap().clone());
                ResponseTemplate::new(200).set_body_json(&f.guild)
            }
            ("PATCH", ["guilds", _, "widget"]) => ResponseTemplate::new(200).set_body_json(body),
            ("PUT" | "DELETE", ["guilds", _, "members", u, "roles", role]) => {
                let m = f
                    .members
                    .iter_mut()
                    .find(|m| m["user"]["id"] == *u)
                    .unwrap();
                let rs = m["roles"].as_array_mut().unwrap();
                if method == "PUT" {
                    if !rs.contains(&json!(role)) {
                        rs.push(json!(role));
                    }
                } else {
                    rs.retain(|r| r != *role);
                }
                ResponseTemplate::new(204)
            }
            ("DELETE", ["channels", target]) => {
                f.channels.retain(|c| id(c) != *target);
                ResponseTemplate::new(200).set_body_json(json!({}))
            }
            ("DELETE", ["guilds", _, "roles", target]) => {
                f.roles.retain(|c| id(c) != *target);
                ResponseTemplate::new(204)
            }
            _ => ResponseTemplate::new(400)
                .set_body_json(json!({"code":50035,"message":"Unexpected fixture request"})),
        }
    }
}
fn state() -> AppState {
    let state = AppState::new();
    let bot = crate::state::BotConfig {
        id: "42".into(),
        name: "Fixture bot".into(),
        token_ciphertext: crypto::encrypt("fake-token").unwrap(),
        avatar: None,
        application_id: None,
        allowed_guilds: vec![],
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    state.config.lock().unwrap().bots.push(bot);
    state.set_active_bot("42");
    state.set_active_guild("555");
    state
}
async fn setup() -> (ServerSnapshot, MockServer, Discord, SqlitePool, String) {
    let fixture = super::super::tests::fixture().await;
    let mut saved = super::super::tests::snapshot(&fixture).await;
    saved.roles[1]["permissions"] = json!("3072");
    // El caso de igualdad usa permisos resolubles; los irresolubles tienen su propio test.
    saved.channels[1]["permission_overwrites"]
        .as_array_mut()
        .unwrap()
        .retain(|o| id(o) != "99");
    saved
        .members
        .push(json!({"id":"99","name":"Former member","roles":[]}));
    saved.roles.push(json!({"id":"8","name":"Second role","permissions":"1024","position":2,"color":0,"hoist":false,"mentionable":false}));
    let mut everyone = saved.roles[0].clone();
    role_defaults(&mut everyone);
    let mut bot_role = json!({"id":"40","name":"Runtime bot role","permissions":"8","position":100,"managed":true});
    role_defaults(&mut bot_role);
    let mut guild = saved.guild.clone();
    guild["owner_id"] = json!("500");
    guild["name"] = json!("Changed server");
    let fake = Discord(Arc::new(Mutex::new(Fake {
        guild,
        roles: vec![everyone, bot_role],
        channels: vec![],
        members: vec![member("9", json!([])), member("42", json!(["40"]))],
        next: 1000,
        writes: 0,
    })));
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(fake.clone())
        .mount(&server)
        .await;
    let pool = super::super::tests::database().await;
    let snapshot = store(&pool, "42", "555", &saved, false).await.unwrap();
    (saved, server, fake, pool, snapshot)
}
fn effective(channel: &Value, roles: &[Value], member: &Value) -> u64 {
    let rs = member["roles"].as_array().unwrap();
    let mut bits = roles
        .iter()
        .filter(|r| id(r) == "555" || rs.contains(&r["id"]))
        .fold(0, |bits, r| {
            bits | r["permissions"].as_str().unwrap().parse::<u64>().unwrap()
        });
    if bits & 8 != 0 {
        return u64::MAX;
    }
    let overwrites = channel["permission_overwrites"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let apply = |bits: u64, entries: Vec<&Value>| {
        let allow = entries.iter().fold(0, |b, o| {
            b | o["allow"].as_str().unwrap().parse::<u64>().unwrap()
        });
        let deny = entries.iter().fold(0, |b, o| {
            b | o["deny"].as_str().unwrap().parse::<u64>().unwrap()
        });
        (bits & !deny) | allow
    };
    bits = apply(
        bits,
        overwrites
            .iter()
            .filter(|o| o["type"] == 0 && o["id"] == "555")
            .collect(),
    );
    bits = apply(
        bits,
        overwrites
            .iter()
            .filter(|o| o["type"] == 0 && o["id"] != "555" && rs.contains(&o["id"]))
            .collect(),
    );
    bits = apply(
        bits,
        overwrites
            .iter()
            .filter(|o| o["type"] == 1 && o["id"] == member["id"])
            .collect(),
    );
    bits
}

async fn generate_more_than_32_proposals(pool: &SqlitePool, snapshot: &str, base: &str) {
    for _ in 0..33 {
        preview(
            pool,
            "42",
            "555",
            snapshot,
            "fake-token",
            Some(base),
            false,
            false,
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn emptied_server_restores_names_order_effective_permissions_and_member_roles_with_stop_resume(
) {
    let (saved, server, fake, pool, snapshot) = setup().await;
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        true,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        fake.0.lock().unwrap().writes,
        0,
        "Restore is read-only until Apply"
    );
    assert!(!p.delete_extra);
    assert!(p.unavailable.iter().any(|w| w.contains("Messages")));
    assert!(p.unavailable.iter().any(|w| w.contains("Former member")));
    let state = state();
    let key = p.id.clone();
    let partial = apply(&state, &key, "", Some(&base), |p| {
        if p.indice == 0 {
            stop_snapshot_restore(key.clone());
        }
    })
    .await
    .unwrap();
    assert_eq!(partial.hechas, 1);
    assert!(partial.canceladas > 0);
    let writes_before_previews = fake.0.lock().unwrap().writes;
    generate_more_than_32_proposals(&pool, &snapshot, &base).await;
    assert_eq!(fake.0.lock().unwrap().writes, writes_before_previews);
    let done = apply(&state, &key, "", Some(&base), |_| {}).await.unwrap();
    assert_eq!(done.fallidas, 0, "{:?}", done.resultados);
    assert_eq!(done.hechas, p.steps.len());
    let writes = fake.0.lock().unwrap().writes;
    let again = apply(&state, &key, "", Some(&base), |_| {}).await.unwrap();
    assert_eq!(again.hechas, p.steps.len());
    assert_eq!(writes, fake.0.lock().unwrap().writes);
    let f = fake.0.lock().unwrap();
    assert_eq!(f.guild["name"], saved.guild["name"]);
    for original in &saved.roles {
        let restored = f
            .roles
            .iter()
            .find(|r| label(r) == label(original))
            .unwrap();
        assert_eq!(restored["permissions"], original["permissions"]);
        assert_eq!(restored["position"], original["position"]);
    }
    for original in &saved.channels {
        let restored = f
            .channels
            .iter()
            .find(|c| label(c) == label(original))
            .unwrap();
        assert_ne!(id(restored), id(original));
        assert_eq!(restored["position"], original["position"]);
        if let Some(parent) = original["parent_id"].as_str() {
            let parent_name = name(&saved.channels, parent).unwrap();
            assert_eq!(
                name(&f.channels, restored["parent_id"].as_str().unwrap()),
                Some(parent_name)
            );
        }
        let mut original_member = saved.members[0].clone();
        original_member["roles"] = json!(["7"]);
        let current = f.members.iter().find(|m| m["user"]["id"] == "9").unwrap();
        let current = json!({"id":"9","roles":current["roles"]});
        assert_eq!(
            effective(original, &saved.roles, &original_member),
            effective(restored, &f.roles, &current),
            "{}",
            label(original)
        );
    }
    let assigned = &f.members[0]["roles"].as_array().unwrap();
    assert_eq!(assigned.len(), 1);
    assert_eq!(
        name(&f.roles, assigned[0].as_str().unwrap()),
        Some("Fixture role")
    );
    assert_eq!(
        f.guild["system_channel_id"],
        f.channels.iter().find(|c| label(c) == "Forum").unwrap()["id"]
    );
}
#[tokio::test]
async fn duplicate_role_names_preserve_ids_and_restore_the_missing_role_separately() {
    for survivor in ["7", "8"] {
        for undo in [false, true] {
            let (mut saved, server, fake, pool, _) = setup().await;
            saved.roles[1].as_object_mut().unwrap().remove("icon_image");
            saved.roles[1]["icon"] = Value::Null;
            saved.roles[2]["name"] = saved.roles[1]["name"].clone();
            saved.members = vec![
                json!({"id":"9","name":"First member","roles":[{"id":"7","name":"Fixture role"}]}),
                json!({"id":"90","name":"Second member","roles":[{"id":"8","name":"Fixture role"}]}),
            ];
            saved.channels[0]["permission_overwrites"] = json!([
                {"id":"7","name":"Fixture role","type":0,"allow":"1024","deny":"2048"},
                {"id":"8","name":"Fixture role","type":0,"allow":"2048","deny":"1024"}
            ]);
            let missing = if survivor == "7" { "8" } else { "7" };
            let mut surviving_role = saved
                .roles
                .iter()
                .find(|r| id(r) == survivor)
                .unwrap()
                .clone();
            role_defaults(&mut surviving_role);
            {
                let mut f = fake.0.lock().unwrap();
                f.roles.push(surviving_role.clone());
                f.members.push(member("90", json!([])));
            }
            let snapshot = store(&pool, "42", "555", &saved, false).await.unwrap();
            let base = server.address().to_string();
            let p = preview(
                &pool,
                "42",
                "555",
                &snapshot,
                "fake-token",
                Some(&base),
                undo,
                false,
            )
            .await
            .unwrap();
            let creations: Vec<_> = p.steps.iter().filter(|s| s.kind == "role_create").collect();
            assert_eq!(creations.len(), 1);
            assert_eq!(creations[0].target, missing);
            assert_eq!(fake.0.lock().unwrap().writes, 0);
            let done = apply(&state(), &p.id, "", Some(&base), |_| {})
                .await
                .unwrap();
            assert_eq!(done.fallidas, 0, "{:?}", done.resultados);
            assert_eq!(done.hechas, p.steps.len());
            let f = fake.0.lock().unwrap();
            let restored_ids: HashMap<_, _> = saved
                .members
                .iter()
                .map(|m| {
                    let current = f
                        .members
                        .iter()
                        .find(|v| v["user"]["id"] == m["id"])
                        .unwrap();
                    let assigned = current["roles"].as_array().unwrap();
                    assert_eq!(assigned.len(), 1);
                    (id(&m["roles"][0]), assigned[0].as_str().unwrap())
                })
                .collect();
            assert_eq!(restored_ids[survivor], survivor);
            assert_ne!(restored_ids[missing], survivor);
            assert_ne!(restored_ids[missing], missing);
            assert_eq!(
                f.roles.iter().find(|r| id(r) == survivor).unwrap(),
                &surviving_role
            );
            for original in &saved.roles {
                let restored_id = restored_ids
                    .get(id(original))
                    .copied()
                    .unwrap_or(id(original));
                let restored = f.roles.iter().find(|r| id(r) == restored_id).unwrap();
                assert_eq!(restored["name"], original["name"]);
                assert_eq!(restored["permissions"], original["permissions"]);
                assert_eq!(restored["position"], original["position"]);
            }
            let restored = f.channels.iter().find(|c| label(c) == "Category").unwrap();
            for (original, overwrite) in saved.channels[0]["permission_overwrites"]
                .as_array()
                .unwrap()
                .iter()
                .zip(restored["permission_overwrites"].as_array().unwrap())
            {
                assert_eq!(overwrite["id"], restored_ids[id(original)]);
                assert_eq!(overwrite["allow"], original["allow"]);
                assert_eq!(overwrite["deny"], original["deny"]);
            }
            for m in &saved.members {
                let original = json!({"id":m["id"],"roles":[id(&m["roles"][0])]});
                let current = json!({"id":m["id"],"roles":[restored_ids[id(&m["roles"][0])]]});
                assert_eq!(
                    effective(&saved.channels[0], &saved.roles, &original),
                    effective(restored, &f.roles, &current)
                );
            }
        }
    }
}

#[tokio::test]
async fn deleted_role_is_not_matched_to_a_managed_role_with_the_same_name() {
    let (saved, _, _, _, _) = setup().await;
    assert_ne!(saved.roles[1]["managed"], true);
    let roles = vec![
        saved.roles[0].clone(),
        json!({"id":"900","name":saved.roles[1]["name"].clone(),"managed":true}),
    ];
    let job = build(&saved, &saved.guild, &roles, &[], &[], false, false);
    assert_ne!(job.roles.get("7").map(String::as_str), Some("900"));
    assert!(job.preview.steps.iter().any(|s| s.kind == "role_create"));
    assert!(!job
        .preview
        .unavailable
        .iter()
        .any(|w| w.contains("Ambiguous role name")));
}

#[tokio::test]
async fn name_fallback_consumes_each_current_role_at_most_once() {
    let (mut saved, _, _, _, _) = setup().await;
    saved.roles[2]["name"] = saved.roles[1]["name"].clone();
    let roles = vec![
        saved.roles[0].clone(),
        json!({"id":"700","name":"Fixture role"}),
    ];
    let job = build(&saved, &saved.guild, &roles, &[], &[], false, false);
    assert!(!job.roles.contains_key("7"));
    assert!(!job.roles.contains_key("8"));
    assert!(job
        .preview
        .unavailable
        .iter()
        .any(|w| w.contains("Ambiguous role name")));
    assert!(!job.preview.steps.iter().any(|s| s.kind == "role_create"));
    assert_eq!(
        job.roles.values().collect::<HashSet<_>>().len(),
        job.roles.len()
    );
}

#[tokio::test]
async fn missing_only_is_default_conflicts_are_reported_and_deletion_requires_name() {
    let (mut saved, server, fake, pool, snapshot) = setup().await;
    saved
        .members
        .push(json!({"id":"88","name":"Absent member","roles":[]}));
    let (roles, channels, members, guild) = {
        let mut f = fake.0.lock().unwrap();
        f.roles
            .push(json!({"id":"700","name":"Fixture role","permissions":"0","position":1}));
        f.roles
            .push(json!({"id":"701","name":"Extra role","permissions":"0","position":2}));
        f.channels.push(json!({"id":"900","name":"Forum","type":0}));
        f.channels
            .push(json!({"id":"901","name":"Extra channel","type":0}));
        (
            f.roles.clone(),
            f.channels.clone(),
            vec![json!({"id":"9","roles":[]})],
            f.guild.clone(),
        )
    };
    let job = build(&saved, &guild, &roles, &channels, &members, false, false);
    assert!(job
        .preview
        .unavailable
        .iter()
        .any(|w| w.contains("Channel name already exists: Forum")));
    assert!(job
        .preview
        .unavailable
        .iter()
        .any(|w| w.contains("Absent member")));
    assert!(!job.preview.steps.iter().any(|s| s.kind.ends_with("delete")
        || s.kind == "role_update"
        || s.kind == "guild_update"
        || s.kind == "member_remove"));
    assert_eq!(job.roles.get("7").map(String::as_str), Some("700"));
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        false,
        true,
    )
    .await
    .unwrap();
    assert!(p
        .steps
        .iter()
        .any(|s| s.kind == "channel_delete" && s.target == "901"));
    assert!(!p
        .steps
        .iter()
        .any(|s| s.kind == "channel_delete" && s.target == "900"));
    assert!(apply(&state(), &p.id, "wrong name", Some(&base), |_| {})
        .await
        .unwrap_err()
        .to_string()
        .contains("Type the server name"));
    assert_eq!(fake.0.lock().unwrap().writes, 0);
}
#[tokio::test]
async fn undo_changes_restores_overwrites_and_removes_extra_member_roles() {
    let (saved, _, _, _, _) = setup().await;
    let mut roles = saved.roles.clone();
    roles[1]["permissions"] = json!("0");
    roles[1]["name"] = json!("Renamed");
    roles.push(json!({"id":"70","name":"Extra","permissions":"0"}));
    let mut channels = saved.channels.clone();
    channels[0]["permission_overwrites"] = json!([]);
    let members = vec![json!({"id":"9","roles":["70"]})];
    let missing = build(
        &saved,
        &saved.guild,
        &roles,
        &channels,
        &members,
        false,
        false,
    );
    assert!(
        !missing.preview.steps.iter().any(|s| s.kind == "role_update"
            || s.kind == "channel_update"
            || s.kind == "member_remove")
    );
    let undo = build(
        &saved,
        &saved.guild,
        &roles,
        &channels,
        &members,
        true,
        false,
    );
    assert!(undo
        .preview
        .steps
        .iter()
        .any(|s| s.kind == "role_update" && s.data["name"] == "Fixture role"));
    assert!(undo.preview.steps.iter().any(|s| s.kind == "channel_update"
        && s.target == "10"
        && s.data["permission_overwrites"][0]["deny"] == "2048"));
    assert!(undo
        .preview
        .steps
        .iter()
        .any(|s| s.kind == "member_remove" && s.data == "70"));
}

#[tokio::test]
async fn failed_action_can_be_retried_without_repeating_successful_creations() {
    let (_, server, fake, pool, snapshot) = setup().await;
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        false,
        false,
    )
    .await
    .unwrap();
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/v10/guilds/555/channels"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"Missing Permissions"})),
        )
        .with_priority(1)
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    let state = state();
    let failed = apply(&state, &p.id, "", Some(&base), |_| {}).await.unwrap();
    assert_eq!(failed.fallidas, 1);
    assert!(failed.hechas > 0);
    assert!(failed.canceladas > 0);
    let role_count = fake.0.lock().unwrap().roles.len();
    let writes_before_previews = fake.0.lock().unwrap().writes;
    generate_more_than_32_proposals(&pool, &snapshot, &base).await;
    assert_eq!(fake.0.lock().unwrap().writes, writes_before_previews);
    let done = apply(&state, &p.id, "", Some(&base), |_| {}).await.unwrap();
    assert_eq!(done.fallidas, 0, "{:?}", done.resultados);
    assert_eq!(done.hechas, p.steps.len());
    assert_eq!(fake.0.lock().unwrap().roles.len(), role_count);
}

#[tokio::test]
async fn channel_name_conflicts_do_not_generate_unresolvable_child_or_setting_actions() {
    let (saved, _, _, _, _) = setup().await;
    let channels = vec![json!({"id":"990","name":"Category","type":4})];
    let job = build(
        &saved,
        &saved.guild,
        &saved.roles,
        &channels,
        &[],
        true,
        false,
    );
    assert!(job
        .preview
        .unavailable
        .iter()
        .any(|w| w.contains("Category")));
    assert!(job
        .preview
        .unavailable
        .iter()
        .any(|w| w.contains("Parent category is unavailable: Forum")));
    assert!(!job
        .preview
        .steps
        .iter()
        .any(|s| s.kind == "channel_create" && s.target == "11"));
    let settings = job
        .preview
        .steps
        .iter()
        .find(|s| s.kind == "guild_update")
        .unwrap();
    assert!(settings.data.get("system_channel_id").is_none());
}

#[test]
fn restore_request_defaults_are_non_destructive() {
    let request: SnapshotRequest =
        serde_json::from_value(json!({"op":"restore","id":"saved-snapshot"})).unwrap();
    assert!(matches!(
        request,
        SnapshotRequest::Restore {
            undo: false,
            delete_extra: false,
            ..
        }
    ));
}

#[tokio::test]
async fn missing_permission_and_different_scope_never_write() {
    let (_, server, fake, pool, snapshot) = setup().await;
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        false,
        false,
    )
    .await
    .unwrap();
    let state = state();
    state.set_active_guild("556");
    assert!(apply(&state, &p.id, "", Some(&base), |_| {}).await.is_err());
    state.set_active_guild("555");
    fake.0.lock().unwrap().roles[1]["permissions"] = json!("0");
    let failed = apply(&state, &p.id, "", Some(&base), |_| {}).await.unwrap();
    assert_eq!(failed.hechas, 0);
    assert_eq!(failed.fallidas, 1);
    assert!(failed.resultados[0]
        .mensaje
        .contains("missing the permission"));
    assert_eq!(fake.0.lock().unwrap().writes, 0);
}

#[tokio::test]
async fn confirmed_deletion_removes_only_extras_and_keeps_managed_roles() {
    let (_, server, fake, pool, snapshot) = setup().await;
    {
        let mut f = fake.0.lock().unwrap();
        let mut extra = json!({"id":"701","name":"Extra role","permissions":"0","position":3});
        role_defaults(&mut extra);
        f.roles.push(extra);
        f.channels
            .push(json!({"id":"901","name":"Extra channel","type":0}));
    }
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        false,
        true,
    )
    .await
    .unwrap();
    let done = apply(&state(), &p.id, &p.guild_name, Some(&base), |_| {})
        .await
        .unwrap();
    assert_eq!(done.fallidas, 0, "{:?}", done.resultados);
    assert_eq!(done.hechas, p.steps.len());
    let f = fake.0.lock().unwrap();
    assert!(!f.roles.iter().any(|r| id(r) == "701"));
    assert!(!f.channels.iter().any(|c| id(c) == "901"));
    assert!(f
        .roles
        .iter()
        .any(|r| id(r) == "40" && r["managed"] == true));
    assert!(f.roles.iter().any(|r| id(r) == "555"));
}

#[tokio::test]
async fn ambiguous_roles_are_conflicts_and_keep_roles_and_assignments_even_with_undo_and_delete() {
    let (saved, server, fake, pool, snapshot) = setup().await;
    {
        let mut f = fake.0.lock().unwrap();
        for rid in ["700", "701"] {
            let mut role = saved.roles[1].clone();
            role["id"] = json!(rid);
            role["icon"] = Value::Null;
            role_defaults(&mut role);
            f.roles.push(role);
        }
        f.members[0]["roles"] = json!(["700", "701"]);
    }
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        true,
        true,
    )
    .await
    .unwrap();
    assert!(p
        .unavailable
        .iter()
        .any(|w| w.contains("Ambiguous role name")));
    assert!(!p.steps.iter().any(|s| (s.kind == "role_delete"
        && ["700", "701"].contains(&s.target.as_str()))
        || s.kind == "member_remove"));
    let done = apply(&state(), &p.id, &p.guild_name, Some(&base), |_| {})
        .await
        .unwrap();
    assert_eq!(done.fallidas, 0, "{:?}", done.resultados);
    let f = fake.0.lock().unwrap();
    assert_eq!(f.members[0]["roles"], json!(["700", "701"]));
    assert!(f.roles.iter().any(|r| id(r) == "700"));
    assert!(f.roles.iter().any(|r| id(r) == "701"));
    let category = f.channels.iter().find(|c| label(c) == "Category").unwrap();
    assert_eq!(
        category["permission_overwrites"],
        json!([{"id":"555","type":0,"allow":"0","deny":"1024"}])
    );
}

#[tokio::test]
async fn unresolved_private_channel_is_created_closed_instead_of_dropping_its_deny() {
    let (mut saved, server, fake, pool, _) = setup().await;
    saved.channels[0]["permission_overwrites"] =
        json!([{"id":"999","name":"Missing managed role","type":0,"allow":"0","deny":"1024"}]);
    let snapshot = store(&pool, "42", "555", &saved, false).await.unwrap();
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        false,
        false,
    )
    .await
    .unwrap();
    assert!(p.unavailable.iter().any(|w| w.contains("access closed")));
    let done = apply(&state(), &p.id, "", Some(&base), |_| {})
        .await
        .unwrap();
    assert_eq!(done.fallidas, 0, "{:?}", done.resultados);
    let f = fake.0.lock().unwrap();
    let channel = f.channels.iter().find(|c| label(c) == "Category").unwrap();
    assert_eq!(
        effective(channel, &f.roles, &json!({"id":"9","roles":[]})) & 1024,
        0
    );
    assert_eq!(channel["permission_overwrites"][0]["deny"], "1024");
}

#[tokio::test]
async fn recreated_role_restores_its_overwrites_in_surviving_channels_in_missing_only_mode() {
    let (saved, server, fake, pool, snapshot) = setup().await;
    {
        let mut f = fake.0.lock().unwrap();
        f.channels = saved.channels.clone();
        for c in &mut f.channels {
            if let Some(os) = c["permission_overwrites"].as_array_mut() {
                os.retain(|o| id(o) != "7");
            }
        }
        f.channels[0]["topic"] = json!("Keep live edit");
    }
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        false,
        false,
    )
    .await
    .unwrap();
    let done = apply(&state(), &p.id, "", Some(&base), |_| {})
        .await
        .unwrap();
    assert_eq!(done.fallidas, 0, "{:?}", done.resultados);
    let f = fake.0.lock().unwrap();
    let role = f.roles.iter().find(|r| label(r) == "Fixture role").unwrap();
    let channel = f.channels.iter().find(|c| id(c) == "10").unwrap();
    assert_eq!(channel["topic"], "Keep live edit");
    let overwrite = channel["permission_overwrites"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == role["id"])
        .unwrap();
    assert_eq!(
        overwrite["allow"],
        saved.channels[0]["permission_overwrites"][0]["allow"]
    );
    assert_eq!(
        overwrite["deny"],
        saved.channels[0]["permission_overwrites"][0]["deny"]
    );
}

#[tokio::test]
async fn recreated_category_recovers_surviving_children_in_missing_only_mode() {
    let (saved, server, fake, pool, snapshot) = setup().await;
    {
        let mut f = fake.0.lock().unwrap();
        f.channels = saved
            .channels
            .iter()
            .filter(|c| c["type"] != 4)
            .cloned()
            .collect();
        for c in &mut f.channels {
            c["parent_id"] = Value::Null;
        }
    }
    let base = server.address().to_string();
    let p = preview(
        &pool,
        "42",
        "555",
        &snapshot,
        "fake-token",
        Some(&base),
        false,
        false,
    )
    .await
    .unwrap();
    let done = apply(&state(), &p.id, "", Some(&base), |_| {})
        .await
        .unwrap();
    assert_eq!(done.fallidas, 0, "{:?}", done.resultados);
    let f = fake.0.lock().unwrap();
    let parent = f.channels.iter().find(|c| c["type"] == 4).unwrap();
    for original in saved
        .channels
        .iter()
        .filter(|c| c["parent_id"].as_str() == Some("10"))
    {
        let child = f.channels.iter().find(|c| id(c) == id(original)).unwrap();
        assert_eq!(child["parent_id"], parent["id"]);
    }
}

#[tokio::test]
async fn mcp_preview_and_approved_action_use_the_same_restore_job() {
    let (_, server, fake, pool, snapshot) = setup().await;
    let state = state();
    db::upsert_bot(&pool, &state.get_active_bot().unwrap())
        .await
        .unwrap();
    state.save_config_to_pool(&pool).await.unwrap();
    let mcp = crate::mcp::ArgusMcp::nuevo(pool, Some(server.address().to_string()))
        .await
        .unwrap();
    let raw = mcp
        .info_restauracion_json(&crate::mcp::RestoreParams {
            snapshot_id: Some(snapshot),
            undo: false,
            delete_extra: false,
        })
        .await
        .unwrap();
    let proposal: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(fake.0.lock().unwrap().writes, 0);
    assert!(proposal["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["kind"] == "role_create"));
    let raw = mcp
        .ejecutar_accion("aplicar_restauracion", json!({"id":proposal["id"]}))
        .await
        .unwrap();
    let summary: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(summary["fallidas"], 0);
    assert_eq!(
        summary["hechas"].as_u64().unwrap() as usize,
        proposal["steps"].as_array().unwrap().len()
    );
    let writes = fake.0.lock().unwrap().writes;
    mcp.ejecutar_accion("aplicar_restauracion", json!({"id":proposal["id"]}))
        .await
        .unwrap();
    assert_eq!(
        fake.0.lock().unwrap().writes,
        writes,
        "MCP uses the same resume/idempotency state"
    );
}
