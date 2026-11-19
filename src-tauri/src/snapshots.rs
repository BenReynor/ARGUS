//! Snapshots portátiles de Discord: lecturas REST completas, sin secretos de webhooks.
use crate::{crypto, db, discord, i18n, state::AppState};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection, SqlitePool};
use tauri::{AppHandle, Emitter, Manager, State};
use twilight_http::routing::Route;

pub(crate) mod compare;
pub mod departed;
pub mod restore;

const MAX_FILE: usize = 128 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerSnapshot {
    pub version: u32,
    pub guild_id: String,
    pub created_at: String,
    pub name: String,
    pub guild: Value,
    pub roles: Vec<Value>,
    pub channels: Vec<Value>,
    pub members: Vec<Value>,
    pub emojis: Vec<Value>,
    pub stickers: Vec<Value>,
    pub webhooks: Vec<Value>,
    pub bans: Vec<Value>,
}

async fn get(token: &str, base: Option<&str>, route: Route<'_>) -> Result<Value> {
    let bytes = discord::build_client(token, base)
        .request::<twilight_http::response::marker::EmptyBody>(
            twilight_http::request::Request::builder(&route).build(),
        )
        .await
        .map_err(discord::traducir_error)
        .map_err(anyhow::Error::msg)?
        .bytes()
        .await?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn array(v: Value) -> Result<Vec<Value>> {
    v.as_array()
        .cloned()
        .context("Invalid response from Discord")
}
fn name<'a>(items: &'a [Value], id: &str) -> Option<&'a str> {
    items
        .iter()
        .find(|v| v["id"].as_str() == Some(id))?
        .get("name")?
        .as_str()
}

// La URL CDN se construye con IDs oficiales, nunca con una URL del archivo importado.
async fn image(base: Option<&str>, path: &str) -> Result<String> {
    let url = match base {
        Some(base) => format!("http://{base}/cdn/{path}"),
        None => format!("https://cdn.discordapp.com/{path}"),
    };
    let bytes = crate::responder::descargar_archivo(&url, 16 * 1024 * 1024)
        .await
        .map_err(anyhow::Error::msg)?;
    if bytes.is_empty() {
        bail!("Invalid response from Discord");
    }
    Ok(STANDARD.encode(bytes))
}

/// Conserva todos los campos REST de canales/roles, incluidos los no soportados aún por la UI.
/// No se guarda un snapshot parcial: cualquier error aborta antes de insertar en SQLite.
pub async fn capture(token: &str, base: Option<&str>, guild: &str) -> Result<ServerSnapshot> {
    let guild_id = guild.parse::<u64>()?;
    if guild_id == 0 {
        bail!("Invalid server ID");
    }
    let mut settings = get(
        token,
        base,
        Route::GetGuild {
            guild_id,
            with_counts: false,
        },
    )
    .await?;
    let mut roles = array(get(token, base, Route::GetGuildRoles { guild_id }).await?)?;
    for role in &mut roles {
        if let Some(hash) = role["icon"].as_str() {
            let id = role["id"]
                .as_str()
                .context("Invalid response from Discord")?;
            role["icon_image"] = json!({"encoding":"base64", "format":"png", "data":image(base, &format!("role-icons/{id}/{hash}.png")).await?});
        }
    }
    let mut channels = array(get(token, base, Route::GetChannels { guild_id }).await?)?;
    let mut members = Vec::new();
    let mut after = None;
    loop {
        let page = array(
            get(
                token,
                base,
                Route::GetGuildMembers {
                    guild_id,
                    after,
                    limit: Some(1000),
                },
            )
            .await?,
        )?;
        let full = page.len() == 1000;
        let next = page
            .last()
            .and_then(|v| v["user"]["id"].as_str())
            .map(str::parse::<u64>)
            .transpose()?;
        for m in page {
            let id = m["user"]["id"].clone();
            let display = m["nick"]
                .as_str()
                .or(m["user"]["global_name"].as_str())
                .or(m["user"]["username"].as_str())
                .context("Invalid response from Discord")?;
            let assignments: Vec<Value> = m["roles"]
                .as_array()
                .context("Invalid response from Discord")?
                .iter()
                .map(|r| json!({"id": r, "name": name(&roles, r.as_str().unwrap_or_default())}))
                .collect();
            members.push(json!({"id": id, "name": display, "username": m["user"]["username"], "roles": assignments}));
        }
        if !full {
            break;
        }
        if next.is_none() || next <= after {
            bail!("Invalid response from Discord");
        }
        after = next;
    }
    for c in &mut channels {
        if let Some(overwrites) = c["permission_overwrites"].as_array_mut() {
            for overwrite in overwrites {
                let id = overwrite["id"]
                    .as_str()
                    .context("Invalid response from Discord")?
                    .to_owned();
                let target_name = if overwrite["type"] == 0 {
                    name(&roles, &id)
                        .context("Invalid response from Discord")?
                        .to_owned()
                } else if let Some(n) = name(&members, &id) {
                    n.to_owned()
                } else {
                    // Un overwrite puede referirse a alguien que ya salió del servidor.
                    let user_id = id.parse::<u64>()?;
                    let user = get(token, base, Route::GetUser { user_id }).await?;
                    user["global_name"]
                        .as_str()
                        .or(user["username"].as_str())
                        .context("Invalid response from Discord")?
                        .to_owned()
                };
                overwrite["name"] = target_name.into();
            }
        }
    }
    let mut emojis = array(get(token, base, Route::GetEmojis { guild_id }).await?)?;
    for emoji in &mut emojis {
        let id = emoji["id"]
            .as_str()
            .context("Invalid response from Discord")?;
        let extension = if emoji["animated"] == true {
            "gif"
        } else {
            "png"
        };
        emoji["image"] = json!({"encoding":"base64", "format":extension, "data":image(base, &format!("emojis/{id}.{extension}")).await?});
    }
    let mut stickers = array(get(token, base, Route::GetGuildStickers { guild_id }).await?)?;
    for sticker in &mut stickers {
        let id = sticker["id"]
            .as_str()
            .context("Invalid response from Discord")?;
        let extension = match sticker["format_type"].as_u64() {
            Some(3) => "json",
            Some(4) => "gif",
            _ => "png",
        };
        sticker["image"] = json!({"encoding":"base64", "format":extension, "data":image(base, &format!("stickers/{id}.{extension}")).await?});
    }
    let webhooks = array(get(token, base, Route::GetGuildWebhooks { guild_id }).await?)?.iter()
        .map(|v| json!({"id":v["id"], "name":v["name"], "channel_id":v["channel_id"], "channel_name": name(&channels, v["channel_id"].as_str().unwrap_or_default())})).collect();
    let mut bans = Vec::new();
    after = None;
    loop {
        let page = array(
            get(
                token,
                base,
                Route::GetBansWithParameters {
                    guild_id,
                    after,
                    before: None,
                    limit: Some(1000),
                },
            )
            .await?,
        )?;
        let full = page.len() == 1000;
        let next = page
            .last()
            .and_then(|v| v["user"]["id"].as_str())
            .map(str::parse::<u64>)
            .transpose()?;
        bans.extend(page);
        if !full {
            break;
        }
        if next.is_none() || next <= after {
            bail!("Invalid response from Discord");
        }
        after = next;
    }
    for (field, directory) in [
        ("icon", "icons"),
        ("banner", "banners"),
        ("splash", "splashes"),
        ("discovery_splash", "discovery-splashes"),
    ] {
        if let Some(hash) = settings[field].as_str() {
            let extension = if hash.starts_with("a_") { "gif" } else { "png" };
            settings[format!("{field}_image")] = json!({"encoding":"base64", "format":extension, "data":image(base, &format!("{directory}/{guild}/{hash}.{extension}")).await?});
        }
    }
    // Conservar los campos REST del servidor sin una lista cerrada que pierda ajustes.
    // Las colecciones capturadas por separado son autoritativas.
    let settings_object = settings
        .as_object_mut()
        .context("Invalid response from Discord")?;
    for collection in ["roles", "emojis", "stickers"] {
        settings_object.remove(collection);
    }
    Ok(ServerSnapshot {
        version: 1,
        guild_id: guild.into(),
        created_at: Utc::now().to_rfc3339(),
        name: settings["name"].as_str().unwrap_or_default().into(),
        guild: settings,
        roles,
        channels,
        members,
        emojis,
        stickers,
        webhooks,
        bans,
    })
}

fn summary(s: &ServerSnapshot) -> Value {
    json!({"channels":s.channels.len(), "roles":s.roles.len(), "members":s.members.len(), "emojis":s.emojis.len(), "stickers":s.stickers.len(), "webhooks":s.webhooks.len(), "bans":s.bans.len()})
}
fn validate(s: &ServerSnapshot, guild: &str) -> Result<()> {
    if s.version != 1
        || s.guild_id != guild
        || s.guild["id"].as_str() != Some(guild)
        || s.guild["name"].as_str().is_none()
        || chrono::DateTime::parse_from_rfc3339(&s.created_at).is_err()
        || s.name.trim().is_empty()
        || s.name.chars().count() > 100
    {
        bail!("Invalid snapshot file or different server");
    }
    for collection in [&s.roles, &s.channels, &s.members, &s.emojis, &s.stickers] {
        if collection
            .iter()
            .any(|v| v["id"].as_str().is_none() || v["name"].as_str().is_none())
        {
            bail!("Invalid snapshot file or different server");
        }
    }
    for channel in &s.channels {
        if channel["type"].as_u64().is_none() {
            bail!("Invalid snapshot file or different server");
        }
        if let Some(overwrites) = channel.get("permission_overwrites") {
            for overwrite in overwrites
                .as_array()
                .context("Invalid snapshot file or different server")?
            {
                if overwrite["id"].as_str().is_none()
                    || overwrite["name"].as_str().is_none()
                    || !matches!(overwrite["type"].as_u64(), Some(0 | 1))
                    || overwrite["allow"].as_str().is_none()
                    || overwrite["deny"].as_str().is_none()
                {
                    bail!("Invalid snapshot file or different server");
                }
            }
        }
    }
    for member in &s.members {
        for role in member["roles"]
            .as_array()
            .context("Invalid snapshot file or different server")?
        {
            if role["id"].as_str().is_none() || role["name"].as_str().is_none() {
                bail!("Invalid snapshot file or different server");
            }
        }
    }
    for asset in s.emojis.iter().chain(s.stickers.iter()) {
        validate_image(&asset["image"])?;
    }
    for (asset, fields) in std::iter::once((
        &s.guild,
        &["icon", "banner", "splash", "discovery_splash"][..],
    ))
    .chain(s.roles.iter().map(|role| (role, &["icon"][..])))
    {
        for field in fields {
            let image_key = format!("{field}_image");
            if !asset[*field].is_null() || !asset[&image_key].is_null() {
                validate_image(&asset[&image_key])?;
            }
        }
    }
    if s.webhooks
        .iter()
        .any(|v| v.get("token").is_some() || v.get("url").is_some())
    {
        bail!("Invalid snapshot file or different server");
    }
    Ok(())
}

/// Checks that the bytes really are what `format` says, not just non-empty
/// base64: a corrupt image would be restored as a broken icon or sticker.
fn validate_image(img: &Value) -> Result<()> {
    let data = img["data"]
        .as_str()
        .context("Invalid snapshot file or different server")?;
    if img["encoding"] != "base64" {
        bail!("Invalid snapshot file or different server");
    }
    let bytes = STANDARD.decode(data)?;
    let valid = match img["format"].as_str() {
        Some("png") => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        Some("gif") => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        Some("json") => serde_json::from_slice::<Value>(&bytes).is_ok_and(|v| v.is_object()),
        _ => false,
    };
    if !valid {
        bail!("Invalid snapshot file or different server");
    }
    Ok(())
}

pub async fn store(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
    s: &ServerSnapshot,
    automatic: bool,
) -> Result<String> {
    let mut connection = pool.acquire().await?;
    insert(&mut connection, bot, guild, s, automatic).await
}
async fn insert(
    connection: &mut SqliteConnection,
    bot: &str,
    guild: &str,
    s: &ServerSnapshot,
    automatic: bool,
) -> Result<String> {
    validate(s, guild)?;
    let data = serde_json::to_string(s)?;
    if data.len() > MAX_FILE {
        bail!("Snapshot file is too large");
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO server_snapshots(id,bot_id,guild_id,name,created_at,size_bytes,summary,data,automatic) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(&id).bind(bot).bind(guild).bind(&s.name).bind(&s.created_at).bind(data.len() as i64).bind(summary(s).to_string()).bind(data).bind(automatic).execute(connection).await?;
    Ok(id)
}
pub(crate) async fn load(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
    id: &str,
) -> Result<ServerSnapshot> {
    let row = sqlx::query(
        "SELECT data,name FROM server_snapshots WHERE id=? AND bot_id=? AND guild_id=?",
    )
    .bind(id)
    .bind(bot)
    .bind(guild)
    .fetch_optional(pool)
    .await?
    .context("Snapshot not found")?;
    let mut s: ServerSnapshot = serde_json::from_str(row.get::<&str, _>("data"))?;
    s.name = row.get("name");
    Ok(s)
}
pub(crate) async fn latest(pool: &SqlitePool, bot: &str, guild: &str) -> Result<String> {
    sqlx::query_scalar("SELECT id FROM server_snapshots WHERE bot_id=? AND guild_id=? ORDER BY created_at DESC,rowid DESC LIMIT 1")
        .bind(bot).bind(guild).fetch_optional(pool).await?.context("Snapshot not found")
}

pub(crate) async fn listing(pool: &SqlitePool, bot: &str, guild: &str) -> Result<Value> {
    let return_roles = departed::enabled(pool, bot, guild).await?;
    let return_roles_ready: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM snapshot_return_roles WHERE bot_id=? AND guild_id=?)",
    )
    .bind(bot)
    .bind(guild)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query("SELECT id,name,created_at,size_bytes,summary,automatic FROM server_snapshots WHERE bot_id=? AND guild_id=? ORDER BY created_at DESC,rowid DESC").bind(bot).bind(guild).fetch_all(pool).await?;
    let rows: Vec<Value> = rows.iter().map(|r| Ok(json!({"id":r.get::<String,_>("id"), "name":r.get::<String,_>("name"), "created_at":r.get::<String,_>("created_at"), "size_bytes":r.get::<i64,_>("size_bytes"), "summary":serde_json::from_str::<Value>(r.get("summary"))?, "automatic":r.get::<bool,_>("automatic")}))).collect::<Result<_>>()?;
    let schedule = sqlx::query("SELECT enabled,keep_last,interval_hours,last_error FROM server_snapshot_schedules WHERE bot_id=? AND guild_id=?").bind(bot).bind(guild).fetch_optional(pool).await?;
    Ok(
        json!({"return_roles_ready":return_roles_ready,"return_roles":return_roles,"snapshots":rows, "schedule":schedule.map(|r| json!({"enabled":r.get::<bool,_>("enabled"), "keep_last":r.get::<u32,_>("keep_last"), "interval_hours":r.get::<u32,_>("interval_hours"), "last_error":r.get::<Option<String>,_>("last_error")})).unwrap_or(json!({"enabled":false,"keep_last":14,"interval_hours":12,"last_error":null}))}),
    )
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SnapshotRequest {
    List,
    Departed {
        id: String,
    },
    InviteDeparted {
        id: String,
        channel_id: String,
    },
    ExportDeparted {
        id: String,
        path: String,
    },
    ReturnRoles {
        enabled: bool,
    },
    Create,
    Compare {
        id: String,
    },
    Restore {
        id: String,
        #[serde(default)]
        undo: bool,
        #[serde(default)]
        delete_extra: bool,
    },
    Rename {
        id: String,
        name: String,
    },
    Delete {
        id: String,
    },
    Export {
        id: String,
        path: String,
    },
    Import {
        path: String,
    },
    Schedule {
        enabled: bool,
        keep_last: u32,
        #[serde(default)]
        interval_hours: Option<u32>,
    },
}
async fn manage(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
    request: SnapshotRequest,
) -> Result<Value> {
    match request {
        SnapshotRequest::ReturnRoles { enabled } => {
            sqlx::query("UPDATE snapshot_return_roles SET enabled=? WHERE bot_id=? AND guild_id=?")
                .bind(enabled)
                .bind(bot)
                .bind(guild)
                .execute(pool)
                .await?;
        }
        SnapshotRequest::Departed { ref id }
        | SnapshotRequest::InviteDeparted { ref id, .. }
        | SnapshotRequest::ExportDeparted { ref id, .. } => {
            let saved = load(pool, bot, guild, id).await?;
            let config = db::list_bots(pool)
                .await?
                .into_iter()
                .find(|b| b.id == bot)
                .context("No bot selected")?;
            let token = crypto::decrypt(&config.token_ciphertext)?;
            let members = departed::missing(&saved, &token, None).await?;
            let mut result = listing(pool, bot, guild).await?;
            if let SnapshotRequest::InviteDeparted { channel_id, .. } = request {
                let channels = discord::fetch_guild_channels(&token, None, guild)
                    .await
                    .map_err(anyhow::Error::msg)?;
                if !channels.iter().any(|c| c.id == channel_id) {
                    bail!("No server selected");
                }
                result["invitations"] =
                    departed::invite(&token, None, &channel_id, &members).await?;
            } else if let SnapshotRequest::ExportDeparted { path, .. } = request {
                let mut options = tokio::fs::OpenOptions::new();
                options.create(true).truncate(true).write(true);
                #[cfg(unix)]
                options.mode(0o600);
                use tokio::io::AsyncWriteExt;
                let mut file = options.open(path).await?;
                file.write_all(&serde_json::to_vec_pretty(&members)?)
                    .await?;
                file.sync_all().await?;
            }
            result["departed"] = json!(members);
            return Ok(result);
        }
        SnapshotRequest::List => {}
        SnapshotRequest::Compare { id } => {
            let config = db::list_bots(pool)
                .await?
                .into_iter()
                .find(|b| b.id == bot)
                .context("No bot selected")?;
            let token = crypto::decrypt(&config.token_ciphertext)?;
            let comparison = compare::compare_saved(pool, bot, guild, &id, &token, None).await?;
            let mut result = listing(pool, bot, guild).await?;
            result["comparison"] = serde_json::to_value(comparison)?;
            return Ok(result);
        }
        SnapshotRequest::Restore {
            id,
            undo,
            delete_extra,
        } => {
            let config = db::list_bots(pool)
                .await?
                .into_iter()
                .find(|b| b.id == bot)
                .context("No bot selected")?;
            let token = crypto::decrypt(&config.token_ciphertext)?;
            let preview =
                restore::preview(pool, bot, guild, &id, &token, None, undo, delete_extra).await?;
            let mut result = listing(pool, bot, guild).await?;
            result["restore"] = serde_json::to_value(preview)?;
            return Ok(result);
        }
        SnapshotRequest::Create => {
            let bot_config = db::list_bots(pool)
                .await?
                .into_iter()
                .find(|b| b.id == bot)
                .context("No bot selected")?;
            let token = crypto::decrypt(&bot_config.token_ciphertext)?;
            let s = capture(&token, None, guild).await?;
            store(pool, bot, guild, &s, false).await?;
        }
        SnapshotRequest::Rename { id, name } => {
            let mut s = load(pool, bot, guild, &id).await?;
            s.name = name.trim().into();
            validate(&s, guild)?;
            let data = serde_json::to_string(&s)?;
            sqlx::query("UPDATE server_snapshots SET name=?,data=?,size_bytes=? WHERE id=? AND bot_id=? AND guild_id=?").bind(&s.name).bind(&data).bind(data.len() as i64).bind(id).bind(bot).bind(guild).execute(pool).await?;
        }
        SnapshotRequest::Delete { id } => {
            sqlx::query("DELETE FROM server_snapshots WHERE id=? AND bot_id=? AND guild_id=?")
                .bind(id)
                .bind(bot)
                .bind(guild)
                .execute(pool)
                .await?;
        }
        SnapshotRequest::Export { id, path } => {
            let s = load(pool, bot, guild, &id).await?;
            let data = serde_json::to_vec(&s)?;
            let mut options = tokio::fs::OpenOptions::new();
            options.create(true).truncate(true).write(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(path).await?;
            use tokio::io::AsyncWriteExt;
            file.write_all(&data).await?;
            file.sync_all().await?;
        }
        SnapshotRequest::Import { path } => {
            let file = tokio::fs::File::open(path).await?;
            use tokio::io::AsyncReadExt;
            let mut data = Vec::new();
            file.take((MAX_FILE + 1) as u64)
                .read_to_end(&mut data)
                .await?;
            if data.len() > MAX_FILE {
                bail!("Snapshot file is too large");
            }
            let s: ServerSnapshot = serde_json::from_slice(&data)
                .context("Invalid snapshot file or different server")?;
            store(pool, bot, guild, &s, false).await?;
        }
        SnapshotRequest::Schedule {
            enabled,
            keep_last,
            interval_hours,
        } => {
            if interval_hours.is_some_and(|hours| !matches!(hours, 6 | 12 | 24)) {
                bail!("Choose a snapshot interval of 6, 12 or 24 hours");
            }
            if !(1..=365).contains(&keep_last) {
                bail!("Keep between 1 and 365 snapshots");
            }
            sqlx::query("INSERT INTO server_snapshot_schedules(bot_id,guild_id,enabled,keep_last,interval_hours) VALUES(?,?,?,?,COALESCE(?,12)) ON CONFLICT(bot_id,guild_id) DO UPDATE SET enabled=excluded.enabled,keep_last=excluded.keep_last,interval_hours=COALESCE(?,server_snapshot_schedules.interval_hours)")
                .bind(bot).bind(guild).bind(enabled).bind(keep_last).bind(interval_hours).bind(interval_hours).execute(pool).await?;
        }
    }
    listing(pool, bot, guild).await
}

#[tauri::command]
pub async fn server_snapshots(
    app: AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    guild_id: String,
    request: SnapshotRequest,
) -> std::result::Result<Value, String> {
    let result = async {
        if state.get_active_bot().map(|b| b.id).as_deref() != Some(&bot_id)
            || state.get_active_guild().as_deref() != Some(&guild_id)
        {
            bail!("No server selected");
        }
        let pool = db::get_pool(&app).await?;
        let mut value = manage(&pool, &bot_id, &guild_id, request).await?;
        if let Some(invitations) = value.get_mut("invitations") {
            let language = i18n::idioma(&state);
            for result in invitations["results"].as_array_mut().into_iter().flatten() {
                if let Some(error) = result["error"].as_str() {
                    result["error"] = json!(i18n::tr(&language, error));
                }
            }
        }
        if let Some(preview) = value.get_mut("restore") {
            let language = state.config.lock().unwrap().ui_language.clone();
            for step in preview["steps"].as_array_mut().into_iter().flatten() {
                if let Some(text) = step["description"].as_str() {
                    step["description"] = json!(i18n::tr(&language, text));
                }
            }
            for warning in preview["unavailable"].as_array_mut().into_iter().flatten() {
                if let Some(text) = warning.as_str() {
                    *warning = json!(i18n::tr(&language, text));
                }
            }
        }
        Ok(value)
    }
    .await;
    i18n::localizar_error(&state, result.map_err(|e| format!("{e:#}")))
}

/// Solo recorta copias automáticas del mismo ámbito; las manuales siempre se conservan.
async fn finish_automatic(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
    s: &ServerSnapshot,
    keep: u32,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    insert(&mut tx, bot, guild, s, true).await?;
    sqlx::query("DELETE FROM server_snapshots WHERE id IN (SELECT id FROM server_snapshots WHERE bot_id=? AND guild_id=? AND automatic=1 ORDER BY created_at DESC,rowid DESC LIMIT -1 OFFSET ?)").bind(bot).bind(guild).bind(keep).execute(&mut *tx).await?;
    sqlx::query("UPDATE server_snapshot_schedules SET last_success=?,last_error=NULL WHERE bot_id=? AND guild_id=?").bind(Utc::now().to_rfc3339()).bind(bot).bind(guild).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

// SQLite normaliza offsets y el formato antiguo de fecha al comparar instantes.
async fn due_schedules(
    pool: &SqlitePool,
    now: chrono::DateTime<Utc>,
) -> Result<Vec<sqlx::sqlite::SqliteRow>> {
    Ok(sqlx::query("SELECT bot_id,guild_id,keep_last FROM server_snapshot_schedules WHERE enabled=1 AND (last_success IS NULL OR julianday(last_success) + interval_hours / 24.0 <= julianday(?))")
        .bind(now.to_rfc3339()).fetch_all(pool).await?)
}

pub fn start_automatic(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            timer.tick().await;
            if let Err(e) = automatic_tick(&app).await {
                tracing::error!("No se pudieron ejecutar los snapshots automáticos: {e}");
            }
        }
    });
}
async fn automatic_tick(app: &AppHandle) -> Result<()> {
    let pool = db::get_pool(app).await?;
    let rows = due_schedules(&pool, Utc::now()).await?;
    for row in rows {
        let bot: String = row.get("bot_id");
        let guild: String = row.get("guild_id");
        let result: Result<()> = async {
            let config = db::list_bots(&pool)
                .await?
                .into_iter()
                .find(|b| b.id == bot)
                .context("No bot selected")?;
            let token = crypto::decrypt(&config.token_ciphertext)?;
            let s = capture(&token, None, &guild).await?;
            finish_automatic(&pool, &bot, &guild, &s, row.get("keep_last")).await
        }
        .await;
        if let Err(e) = result {
            let state = app.state::<AppState>();
            let error = i18n::localizar_error::<()>(&state, Err(format!("{e:#}"))).unwrap_err();
            sqlx::query(
                "UPDATE server_snapshot_schedules SET last_error=? WHERE bot_id=? AND guild_id=?",
            )
            .bind(error)
            .bind(&bot)
            .bind(&guild)
            .execute(&pool)
            .await?;
        }
        app.emit("snapshots-updated", json!({"bot_id":bot,"guild_id":guild}))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
