//! Tests wiremock de los eventos programados del servidor.

use crate::eventos::*;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const INICIO: &str = "2099-05-01T20:00:00+02:00";
const FIN: &str = "2099-05-01T22:00:00+02:00";

fn base() -> DatosEvento {
    DatosEvento {
        nombre: Some("Torneo".into()),
        descripcion: Some("Final".into()),
        inicio: Some(INICIO.into()),
        fin: Some(FIN.into()),
        ..Default::default()
    }
}

async fn canal(server: &MockServer, id: &str, tipo: u8) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v10/channels/{id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": id, "type": tipo})))
        .mount(server)
        .await;
}

#[tokio::test]
async fn lista_los_eventos() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/5/scheduled-events"))
        .and(header("authorization", "Bot tk"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "id": "9", "name": "Torneo", "entity_type": 3, "status": 1,
            "scheduled_start_time": "2099-05-01T18:00:00+00:00",
            "entity_metadata": {"location": "Plaza"}
        }])))
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let l = listar("tk", Some(&addr), "5").await.unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0]["tipo"], "external");
    assert_eq!(l[0]["estado"], "scheduled");
    assert_eq!(l[0]["lugar"], "Plaza");
}

#[tokio::test]
async fn crea_un_evento_externo_con_entity_type_3() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/guilds/5/scheduled-events"))
        .and(body_json(json!({
            "name": "Torneo", "description": "Final", "privacy_level": 2,
            "scheduled_start_time": "2099-05-01T18:00:00Z",
            "scheduled_end_time": "2099-05-01T20:00:00Z",
            "entity_type": 3, "entity_metadata": {"location": "Plaza"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "77"})))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let mut d = base();
    d.lugar = Some("Plaza".into());
    assert_eq!(crear("tk", Some(&addr), "5", &d).await.unwrap(), "77");
}

#[tokio::test]
async fn crea_un_evento_de_voz_con_entity_type_2_y_uno_de_escenario_con_1() {
    let server = MockServer::start().await;
    canal(&server, "30", 2).await;
    canal(&server, "31", 13).await;
    canal(&server, "32", 0).await;
    Mock::given(method("POST"))
        .and(path("/api/v10/guilds/5/scheduled-events"))
        .and(body_json(json!({
            "name": "Torneo", "description": "Final", "privacy_level": 2,
            "scheduled_start_time": "2099-05-01T18:00:00Z",
            "scheduled_end_time": "2099-05-01T20:00:00Z",
            "entity_type": 2, "channel_id": "30"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "78"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v10/guilds/5/scheduled-events"))
        .and(body_json(json!({
            "name": "Torneo", "description": "Final", "privacy_level": 2,
            "scheduled_start_time": "2099-05-01T18:00:00Z",
            "scheduled_end_time": "2099-05-01T20:00:00Z",
            "entity_type": 1, "channel_id": "31"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "79"})))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let mut d = base();
    d.canal_id = Some("30".into());
    assert_eq!(crear("tk", Some(&addr), "5", &d).await.unwrap(), "78");
    d.canal_id = Some("31".into());
    assert_eq!(crear("tk", Some(&addr), "5", &d).await.unwrap(), "79");
    d.canal_id = Some("32".into());
    assert_eq!(
        crear("tk", Some(&addr), "5", &d).await.unwrap_err(),
        "The event channel must be a voice or stage channel"
    );
}

#[tokio::test]
async fn rechaza_fechas_invalidas_sin_llamar_a_discord() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "1"})))
        .expect(0)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let mut d = base();
    d.lugar = Some("Plaza".into());

    let mut pasado = d.clone();
    pasado.inicio = Some("2001-01-01T10:00:00+00:00".into());
    pasado.fin = Some("2001-01-01T11:00:00+00:00".into());
    assert_eq!(
        crear("tk", Some(&addr), "5", &pasado).await.unwrap_err(),
        "The event start must be in the future"
    );

    let mut fin_antes = d.clone();
    fin_antes.fin = Some("2099-05-01T10:00:00+02:00".into());
    assert_eq!(
        crear("tk", Some(&addr), "5", &fin_antes).await.unwrap_err(),
        "The event end must be after its start"
    );

    let mut sin_zona = d.clone();
    sin_zona.inicio = Some("2099-05-01T20:00:00".into());
    assert!(crear("tk", Some(&addr), "5", &sin_zona)
        .await
        .unwrap_err()
        .starts_with("Invalid start date"));

    let mut sin_fin = d.clone();
    sin_fin.fin = None;
    assert_eq!(
        crear("tk", Some(&addr), "5", &sin_fin).await.unwrap_err(),
        "An event with an external location needs an end time"
    );

    let mut sin_lugar = base();
    sin_lugar.lugar = None;
    assert_eq!(
        crear("tk", Some(&addr), "5", &sin_lugar).await.unwrap_err(),
        "Provide either an external location or a voice/stage channel"
    );
}

#[tokio::test]
async fn editar_envia_solo_lo_indicado() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/5/scheduled-events/9"))
        .and(body_json(json!({"name": "Nuevo"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "9"})))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let d = DatosEvento {
        nombre: Some("Nuevo".into()),
        ..Default::default()
    };
    editar("tk", Some(&addr), "5", "9", &d).await.unwrap();
    assert_eq!(
        editar("tk", Some(&addr), "5", "9", &DatosEvento::default())
            .await
            .unwrap_err(),
        "Provide at least one field to change"
    );
}

#[tokio::test]
async fn editar_compara_el_nuevo_fin_con_el_inicio_actual() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/5/scheduled-events/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "9", "scheduled_start_time": "2099-05-01T18:00:00+00:00"
        })))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "9"})))
        .expect(0)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let d = DatosEvento {
        fin: Some("2099-05-01T17:00:00+00:00".into()),
        ..Default::default()
    };
    assert_eq!(
        editar("tk", Some(&addr), "5", "9", &d).await.unwrap_err(),
        "The event end must be after its start"
    );
    let pasado = DatosEvento {
        inicio: Some("2001-01-01T10:00:00+00:00".into()),
        ..Default::default()
    };
    assert_eq!(
        editar("tk", Some(&addr), "5", "9", &pasado)
            .await
            .unwrap_err(),
        "The event start must be in the future"
    );
}

#[tokio::test]
async fn editar_a_lugar_externo_usa_entity_type_3() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/guilds/5/scheduled-events/9"))
        .and(body_json(json!({
            "entity_type": 3, "channel_id": null, "entity_metadata": {"location": "Casa"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "9"})))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    let d = DatosEvento {
        lugar: Some("Casa".into()),
        ..Default::default()
    };
    editar("tk", Some(&addr), "5", "9", &d).await.unwrap();
}

#[tokio::test]
async fn cancelar_borra_el_evento_aunque_la_respuesta_venga_vacia() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v10/guilds/5/scheduled-events/9"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let addr = server.address().to_string();
    cancelar("tk", Some(&addr), "5", "9").await.unwrap();
    assert_eq!(
        cancelar("tk", Some(&addr), "5", "x").await.unwrap_err(),
        "Invalid event ID"
    );
}
