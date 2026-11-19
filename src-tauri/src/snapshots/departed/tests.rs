use super::*;
use serde::de::DeserializeSeed;
use wiremock::{
    matchers::{body_json, method, path},
    Mock, MockServer, ResponseTemplate,
};
fn event(guild: &str, user: &str) -> Event {
    let raw = json!({"op":0,"s":1,"t":"GUILD_MEMBER_ADD","d":{"guild_id":guild,"user":{"id":user,"username":"Returning","discriminator":"0","avatar":null},"roles":[],"joined_at":"2026-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0}}).to_string();
    let seed = twilight_model::gateway::event::GatewayEventDeserializer::from_json(&raw).unwrap();
    seed.deserialize(&mut serde_json::Deserializer::from_str(&raw))
        .unwrap()
        .into()
}
#[tokio::test]
async fn invitation_dm_failure_is_reported_and_remaining_members_are_attempted() {
    let server = MockServer::start().await;
    let base = server.address().to_string();
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/10/invites"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"code":"fixture-invite"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v10/users/@me/channels"))
        .and(body_json(json!({"recipient_id":"9"})))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50007,"message":"Cannot send messages to this user"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v10/users/@me/channels"))
        .and(body_json(json!({"recipient_id":"8"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"20","type":1})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/20/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"30","channel_id":"20","content":"https://discord.gg/fixture-invite","timestamp":"2026-01-01T00:00:00+00:00","edited_timestamp":null,"tts":false,"mention_everyone":false,"mentions":[],"mention_roles":[],"attachments":[],"embeds":[],"pinned":false,"type":0,"author":{"id":"42","username":"Bot","discriminator":"0","avatar":null}})))
        .expect(1)
        .mount(&server)
        .await;
    let out = invite(
        "fake-token",
        Some(&base),
        "10",
        &[
            json!({"id":"9","name":"First"}),
            json!({"id":"8","name":"Second"}),
        ],
    )
    .await
    .unwrap();
    assert_eq!(out["results"][0]["sent"], false);
    assert!(out["results"][0]["error"].is_string());
    assert_eq!(out["results"][1]["sent"], true);
}
#[tokio::test]
async fn simulated_join_restores_mapped_roles_logs_and_respects_scope_and_toggle() {
    let pool = super::super::tests::database().await;
    enable(
        &pool,
        "42",
        "555",
        &[json!({"id":"9","name":"Former","roles":[{"id":"7","name":"Old"}]})],
        &HashMap::from([("7".into(), "70".into())]),
    )
    .await
    .unwrap();
    assert!(enabled(&pool, "42", "555").await.unwrap());
    let server = MockServer::start().await;
    let base = server.address().to_string();
    assert!(
        on_event(&pool, "43", "fake", Some(&base), &event("555", "9"))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        on_event(&pool, "42", "fake", Some(&base), &event("556", "9"))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        on_event(&pool, "42", "fake", Some(&base), &event("555", "8"))
            .await
            .unwrap()
            .is_empty()
    );
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/555"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"owner_id":"99"})))
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/api/v10/guilds/555/roles")).respond_with(ResponseTemplate::new(200).set_body_json(json!([
        {"id":"555","name":"@everyone","permissions":"0","position":0,"color":0,"hoist":false,"managed":false,"mentionable":false,"flags":0},
        {"id":"40","name":"Bot role","permissions":"8","position":10,"color":0,"hoist":false,"managed":true,"mentionable":false,"flags":0},
        {"id":"70","name":"Restored","permissions":"0","position":1,"color":0,"hoist":false,"managed":false,"mentionable":false,"flags":0}
    ]))).mount(&server).await;
    Mock::given(method("GET")).and(path("/api/v10/guilds/555/members/42")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"user":{"id":"42","username":"Bot","discriminator":"0","avatar":null},"roles":["40"],"joined_at":"2026-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0}))).mount(&server).await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/guilds/555/members/9/roles/70"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let logs = on_event(&pool, "42", "fake", Some(&base), &event("555", "9"))
        .await
        .unwrap();
    assert_eq!(logs, vec!["Restored role 70 to returning member 9"]);
    assert_eq!(
        db::list_incidents(&pool, Some("555"), 10).await.unwrap()[0].payload,
        logs[0]
    );
    sqlx::query("UPDATE snapshot_return_roles SET enabled=0")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        on_event(&pool, "42", "fake", Some(&base), &event("555", "9"))
            .await
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn missing_members_include_identity_and_historical_roles() {
    let fixture = super::super::tests::fixture().await;
    let mut snapshot = super::super::tests::snapshot(&fixture).await;
    let former =
        json!({"id":"123","name":"Former name","roles":[{"id":"7","name":"Historical role"}]});
    snapshot.members.push(former.clone());
    Mock::given(method("GET")).and(path("/api/v10/guilds/555/members")).respond_with(ResponseTemplate::new(200).set_body_json(json!([{"user":{"id":"9","username":"Current","discriminator":"0","avatar":null},"roles":["7"],"joined_at":"2026-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0}]))).with_priority(1).mount(&fixture).await;
    let result = missing(
        &snapshot,
        "fake-token",
        Some(&fixture.address().to_string()),
    )
    .await
    .unwrap();
    assert_eq!(result, vec![former]);
}
