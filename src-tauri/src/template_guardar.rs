//! Guardar plantillas propias desde un snapshot o desde el plan del asistente.
//!
//! Ambas fuentes se convierten al documento portátil de `template_transfer`:
//! claves propias en vez de IDs de Discord y nada de miembros, mensajes ni
//! tokens. Lo que el formato no admite se omite y se informa, sin rechazar todo.
use crate::db;
use crate::template_transfer::{parse, Channel, Document, Overwrite, Role};
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::HashMap;
use tauri::AppHandle;

/// Plantilla guardada y lo que no cupo en ella.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Guardada {
    pub id: String,
    pub omitidos: Vec<String>,
}

fn bits(v: &Value) -> Option<u64> {
    v.as_str()
        .and_then(|x| x.parse().ok())
        .or_else(|| v.as_u64())
}

fn tipo_snapshot(t: Option<u64>) -> Option<&'static str> {
    match t {
        Some(0) => Some("text"),
        Some(2) => Some("voice"),
        Some(4) => Some("category"),
        Some(5) => Some("announcement"),
        Some(15) => Some("forum"),
        _ => None,
    }
}

/// Documento desde un snapshot. Los roles gestionados (bots, integraciones)
/// no se pueden crear y los permisos de miembros no son portátiles.
pub fn desde_snapshot(
    s: &crate::snapshots::ServerSnapshot,
    nombre: String,
    descripcion: String,
) -> Result<(Document, Vec<String>), String> {
    let mut omitidos = Vec::new();
    let mut claves_rol = HashMap::new();
    let mut roles = Vec::new();
    for r in &s.roles {
        let Some(id) = r["id"].as_str() else { continue };
        let everyone = id == s.guild_id;
        let nombre_rol = r["name"].as_str().unwrap_or_default();
        if !everyone && r["managed"] == true {
            omitidos.push(format!("Managed role «{nombre_rol}»"));
            continue;
        }
        let clave = format!("role_{}", roles.len());
        claves_rol.insert(id.to_owned(), clave.clone());
        roles.push(Role {
            key: clave,
            name: if everyone {
                "@everyone".into()
            } else {
                nombre_rol.into()
            },
            color: r["color"].as_u64().unwrap_or(0).min(0xFF_FFFF) as u32,
            permissions: bits(&r["permissions"]).unwrap_or(0) & ((1u64 << 53) - 1),
        });
    }
    let mut claves_canal = HashMap::new();
    for c in &s.channels {
        let (Some(id), Some(_)) = (c["id"].as_str(), tipo_snapshot(c["type"].as_u64())) else {
            omitidos.push(format!(
                "Channel «{}» (type not supported in templates)",
                c["name"].as_str().unwrap_or("?")
            ));
            continue;
        };
        claves_canal.insert(id.to_owned(), format!("channel_{}", claves_canal.len()));
    }
    let mut canales = Vec::new();
    let mut miembros = 0usize;
    for c in &s.channels {
        let Some(clave) = c["id"].as_str().and_then(|id| claves_canal.get(id)) else {
            continue;
        };
        let tipo = tipo_snapshot(c["type"].as_u64()).unwrap_or("text");
        let mut overwrites = Vec::new();
        for o in c["permission_overwrites"].as_array().into_iter().flatten() {
            if o["type"].as_u64() != Some(0) {
                miembros += 1;
                continue;
            }
            let Some(rol) = o["id"].as_str().and_then(|id| claves_rol.get(id)) else {
                continue;
            };
            overwrites.push(Overwrite {
                role: rol.clone(),
                allow: bits(&o["allow"]).unwrap_or(0),
                deny: bits(&o["deny"]).unwrap_or(0),
            });
        }
        canales.push(Channel {
            key: clave.clone(),
            name: c["name"].as_str().unwrap_or("channel").into(),
            kind: tipo.into(),
            parent: c["parent_id"]
                .as_str()
                .and_then(|p| claves_canal.get(p))
                .cloned()
                .filter(|_| tipo != "category"),
            description: c["topic"]
                .as_str()
                .filter(|t| !t.is_empty() && tipo != "voice" && tipo != "category")
                .map(str::to_owned),
            overwrites,
        });
    }
    if miembros > 0 {
        omitidos.push(format!("{miembros} member permission(s)"));
    }
    let d = Document {
        format: "argus-template".into(),
        version: 1,
        name: nombre,
        description: descripcion,
        roles,
        channels: canales,
    };
    Ok((validar(d)?, omitidos))
}

fn tipo_plan(t: &str) -> &'static str {
    match t {
        "voz" => "voice",
        "anuncios" => "announcement",
        "foro" => "forum",
        "categoria" => "category",
        _ => "text",
    }
}

/// Documento desde el texto del plan, con el mismo compilador que «Build
/// from plan» (sin servidor: todo es nuevo). Las descripciones que la IA
/// tendría que escribir no se guardan.
pub fn desde_plan(
    plan: &str,
    nombre: String,
    descripcion: String,
) -> Result<(Document, Vec<String>), String> {
    let analizado = crate::plan_estructurado::analizar(plan);
    if !analizado.tiene_estructura() {
        return Err("The plan has no roles or categories to save".into());
    }
    let comp = crate::plan_compilar::compilar(
        &analizado,
        &crate::plan_compilar::Existentes {
            roles: &[],
            canales: &[],
        },
    );
    let mut omitidos: Vec<String> = comp.avisos.clone();
    if !comp.pendientes.is_empty() {
        omitidos.push(format!(
            "{} channel description(s) the AI would write",
            comp.pendientes.len()
        ));
    }
    let mut roles = Vec::new();
    let mut claves_rol: HashMap<String, String> = HashMap::new();
    for a in comp
        .propuesta
        .acciones
        .iter()
        .filter(|a| a.herramienta == "crear_rol")
    {
        let nombre_rol = a.args["nombre"].as_str().unwrap_or_default().to_string();
        let color = match a.args.get("color") {
            Some(v) => serde_json::from_value::<crate::mcp::ColorParam>(v.clone())
                .map_err(|e| e.to_string())?
                .a_u32()?,
            None => 0,
        };
        let permisos = match a.args.get("permisos") {
            Some(v) => serde_json::from_value::<crate::mcp::PermisosParam>(v.clone())
                .map_err(|e| e.to_string())?
                .a_bits()?
                .parse()
                .unwrap_or(0),
            None => 0,
        };
        let clave = format!("role_{}", roles.len());
        claves_rol.insert(nombre_rol.to_lowercase(), clave.clone());
        roles.push(Role {
            key: clave,
            name: nombre_rol,
            color,
            permissions: permisos,
        });
    }
    let mut canales: Vec<Channel> = Vec::new();
    let mut claves_categoria: HashMap<String, String> = HashMap::new();
    for a in comp
        .propuesta
        .acciones
        .iter()
        .filter(|a| a.herramienta == "crear_canal")
    {
        let tipo = tipo_plan(a.args["tipo"].as_str().unwrap_or("texto"));
        let nombre_canal = a.args["nombre"].as_str().unwrap_or_default().to_string();
        let clave = format!("channel_{}", canales.len());
        if tipo == "category" {
            claves_categoria.insert(nombre_canal.to_lowercase(), clave.clone());
        }
        let mut overwrites = Vec::new();
        for p in a.args["permisos"].as_array().into_iter().flatten() {
            let objetivo = p["objetivo"].as_str().unwrap_or_default();
            let rol = if objetivo == "@everyone" {
                let clave = claves_rol.entry("@everyone".into()).or_insert_with(|| {
                    roles.push(Role {
                        key: "everyone".into(),
                        name: "@everyone".into(),
                        color: 0,
                        permissions: 0,
                    });
                    "everyone".into()
                });
                clave.clone()
            } else {
                let Some(clave) = claves_rol.get(&objetivo.to_lowercase()) else {
                    continue;
                };
                clave.clone()
            };
            let a_bits = |v: &Value| -> Result<u64, String> {
                if v.is_null() {
                    return Ok(0);
                }
                Ok(
                    serde_json::from_value::<crate::mcp::PermisosParam>(v.clone())
                        .map_err(|e| e.to_string())?
                        .a_bits()?
                        .parse()
                        .unwrap_or(0),
                )
            };
            overwrites.push(Overwrite {
                role: rol,
                allow: a_bits(&p["permitir"])?,
                deny: a_bits(&p["denegar"])?,
            });
        }
        canales.push(Channel {
            key: clave,
            name: nombre_canal,
            kind: tipo.into(),
            parent: a.args["categoria_id"]
                .as_str()
                .and_then(|c| claves_categoria.get(&c.to_lowercase()))
                .cloned(),
            description: a.args["tema"].as_str().map(str::to_owned),
            overwrites,
        });
    }
    let d = Document {
        format: "argus-template".into(),
        version: 1,
        name: nombre,
        description: descripcion,
        roles,
        channels: canales,
    };
    Ok((validar(d)?, omitidos))
}

fn validar(d: Document) -> Result<Document, String> {
    parse(&serde_json::to_string(&d).map_err(|e| e.to_string())?)
        .map_err(|_| "The structure does not fit a template (check names and limits)".into())
}

/// Etiquetas como las guarda la biblioteca: texto separado por comas.
fn etiquetas(tags: &str) -> String {
    tags.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

pub async fn guardar(pool: &SqlitePool, d: Document, tags: &str) -> Result<String, String> {
    let ahora = chrono::Utc::now().to_rfc3339();
    let plantilla = db::Template {
        id: uuid::Uuid::new_v4().to_string(),
        name: d.name.clone(),
        description: d.description.clone(),
        tags: etiquetas(tags),
        data: serde_json::to_string(&d).map_err(|e| e.to_string())?,
        version: 1,
        created_at: ahora.clone(),
        updated_at: ahora,
    };
    db::save_template(pool, &plantilla)
        .await
        .map_err(|e| e.to_string())?;
    Ok(plantilla.id)
}

#[tauri::command]
pub async fn crear_plantilla_desde_snapshot(
    app: AppHandle,
    bot_id: String,
    guild_id: String,
    snapshot_id: String,
    nombre: String,
    descripcion: String,
    etiquetas: String,
) -> Result<Guardada, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let snapshot = crate::snapshots::load(&pool, &bot_id, &guild_id, &snapshot_id)
        .await
        .map_err(|_| "Snapshot not found".to_string())?;
    let (d, omitidos) = desde_snapshot(&snapshot, nombre, descripcion)?;
    let id = guardar(&pool, d, &etiquetas).await?;
    Ok(Guardada { id, omitidos })
}

#[tauri::command]
pub async fn crear_plantilla_desde_plan(
    app: AppHandle,
    plan: String,
    nombre: String,
    descripcion: String,
    etiquetas: String,
) -> Result<Guardada, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let (d, omitidos) = desde_plan(&plan, nombre, descripcion)?;
    let id = guardar(&pool, d, &etiquetas).await?;
    Ok(Guardada { id, omitidos })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> crate::snapshots::ServerSnapshot {
        serde_json::from_value(json!({
            "version": 1, "guild_id": "100", "created_at": "today", "name": "source",
            "guild": {"name": "secret-guild"},
            "roles": [
                {"id": "100", "name": "@everyone", "permissions": "1024", "color": 0},
                {"id": "101", "name": "Staff", "permissions": "8", "color": 12},
                {"id": "102", "name": "SomeBot", "permissions": "8", "color": 0, "managed": true}
            ],
            "channels": [
                {"id": "200", "type": 4, "name": "Group", "permission_overwrites": []},
                {"id": "201", "type": 0, "name": "chat", "parent_id": "200", "topic": "Details",
                 "permission_overwrites": [
                    {"id": "101", "type": 0, "allow": "1024", "deny": "0"},
                    {"id": "999", "type": 1, "allow": "1", "deny": "0"},
                    {"id": "102", "type": 0, "allow": "1", "deny": "0"}
                 ]},
                {"id": "202", "type": 13, "name": "Stage", "parent_id": "200", "permission_overwrites": []},
                {"id": "203", "type": 2, "name": "Voice", "parent_id": "200", "topic": "ignored", "permission_overwrites": []}
            ],
            "members": [{"user": {"id": "999", "username": "private-member"}}],
            "emojis": [], "stickers": [], "webhooks": [{"token": "secret-token"}], "bans": []
        }))
        .unwrap()
    }

    #[test]
    fn snapshot_with_unsupported_parts_is_saved_and_reports_them() {
        let (d, omitidos) = desde_snapshot(&snapshot(), "Copy".into(), String::new()).unwrap();
        assert_eq!(
            d.channels.len(),
            3,
            "the stage channel is skipped, not fatal"
        );
        assert!(omitidos.iter().any(|o| o.contains("Stage")), "{omitidos:?}");
        assert!(
            omitidos.iter().any(|o| o.contains("SomeBot")),
            "{omitidos:?}"
        );
        assert!(
            omitidos.iter().any(|o| o.contains("1 member permission")),
            "{omitidos:?}"
        );
        let chat = d.channels.iter().find(|c| c.name == "chat").unwrap();
        let group = d.channels.iter().find(|c| c.name == "Group").unwrap();
        assert_eq!(chat.parent.as_ref(), Some(&group.key));
        assert_eq!(chat.description.as_deref(), Some("Details"));
        assert_eq!(chat.overwrites.len(), 1);
        let staff = d.roles.iter().find(|r| r.name == "Staff").unwrap();
        assert_eq!(chat.overwrites[0].role, staff.key);
        let voice = d.channels.iter().find(|c| c.name == "Voice").unwrap();
        assert_eq!(voice.description, None, "voice channels have no topic");
        let texto = serde_json::to_string(&d).unwrap();
        for prohibido in ["\"100\"", "\"101\"", "\"200\"", "999", "secret", "private"] {
            assert!(!texto.contains(prohibido), "{prohibido} leaked: {texto}");
        }
    }

    #[test]
    fn plan_becomes_a_portable_template() {
        let plan = "# Archivo\n\
            ## Rol: Archivista | color: #112233\n\
            ## Categoría: Archivo | ver: Archivista | ocultar a: Desconocido\n\
            - texto: expediente-{1..2}\n\
            - voz: sala";
        let (d, omitidos) = desde_plan(plan, "Archivo".into(), "Desc".into()).unwrap();
        assert_eq!(d.roles.iter().filter(|r| r.name == "Archivista").count(), 1);
        assert_eq!(
            d.roles
                .iter()
                .find(|r| r.name == "Archivista")
                .unwrap()
                .color,
            0x112233
        );
        let categoria = d.channels.iter().find(|c| c.kind == "category").unwrap();
        assert_eq!(categoria.name, "Archivo");
        let hijos: Vec<_> = d
            .channels
            .iter()
            .filter(|c| c.parent.as_ref() == Some(&categoria.key))
            .map(|c| (c.name.as_str(), c.kind.as_str()))
            .collect();
        assert_eq!(
            hijos,
            vec![
                ("expediente-1", "text"),
                ("expediente-2", "text"),
                ("sala", "voice")
            ]
        );
        // «ver: Archivista» cierra el canal a @everyone y lo abre al rol.
        let everyone = d.roles.iter().find(|r| r.name == "@everyone").unwrap();
        assert!(categoria
            .overwrites
            .iter()
            .any(|o| o.role == everyone.key && o.deny & (1 << 10) != 0));
        // El rol que no existe en el plan se informa, no rompe el guardado.
        assert!(
            omitidos.iter().any(|o| o.contains("Desconocido")),
            "{omitidos:?}"
        );
        assert!(desde_plan("solo texto libre", "X".into(), String::new()).is_err());
    }

    #[tokio::test]
    async fn legacy_rows_and_new_templates_coexist() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE templates (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT, data TEXT NOT NULL, version INTEGER DEFAULT 1, created_at DATETIME DEFAULT CURRENT_TIMESTAMP, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO templates (id, name, data) VALUES ('old', 'Old', '{}')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../migrations/016_template_tags.sql"))
            .execute(&pool)
            .await
            .unwrap();
        let old = db::get_template(&pool, "old").await.unwrap().unwrap();
        assert_eq!(old.description, "");
        assert_eq!(old.tags, "");
        let (d, _) = desde_snapshot(&snapshot(), "Copy".into(), "From snapshot".into()).unwrap();
        let id = guardar(&pool, d, " community ,, example ").await.unwrap();
        let nueva = db::get_template(&pool, &id).await.unwrap().unwrap();
        assert_eq!(nueva.tags, "community, example");
        assert_eq!(nueva.description, "From snapshot");
        assert!(!nueva.created_at.is_empty());
        assert_eq!(parse(&nueva.data).unwrap().name, "Copy");
        assert_eq!(db::list_templates(&pool).await.unwrap().len(), 2);
    }
}
