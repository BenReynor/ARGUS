//! Detector de ráfagas del Gateway. La copia se toma ANTES de mutar el mapa;
//! Restore siempre delega en snapshots::restore mediante la UI compartida.
use crate::{db, discord, i18n, snapshots, state::ServerMap};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use twilight_model::gateway::event::Event;

pub use crate::db::DestructionSettings as Settings;
#[derive(Clone)]
pub struct Deletion {
    pub target: String,
    pub action: u16,
    pub at_ms: i64,
}
struct PendingDeletion {
    deletion: Deletion,
    before: ServerMap,
}
#[derive(Default)]
struct Window {
    deletions: VecDeque<PendingDeletion>,
}
#[derive(Default)]
pub struct Detector {
    windows: HashMap<String, Window>,
}
pub struct Burst {
    pub before: ServerMap,
    pub deletions: Vec<Deletion>,
    pub remove_roles: bool,
}
/// Solo borrados de objetos presentes: duplicados, hilos y otros guilds no cuentan.
pub fn target(event: &Event) -> Option<(String, String, u16)> {
    match event {
        Event::ChannelDelete(c) if !matches!(u8::from(c.kind), 10..=12) => {
            Some((c.guild_id?.to_string(), c.id.to_string(), 12))
        }
        Event::RoleDelete(r) => Some((r.guild_id.to_string(), r.role_id.to_string(), 32)),
        _ => None,
    }
}
impl Detector {
    pub fn observe(
        &mut self,
        map: &ServerMap,
        event: &Event,
        settings: &Settings,
        at_ms: i64,
    ) -> Option<Burst> {
        let (guild, id, action) = target(event)?;
        if !settings.enabled {
            self.windows.remove(&guild);
            return None;
        }
        if guild != map.guild.id
            || !(if action == 12 {
                map.channels.iter().any(|c| c.id == id)
            } else {
                map.roles.iter().any(|r| r.id == id)
            })
        {
            return None;
        }
        let mut config = settings.clone();
        config.normalize();
        let window = self.windows.entry(guild.clone()).or_default();
        while window
            .deletions
            .front()
            .is_some_and(|d| at_ms - d.deletion.at_ms > config.window_seconds as i64 * 1000)
        {
            window.deletions.pop_front();
        }
        if window
            .deletions
            .iter()
            .any(|d| d.deletion.target == id && d.deletion.action == action)
        {
            return None;
        }
        // Cada borrado conserva su estado previo: cuando caduca el primero,
        // el siguiente ya incluye todos los cambios legítimos intermedios.
        window.deletions.push_back(PendingDeletion {
            deletion: Deletion {
                target: id,
                action,
                at_ms,
            },
            before: map.clone(),
        });
        if window.deletions.len() < config.threshold {
            return None;
        }
        let mut w = self.windows.remove(&guild).unwrap();
        let first = w.deletions.pop_front().unwrap();
        Some(Burst {
            before: first.before,
            deletions: std::iter::once(first.deletion)
                .chain(w.deletions.into_iter().map(|d| d.deletion))
                .collect(),
            remove_roles: config.remove_roles,
        })
    }
}
/// El mapa no conoce imágenes ni ajustes avanzados: no inventar esos campos.
pub fn from_map(map: &ServerMap) -> snapshots::ServerSnapshot {
    let role_name = |id: &str| {
        map.roles
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.name.as_str())
            .unwrap_or(id)
            .to_string()
    };
    snapshots::ServerSnapshot {
        version: 1, guild_id: map.guild.id.clone(), created_at: chrono::Utc::now().to_rfc3339(), name: map.guild.name.clone(),
        guild: json!({"id":map.guild.id, "name":map.guild.name}),
        roles: map.roles.iter().map(|r| json!({"id":r.id,"name":r.name,"color":r.color,"position":r.position,"permissions":r.permissions_bitfield.as_ref().unwrap_or(&r.permissions),"mentionable":r.mentionable,"hoist":r.hoist})).collect(),
        channels: map.channels.iter().map(|c| json!({"id":c.id,"name":c.name,"type":c.channel_type,"parent_id":c.parent_id,"position":c.position,"topic":c.topic,"permission_overwrites":c.overwrites.iter().flatten().map(|o| json!({"id":o.id,"type":o.type_,"allow":o.allow,"deny":o.deny,"name":if o.type_==0 { role_name(&o.id) } else { map.members.iter().find(|m|m.user_id==o.id).map(|m|m.username.clone()).unwrap_or_else(||o.id.clone()) }})).collect::<Vec<_>>()})).collect(),
        members: map.members.iter().map(|m| json!({"id":m.user_id,"name":m.nickname.as_ref().unwrap_or(&m.username),"username":m.username,"roles":m.roles.iter().map(|r|json!({"id":r,"name":role_name(r)})).collect::<Vec<_>>()})).collect(),
        emojis: vec![], stickers: vec![], webhooks: vec![], bans: vec![],
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub bot_id: String,
    pub guild_id: String,
    pub snapshot_id: String,
    pub count: usize,
    pub actors: Vec<String>,
    pub detail: Vec<String>,
    pub created_at: String,
}
async fn audit(token: &str, base: Option<&str>, guild: &str) -> Result<Value> {
    use twilight_http::{request::Request, response::marker::EmptyBody, routing::Route};
    let request = Request::builder(&Route::GetAuditLogs {
        action_type: None,
        after: None,
        before: None,
        guild_id: guild.parse()?,
        limit: Some(100),
        user_id: None,
    })
    .build();
    let data = discord::build_client(token, base)
        .request::<EmptyBody>(request)
        .await?
        .bytes()
        .await?;
    Ok(serde_json::from_slice(&data)?)
}
/// Correlacionar tipo, objetivo y fecha; jamás sancionar al autor del último log sin más.
fn actors(log: &Value, deletions: &[Deletion]) -> (Vec<String>, Vec<String>) {
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for entry in log["audit_log_entries"].as_array().into_iter().flatten() {
        let Some(id) = entry["user_id"].as_str() else {
            continue;
        };
        let timestamp = entry["id"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .map(|n| (n >> 22) as i64 + 1_420_070_400_000);
        if !deletions.iter().any(|d| {
            entry["target_id"] == d.target
                && entry["action_type"] == d.action
                && timestamp.is_some_and(|t| (t - d.at_ms).abs() <= 15_000)
        }) || ids.iter().any(|s| s == id)
        {
            continue;
        }
        let name = log["users"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|u| u["id"] == id)
            .and_then(|u| u["global_name"].as_str().or(u["username"].as_str()))
            .unwrap_or(id);
        ids.push(id.to_string());
        names.push(format!("{name} ({id})"));
    }
    (ids, names)
}
async fn save_alert(pool: &sqlx::SqlitePool, alert: &Alert) -> Result<i64> {
    Ok(sqlx::query(
        "INSERT INTO incidents(guild_id,type,payload,created_at) VALUES(?,'MASS_DESTRUCTION',?,?)",
    )
    .bind(&alert.guild_id)
    .bind(serde_json::to_string(alert)?)
    .bind(&alert.created_at)
    .execute(pool)
    .await?
    .last_insert_rowid())
}
/// Guardar primero: la auditoría o la retirada de roles nunca impiden el snapshot.
pub async fn protect(
    pool: &sqlx::SqlitePool,
    bot: &str,
    token: &str,
    base: Option<&str>,
    burst: Burst,
    report: impl Fn(&Alert) + Send,
) -> Result<Alert> {
    let snapshot = from_map(&burst.before);
    // Manual a efectos de retención: una rotación diaria no debe eliminar emergencias.
    let snapshot_id = snapshots::store(pool, bot, &snapshot.guild_id, &snapshot, false).await?;
    let mut alert = Alert {
        bot_id: bot.into(),
        guild_id: snapshot.guild_id.clone(),
        snapshot_id,
        count: burst.deletions.len(),
        actors: vec![],
        detail: vec![],
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let incident_id = save_alert(pool, &alert).await?;
    let mut actor_ids = Vec::new();
    // Discord puede publicar la auditoría después del evento del Gateway.
    for attempt in 0..3 {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            audit(token, base, &alert.guild_id),
        )
        .await
        {
            Ok(Ok(log)) => {
                let (ids, names) = actors(&log, &burst.deletions);
                for (id, name) in ids.into_iter().zip(names) {
                    if !actor_ids.contains(&id) {
                        actor_ids.push(id);
                        alert.actors.push(name);
                    }
                }
            }
            Ok(Err(e)) => {
                alert.detail.push(format!("Audit log unavailable: {e}"));
                break;
            }
            Err(_) => {
                alert.detail.push("Audit log request timed out".into());
                break;
            }
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        }
    }
    sqlx::query("UPDATE incidents SET payload=? WHERE id=?")
        .bind(serde_json::to_string(&alert)?)
        .bind(incident_id)
        .execute(pool)
        .await?;
    // Avisar antes de una mitigación que puede tardar por el rate limit.
    report(&alert);
    if burst.remove_roles {
        for actor in actor_ids.iter().filter(|id| id.as_str() != bot) {
            // Discord valida permisos, jerarquía y roles gestionados; los errores se muestran.
            let result = async {
                let gid = alert.guild_id.parse::<u64>()?;
                let uid = actor.parse::<u64>()?;
                let client = discord::build_client(token, base);
                let member = client
                    .guild_member(
                        twilight_model::id::Id::new(gid),
                        twilight_model::id::Id::new(uid),
                    )
                    .await?
                    .model()
                    .await?;
                let roles = client
                    .roles(twilight_model::id::Id::new(gid))
                    .await?
                    .models()
                    .await?;
                let bot_member = client
                    .guild_member(
                        twilight_model::id::Id::new(gid),
                        twilight_model::id::Id::new(bot.parse()?),
                    )
                    .await?
                    .model()
                    .await?;
                let permissions = roles
                    .iter()
                    .filter(|r| r.id.get() == gid || bot_member.roles.contains(&r.id))
                    .fold(twilight_model::guild::Permissions::empty(), |p, r| {
                        p | r.permissions
                    });
                if !permissions.intersects(
                    twilight_model::guild::Permissions::ADMINISTRATOR
                        | twilight_model::guild::Permissions::MANAGE_ROLES,
                ) {
                    anyhow::bail!("The bot lacks the Manage Roles permission");
                }
                let mut errors = Vec::new();
                for role in member
                    .roles
                    .into_iter()
                    .filter(|id| roles.iter().any(|r| r.id == *id && !r.managed))
                {
                    if let Err(error) = discord::set_member_role(
                        token,
                        base,
                        &alert.guild_id,
                        actor,
                        &role.to_string(),
                        false,
                    )
                    .await
                    {
                        errors.push(error);
                    }
                }
                if !errors.is_empty() {
                    anyhow::bail!(errors.join("; "));
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            match result {
                Ok(()) => alert.detail.push(format!("Roles removed from {actor}")),
                Err(e) => alert
                    .detail
                    .push(format!("Could not remove roles from {actor}: {e}")),
            }
        }
    }
    // Actualizar el registro inicial, conservándolo aunque falle la auditoría.
    sqlx::query("UPDATE incidents SET payload=? WHERE id=?")
        .bind(serde_json::to_string(&alert)?)
        .bind(incident_id)
        .execute(pool)
        .await?;
    Ok(alert)
}
pub fn notify(app: &tauri::AppHandle, alert: &Alert) {
    let lang = app
        .try_state::<crate::state::AppState>()
        .map(|s| i18n::idioma(&s))
        .unwrap_or_else(|| "en".into());
    let actors = if alert.actors.is_empty() {
        i18n::tr(&lang, "Unknown actor (audit log unavailable or delayed)")
    } else {
        alert.actors.join(", ")
    };
    let text = i18n::tr(
        &lang,
        &format!(
            "Mass deletion detected: {} items. Actor: {}",
            alert.count, actors
        ),
    );
    if let Err(e) = app
        .notification()
        .builder()
        .title(i18n::tr(&lang, "Emergency snapshot saved"))
        .body(&text)
        .show()
    {
        tracing::warn!("No se pudo mostrar la notificación: {e}");
    }
    let _ = app.emit("bot-log", format!("[WARN] {text}"));
    let _ = app.emit("destruction-alert", alert);
}
#[tauri::command]
pub async fn destruction_alerts(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
) -> std::result::Result<Vec<Alert>, String> {
    let result = async {
        let bot = state.get_active_bot().context("No bot selected")?;
        let guild = state.get_active_guild().context("No server selected")?;
        let pool = db::get_pool(&app).await?;
        let rows: Vec<String> = sqlx::query_scalar("SELECT payload FROM incidents WHERE guild_id=? AND type='MASS_DESTRUCTION' ORDER BY created_at DESC").bind(&guild).fetch_all(&pool).await?;
        let language = i18n::idioma(&state);
        Ok(rows.into_iter().filter_map(|r|serde_json::from_str::<Alert>(&r).ok()).filter(|a|a.bot_id==bot.id).map(|mut a| { a.detail = a.detail.iter().map(|d|i18n::tr(&language,d)).collect(); a }).collect())
    }.await;
    i18n::localizar_error(&state, result.map_err(|e: anyhow::Error| e.to_string()))
}
#[cfg(test)]
mod tests;
