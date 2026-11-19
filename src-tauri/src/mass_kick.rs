//! Operación exclusiva de la ventana Tauri; nunca se registra como herramienta del agente.
use crate::{commands, db, i18n, snapshots, state::AppState};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, State};
use twilight_model::id::Id;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    op: String,
    actor: Option<String>,
    identity_key: Option<String>,
    identity_code: Option<String>,
    excluded_roles: Option<Vec<String>>,
    key: Option<String>,
    confirmation: Option<String>,
    count: Option<usize>,
}
struct Job {
    key: String,
    actor: String,
    excluded: Vec<String>,
    preview: Value,
    status: Value,
    stop: Arc<AtomicBool>,
}
type Jobs = Mutex<HashMap<(String, String), Arc<Mutex<Job>>>>;
fn jobs() -> &'static Jobs {
    static JOBS: OnceLock<Jobs> = OnceLock::new();
    JOBS.get_or_init(Default::default)
}
// El ID escrito no es una identidad. El secreto se entrega solo al DM de esa
// cuenta; la app recibe únicamente una referencia pública. Nunca se persiste.
struct Identity {
    actor: String,
    key: String,
    code_hash: Vec<u8>,
    expires: Instant,
}
type Identities = HashMap<(String, String), Identity>;
fn identities() -> &'static Mutex<Identities> {
    static IDENTITIES: OnceLock<Mutex<Identities>> = OnceLock::new();
    IDENTITIES.get_or_init(Default::default)
}
async fn identify(
    token: &str,
    base: Option<&str>,
    bot: &str,
    guild: &str,
    actor: &str,
    language: &str,
) -> Result<Identity> {
    // Validar membresía y rechazar cuentas bot antes de enviar el desafío.
    let client = crate::discord::build_client(token, base);
    let member = client
        .guild_member(
            Id::new_checked(guild.parse()?).context("No server selected")?,
            Id::new_checked(actor.parse()?).context("Select your own server member ID")?,
        )
        .await
        .map_err(crate::discord::traducir_error)
        .map_err(anyhow::Error::msg)?
        .model()
        .await?;
    if actor == bot || member.user.bot {
        bail!("Select your own server member ID");
    }
    let code = uuid::Uuid::new_v4().to_string();
    let message = i18n::tr(language, &format!("ARGUS member removal verification code: {code}. Enter it only in ARGUS. Never share this code."));
    crate::discord::send_direct_message(token, base, actor, &message)
        .await
        .map_err(anyhow::Error::msg)?;
    Ok(Identity {
        actor: actor.to_owned(),
        key: uuid::Uuid::new_v4().to_string(),
        code_hash: Sha256::digest(code.as_bytes()).to_vec(),
        expires: Instant::now() + Duration::from_secs(300),
    })
}
fn verified_actor(
    identities: &Identities,
    scope: &(String, String),
    r: &Request,
) -> Result<String> {
    let identity = identities
        .get(scope)
        .context("Verify your Discord identity again")?;
    if r.actor.is_some()
        || identity.expires <= Instant::now()
        || r.identity_key.as_deref() != Some(&identity.key)
        || r.identity_code
            .as_ref()
            .is_none_or(|code| Sha256::digest(code.as_bytes()).as_slice() != identity.code_hash)
    {
        bail!("Verify your Discord identity again");
    }
    Ok(identity.actor.clone())
}
fn plan(
    s: &snapshots::ServerSnapshot,
    bot: &str,
    actor: &str,
    excluded: Option<&[String]>,
) -> Result<Value> {
    let owner = s.guild["owner_id"]
        .as_str()
        .context("Invalid response from Discord")?;
    if actor == bot || !s.members.iter().any(|m| m["id"] == actor) {
        bail!("Select your own server member ID");
    }
    let excluded: Vec<String> = excluded.map(|v| v.to_vec()).unwrap_or_else(|| {
        s.roles
            .iter()
            .filter(|r| {
                r["permissions"]
                    .as_str()
                    .and_then(|p| p.parse::<u64>().ok())
                    .unwrap_or(0)
                    & 8
                    != 0
            })
            .filter_map(|r| r["id"].as_str().map(str::to_owned))
            .collect()
    });
    let highest = |m: &Value| {
        s.roles
            .iter()
            .filter(|r| {
                r["id"] != s.guild_id
                    && m["roles"]
                        .as_array()
                        .is_some_and(|rs| rs.iter().any(|x| x["id"] == r["id"]))
            })
            .map(|r| r["position"].as_i64().unwrap_or(0))
            .max()
            .unwrap_or(-1)
    };
    let bot_member = s
        .members
        .iter()
        .find(|m| m["id"] == bot)
        .context("No bot selected")?;
    let mut targets = vec![];
    let mut skipped = vec![];
    for m in &s.members {
        let id = m["id"].as_str().context("Invalid response from Discord")?;
        if [owner, bot, actor].contains(&id)
            || m["roles"]
                .as_array()
                .is_some_and(|rs| rs.iter().any(|r| excluded.iter().any(|e| r["id"] == *e)))
            || excluded.iter().any(|r| r == &s.guild_id)
        {
            continue;
        }
        if highest(m) >= highest(bot_member) {
            skipped.push(
                json!({"id":id,"name":m["name"],"error":"Role is unavailable or above the bot"}),
            );
        } else {
            targets.push(json!({"id":id,"name":m["name"]}));
        }
    }
    Ok(
        json!({"guild_name":s.guild["name"],"targets":targets,"skipped":skipped,"roles":s.roles,"excluded_roles":excluded}),
    )
}

fn validate_start(job: &Job, request: &Request, verified: &str) -> Result<()> {
    if job.actor != verified
        || job.status["phase"] != "ready"
        || request.key.as_deref() != Some(&job.key)
        || request.confirmation.as_deref() != job.preview["guild_name"].as_str()
        || request.count != job.preview["targets"].as_array().map(Vec::len)
    {
        bail!("Review the member removal again");
    }
    Ok(())
}

// La misma barrera se usa en producción y con Discord falso: cualquier fallo
// de captura, cambio de candidatos o escritura en SQLite impide los DELETE.
async fn automatic_snapshot(
    pool: &sqlx::SqlitePool,
    token: &str,
    base: Option<&str>,
    bot: &str,
    guild: &str,
    actor: &str,
    excluded: &[String],
    preview: &Value,
) -> Result<String> {
    let snapshot = snapshots::capture(token, base, guild).await?;
    if plan(&snapshot, bot, actor, Some(excluded))? != *preview {
        bail!("Review the member removal again");
    }
    snapshots::store(pool, bot, guild, &snapshot, true).await
}

// Un único cliente conserva los buckets de twilight entre expulsiones y sus reintentos 429.
async fn execute(
    token: &str,
    base: Option<&str>,
    guild: &str,
    targets: &[Value],
    stop: &AtomicBool,
    excluded: &[String],
    mut progress: impl FnMut(Value),
) -> Result<()> {
    let client = crate::discord::build_client(token, base);
    for (index, m) in targets.iter().enumerate() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let gid = Id::new(guild.parse()?);
        let uid = Id::new(
            m["id"]
                .as_str()
                .context("Invalid response from Discord")?
                .parse()?,
        );
        let result: std::result::Result<(), String> = async {
            let member = client
                .guild_member(gid, uid)
                .await
                .map_err(crate::discord::traducir_error)?
                .model()
                .await
                .map_err(|_| "Invalid response from Discord".to_owned())?;
            if member
                .roles
                .iter()
                .any(|r| excluded.contains(&r.to_string()))
            {
                return Err("Member now has an excluded role".to_owned());
            }
            if stop.load(Ordering::SeqCst) {
                return Err("Stopped".to_owned());
            }
            client
                .remove_guild_member(gid, uid)
                .await
                .map_err(crate::discord::traducir_error)?;
            Ok(())
        }
        .await;
        progress(
            json!({"id":m["id"],"name":m["name"],"ok":result.is_ok(),"error":result.err(),"done":index+1}),
        );
        // Stop se comprueba antes de la próxima petición; la actual puede estar en curso.
        if index + 1 < targets.len() {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn app_mass_kick(
    app: AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
    guild_id: String,
    request: Request,
) -> std::result::Result<Value, String> {
    let language = i18n::idioma(&state);
    handle(&app, &state, &bot_id, &guild_id, request)
        .await
        .map_err(|e| i18n::tr(&language, &format!("{e:#}")))
}
async fn handle(
    app: &AppHandle,
    state: &AppState,
    bot: &str,
    guild: &str,
    r: Request,
) -> Result<Value> {
    let scope = (bot.to_owned(), guild.to_owned());
    // Consultar o detener sigue siendo posible después de cambiar de servidor/bot.
    if r.op == "status" || r.op == "stop" {
        let job = jobs().lock().unwrap().get(&scope).cloned();
        return Ok(if let Some(job) = job {
            let j = job.lock().unwrap();
            if r.op == "stop" {
                j.stop.store(true, Ordering::SeqCst);
            }
            j.status.clone()
        } else {
            Value::Null
        });
    }
    let selected = state.get_active_bot().context("No bot selected")?;
    if selected.id != bot || state.get_active_guild().as_deref() != Some(guild) {
        bail!("No server selected");
    }
    // Fijar el token del bot revisado incluso si la UI cambia de selección.
    let token = crate::crypto::decrypt(&selected.token_ciphertext)?;
    if r.op == "identify" {
        // Un intento nuevo invalida siempre el anterior, incluso si falla el DM.
        identities().lock().unwrap().remove(&scope);
        let identity = identify(
            &token,
            None,
            bot,
            guild,
            r.actor
                .as_deref()
                .context("Select your own server member ID")?,
            &i18n::idioma(state),
        )
        .await?;
        let result = json!({"phase":"verification", "key":identity.key});
        identities().lock().unwrap().insert(scope, identity);
        return Ok(result);
    }
    if r.op == "prepare" {
        let actor = verified_actor(&identities().lock().unwrap(), &scope, &r)?;
        // Reserva antes de lecturas async: impide dos ejecuciones o previews concurrentes.
        let job = Arc::new(Mutex::new(Job {
            key: uuid::Uuid::new_v4().to_string(),
            actor,
            excluded: vec![],
            preview: Value::Null,
            status: json!({"phase":"preparing"}),
            stop: Arc::new(AtomicBool::new(false)),
        }));
        {
            let mut all = jobs().lock().unwrap();
            if all.get(&scope).is_some_and(|j| {
                matches!(
                    j.lock().unwrap().status["phase"].as_str(),
                    Some("running" | "preparing")
                )
            }) {
                bail!("A member removal is already running");
            }
            all.insert(scope, job.clone());
        }
        let result = async {
            commands::exigir_permiso(state, guild, "Expulsar miembros", None)
                .await
                .map_err(anyhow::Error::msg)?;
            let snapshot = snapshots::capture(&token, None, guild).await?;
            let actor = job.lock().unwrap().actor.clone();
            plan(&snapshot, bot, &actor, r.excluded_roles.as_deref())
        }
        .await;
        let mut j = job.lock().unwrap();
        match result {
            Ok(p) => {
                j.excluded = serde_json::from_value(p["excluded_roles"].clone())?;
                j.preview = p.clone();
                j.status = json!({"phase":"ready","key":j.key,"preview":p,"done":0,"total":p["targets"].as_array().unwrap().len(),"results":[]});
            }
            Err(e) => {
                j.status = json!({"phase":"failed"});
                return Err(e);
            }
        }
        return Ok(j.status.clone());
    }
    if r.op != "start" {
        bail!("Invalid member removal request");
    }
    let verified = verified_actor(&identities().lock().unwrap(), &scope, &r)?;
    let job = jobs()
        .lock()
        .unwrap()
        .get(&scope)
        .cloned()
        .context("Review the member removal again")?;
    let (actor, excluded, preview, stop) = {
        let mut j = job.lock().unwrap();
        validate_start(&j, &r, &verified)?;
        j.status["phase"] = json!("running");
        (
            j.actor.clone(),
            j.excluded.clone(),
            j.preview.clone(),
            j.stop.clone(),
        )
    };
    let result = async {
        commands::exigir_permiso(state, guild, "Expulsar miembros", None)
            .await
            .map_err(anyhow::Error::msg)?;
        let pool = db::get_pool(app).await?;
        let id = automatic_snapshot(&pool, &token, None, bot, guild, &actor, &excluded, &preview)
            .await?;
        job.lock().unwrap().status["snapshot_id"] = json!(id);
        let _ = app.emit("snapshots-updated", json!({"bot_id":bot,"guild_id":guild}));
        execute(
            &token,
            None,
            guild,
            preview["targets"].as_array().unwrap(),
            &stop,
            &excluded,
            |mut v| {
                if let Some(error) = v["error"].as_str() {
                    v["error"] = json!(i18n::tr(&i18n::idioma(state), error));
                }
                let mut j = job.lock().unwrap();
                j.status["done"] = v["done"].clone();
                j.status["results"].as_array_mut().unwrap().push(v);
            },
        )
        .await
    }
    .await;
    let mut j = job.lock().unwrap();
    j.status["phase"] = json!(if result.is_err() {
        "failed"
    } else if stop.load(Ordering::SeqCst) {
        "stopped"
    } else {
        "completed"
    });
    if let Err(e) = &result {
        j.status["error"] = json!(i18n::tr(&i18n::idioma(state), &format!("{e:#}")));
    }
    result?;
    Ok(j.status.clone())
}
#[cfg(test)]
mod tests;
