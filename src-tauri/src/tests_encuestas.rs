//! Tests wiremock de las encuestas nativas.

use crate::encuestas::*;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn r(texto: &str) -> RespuestaEncuesta {
    RespuestaEncuesta {
        texto: texto.into(),
        emoji: None,
    }
}

#[tokio::test]
async fn crea_la_encuesta_con_el_json_de_poll() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/7/messages"))
        .and(header("authorization", "Bot tk"))
        .and(body_json(json!({"poll": {
            "question": {"text": "¿Pizza?"},
            "answers": [
                {"poll_media": {"text": "Sí", "emoji": {"name": "🍕"}}},
                {"poll_media": {"text": "No", "emoji": {"name": "party", "id": "123"}}},
                {"poll_media": {"text": "Quizá"}}
            ],
            "duration": 48,
            "allow_multiselect": true,
            "layout_type": 1
        }})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "99"})))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let respuestas = vec![
        RespuestaEncuesta {
            texto: "Sí".into(),
            emoji: Some("🍕".into()),
        },
        RespuestaEncuesta {
            texto: "No".into(),
            emoji: Some("party:123".into()),
        },
        r("Quizá"),
    ];
    let id = crear("tk", Some(&addr), "7", "¿Pizza?", &respuestas, 48, true)
        .await
        .unwrap();
    assert_eq!(id, "99");
}

#[tokio::test]
async fn los_limites_se_rechazan_sin_llamar_a_discord() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "1"})))
        .expect(0)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let dos = vec![r("a"), r("b")];
    let largo = |n: usize| "x".repeat(n);

    let e = crear("tk", Some(&addr), "7", &largo(301), &dos, 24, false).await;
    assert_eq!(
        e.unwrap_err(),
        "The poll question is too long (the maximum is 300 characters)"
    );
    let e = crear("tk", Some(&addr), "7", "?", &[r("a")], 24, false).await;
    assert_eq!(e.unwrap_err(), "A poll needs between 2 and 10 answers");
    let once: Vec<_> = (0..11).map(|i| r(&i.to_string())).collect();
    let e = crear("tk", Some(&addr), "7", "?", &once, 24, false).await;
    assert_eq!(e.unwrap_err(), "A poll needs between 2 and 10 answers");
    let e = crear(
        "tk",
        Some(&addr),
        "7",
        "?",
        &[r(&largo(56)), r("b")],
        24,
        false,
    )
    .await;
    assert_eq!(
        e.unwrap_err(),
        "A poll answer is too long (the maximum is 55 characters)"
    );
    for horas in [0, 769] {
        let e = crear("tk", Some(&addr), "7", "?", &dos, horas, false).await;
        assert_eq!(
            e.unwrap_err(),
            "The poll duration must be between 1 and 768 hours"
        );
    }
    let mal = vec![
        RespuestaEncuesta {
            texto: "a".into(),
            emoji: Some("nope".into()),
        },
        r("b"),
    ];
    assert!(crear("tk", Some(&addr), "7", "?", &mal, 24, false)
        .await
        .is_err());
}

#[test]
fn los_limites_exactos_se_aceptan() {
    let diez: Vec<_> = (0..10).map(|_| r(&"x".repeat(55))).collect();
    let p = construir_poll(&"q".repeat(300), &diez, 768, false).unwrap();
    assert_eq!(p["duration"], 768);
    assert!(construir_poll("?", &[r("a"), r("b")], 1, false).is_ok());
}

#[tokio::test]
async fn resultados_devuelve_los_recuentos_por_respuesta() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/7/messages/99"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "99", "channel_id": "7", "content": "",
            "timestamp": "2026-10-27T10:00:00.000000+00:00",
            "edited_timestamp": null, "tts": false, "mention_everyone": false,
            "mentions": [], "mention_roles": [], "attachments": [], "embeds": [],
            "pinned": false, "type": 0,
            "author": {"id": "1", "username": "bot", "discriminator": "0", "avatar": null},
            "poll": {
                "question": {"text": "¿Pizza?"},
                "answers": [
                    {"answer_id": 1, "poll_media": {"text": "Sí", "emoji": {"name": "🍕"}}},
                    {"answer_id": 2, "poll_media": {"text": "No"}},
                    {"answer_id": 3, "poll_media": {"text": "Quizá"}}
                ],
                "allow_multiselect": false,
                "expiry": "2030-01-01T00:00:00.000000+00:00",
                "results": {"is_finalized": false, "answer_counts": [
                    {"id": 1, "count": 5, "me_voted": false},
                    {"id": 2, "count": 2, "me_voted": false}
                ]}
            }
        })))
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let v = resultados("tk", Some(&addr), "7", "99").await.unwrap();
    assert_eq!(v["pregunta"], "¿Pizza?");
    assert_eq!(v["total_votos"], 7);
    assert_eq!(v["terminada"], false);
    let rs = v["respuestas"].as_array().unwrap();
    assert_eq!(rs[0]["texto"], "Sí");
    assert_eq!(rs[0]["emoji"], "🍕");
    assert_eq!(rs[0]["votos"], 5);
    assert_eq!(rs[1]["votos"], 2);
    assert_eq!(rs[2]["votos"], 0);
}

#[tokio::test]
async fn resultados_de_un_mensaje_sin_encuesta_da_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/7/messages/5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "5", "channel_id": "7", "content": "hola",
            "timestamp": "2026-10-27T10:00:00.000000+00:00",
            "edited_timestamp": null, "tts": false, "mention_everyone": false,
            "mentions": [], "mention_roles": [], "attachments": [], "embeds": [],
            "pinned": false, "type": 0,
            "author": {"id": "1", "username": "bot", "discriminator": "0", "avatar": null}
        })))
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let e = resultados("tk", Some(&addr), "7", "5").await.unwrap_err();
    assert_eq!(e, "That message has no poll");
}

#[tokio::test]
async fn cerrar_llama_a_expire_con_el_token_del_bot() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/7/polls/99/expire"))
        .and(header("authorization", "Bot tk"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "99"})))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    cerrar("tk", Some(&addr), "7", "99").await.unwrap();
}

#[tokio::test]
async fn cerrar_propaga_el_rechazo_de_discord_y_valida_los_ids() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/7/polls/98/expire"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"message": "Missing Access", "code": 50001})),
        )
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let e = cerrar("tk", Some(&addr), "7", "98").await.unwrap_err();
    assert!(e.contains("403") && e.contains("Missing Access"), "{e}");
    assert!(cerrar("tk", Some(&addr), "7", "abc").await.is_err());
}
