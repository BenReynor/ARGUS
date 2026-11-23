//! Tests wiremock de las etiquetas de foro y de los posts etiquetados.

use crate::discord::*;
use serde_json::json;
use wiremock::matchers::{body_json, body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn foro() -> serde_json::Value {
    json!({
        "id": "42", "type": 15, "name": "ideas",
        "available_tags": [
            {"id": "1", "name": "Torneo", "moderated": false, "emoji_name": "🏆"},
            {"id": "2", "name": "Ayuda", "moderated": true, "emoji_id": "99", "emoji_name": "sos"}
        ]
    })
}

fn post() -> serde_json::Value {
    json!({"id": "900", "type": 11, "parent_id": "42"})
}

async fn mock_foro(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/42"))
        .and(header("authorization", "Bot tk"))
        .respond_with(ResponseTemplate::new(200).set_body_json(foro()))
        .mount(server)
        .await;
}

async fn mock_post(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/900"))
        .respond_with(ResponseTemplate::new(200).set_body_json(post()))
        .mount(server)
        .await;
}

fn tags_n(n: usize) -> Vec<CambioEtiqueta> {
    (0..n)
        .map(|i| CambioEtiqueta {
            nombre: format!("t{i}"),
            ..Default::default()
        })
        .collect()
}

#[tokio::test]
async fn lista_las_etiquetas_del_foro() {
    let server = MockServer::start().await;
    mock_foro(&server).await;
    let addr = server.address().to_string();
    let t = get_forum_tags("tk", Some(&addr), "42").await.unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].emoji.as_deref(), Some("🏆"));
    assert_eq!(t[1].emoji.as_deref(), Some("sos:99"));
    assert!(t[1].moderated);
}

#[tokio::test]
async fn un_canal_que_no_es_foro_da_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "7", "type": 0})))
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let e = get_forum_tags("tk", Some(&addr), "7").await.unwrap_err();
    assert_eq!(e, "The channel is not a forum");
}

#[tokio::test]
async fn crea_y_renombra_etiquetas_enviando_la_lista_completa() {
    let server = MockServer::start().await;
    mock_foro(&server).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/channels/42"))
        .and(body_json(json!({"available_tags": [
            {"id": "1", "name": "Campeonato", "moderated": false, "emoji_name": "🏆"},
            {"id": "2", "name": "Ayuda", "moderated": true, "emoji_id": "99", "emoji_name": "sos"},
            {"name": "Nueva", "moderated": false, "emoji_name": "🔥"}
        ]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(foro()))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let r = edit_forum_tags(
        "tk",
        Some(&addr),
        "42",
        &[
            CambioEtiqueta {
                nombre: "torneo".into(),
                nuevo_nombre: Some("Campeonato".into()),
                emoji: None,
            },
            CambioEtiqueta {
                nombre: "Nueva".into(),
                nuevo_nombre: None,
                emoji: Some("🔥".into()),
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(r.len(), 2);
}

#[tokio::test]
async fn limites_de_etiquetas_se_rechazan_antes_de_enviar() {
    let server = MockServer::start().await;
    mock_foro(&server).await;
    let addr = server.address().to_string();
    let largo = edit_forum_tags(
        "tk",
        Some(&addr),
        "42",
        &[CambioEtiqueta {
            nombre: "x".repeat(21),
            ..Default::default()
        }],
    )
    .await
    .unwrap_err();
    assert_eq!(
        largo,
        "The tag name is too long (the maximum is 20 characters)"
    );
    // 2 existentes + 19 nuevas = 21 > 20 (no llega a enviarse ningún PATCH).
    let e = edit_forum_tags("tk", Some(&addr), "42", &tags_n(19))
        .await
        .unwrap_err();
    assert_eq!(e, "A forum can have at most 20 tags");
    let vacio = edit_forum_tags(
        "tk",
        Some(&addr),
        "42",
        &[CambioEtiqueta {
            nombre: "  ".into(),
            ..Default::default()
        }],
    )
    .await
    .unwrap_err();
    assert_eq!(vacio, "The tag name cannot be empty");
    let emoji = edit_forum_tags(
        "tk",
        Some(&addr),
        "42",
        &[CambioEtiqueta {
            nombre: "a".into(),
            emoji: Some("no es emoji".into()),
            ..Default::default()
        }],
    )
    .await;
    assert!(emoji.is_err());
    // Justo 20 caracteres y justo 20 etiquetas sí se aceptan.
    Mock::given(method("PATCH"))
        .and(path("/api/v10/channels/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(foro()))
        .expect(2)
        .mount(&server)
        .await;
    let justo = CambioEtiqueta {
        nombre: "x".repeat(20),
        ..Default::default()
    };
    edit_forum_tags("tk", Some(&addr), "42", &[justo])
        .await
        .unwrap();
    edit_forum_tags("tk", Some(&addr), "42", &tags_n(18))
        .await
        .unwrap();
}

#[tokio::test]
async fn etiqueta_un_post_por_nombre_o_id() {
    let server = MockServer::start().await;
    mock_foro(&server).await;
    mock_post(&server).await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/channels/900"))
        .and(body_json(json!({"applied_tags": ["1", "2"]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(post()))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let ids = tag_forum_post("tk", Some(&addr), "900", &["TORNEO".into(), "2".into()])
        .await
        .unwrap();
    assert_eq!(ids, vec!["1", "2"]);
}

#[tokio::test]
async fn etiqueta_inexistente_y_mas_de_cinco_dan_error() {
    let server = MockServer::start().await;
    mock_foro(&server).await;
    mock_post(&server).await;
    let addr = server.address().to_string();
    let e = tag_forum_post("tk", Some(&addr), "900", &["Nada".into()])
        .await
        .unwrap_err();
    assert_eq!(e, "Unknown forum tag: Nada");
    let seis: Vec<String> = (0..6).map(|i| i.to_string()).collect();
    let e = tag_forum_post("tk", Some(&addr), "900", &seis)
        .await
        .unwrap_err();
    assert_eq!(e, "A post can have at most 5 tags");
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "5", "type": 0})))
        .mount(&server)
        .await;
    let e = tag_forum_post("tk", Some(&addr), "5", &["Torneo".into()])
        .await
        .unwrap_err();
    assert_eq!(e, "The channel is not a forum post");
}

#[tokio::test]
async fn crear_post_con_etiquetas_envia_applied_tags() {
    let server = MockServer::start().await;
    mock_foro(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/42/threads"))
        .and(body_partial_json(json!({
            "name": "Liga",
            "message": {"content": "hola"},
            "applied_tags": ["1"]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "777", "type": 11})))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let id = create_forum_post("tk", Some(&addr), "42", "Liga", "hola", &["torneo".into()])
        .await
        .unwrap();
    assert_eq!(id, "777");
    let e = create_forum_post("tk", Some(&addr), "42", "Liga", "hola", &["zzz".into()])
        .await
        .unwrap_err();
    assert_eq!(e, "Unknown forum tag: zzz");
}

#[tokio::test]
async fn responder_en_un_post_es_enviar_mensaje_al_id_del_hilo() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/900/messages"))
        .and(body_partial_json(json!({"content": "respuesta"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "1", "channel_id": "900", "content": "respuesta",
            "timestamp": "2026-10-27T10:00:00.000000+00:00", "edited_timestamp": null,
            "tts": false, "mention_everyone": false, "mentions": [], "mention_roles": [],
            "attachments": [], "embeds": [], "pinned": false, "type": 0,
            "author": {"id": "1", "username": "bot", "discriminator": "0", "avatar": null}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    send_channel_message("tk", Some(&addr), "900", "respuesta")
        .await
        .unwrap();
}
