//! Restauración explícita. La propuesta aprobada vive en memoria: Apply recibe
//! solo su identificador, nunca rutas REST ni argumentos editables desde la UI.
use super::*;
use crate::propuesta::{AvanceAccion, EstadoAccion, ResultadoAccion, ResumenPropuesta};
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Step {
    kind: String,
    target: String,
    data: Value,
    description: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Preview {
    pub(crate) id: String,
    guild_name: String,
    steps: Vec<Step>,
    pub(crate) unavailable: Vec<String>,
    estimated_seconds: usize,
    delete_extra: bool,
}
#[derive(Serialize, Deserialize)]
struct Job {
    #[serde(skip)]
    pool: Option<SqlitePool>,
    bot: String,
    guild: String,
    preview: Preview,
    roles: HashMap<String, String>,
    channels: HashMap<String, String>,
    results: Vec<ResultadoAccion>,
    #[serde(default)]
    members: Vec<Value>,
    #[serde(skip)]
    stop: Arc<AtomicBool>,
    #[serde(skip)]
    running: Arc<AtomicBool>,
}
static JOBS: OnceLock<Mutex<HashMap<String, Arc<Mutex<Job>>>>> = OnceLock::new();
fn jobs() -> &'static Mutex<HashMap<String, Arc<Mutex<Job>>>> {
    JOBS.get_or_init(Mutex::default)
}
fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap_or_default()
}
fn label(v: &Value) -> &str {
    v["name"].as_str().unwrap_or_default()
}
fn fields(v: &Value, keys: &[&str]) -> Value {
    let mut out = json!({});
    for key in keys {
        if let Some(value) = v.get(*key) {
            out[*key] = value.clone();
        }
    }
    out
}
fn image_field(v: &Value, out: &mut Value, key: &str) {
    if let Some(img) = v.get(format!("{key}_image")) {
        out[key] = json!(format!(
            "data:image/{};base64,{}",
            img["format"].as_str().unwrap_or("png"),
            img["data"].as_str().unwrap_or_default()
        ));
    } else if v.get(key).is_some_and(Value::is_null) {
        out[key] = Value::Null;
    }
}
fn reference(kind: &str, old: &str) -> Value {
    json!({kind:old})
}
fn step(steps: &mut Vec<Step>, kind: &str, target: &str, data: Value, text: &str) {
    steps.push(Step {
        kind: kind.into(),
        target: target.into(),
        data,
        description: text.into(),
    });
}
fn build(
    saved: &ServerSnapshot,
    guild: &Value,
    roles: &[Value],
    channels: &[Value],
    members: &[Value],
    undo: bool,
    delete_extra: bool,
) -> Job {
    let mut steps = Vec::new();
    let mut unavailable = vec![
        "Messages cannot be restored".into(),
        "Emojis, stickers, webhooks and bans are not restored by this proposal".into(),
    ];
    let mut role_map = HashMap::new();
    let mut channel_map = HashMap::new();
    let mut used_roles = HashSet::new();
    let mut used_channels = HashSet::new();
    let mut protected_roles = HashSet::new();
    let current_members: HashSet<&str> = members.iter().map(id).collect();
    let mut ordered = saved.roles.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|r| r["position"].as_i64().unwrap_or_default());
    // Reservar todas las coincidencias por ID antes de resolver nombres: un
    // rol borrado no debe apropiarse de un superviviente procesado más tarde.
    for r in &ordered {
        if let Some(v) = roles.iter().find(|v| id(v) == id(r)) {
            role_map.insert(id(r).into(), id(v).into());
            used_roles.insert(id(v));
        }
    }
    let mut positions = Vec::new();
    for r in ordered {
        let old = id(r);
        let same_id = roles.iter().find(|v| id(v) == old);
        // Names only match roles of the same kind: an integration's managed
        // role can never stand in for a deleted normal role (Discord refuses
        // to edit or assign it, which stopped the restore), and the other way.
        let managed = r["managed"] == true;
        let same_kind = |v: &&Value| (v["managed"] == true) == managed;
        let same_name: Vec<_> = roles
            .iter()
            .filter(|v| same_kind(v) && label(v) == label(r) && !used_roles.contains(id(v)))
            .collect();
        let existing = same_id.or_else(|| (same_name.len() == 1).then(|| same_name[0]));
        let all_same_name: Vec<_> = roles
            .iter()
            .filter(|v| same_kind(v) && label(v) == label(r))
            .collect();
        let unmatched_snapshot_names = saved
            .roles
            .iter()
            .filter(|v| same_kind(v) && label(v) == label(r) && !role_map.contains_key(id(v)))
            .count();
        if same_id.is_none()
            && !same_name.is_empty()
            && (all_same_name.len() > 1 || unmatched_snapshot_names > 1)
        {
            protected_roles.extend(all_same_name.iter().map(|v| id(v).to_owned()));
            unavailable.push(format!("Ambiguous role name: {}", label(r)));
            continue;
        }
        if r["managed"] == true {
            if let Some(v) = existing {
                role_map.insert(old.into(), id(v).into());
                used_roles.insert(id(v));
            } else {
                unavailable.push(format!("Managed role cannot be recreated: {}", label(r)));
            }
            continue;
        }
        let mut data = fields(
            r,
            &[
                "name",
                "color",
                "permissions",
                "hoist",
                "mentionable",
                "unicode_emoji",
            ],
        );
        image_field(r, &mut data, "icon");
        if old == saved.guild_id {
            data = fields(r, &["permissions"]);
        }
        if let Some(v) = existing {
            role_map.insert(old.into(), id(v).into());
            used_roles.insert(id(v));
            // @everyone es un rol existente: su permiso se restaura al deshacer cambios.
            if undo
                && data
                    .as_object()
                    .unwrap()
                    .iter()
                    .any(|(k, val)| v.get(k) != Some(val))
            {
                step(&mut steps, "role_update", old, data, label(r));
            }
        } else {
            step(&mut steps, "role_create", old, data, label(r));
        }
        if old != saved.guild_id && (existing.is_none() || undo) {
            positions.push(json!({"id":reference("role",old),"position":r["position"]}));
        }
    }
    if !positions.is_empty() {
        step(
            &mut steps,
            "role_order",
            "",
            json!(positions),
            "Restore role order",
        );
    }
    let mut ordered = saved.channels.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|c| (c["type"] != 4, c["position"].as_i64().unwrap_or_default()));
    for c in ordered {
        let old = id(c);
        let existing = channels.iter().find(|v| id(v) == old);
        if existing.is_none() && channels.iter().any(|v| label(v) == label(c)) {
            unavailable.push(format!("Channel name already exists: {}", label(c)));
            continue;
        }
        if let Some(v) = existing {
            channel_map.insert(old.into(), id(v).into());
            used_channels.insert(id(v));
        }
        let recreated_parent = c["parent_id"].as_str().is_some_and(|p| {
            steps
                .iter()
                .any(|s| s.kind == "channel_create" && s.target == p)
        });
        let recreated_permission = c["permission_overwrites"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|o| {
                o["type"] == 0
                    && steps
                        .iter()
                        .any(|s| s.kind == "role_create" && s.target == id(o))
            });
        if existing.is_some() && !undo && !recreated_parent && !recreated_permission {
            continue;
        }
        if !matches!(c["type"].as_u64(), Some(0 | 2 | 4 | 5 | 13 | 15 | 16)) {
            unavailable.push(format!("Unsupported channel type: {}", label(c)));
            continue;
        }
        if let Some(parent) = c["parent_id"].as_str() {
            if !channel_map.contains_key(parent)
                && !steps
                    .iter()
                    .any(|s| s.kind == "channel_create" && s.target == parent)
            {
                unavailable.push(format!("Parent category is unavailable: {}", label(c)));
                continue;
            }
        }
        let mut data = fields(
            c,
            &[
                "name",
                "type",
                "position",
                "topic",
                "nsfw",
                "rate_limit_per_user",
                "bitrate",
                "user_limit",
                "rtc_region",
                "video_quality_mode",
                "default_auto_archive_duration",
                "default_thread_rate_limit_per_user",
                "default_sort_order",
                "default_forum_layout",
                "default_reaction_emoji",
                "available_tags",
            ],
        );
        if let Some(tags) = data["available_tags"].as_array_mut() {
            for tag in tags {
                tag.as_object_mut().map(|o| o.remove("id"));
            }
        }
        data["parent_id"] = c["parent_id"]
            .as_str()
            .map(|p| reference("channel", p))
            .unwrap_or(Value::Null);
        let mut overwrites = Vec::new();
        let mut unresolved = false;
        for o in c["permission_overwrites"].as_array().into_iter().flatten() {
            if o["type"] == 1 && !current_members.contains(id(o)) {
                unresolved = true;
                unavailable.push(format!("Member is no longer in the server: {}", label(o)));
                continue;
            }
            if o["type"] == 0
                && !role_map.contains_key(id(o))
                && !steps
                    .iter()
                    .any(|s| s.kind == "role_create" && s.target == id(o))
            {
                unresolved = true;
                unavailable.push(format!("Permission role unavailable: {}", label(o)));
                continue;
            }
            let mut value = fields(o, &["id", "type", "allow", "deny"]);
            if o["type"] == 0 {
                value["id"] = reference("role", id(o));
            }
            overwrites.push(value);
        }
        if unresolved {
            unavailable.push(format!(
                "Channel permissions unresolved; access closed: {}",
                label(c)
            ));
            // No se conserva ningún allow que pudiera volver a conceder ViewChannel.
            overwrites = vec![json!({"id":saved.guild_id,"type":0,"allow":"0","deny":"1024"})];
        }
        if let Some(current) = existing.filter(|_| !undo) {
            // Recuperar dependencias borradas sin deshacer otros cambios vivos.
            data = json!({});
            if recreated_parent {
                data["parent_id"] = reference("channel", c["parent_id"].as_str().unwrap());
            }
            if recreated_permission {
                if !unresolved {
                    let mut merged = current["permission_overwrites"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    for overwrite in overwrites {
                        if let Some(old) = overwrite["id"]["role"].as_str() {
                            if steps
                                .iter()
                                .any(|s| s.kind == "role_create" && s.target == old)
                            {
                                merged.push(overwrite);
                            }
                        }
                    }
                    overwrites = merged;
                }
                data["permission_overwrites"] = json!(overwrites);
            }
        } else {
            data["permission_overwrites"] = json!(overwrites);
        }
        if let Some(current) = existing {
            if current["type"] != c["type"]
                && matches!(current["type"].as_u64(), Some(0 | 5))
                && matches!(c["type"].as_u64(), Some(0 | 5))
            {
                // Discord permite convertir texto ↔ anuncios si el servidor lo admite.
            } else {
                data.as_object_mut().unwrap().remove("type");
                if current["type"] != c["type"] {
                    unavailable.push(format!("Channel type cannot be restored: {}", label(c)));
                }
            }
        }
        step(
            &mut steps,
            if existing.is_some() {
                "channel_update"
            } else {
                "channel_create"
            },
            old,
            data,
            label(c),
        );
    }
    let channel_positions: Vec<Value> = steps
        .iter()
        .filter(|s| {
            matches!(s.kind.as_str(), "channel_create" | "channel_update")
                && s.data.get("position").is_some()
        })
        .map(|s| json!({"id":reference("channel",&s.target),"position":s.data["position"]}))
        .collect();
    if !channel_positions.is_empty() {
        step(
            &mut steps,
            "channel_order",
            "",
            json!(channel_positions),
            "Restore channel order",
        );
    }
    // En modo por defecto solo se añaden asignaciones que faltan.
    for member in &saved.members {
        let Some(current) = members.iter().find(|m| id(m) == id(member)) else {
            unavailable.push(format!(
                "Member is no longer in the server: {}",
                label(member)
            ));
            continue;
        };
        let wanted: HashSet<&str> = member["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["id"].as_str())
            .collect();
        for old in &wanted {
            let available = role_map.contains_key(*old)
                || steps
                    .iter()
                    .any(|s| s.kind == "role_create" && s.target == *old);
            if !available {
                continue;
            }
            let has = role_map.get(*old).is_some_and(|mapped| {
                current["roles"]
                    .as_array()
                    .is_some_and(|rs| rs.contains(&json!(mapped)))
            });
            if !has {
                step(
                    &mut steps,
                    "member_add",
                    id(member),
                    reference("role", old),
                    label(member),
                );
            }
        }
        if undo {
            for role in current["roles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !protected_roles.contains(role)
                    && !wanted
                        .iter()
                        .any(|r| role_map.get(*r).is_some_and(|v| v == role))
                    && roles.iter().any(|r| id(r) == role && r["managed"] != true)
                {
                    step(
                        &mut steps,
                        "member_remove",
                        id(member),
                        json!(role),
                        label(member),
                    );
                }
            }
        }
    }
    if undo {
        let mut data = fields(
            &saved.guild,
            &[
                "name",
                "description",
                "verification_level",
                "default_message_notifications",
                "explicit_content_filter",
                "afk_timeout",
                "system_channel_flags",
                "preferred_locale",
                "premium_progress_bar_enabled",
            ],
        );
        for key in ["icon", "banner", "splash", "discovery_splash"] {
            image_field(&saved.guild, &mut data, key);
        }
        for key in [
            "afk_channel_id",
            "system_channel_id",
            "rules_channel_id",
            "public_updates_channel_id",
            "safety_alerts_channel_id",
        ] {
            if let Some(v) = saved.guild.get(key) {
                if let Some(old) = v.as_str() {
                    if !channel_map.contains_key(old)
                        && !steps
                            .iter()
                            .any(|s| s.kind == "channel_create" && s.target == old)
                    {
                        unavailable.push(format!(
                            "Server setting references an unavailable channel: {key}"
                        ));
                        continue;
                    }
                }
                data[key] = v
                    .as_str()
                    .map(|s| reference("channel", s))
                    .unwrap_or(Value::Null);
            }
        }
        step(
            &mut steps,
            "guild_update",
            "",
            data,
            "Restore server settings",
        );
        if saved.guild.get("widget_enabled").is_some()
            && saved.guild["widget_channel_id"].as_str().is_none_or(|old| {
                channel_map.contains_key(old)
                    || steps
                        .iter()
                        .any(|s| s.kind == "channel_create" && s.target == old)
            })
        {
            let data = json!({"enabled":saved.guild["widget_enabled"],"channel_id":saved.guild["widget_channel_id"].as_str().map(|s|reference("channel",s)).unwrap_or(Value::Null)});
            step(
                &mut steps,
                "widget_update",
                "",
                data,
                "Restore widget settings",
            );
        }
    }
    if delete_extra {
        // Los conflictos por nombre se conservan: no borrarlos para recrearlos.
        for c in channels.iter().filter(|c| {
            !used_channels.contains(id(c)) && !saved.channels.iter().any(|s| label(s) == label(c))
        }) {
            step(&mut steps, "channel_delete", id(c), Value::Null, label(c));
        }
        for r in roles.iter().filter(|r| {
            !used_roles.contains(id(r))
                && !protected_roles.contains(id(r))
                && id(r) != saved.guild_id
                && r["managed"] != true
        }) {
            step(&mut steps, "role_delete", id(r), Value::Null, label(r));
        }
    }
    unavailable.sort();
    unavailable.dedup();
    Job {
        pool: None,
        bot: String::new(),
        guild: saved.guild_id.clone(),
        preview: Preview {
            id: uuid::Uuid::new_v4().to_string(),
            guild_name: label(guild).into(),
            estimated_seconds: steps.len() * 2,
            steps,
            unavailable,
            delete_extra,
        },
        roles: role_map,
        channels: channel_map,
        results: Vec::new(),
        members: saved.members.clone(),
        stop: Arc::new(AtomicBool::new(false)),
        running: Arc::new(AtomicBool::new(false)),
    }
}

pub(crate) async fn preview(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
    snapshot: &str,
    token: &str,
    base: Option<&str>,
    undo: bool,
    delete_extra: bool,
) -> Result<Preview> {
    let saved = load(pool, bot, guild, snapshot).await?;
    let guild_id = guild.parse()?;
    let settings = get(
        token,
        base,
        Route::GetGuild {
            guild_id,
            with_counts: false,
        },
    )
    .await?;
    if label(&settings).trim().is_empty() {
        bail!("Invalid response from Discord");
    }
    let roles = array(get(token, base, Route::GetGuildRoles { guild_id }).await?)?;
    let channels = array(get(token, base, Route::GetChannels { guild_id }).await?)?;
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
            .and_then(|m| m["user"]["id"].as_str())
            .map(str::parse::<u64>)
            .transpose()?;
        members.extend(
            page.into_iter()
                .map(|m| json!({"id":m["user"]["id"],"roles":m["roles"]})),
        );
        if !full {
            break;
        }
        if next.is_none() || next <= after {
            bail!("Invalid response from Discord");
        }
        after = next;
    }
    let mut job = build(
        &saved,
        &settings,
        &roles,
        &channels,
        &members,
        undo,
        delete_extra,
    );
    job.bot = bot.into();
    job.pool = Some(pool.clone());
    let result = job.preview.clone();
    // El MCP y la app son procesos separados: conservar exactamente lo revisado.
    sqlx::query("INSERT INTO snapshot_restore_proposals(id,bot_id,guild_id,data) VALUES(?,?,?,?)")
        .bind(&result.id)
        .bind(bot)
        .bind(guild)
        .bind(serde_json::to_string(&job)?)
        .execute(pool)
        .await?;
    let mut jobs = jobs().lock().unwrap();
    // Conservar las propuestas toda la sesión: una restauración detenida o
    // fallida necesita sus resultados e IDs creados para continuar sin repetir
    // acciones. Generar otra propuesta no invalida ninguna aprobación anterior.
    jobs.insert(result.id.clone(), Arc::new(Mutex::new(job)));
    Ok(result)
}
/// Solo el ejecutor de propuestas aprobadas importa el trabajo del MCP.
pub(crate) async fn load_reviewed(pool: &SqlitePool, state: &AppState, key: &str) -> Result<()> {
    if jobs().lock().unwrap().contains_key(key) {
        return Ok(());
    }
    let bot = state.get_active_bot().context("No bot selected")?;
    let guild = state.get_active_guild().context("No server selected")?;
    let data: String = sqlx::query_scalar(
        "SELECT data FROM snapshot_restore_proposals WHERE id=? AND bot_id=? AND guild_id=?",
    )
    .bind(key)
    .bind(&bot.id)
    .bind(&guild)
    .fetch_optional(pool)
    .await?
    .context("Restore proposal is unavailable")?;
    let mut job: Job = serde_json::from_str(&data)?;
    job.pool = Some(pool.clone());
    jobs()
        .lock()
        .unwrap()
        .entry(key.into())
        .or_insert_with(|| Arc::new(Mutex::new(job)));
    Ok(())
}

#[cfg(test)]
pub(crate) fn forget_for_test(key: &str) {
    jobs().lock().unwrap().remove(key);
}

fn resolve(
    value: &Value,
    roles: &HashMap<String, String>,
    channels: &HashMap<String, String>,
) -> Result<Value> {
    if let Some(old) = value.get("role").and_then(Value::as_str) {
        return roles
            .get(old)
            .map(|v| json!(v))
            .context("Restore dependency is unavailable");
    }
    if let Some(old) = value.get("channel").and_then(Value::as_str) {
        return channels
            .get(old)
            .map(|v| json!(v))
            .context("Restore dependency is unavailable");
    }
    Ok(match value {
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| resolve(v, roles, channels))
                .collect::<Result<_>>()?,
        ),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| Ok((k.clone(), resolve(v, roles, channels)?)))
                .collect::<Result<_>>()?,
        ),
        _ => value.clone(),
    })
}
async fn send(token: &str, base: Option<&str>, route: Route<'_>, data: &Value) -> Result<Value> {
    let mut builder = twilight_http::request::Request::builder(&route);
    if !data.is_null() {
        builder = builder.json(data)?;
    }
    let bytes = discord::build_client(token, base)
        .request::<twilight_http::response::marker::EmptyBody>(builder.build())
        .await
        .map_err(discord::traducir_error)
        .map_err(anyhow::Error::msg)?
        .bytes()
        .await?;
    if bytes.is_empty() {
        Ok(Value::Null)
    } else {
        Ok(serde_json::from_slice(&bytes)?)
    }
}
fn target(map: &HashMap<String, String>, old: &str) -> Result<u64> {
    map.get(old)
        .context("Restore dependency is unavailable")?
        .parse()
        .map_err(Into::into)
}
async fn perform(
    token: &str,
    base: Option<&str>,
    guild: &str,
    s: &Step,
    roles: &mut HashMap<String, String>,
    channels: &mut HashMap<String, String>,
) -> Result<()> {
    let guild_id = guild.parse()?;
    let data = resolve(&s.data, roles, channels)?;
    let route = match s.kind.as_str() {
        "role_create" => Route::CreateRole { guild_id },
        "role_update" => Route::UpdateRole {
            guild_id,
            role_id: target(roles, &s.target)?,
        },
        "role_order" => Route::UpdateRolePositions { guild_id },
        "channel_create" => Route::CreateChannel { guild_id },
        "channel_update" => Route::UpdateChannel {
            channel_id: target(channels, &s.target)?,
        },
        "channel_order" => Route::UpdateGuildChannels { guild_id },
        "guild_update" => Route::UpdateGuild { guild_id },
        "widget_update" => Route::UpdateGuildWidgetSettings { guild_id },
        "member_add" => Route::AddMemberRole {
            guild_id,
            user_id: s.target.parse()?,
            role_id: data
                .as_str()
                .context("Restore dependency is unavailable")?
                .parse()?,
        },
        "member_remove" => Route::RemoveMemberRole {
            guild_id,
            user_id: s.target.parse()?,
            role_id: data
                .as_str()
                .context("Restore dependency is unavailable")?
                .parse()?,
        },
        "role_delete" => Route::DeleteRole {
            guild_id,
            role_id: s.target.parse()?,
        },
        "channel_delete" => Route::DeleteChannel {
            channel_id: s.target.parse()?,
        },
        _ => bail!("Restore proposal is unavailable"),
    };
    let body = if s.kind.starts_with("member_") {
        &Value::Null
    } else {
        &data
    };
    let response = send(token, base, route, body).await?;
    if s.kind == "role_create" || s.kind == "channel_create" {
        let new_id = response["id"]
            .as_str()
            .context("Invalid response from Discord")?;
        let map = if s.kind == "role_create" {
            roles
        } else {
            channels
        };
        map.insert(s.target.clone(), new_id.into());
    }
    Ok(())
}

#[tauri::command]
pub fn stop_snapshot_restore(id: String) {
    if let Some(job) = jobs().lock().unwrap().get(&id) {
        job.lock().unwrap().stop.store(true, Ordering::SeqCst);
    }
}
struct Running(Arc<AtomicBool>);
impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
#[tauri::command]
pub async fn apply_snapshot_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    confirmation: String,
) -> std::result::Result<ResumenPropuesta, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    load_reviewed(&pool, &state, &id)
        .await
        .map_err(|e| e.to_string())?;
    let scope = (
        state.get_active_bot().map(|b| b.id),
        state.get_active_guild(),
    );
    let result = apply(&state, &id, &confirmation, None, |progress| {
        let _ = app.emit(
            "snapshot-restore-progress",
            json!({"id":id,"progress":progress}),
        );
    })
    .await;
    if result.as_ref().is_ok_and(|summary| summary.hechas > 0)
        && scope
            == (
                state.get_active_bot().map(|b| b.id),
                state.get_active_guild(),
            )
    {
        if let (Some(guild), Ok(token)) = (&scope.1, crate::commands::token_del_bot_activo(&state))
        {
            match crate::sync::sincronizar_servidor(&token, None, guild, |_| {}).await {
                Ok(map) => {
                    if scope
                        == (
                            state.get_active_bot().map(|b| b.id),
                            state.get_active_guild(),
                        )
                    {
                        state.set_map(guild.clone(), map);
                        let _ = app.emit(crate::gateway::EVENTO_MAPA, ());
                    }
                }
                Err(error) => {
                    tracing::warn!("No se pudo actualizar el mapa tras restaurar: {error}")
                }
            }
        }
    }
    i18n::localizar_error(&state, result.map_err(|e| format!("{e:#}")))
}
pub(crate) async fn apply(
    state: &AppState,
    key: &str,
    confirmation: &str,
    base: Option<&str>,
    mut progress: impl FnMut(AvanceAccion),
) -> Result<ResumenPropuesta> {
    let external_stop = AtomicBool::new(false);
    apply_cancelable(
        state,
        key,
        confirmation,
        base,
        &external_stop,
        &mut progress,
    )
    .await
}

pub(crate) async fn apply_cancelable(
    state: &AppState,
    key: &str,
    confirmation: &str,
    base: Option<&str>,
    external_stop: &AtomicBool,
    mut progress: impl FnMut(AvanceAccion),
) -> Result<ResumenPropuesta> {
    let job = jobs()
        .lock()
        .unwrap()
        .get(key)
        .cloned()
        .context("Restore proposal is unavailable")?;
    let (bot, guild, preview, mut roles, mut channels, mut results, stop, running) = {
        let j = job.lock().unwrap();
        // Reservar antes de copiar el avance evita que dos Apply usen una copia atrasada.
        if j.running.swap(true, Ordering::SeqCst) {
            bail!("A restore is already running");
        }
        (
            j.bot.clone(),
            j.guild.clone(),
            j.preview.clone(),
            j.roles.clone(),
            j.channels.clone(),
            j.results.clone(),
            j.stop.clone(),
            j.running.clone(),
        )
    };
    let _running = Running(running);
    if state.get_active_bot().map(|b| b.id).as_deref() != Some(&bot)
        || state.get_active_guild().as_deref() != Some(&guild)
    {
        bail!("No server selected");
    }
    if preview.delete_extra && confirmation != preview.guild_name {
        bail!("Type the server name to confirm deletion of extra items");
    }
    stop.store(false, Ordering::SeqCst);
    if results.last().is_some_and(|r| r.estado != EstadoAccion::Ok) {
        results.pop();
    }
    let token = crate::commands::token_del_bot_activo(state).map_err(anyhow::Error::msg)?;
    for (index, s) in preview.steps.iter().enumerate().skip(results.len()) {
        if stop.load(Ordering::SeqCst) || external_stop.load(Ordering::SeqCst) {
            break;
        }
        let attempt = async {
            if state.get_active_bot().map(|b| b.id).as_deref() != Some(&bot)
                || state.get_active_guild().as_deref() != Some(&guild)
            {
                bail!("No server selected");
            }
            let permission = if s.kind.starts_with("role_") || s.kind.starts_with("member_") {
                "Gestionar roles"
            } else if s.kind.ends_with("update") && s.kind.starts_with("guild")
                || s.kind == "widget_update"
            {
                "Gestionar servidor"
            } else {
                "Gestionar canales"
            };
            crate::commands::exigir_permiso(state, &guild, permission, base)
                .await
                .map_err(anyhow::Error::msg)?;
            if s.kind.starts_with("channel_")
                && s.data["permission_overwrites"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty())
            {
                crate::commands::exigir_permiso(state, &guild, "Gestionar roles", base)
                    .await
                    .map_err(anyhow::Error::msg)?;
            }
            if matches!(
                s.kind.as_str(),
                "role_update" | "role_delete" | "member_add" | "member_remove"
            ) {
                let role = if s.kind == "role_delete" {
                    s.target.clone()
                } else if s.kind == "role_update" {
                    roles
                        .get(&s.target)
                        .cloned()
                        .context("Restore dependency is unavailable")?
                } else {
                    resolve(&s.data, &roles, &channels)?
                        .as_str()
                        .context("Restore dependency is unavailable")?
                        .to_owned()
                };
                if role != guild {
                    crate::commands::exigir_jerarquia(state, &guild, &[(&role, None)], base)
                        .await
                        .map_err(anyhow::Error::msg)?;
                }
            }
            if s.kind == "role_order" {
                let resolved = resolve(&s.data, &roles, &channels)?;
                for p in resolved
                    .as_array()
                    .context("Restore dependency is unavailable")?
                {
                    crate::commands::exigir_jerarquia(
                        state,
                        &guild,
                        &[(p["id"].as_str().unwrap_or_default(), p["position"].as_u64())],
                        base,
                    )
                    .await
                    .map_err(anyhow::Error::msg)?;
                }
            }
            perform(&token, base, &guild, s, &mut roles, &mut channels).await
        }
        .await;
        let ok = attempt.is_ok();
        let result = ResultadoAccion {
            indice: index,
            herramienta: s.kind.clone(),
            descripcion: i18n::tr(&i18n::idioma(state), &s.description),
            estado: if ok {
                EstadoAccion::Ok
            } else {
                EstadoAccion::Error
            },
            mensaje: attempt
                .err()
                .map(|e| i18n::tr(&state.config.lock().unwrap().ui_language, &format!("{e:#}")))
                .unwrap_or_default(),
        };
        progress(AvanceAccion::de(preview.steps.len(), &result));
        results.push(result);
        let persisted = {
            let mut j = job.lock().unwrap();
            j.roles = roles.clone();
            j.channels = channels.clone();
            j.results = results.clone();
            j.pool
                .clone()
                .map(|pool| serde_json::to_string(&*j).map(|data| (pool, data)))
                .transpose()?
        };
        if let Some((pool, data)) = persisted {
            sqlx::query("UPDATE snapshot_restore_proposals SET data=? WHERE id=?")
                .bind(data)
                .bind(key)
                .execute(&pool)
                .await?;
        }
        // Detener ante error evita aplicar dependencias o borrados posteriores.
        if !ok {
            break;
        }
    }
    if results.len() == preview.steps.len() && results.iter().all(|r| r.estado == EstadoAccion::Ok)
    {
        let (pool, members) = {
            let j = job.lock().unwrap();
            (j.pool.clone(), j.members.clone())
        };
        if let Some(pool) = pool {
            super::departed::enable(&pool, &bot, &guild, &members, &roles).await?;
        }
    }
    Ok(ResumenPropuesta::de(preview.steps.len(), results))
}

#[cfg(test)]
mod tests;
