//! Tests del MCP actualizado: herramientas de plantillas y correcciones,
//! instrucciones y lista de herramientas con y sin conectores.
use super::*;
use crate::agregador::{Aggregator, BoxFut, Limits, Upstream};
use crate::conectores::ConnectorSpec;
use crate::db;
use wiremock::MockServer;

const PLAN: &str = "# Archive\n## Rol: Keeper | color: #112233\n## Categoría: Archive | ver: Keeper\n- texto: notes\n- voz: room";

struct Falso;
impl Upstream for Falso {
    fn list_tools(&self) -> BoxFut<'_, anyhow::Result<Vec<String>>> {
        Box::pin(async { Ok(vec!["search".to_string(), "write_all".to_string()]) })
    }
    fn call(
        &self,
        tool: &str,
        _a: serde_json::Value,
    ) -> BoxFut<'_, anyhow::Result<serde_json::Value>> {
        let t = tool.to_string();
        Box::pin(async move { Ok(json!({ "tool": t })) })
    }
}
static CAT: &[ConnectorSpec] = &[ConnectorSpec {
    id: "svc",
    accepts_credential: true,
    read_tools: &["search"],
}];

async fn mcp() -> (std::path::PathBuf, ArgusMcp, MockServer) {
    let dir = std::env::temp_dir().join(format!("argus-mcp-nuevo-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let pool = db::abrir_bd(&dir.join("argus.db")).await.unwrap();
    let server = MockServer::start().await;
    let m = ArgusMcp::con_contexto(pool, Some(server.address().to_string()), Default::default());
    (dir, m, server)
}

fn nombres(tools: &[rmcp::model::Tool]) -> Vec<String> {
    tools.iter().map(|t| t.name.to_string()).collect()
}

#[test]
fn las_herramientas_nuevas_estan_registradas_y_clasificadas() {
    let reg = nombres(&ArgusMcp::tool_router().list_all());
    for n in ["proponer_plantilla", "proponer_correccion"] {
        assert!(reg.contains(&n.to_string()), "falta {n}");
        assert!(HERRAMIENTAS_LECTURA.contains(&n));
    }
    for n in [
        "guardar_plantilla_desde_plan",
        "guardar_plantilla_desde_snapshot",
    ] {
        assert!(reg.contains(&n.to_string()), "falta {n}");
        assert!(HERRAMIENTAS_ESCRITURA.contains(&n));
        assert!(esquema_args(n).is_some());
    }
    assert!(reg.contains(&"listar_plantillas".to_string()));
}

#[test]
fn las_instrucciones_describen_las_funciones_nuevas() {
    let i = INSTRUCCIONES.to_lowercase();
    for k in [
        "verif",
        "history",
        "diagnostic",
        "template",
        "audit",
        "correction",
        "connector",
        "proposal",
    ] {
        assert!(i.contains(k), "falta {k}");
    }
    // Solo cita herramientas que existen.
    let reg = nombres(&ArgusMcp::tool_router().list_all());
    for n in [
        "listar_plantillas",
        "consultar_plantilla",
        "proponer_plantilla",
        "guardar_plantilla_desde_plan",
        "guardar_plantilla_desde_snapshot",
        "informe_permisos",
        "proponer_correccion",
        "estado",
    ] {
        assert!(
            INSTRUCCIONES.contains(n) && reg.contains(&n.to_string()),
            "{n}"
        );
    }
}

#[test]
fn toda_herramienta_registrada_esta_en_la_documentacion() {
    let doc = include_str!("../../docs/MCP.md");
    for n in nombres(&ArgusMcp::tool_router().list_all()) {
        assert!(
            doc.contains(&format!("`{n}`")),
            "docs/MCP.md no documenta la herramienta {n}"
        );
    }
    assert!(doc.contains("skills/argus/SKILL.md"));
}

#[tokio::test]
async fn guardar_plantilla_no_escribe_sin_aprobacion() {
    let (dir, m, server) = mcp().await;
    let err = m
        .guardar_plantilla_desde_plan(Parameters(GuardarPlantillaPlanParams {
            plan: PLAN.into(),
            nombre: "T".into(),
            descripcion: String::new(),
            etiquetas: String::new(),
        }))
        .await
        .unwrap_err();
    assert!(err.contains("requires approval"), "{err}");
    let err = m
        .guardar_plantilla_desde_snapshot(Parameters(GuardarPlantillaSnapshotParams {
            snapshot_id: "x".into(),
            nombre: "T".into(),
            descripcion: String::new(),
            etiquetas: String::new(),
        }))
        .await
        .unwrap_err();
    assert!(err.contains("requires approval"), "{err}");
    assert!(db::list_templates(&m.pool).await.unwrap().is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
    // Con aprobación (propuesta ejecutada por ARGUS) sí se guarda.
    validar_accion(
        "guardar_plantilla_desde_plan",
        &json!({"plan": PLAN, "nombre": "T"}),
    )
    .unwrap();
    m.despachar_accion(
        "guardar_plantilla_desde_plan",
        json!({"plan": PLAN, "nombre": "T"}),
    )
    .await
    .unwrap();
    assert_eq!(db::list_templates(&m.pool).await.unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn proponer_plantilla_solo_prepara() {
    let (dir, m, server) = mcp().await;
    m.despachar_accion(
        "guardar_plantilla_desde_plan",
        json!({"plan": PLAN, "nombre": "T"}),
    )
    .await
    .unwrap();
    let id = db::list_templates(&m.pool).await.unwrap()[0].id.clone();
    let r: serde_json::Value = serde_json::from_str(
        &m.proponer_plantilla(Parameters(PlantillaParams { id }))
            .await
            .unwrap(),
    )
    .unwrap();
    let acciones = r["propuesta"]["acciones"].as_array().unwrap();
    assert!(!acciones.is_empty());
    for a in acciones {
        let h = a["herramienta"].as_str().unwrap();
        assert!(HERRAMIENTAS_ESCRITURA.contains(&h), "{h}");
    }
    assert!(m
        .proponer_plantilla(Parameters(PlantillaParams { id: "nope".into() }))
        .await
        .is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(db::list_templates(&m.pool).await.unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn proponer_correccion_sin_bot_no_escribe() {
    let (dir, m, server) = mcp().await;
    let err = m
        .proponer_correccion(Parameters(ProponerCorreccionParams {
            guild_id: Some("555".into()),
            finding: json!({"tipo": "role_grant"}),
        }))
        .await
        .unwrap_err();
    assert!(!err.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn la_lista_incluye_conectores_solo_si_estan_habilitados() {
    let (dir, m, _s) = mcp().await;
    let base = nombres(&m.herramientas_publicas().await);
    assert!(base.iter().all(|n| !n.contains("__")));
    assert_eq!(base.len(), ArgusMcp::tool_router().list_all().len());

    let capa = Aggregator::new(
        CAT,
        &["svc".to_string()],
        vec![("svc", Arc::new(Falso), Limits::default())],
    );
    let con = m.clone().con_conectores(Arc::new(capa));
    let lista = nombres(&con.herramientas_publicas().await);
    assert!(lista.contains(&"svc__search".to_string()));
    assert!(!lista.iter().any(|n| n.contains("write_all")));
    assert_eq!(lista.len(), base.len() + 1);

    // Sin ningún conector habilitado vuelve la lista base.
    let vacia = Aggregator::new(CAT, &[], vec![("svc", Arc::new(Falso), Limits::default())]);
    let sin = m.clone().con_conectores(Arc::new(vacia));
    assert_eq!(
        nombres(&sin.herramientas_publicas().await).len(),
        base.len()
    );
    let _ = std::fs::remove_dir_all(dir);
}
