use super::*;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

async fn database() -> SqlitePool {
    let p = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db::run_migrations(&p).await.unwrap();
    p
}
async fn fixture() -> MockServer {
    let server = MockServer::start().await;
    let entries = [
        (
            "",
            json!({"id":"555","name":"Fixture guild","icon":"hash","banner":"a_bannerhash","splash":"splashhash","discovery_splash":"discoveryhash","premium_progress_bar_enabled":true,"widget_enabled":true,"widget_channel_id":"11","afk_channel_id":"12","afk_timeout":300,"system_channel_id":"11","system_channel_flags":1,"rules_channel_id":"11","public_updates_channel_id":"11","safety_alerts_channel_id":"11","verification_level":3,"default_message_notifications":1,"explicit_content_filter":2}),
        ),
        (
            "/roles",
            json!([{"id":"555","name":"@everyone","color":0,"permissions":"1024","position":0,"hoist":false,"mentionable":false,"icon":null},{"id":"7","name":"Fixture role","color":12345,"permissions":"8","position":1,"hoist":true,"mentionable":true,"icon":"rolehash"}]),
        ),
        (
            "/channels",
            json!([
                {"id":"10","name":"Category","type":4,"position":0,"permission_overwrites":[{"id":"7","type":0,"allow":"1024","deny":"2048"}]},
                {"id":"11","name":"Forum","type":15,"parent_id":"10","position":1,"topic":"Fixture topic","nsfw":true,"rate_limit_per_user":30,"available_tags":[{"id":"50","name":"Tag","moderated":false,"emoji_id":null,"emoji_name":null}],"permission_overwrites":[{"id":"9","type":1,"allow":"2048","deny":"0"},{"id":"99","type":1,"allow":"0","deny":"1024"}]},
                {"id":"12","name":"Voice","type":2,"position":2,"bitrate":64000,"user_limit":5,"permission_overwrites":[]}
            ]),
        ),
        (
            "/members",
            json!([{"user":{"id":"9","username":"Fixture user","global_name":"Global"},"nick":"Nickname","roles":["7"]}]),
        ),
        (
            "/emojis",
            json!([{"id":"21","name":"Fixture emoji","animated":true,"roles":["7"]}]),
        ),
        (
            "/stickers",
            json!([{"id":"22","name":"Fixture sticker","format_type":2,"tags":"tag"}]),
        ),
        (
            "/webhooks",
            json!([{"id":"31","name":"Fixture hook","channel_id":"11","token":"must-not-be-exported","url":"must-not-be-exported"}]),
        ),
        (
            "/bans",
            json!([{"user":{"id":"32","username":"Banned user"},"reason":"Fixture reason"}]),
        ),
    ];
    for (suffix, value) in entries {
        Mock::given(method("GET"))
            .and(path(format!("/api/v10/guilds/555{suffix}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(value))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v10/users/99"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id":"99","username":"Former member"})),
        )
        .mount(&server)
        .await;
    for p in [
        "emojis/21.gif",
        "stickers/22.png",
        "icons/555/hash.png",
        "banners/555/a_bannerhash.gif",
        "splashes/555/splashhash.png",
        "discovery-splashes/555/discoveryhash.png",
        "role-icons/7/rolehash.png",
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/cdn/{p}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(fake_image(p)))
            .mount(&server)
            .await;
    }
    server
}
/// Real image signatures, so the import validation can tell them apart.
fn fake_image(path: &str) -> Vec<u8> {
    let firma: &[u8] = if path.ends_with(".gif") {
        b"GIF89a"
    } else {
        b"\x89PNG\r\n\x1a\n"
    };
    [firma, b"fixture-image"].concat()
}

async fn snapshot(server: &MockServer) -> ServerSnapshot {
    capture(
        "fake-token",
        Some(server.address().to_string().as_str()),
        "555",
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn snapshot_content_matches_fake_discord_without_losing_fields_or_images() {
    let server = fixture().await;
    let s = snapshot(&server).await;
    assert_eq!(s.guild["name"], "Fixture guild");
    assert_eq!(s.guild["icon"], "hash");
    assert!(STANDARD
        .decode(s.guild["icon_image"]["data"].as_str().unwrap())
        .unwrap()
        .ends_with(b"fixture-image"));
    for key in [
        "afk_channel_id",
        "system_channel_id",
        "rules_channel_id",
        "public_updates_channel_id",
        "safety_alerts_channel_id",
    ] {
        assert!(s.guild[key].is_string(), "{key}");
    }
    assert_eq!(s.guild["banner"], "a_bannerhash");
    assert_eq!(s.guild["splash"], "splashhash");
    assert_eq!(s.guild["discovery_splash"], "discoveryhash");
    assert_eq!(s.guild["premium_progress_bar_enabled"], true);
    assert_eq!(s.guild["widget_enabled"], true);
    assert_eq!(s.guild["widget_channel_id"], "11");
    for field in ["banner_image", "splash_image", "discovery_splash_image"] {
        assert!(STANDARD
            .decode(s.guild[field]["data"].as_str().unwrap())
            .unwrap()
            .ends_with(b"fixture-image"));
    }
    assert_eq!(s.guild["banner_image"]["format"], "gif");
    assert_eq!(s.guild["verification_level"], 3);
    assert_eq!(s.guild["default_message_notifications"], 1);
    assert_eq!(
        s.roles[1],
        json!({"id":"7","name":"Fixture role","color":12345,"permissions":"8","position":1,"hoist":true,"mentionable":true,"icon":"rolehash","icon_image":{"encoding":"base64","format":"png","data":STANDARD.encode(fake_image("role-icons/7/rolehash.png"))}})
    );
    assert!(s.roles[0].get("icon_image").is_none());
    assert_eq!(s.channels[0]["type"], 4);
    assert_eq!(
        s.channels[0]["permission_overwrites"][0],
        json!({"id":"7","name":"Fixture role","type":0,"allow":"1024","deny":"2048"})
    );
    assert_eq!(
        s.channels[1]["permission_overwrites"][0]["name"],
        "Nickname"
    );
    assert_eq!(
        s.channels[1]["permission_overwrites"][1]["name"],
        "Former member"
    );
    assert_eq!(s.channels[1]["parent_id"], "10");
    assert_eq!(s.channels[1]["topic"], "Fixture topic");
    assert_eq!(s.channels[1]["position"], 1);
    assert_eq!(s.channels[1]["nsfw"], true);
    assert_eq!(s.channels[1]["rate_limit_per_user"], 30);
    assert_eq!(s.channels[1]["available_tags"][0]["name"], "Tag");
    assert_eq!(s.channels[2]["bitrate"], 64000);
    assert_eq!(s.channels[2]["user_limit"], 5);
    assert_eq!(
        s.members[0],
        json!({"id":"9","name":"Nickname","username":"Fixture user","roles":[{"id":"7","name":"Fixture role"}]})
    );
    assert_eq!(s.emojis[0]["name"], "Fixture emoji");
    assert_eq!(s.stickers[0]["name"], "Fixture sticker");
    for asset in [&s.emojis[0], &s.stickers[0]] {
        assert!(STANDARD
            .decode(asset["image"]["data"].as_str().unwrap())
            .unwrap()
            .ends_with(b"fixture-image"));
    }
    assert_eq!(
        s.webhooks[0],
        json!({"id":"31","name":"Fixture hook","channel_id":"11","channel_name":"Forum"})
    );
    assert_eq!(s.bans[0]["reason"], "Fixture reason");
    assert_eq!(s.bans[0]["user"]["id"], "32");
    assert!(!serde_json::to_string(&s)
        .unwrap()
        .contains("must-not-be-exported"));
    validate(&s, "555").unwrap();
}

#[tokio::test]
async fn sqlite_scope_rename_export_import_delete_and_corruption() {
    let server = fixture().await;
    let s = snapshot(&server).await;
    let p = database().await;
    let id = store(&p, "bot-a", "555", &s, false).await.unwrap();
    let rows = listing(&p, "bot-a", "555").await.unwrap();
    assert_eq!(rows["snapshots"][0]["summary"]["channels"], 3);
    assert_eq!(rows["snapshots"][0]["summary"]["roles"], 2);
    assert_eq!(
        rows["snapshots"][0]["size_bytes"],
        serde_json::to_vec(&s).unwrap().len()
    );
    assert_eq!(rows["schedule"]["keep_last"], 7);
    assert!(listing(&p, "other-bot", "555").await.unwrap()["snapshots"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(load(&p, "bot-a", "other-guild", &id).await.is_err());
    assert!(load(&p, "other-bot", "555", &id).await.is_err());
    manage(
        &p,
        "bot-a",
        "555",
        SnapshotRequest::Rename {
            id: id.clone(),
            name: "Renamed snapshot".into(),
        },
    )
    .await
    .unwrap();
    let path = std::env::temp_dir()
        .join(format!("{}.argussnapshot", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned();
    manage(
        &p,
        "bot-a",
        "555",
        SnapshotRequest::Export {
            id: id.clone(),
            path: path.clone(),
        },
    )
    .await
    .unwrap();
    let exported: Value = serde_json::from_slice(&tokio::fs::read(&path).await.unwrap()).unwrap();
    assert_eq!(exported["name"], "Renamed snapshot");
    assert_eq!(exported["guild"], s.guild);
    assert_eq!(exported["roles"], serde_json::to_value(&s.roles).unwrap());
    // El archivo debe ser autónomo incluso cuando Discord ya no esté disponible.
    drop(server);
    assert_eq!(
        exported["channels"],
        serde_json::to_value(&s.channels).unwrap()
    );
    assert_eq!(exported["emojis"], serde_json::to_value(&s.emojis).unwrap());
    let empty = database().await;
    manage(
        &empty,
        "new-bot",
        "555",
        SnapshotRequest::Import { path: path.clone() },
    )
    .await
    .unwrap();
    let imported = listing(&empty, "new-bot", "555").await.unwrap();
    let restored = load(
        &empty,
        "new-bot",
        "555",
        imported["snapshots"][0]["id"].as_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), exported);
    assert!(manage(
        &p,
        "bot-a",
        "other-guild",
        SnapshotRequest::Import { path: path.clone() }
    )
    .await
    .is_err());
    tokio::fs::write(&path, b"{broken file}").await.unwrap();
    assert!(manage(
        &p,
        "bot-a",
        "555",
        SnapshotRequest::Import { path: path.clone() }
    )
    .await
    .is_err());
    assert_eq!(
        listing(&p, "bot-a", "555").await.unwrap()["snapshots"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    tokio::fs::remove_file(path).await.unwrap();
    manage(
        &p,
        "other-bot",
        "555",
        SnapshotRequest::Delete { id: id.clone() },
    )
    .await
    .unwrap();
    assert!(load(&p, "bot-a", "555", &id).await.is_ok());
    manage(
        &p,
        "bot-a",
        "555",
        SnapshotRequest::Delete { id: id.clone() },
    )
    .await
    .unwrap();
    assert!(load(&p, "bot-a", "555", &id).await.is_err());
}

#[tokio::test]
async fn automatic_retention_is_per_scope_and_preserves_manual_snapshots() {
    let server = fixture().await;
    let mut s = snapshot(&server).await;
    let p = database().await;
    let manual = store(&p, "bot-a", "555", &s, false).await.unwrap();
    let other = store(&p, "bot-b", "555", &s, true).await.unwrap();
    manage(
        &p,
        "bot-a",
        "555",
        SnapshotRequest::Schedule {
            enabled: true,
            keep_last: 7,
        },
    )
    .await
    .unwrap();
    for n in 1..=9 {
        s.created_at = format!("2026-01-{n:02}T00:00:00Z");
        finish_automatic(&p, "bot-a", "555", &s, 7).await.unwrap();
    }
    let rows = listing(&p, "bot-a", "555").await.unwrap();
    assert_eq!(rows["snapshots"].as_array().unwrap().len(), 8);
    assert!(load(&p, "bot-a", "555", &manual).await.is_ok());
    assert!(load(&p, "bot-b", "555", &other).await.is_ok());
    let due:i64 = sqlx::query_scalar("SELECT COUNT(*) FROM server_snapshot_schedules WHERE enabled=1 AND (last_success IS NULL OR last_success < ?)").bind(Utc::now().date_naive().to_string()).fetch_one(&p).await.unwrap();
    assert_eq!(due, 0);
    assert!(manage(
        &p,
        "bot-a",
        "555",
        SnapshotRequest::Schedule {
            enabled: true,
            keep_last: 0
        }
    )
    .await
    .is_err());
    manage(
        &p,
        "bot-a",
        "555",
        SnapshotRequest::Schedule {
            enabled: false,
            keep_last: 3,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        listing(&p, "bot-a", "555").await.unwrap()["schedule"]["enabled"],
        false
    );
}

#[tokio::test]
async fn member_and_ban_pagination_and_permission_errors() {
    let server = fixture().await;
    let members: Vec<Value> = (1..=1000)
        .map(|n| json!({"user":{"id":n.to_string(),"username":"Paged user"},"roles":["7"]}))
        .collect();
    let bans: Vec<Value> = (1..=1000)
        .map(|n| json!({"user":{"id":n.to_string(),"username":"Paged ban"},"reason":null}))
        .collect();
    for (route, page) in [("members", members), ("bans", bans)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v10/guilds/555/{route}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(page))
            .with_priority(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v10/guilds/555/{route}")))
            .and(query_param("after", "1000"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(if route == "members" {
                    json!([{"user":{"id":"1001","username":"Last member"},"roles":[]}])
                } else {
                    json!([{"user":{"id":"1001","username":"Last ban"},"reason":"last"}])
                }),
            )
            .with_priority(1)
            .expect(if route == "members" { 2 } else { 1 })
            .mount(&server)
            .await;
    }
    let s = snapshot(&server).await;
    assert_eq!(s.members.len(), 1001);
    assert_eq!(s.bans.len(), 1001);
    assert_eq!(s.members[1000]["name"], "Last member");
    assert_eq!(s.bans[1000]["reason"], "last");
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555/webhooks"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"Missing Permissions"})),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    assert!(
        capture("fake-token", Some(&server.address().to_string()), "555")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn missing_or_corrupt_embedded_images_are_rejected_without_inserting() {
    let server = fixture().await;
    let s = snapshot(&server).await;
    let original = serde_json::to_value(&s).unwrap();
    let pool = database().await;
    for pointer in [
        "/guild/icon_image",
        "/guild/banner_image",
        "/guild/splash_image",
        "/guild/discovery_splash_image",
        "/roles/1/icon_image",
    ] {
        for replacement in [
            Value::Null,
            json!({"encoding":"base64","format":"png","data":"%%%"}),
            // Valid base64, but not a PNG: a corrupt image must be rejected too.
            json!({"encoding":"base64","format":"png","data":STANDARD.encode(b"fixture-image")}),
            json!({"encoding":"base64","format":"gif","data":STANDARD.encode(b"\x89PNG\r\n\x1a\n")}),
        ] {
            let mut invalid = original.clone();
            *invalid.pointer_mut(pointer).unwrap() = replacement;
            let invalid: ServerSnapshot = serde_json::from_value(invalid).unwrap();
            assert!(
                store(&pool, "bot-a", "555", &invalid, false).await.is_err(),
                "{pointer}"
            );
        }
    }
    assert!(listing(&pool, "bot-a", "555").await.unwrap()["snapshots"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn unavailable_guild_or_role_image_aborts_capture() {
    for asset in [
        "role-icons/7/rolehash.png",
        "banners/555/a_bannerhash.gif",
        "splashes/555/splashhash.png",
        "discovery-splashes/555/discoveryhash.png",
    ] {
        let server = fixture().await;
        Mock::given(method("GET"))
            .and(path(format!("/cdn/{asset}")))
            .respond_with(ResponseTemplate::new(404))
            .with_priority(1)
            .expect(1)
            .mount(&server)
            .await;
        assert!(
            capture("fake-token", Some(&server.address().to_string()), "555")
                .await
                .is_err(),
            "{asset}"
        );
    }
}

#[tokio::test]
async fn guild_and_roles_without_images_do_not_request_cdn_assets() {
    let server = fixture().await;
    let mut s = snapshot(&server).await;
    for field in ["icon", "banner", "splash", "discovery_splash"] {
        s.guild[field] = Value::Null;
        s.guild
            .as_object_mut()
            .unwrap()
            .remove(&format!("{field}_image"));
    }
    for role in &mut s.roles {
        role["icon"] = Value::Null;
        role.as_object_mut().unwrap().remove("icon_image");
    }
    // Una fixture nueva permite comprobar que no se descargan hashes nulos.
    let server = fixture().await;
    for (suffix, body) in [("", s.guild.clone()), ("/roles", json!(s.roles))] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v10/guilds/555{suffix}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .with_priority(1)
            .mount(&server)
            .await;
    }
    let captured = snapshot(&server).await;
    validate(&captured, "555").unwrap();
    assert!(captured.guild.get("icon_image").is_none());
    assert!(captured.roles[1].get("icon_image").is_none());
    for request in server.received_requests().await.unwrap() {
        let path = request.url.path();
        assert!(
            !path.starts_with("/cdn/")
                || path.starts_with("/cdn/emojis/")
                || path.starts_with("/cdn/stickers/"),
            "{path}"
        );
    }
}
