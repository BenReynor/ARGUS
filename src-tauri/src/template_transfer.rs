//! Formato portátil deliberadamente cerrado: ningún identificador Discord es un destino.
use crate::db;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashSet;
use tauri::AppHandle;

const MAX_BYTES: usize = 1_048_576;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub description: String,
    pub roles: Vec<Role>,
    pub channels: Vec<Channel>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub key: String,
    pub name: String,
    pub color: u32,
    pub permissions: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub parent: Option<String>,
    /// Tema del canal; los documentos antiguos no lo traen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub overwrites: Vec<Overwrite>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overwrite {
    pub role: String,
    pub allow: u64,
    pub deny: u64,
}
fn invalid() -> String {
    "Invalid or unsupported template file".into()
}
fn key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_alphabetic()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn name(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}
pub fn parse(data: &str) -> Result<Document, String> {
    if data.len() > MAX_BYTES {
        return Err(invalid());
    }
    let d: Document = serde_json::from_str(data).map_err(|_| invalid())?;
    if d.format != "argus-template"
        || d.version != 1
        || !name(&d.name, 100)
        || d.description.chars().count() > 1000
        || d.roles.len() > 250
        || d.channels.len() > 500
    {
        return Err(invalid());
    }
    let mut roles = HashSet::new();
    for r in &d.roles {
        if !key(&r.key)
            || !roles.insert(&r.key)
            || !name(&r.name, 100)
            || r.color > 0xffffff
            || r.permissions > ((1u64 << 53) - 1)
        {
            return Err(invalid());
        }
    }
    let mut channels = HashSet::new();
    for c in &d.channels {
        if !key(&c.key)
            || !channels.insert(&c.key)
            || !name(&c.name, 100)
            || !["category", "text", "voice", "announcement", "forum"].contains(&c.kind.as_str())
            || c.overwrites.len() > 250
            || c.description.as_ref().is_some_and(|t| {
                t.chars().count() > 1024 || t.chars().any(|ch| ch.is_control() && ch != '\n')
            })
        {
            return Err(invalid());
        }
    }
    for c in &d.channels {
        if let Some(p) = &c.parent {
            if c.kind == "category"
                || !d
                    .channels
                    .iter()
                    .any(|x| &x.key == p && x.kind == "category")
            {
                return Err(invalid());
            }
        }
        let mut seen = HashSet::new();
        for o in &c.overwrites {
            if !roles.contains(&o.role)
                || !seen.insert(&o.role)
                || o.allow > ((1u64 << 53) - 1)
                || o.deny > ((1u64 << 53) - 1)
                || o.allow & o.deny != 0
            {
                return Err(invalid());
            }
        }
    }
    Ok(d)
}
#[derive(Serialize)]
pub struct Preview {
    pub document: Document,
    pub name_exists: bool,
}
pub async fn preview(pool: &SqlitePool, data: &str) -> Result<Preview, String> {
    let document = parse(data)?;
    let name_exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM templates WHERE name = ? COLLATE NOCASE)",
    )
    .bind(&document.name)
    .fetch_one(pool)
    .await
    .map_err(|_| invalid())?
        != 0;
    Ok(Preview {
        document,
        name_exists,
    })
}
pub async fn import(pool: &SqlitePool, data: &str, confirmed: bool) -> Result<(), String> {
    let d = parse(data)?;
    let mut tx = pool.begin().await.map_err(|_| invalid())?;
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM templates WHERE name = ? COLLATE NOCASE)",
    )
    .bind(&d.name)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| invalid())?
        != 0;
    if exists && !confirmed {
        return Err("A template with this name already exists".into());
    }
    sqlx::query("INSERT INTO templates (id,name,description,data,version,created_at,updated_at) VALUES (?,?,?,?,1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&d.name).bind(&d.description).bind(serde_json::to_string(&d).map_err(|_| invalid())?).execute(&mut *tx).await.map_err(|_| invalid())?;
    tx.commit().await.map_err(|_| invalid())?;
    Ok(())
}
#[tauri::command]
pub async fn preview_template_file(app: AppHandle, data: String) -> Result<Preview, String> {
    let pool = db::get_pool(&app).await.map_err(|_| invalid())?;
    preview(&pool, &data).await
}
#[tauri::command]
pub async fn import_template_file(
    app: AppHandle,
    data: String,
    confirmed: bool,
) -> Result<(), String> {
    let pool = db::get_pool(&app).await.map_err(|_| invalid())?;
    import(&pool, &data, confirmed).await
}
#[tauri::command]
pub async fn export_template_file(app: AppHandle, id: String) -> Result<String, String> {
    let pool = db::get_pool(&app).await.map_err(|_| invalid())?;
    export(&pool, &id).await
}
pub async fn export(pool: &SqlitePool, id: &str) -> Result<String, String> {
    let t = db::get_template(pool, id)
        .await
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?;
    let mut d = parse(&t.data)?;
    d.name = t.name;
    d.description = t.description;
    let data = serde_json::to_string_pretty(&d).map_err(|_| invalid())?;
    parse(&data)?;
    Ok(data)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn edit_structure_duplicate_and_delete_preserve_other_records() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        db::run_migrations(&pool).await.unwrap();
        let data = r#"{"format":"argus-template","version":1,"name":"Example","description":"","roles":[],"channels":[]}"#;
        import(&pool, data, false).await.unwrap();
        let mut original = db::list_templates(&pool).await.unwrap().remove(0);
        let mut document = parse(&original.data).unwrap();
        document.channels.push(Channel {
            key: "chat".into(),
            name: "chat".into(),
            kind: "text".into(),
            parent: None,
            description: None,
            overwrites: vec![],
        });
        original.data = serde_json::to_string(&document).unwrap();
        original.tags = "community, example".into();
        original.name = "Edited".into();
        db::save_template(&pool, &original).await.unwrap();
        let saved = db::get_template(&pool, &original.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(parse(&saved.data).unwrap().channels[0].name, "chat");
        assert_eq!(saved.tags, "community, example");
        let mut copy = saved.clone();
        copy.id = uuid::Uuid::new_v4().to_string();
        db::save_template(&pool, &copy).await.unwrap();
        db::delete_template(&pool, &original.id).await.unwrap();
        assert!(db::get_template(&pool, &original.id)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            db::get_template(&pool, &copy.id)
                .await
                .unwrap()
                .unwrap()
                .data,
            saved.data
        );
    }
    #[tokio::test]
    async fn roundtrip_and_rejections_leave_library_unchanged() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE templates (id TEXT PRIMARY KEY, name TEXT, description TEXT, tags TEXT NOT NULL DEFAULT '', data TEXT, version INTEGER, created_at TEXT, updated_at TEXT)").execute(&pool).await.unwrap();
        let data = r#"{"format":"argus-template","version":1,"name":"Example","description":"","roles":[{"key":"staff","name":"Staff","color":12,"permissions":8}],"channels":[{"key":"group","name":"Group","kind":"category","parent":null,"overwrites":[]},{"key":"chat","name":"chat","kind":"text","parent":"group","overwrites":[{"role":"staff","allow":1,"deny":0}]}]}"#;
        let original = parse(data).unwrap();
        assert!(!preview(&pool, data).await.unwrap().name_exists);
        import(&pool, data, false).await.unwrap();
        let saved = db::list_templates(&pool).await.unwrap();
        let exported = export(&pool, &saved[0].id).await.unwrap();
        assert_eq!(parse(&exported).unwrap(), original);
        assert!(preview(&pool, data).await.unwrap().name_exists);
        assert!(import(&pool, data, false).await.is_err());
        for bad in [
            "{",
            &data.replace("\"staff\",\"allow\"", "\"missing\",\"allow\""),
            &data.replace("\"version\":1", "\"version\":2"),
            &data.replace("\"parent\":\"group\"", "\"parent\":\"missing\""),
        ] {
            assert!(import(&pool, bad, true).await.is_err());
        }
        assert_eq!(db::list_templates(&pool).await.unwrap().len(), 1);
        import(&pool, &exported, true).await.unwrap();
        let copies = db::list_templates(&pool).await.unwrap();
        assert_eq!(copies.len(), 2);
        assert_eq!(parse(&copies[0].data).unwrap(), original);
    }
}

/// Read-only plan. New objects use template keys as references until created.
#[derive(Debug, Serialize)]
pub struct TargetPreview {
    pub document: Document,
    pub guild_id: String,
    pub differences: Vec<String>,
    pub conflicts: Vec<String>,
    pub actions: Vec<serde_json::Value>,
}

pub fn target_preview(
    document: Document,
    guild_id: &str,
    roles: &[crate::state::Role],
    channels: &[crate::state::Channel],
) -> Result<TargetPreview, String> {
    use serde_json::json;
    use std::collections::HashMap;
    let mut differences = Vec::new();
    let mut conflicts = Vec::new();
    let mut actions = Vec::new();
    let mut refs = HashMap::new();
    for role in &document.roles {
        if role.name.eq_ignore_ascii_case("@everyone") {
            if !roles.iter().any(|r| r.id == guild_id) {
                return Err("Server everyone role not found".into());
            }
            refs.insert(role.key.clone(), json!({"id": guild_id}));
        } else {
            let matches = roles
                .iter()
                .filter(|r| r.name.eq_ignore_ascii_case(&role.name))
                .count();
            if matches > 0 {
                conflicts.push(format!(
                    "Role ‘{}’ has {} existing name match(es); a new role is planned",
                    role.name, matches
                ));
            }
            refs.insert(role.key.clone(), json!({"new_role": role.key}));
            differences.push(format!("Create role: {}", role.name));
            actions.push(json!({"action":"create_role", "key":role.key, "name":role.name, "color":role.color, "permissions":role.permissions}));
        }
    }
    for channel in document
        .channels
        .iter()
        .filter(|c| c.kind == "category")
        .chain(document.channels.iter().filter(|c| c.kind != "category"))
    {
        let count = channels
            .iter()
            .filter(|c| c.name.eq_ignore_ascii_case(&channel.name))
            .count();
        if count > 0 {
            conflicts.push(format!(
                "Channel ‘{}’ has {} existing name match(es); a new channel is planned",
                channel.name, count
            ));
        }
        differences.push(format!("Create {}: {}", channel.kind, channel.name));
        let overwrites: Vec<_> = channel
            .overwrites
            .iter()
            .map(|o| json!({"role": refs[&o.role], "allow":o.allow, "deny":o.deny}))
            .collect();
        actions.push(json!({"action":"create_channel", "key":channel.key, "name":channel.name, "kind":channel.kind, "parent":channel.parent.as_ref().map(|p| json!({"new_category":p})), "overwrites":overwrites}));
    }
    Ok(TargetPreview {
        document,
        guild_id: guild_id.into(),
        differences,
        conflicts,
        actions,
    })
}

#[tauri::command]
pub async fn preview_template_target(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    template_id: String,
    guild_id: String,
) -> Result<TargetPreview, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let bot_id = state.get_active_bot().ok_or("No bot selected")?.id;
    let guilds = crate::commands::sincronizar_guilds(&pool, &state, None).await?;
    if !guilds.iter().any(|g| g.id == guild_id) {
        return Err("Server is not available to the active bot".into());
    }
    let template = db::get_template(&pool, &template_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(invalid)?;
    let document = parse(&template.data)?;
    let roles = crate::commands::leer_roles(&state, &guild_id, None).await?;
    let channels = crate::commands::leer_canales(&state, &guild_id, None).await?;
    if state.get_active_bot().map(|b| b.id) != Some(bot_id) {
        return Err("Active bot changed; retry the preview".into());
    }
    target_preview(document, &guild_id, &roles, &channels)
}

#[cfg(test)]
mod target_tests {
    use super::*;
    fn role(id: &str, name: &str) -> crate::state::Role {
        crate::state::Role {
            id: id.into(),
            name: name.into(),
            color: 0,
            position: 0,
            permissions: "0".into(),
            permissions_bitfield: None,
            member_count: 0,
            mentionable: false,
            hoist: false,
        }
    }
    #[test]
    fn preview_remaps_two_servers_and_preserves_ambiguous_roles() {
        let document = parse(r#"{"format":"argus-template","version":1,"name":"Example","description":"","roles":[{"key":"everyone","name":"@everyone","color":0,"permissions":0},{"key":"staff","name":"Staff","color":12,"permissions":8}],"channels":[{"key":"chat","name":"chat","kind":"text","parent":"group","overwrites":[{"role":"everyone","allow":1,"deny":0},{"role":"staff","allow":2,"deny":0}]},{"key":"group","name":"Group","kind":"category","parent":null,"overwrites":[]}]}"#).unwrap();
        for guild in ["100", "200"] {
            let roles = vec![
                role(guild, "@everyone"),
                role("301", "Staff"),
                role("302", "Staff"),
            ];
            let preview = target_preview(document.clone(), guild, &roles, &[]).unwrap();
            assert_eq!(preview.conflicts.len(), 1);
            assert_eq!(preview.actions.len(), 3);
            assert_eq!(preview.actions[1]["key"], "group");
            assert_eq!(preview.actions[2]["parent"]["new_category"], "group");
            assert_eq!(preview.actions[2]["overwrites"][0]["role"]["id"], guild);
            assert_eq!(
                preview.actions[2]["overwrites"][1]["role"]["new_role"],
                "staff"
            );
            assert!(preview
                .actions
                .iter()
                .all(|a| a["action"] == "create_role" || a["action"] == "create_channel"));
        }
        assert!(target_preview(document, "999", &[role("100", "@everyone")], &[]).is_err());
    }
}
