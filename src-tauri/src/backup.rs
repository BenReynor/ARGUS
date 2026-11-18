//! Copias portátiles. Solo datos de una lista cerrada; nunca SQL procedente del archivo.
use crate::{crypto, db, i18n, state::AppState};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::{collections::BTreeMap, path::Path, sync::Mutex};
use tauri::{AppHandle, Manager, State};

const TABLES: &[&str] = &[
    "bots",
    "config",
    "settings",
    "guilds",
    "guild_settings",
    "plan_servidor",
    "contexto_servidor",
    "templates",
    "cli_agents",
    "vigilance",
    "chat_diario",
];
const CONVERSATIONS: &[&str] = &["conversacion_asistente", "messages"];
const SCHEDULE: &str = "backup_schedule";
const MAX_SIZE: usize = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    version: u32,
    created_at: String,
    conversations: bool,
    tables: BTreeMap<String, Vec<Value>>,
}
#[derive(Serialize)]
pub struct Preview {
    id: String,
    created_at: String,
    conversations: bool,
    counts: BTreeMap<String, usize>,
    bots: Vec<String>,
}
#[derive(Default)]
pub struct Pending(Mutex<Option<(String, Snapshot)>>);

/// Convierte también credenciales en JSON embebido (config y personalidad).
/// En el archivo, los secretos quedan protegidos por el cifrado exterior.
fn secrets(value: &mut Value, exporting: bool) -> Result<()> {
    match value {
        Value::Object(fields) => {
            for (name, v) in fields {
                if name.ends_with("_ciphertext") {
                    if let Some(s) = v.as_str().filter(|s| !s.is_empty()) {
                        *v = if exporting {
                            crypto::decrypt(s)?
                        } else {
                            crypto::encrypt(s)?
                        }
                        .into();
                    }
                } else {
                    secrets(v, exporting)?;
                }
            }
        }
        Value::Array(values) => {
            for v in values {
                secrets(v, exporting)?;
            }
        }
        _ => {}
    }
    Ok(())
}

// Solo interpretamos JSON en columnas estructuradas. Un plan, prompt o mensaje
// puede contener JSON literal y debe conservarse exactamente como lo escribió el usuario.
fn row_secrets(table: &str, row: &mut Value, exporting: bool) -> Result<()> {
    if table == "bots" {
        return secrets(row, exporting);
    }
    let field = match table {
        "config" if row["key"] == "config" => "value",
        "guild_settings" => "data",
        "cli_agents" => "config",
        _ => return Ok(()),
    };
    if let Some(text) = row[field].as_str() {
        let mut nested: Value = serde_json::from_str(text)?;
        let original = nested.clone();
        secrets(&mut nested, exporting)?;
        if nested != original {
            row[field] = serde_json::to_string(&nested)?.into();
        }
    }
    Ok(())
}

async fn columns(conn: &mut SqliteConnection, table: &str) -> Result<Vec<String>> {
    Ok(sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(conn)
        .await?
        .iter()
        .map(|r| r.get("name"))
        .collect())
}

pub async fn snapshot(pool: &SqlitePool, conversations: bool) -> Result<Snapshot> {
    let mut tx = pool.begin().await?;
    let mut tables = BTreeMap::new();
    for table in TABLES
        .iter()
        .chain(if conversations { CONVERSATIONS } else { &[] })
    {
        let cols = columns(&mut tx, table).await?;
        let fields = cols
            .iter()
            .map(|c| format!("'{c}', \"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!("SELECT json_object({fields}) FROM {table}");
        let rows: Vec<String> = sqlx::query_scalar(&query).fetch_all(&mut *tx).await?;
        let mut rows = rows
            .into_iter()
            .map(|s| serde_json::from_str::<Value>(&s))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if *table == "config" {
            rows.retain(|r| r["key"] != SCHEDULE);
        }
        for row in &mut rows {
            row_secrets(table, row, true)?;
        }
        tables.insert(table.to_string(), rows);
    }
    tx.commit().await?;
    Ok(Snapshot {
        version: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
        conversations,
        tables,
    })
}

pub async fn export(pool: &SqlitePool, password: String, conversations: bool) -> Result<Vec<u8>> {
    let plain = serde_json::to_string(&snapshot(pool, conversations).await?)?;
    let data =
        tokio::task::spawn_blocking(move || crypto::encrypt_backup(&plain, &password)).await??;
    if data.len() > MAX_SIZE {
        bail!("Backup file is too large");
    }
    Ok(data)
}

async fn decode(data: Vec<u8>, password: String) -> Result<Snapshot> {
    if data.len() > MAX_SIZE {
        bail!("Backup file is too large");
    }
    tokio::task::spawn_blocking(move || {
        let plain = crypto::decrypt_backup(&data, &password)?;
        let s: Snapshot = serde_json::from_str(&plain)
            .map_err(|_| anyhow::anyhow!("Invalid or unsupported backup file"))?;
        if s.version != 1
            || TABLES.iter().any(|t| !s.tables.contains_key(*t))
            || CONVERSATIONS
                .iter()
                .any(|t| s.tables.contains_key(*t) != s.conversations)
            || s.tables
                .keys()
                .any(|t| !TABLES.contains(&t.as_str()) && !CONVERSATIONS.contains(&t.as_str()))
        {
            bail!("Invalid or unsupported backup file");
        }
        Ok(s)
    })
    .await?
}

async fn validate(conn: &mut SqliteConnection, s: &Snapshot) -> Result<()> {
    for (table, rows) in &s.tables {
        let cols = columns(conn, table).await?;
        for row in rows {
            let Some(fields) = row.as_object() else {
                bail!("Invalid or unsupported backup file")
            };
            if fields.len() != cols.len()
                || cols.iter().any(|c| !fields.contains_key(c))
                || fields
                    .values()
                    .any(|v| !v.is_null() && !v.is_string() && !v.is_number())
                || (table == "config" && row["key"] == SCHEDULE)
            {
                bail!("Invalid or unsupported backup file");
            }
            if table == "config" && row["key"] == "config" {
                serde_json::from_str::<crate::state::Config>(row["value"].as_str().unwrap_or(""))
                    .map_err(|_| anyhow::anyhow!("Invalid or unsupported backup file"))?;
            }
        }
    }
    Ok(())
}

/// Una transacción garantiza que cualquier error deja todos los datos intactos.
/// Fusionar conserva los registros locales que coinciden en su clave primaria.
pub async fn restore(pool: &SqlitePool, mut s: Snapshot, replace: bool) -> Result<()> {
    let mut tx = pool.begin().await?;
    validate(&mut tx, &s).await?;
    for (table, rows) in &mut s.tables {
        for row in rows {
            row_secrets(table, row, false)?;
        }
    }
    if replace {
        // Si no hay conversaciones en la copia, reemplazar también elimina las locales.
        for table in TABLES.iter().chain(CONVERSATIONS) {
            let q = if *table == "config" {
                format!("DELETE FROM {table} WHERE key != '{SCHEDULE}'")
            } else {
                format!("DELETE FROM {table}")
            };
            sqlx::query(&q).execute(&mut *tx).await?;
        }
    }
    for (table, rows) in s.tables {
        let cols = columns(&mut tx, &table).await?;
        let names = cols
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        let placeholders = vec!["?"; cols.len()].join(",");
        let query =
            format!("INSERT INTO {table} ({names}) VALUES ({placeholders}) ON CONFLICT DO NOTHING");
        for row in rows {
            let mut q = sqlx::query(&query);
            for col in &cols {
                q = match &row[col] {
                    Value::Null => q.bind(None::<String>),
                    Value::String(s) => q.bind(s.clone()),
                    Value::Number(n) if n.is_i64() => q.bind(n.as_i64()),
                    Value::Number(n) => q.bind(n.as_f64()),
                    _ => unreachable!(),
                };
            }
            q.execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

fn localized<T>(state: &AppState, result: Result<T>) -> std::result::Result<T, String> {
    i18n::localizar_error(state, result.map_err(|e| e.to_string()))
}

#[tauri::command]
pub async fn export_configuration(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    password: String,
    conversations: bool,
) -> std::result::Result<(), String> {
    let result = async {
        if !path.ends_with(".argusbak") {
            bail!("Choose a .argusbak file");
        }
        let data = export(&db::get_pool(&app).await?, password, conversations).await?;
        write_private(Path::new(&path), &data).await
    }
    .await;
    localized(&state, result)
}

#[tauri::command]
pub async fn preview_configuration(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, Pending>,
    path: String,
    password: String,
) -> std::result::Result<Preview, String> {
    *pending.0.lock().unwrap() = None;
    let result = async {
        if tokio::fs::metadata(&path).await?.len() > MAX_SIZE as u64 {
            bail!("Backup file is too large");
        }
        let s = decode(tokio::fs::read(path).await?, password).await?;
        let pool = db::get_pool(&app).await?;
        validate(&mut *pool.acquire().await?, &s).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let preview = Preview {
            id: id.clone(),
            created_at: s.created_at.clone(),
            conversations: s.conversations,
            counts: s.tables.iter().map(|(k, v)| (k.clone(), v.len())).collect(),
            bots: s.tables["bots"]
                .iter()
                .filter_map(|r| r["name"].as_str().map(str::to_string))
                .collect(),
        };
        *pending.0.lock().unwrap() = Some((id, s));
        Ok(preview)
    }
    .await;
    localized(&state, result)
}

#[tauri::command]
pub async fn import_configuration(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, Pending>,
    id: String,
    replace: bool,
    confirmed: bool,
) -> std::result::Result<(), String> {
    let result = async {
        if !confirmed {
            bail!("Confirm the backup import before continuing");
        }
        if state.gateway_task.lock().unwrap().is_some() {
            bail!("Stop the bot before importing a backup");
        }
        let s = {
            let mut p = pending.0.lock().unwrap();
            if p.as_ref().is_none_or(|(key, _)| key != &id) {
                bail!("Preview the backup before importing");
            }
            p.take().unwrap().1
        };
        let pool = db::get_pool(&app).await?;
        restore(&pool, s, replace).await?;
        // Evita que el guardado periódico vuelva a escribir mapas anteriores.
        state.server_maps.lock().unwrap().clear();
        *state.config.lock().unwrap() = crate::state::Config::default();
        state
            .load_config_from_pool(&pool)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(())
    }
    .await;
    localized(&state, result)
}

async fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let result = async {
        let mut f = options.open(&temporary).await?;
        f.write_all(data).await?;
        f.sync_all().await?;
        tokio::fs::rename(&temporary, path).await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(temporary).await;
    }
    result
}

#[derive(Serialize, Deserialize, Default)]
struct Schedule {
    enabled: bool,
    password_ciphertext: String,
    conversations: bool,
}
#[derive(Serialize)]
pub struct ScheduleStatus {
    enabled: bool,
    conversations: bool,
    folder: String,
}

#[tauri::command]
pub async fn backup_schedule(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: Option<bool>,
    password: Option<String>,
    conversations: Option<bool>,
) -> std::result::Result<ScheduleStatus, String> {
    let result = async {
        let pool = db::get_pool(&app).await?;
        let mut schedule = read_schedule(&pool).await?;
        if let Some(enabled) = enabled {
            if enabled {
                let password = password.filter(|s| s.len() >= 8).ok_or_else(|| {
                    anyhow::anyhow!("Use a backup password with at least 8 characters")
                })?;
                schedule.password_ciphertext = crypto::encrypt(&password)?;
            } else {
                schedule.password_ciphertext.clear();
            }
            schedule.enabled = enabled;
            schedule.conversations = conversations.unwrap_or(false);
            db::set_config(&pool, SCHEDULE, &serde_json::to_string(&schedule)?).await?;
        }
        Ok(ScheduleStatus {
            enabled: schedule.enabled,
            conversations: schedule.conversations,
            folder: backup_folder(&app)?.display().to_string(),
        })
    }
    .await;
    localized(&state, result)
}
async fn read_schedule(pool: &SqlitePool) -> Result<Schedule> {
    Ok(db::get_config(pool, SCHEDULE)
        .await?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}
fn backup_folder(app: &AppHandle) -> Result<std::path::PathBuf> {
    Ok(db::ruta_efectiva(&db::ruta_bd(app)?)
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Configuration folder not found"))?
        .join("backups"))
}

/// Se comprueba al arrancar y cada hora: como máximo una copia por fecha UTC.
pub fn start_automatic(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let result = async {
                let pool = db::get_pool(&app).await?;
                let schedule = read_schedule(&pool).await?;
                if schedule.enabled {
                    automatic(
                        &pool,
                        &backup_folder(&app)?,
                        crypto::decrypt(&schedule.password_ciphertext)?,
                        schedule.conversations,
                        &chrono::Utc::now().format("%Y-%m-%d").to_string(),
                    )
                    .await?;
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if result.is_err() {
                // No incluir datos de SQLite, rutas privadas ni secretos en logs.
                tracing::error!("Automatic configuration backup failed");
                use tauri::Emitter;
                let _ = app.emit("backup-error", "Automatic configuration backup failed");
            }
        }
    });
}
async fn automatic(
    pool: &SqlitePool,
    dir: &Path,
    password: String,
    conversations: bool,
    day: &str,
) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let path = dir.join(format!("automatic-{day}.argusbak"));
    if !path.exists() {
        let data = export(pool, password, conversations).await?;
        write_private(&path, &data).await?;
    }
    let mut entries = tokio::fs::read_dir(dir).await?;
    let mut files = Vec::new();
    while let Some(e) = entries.next_entry().await? {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with("automatic-")
            && name.ends_with(".argusbak")
            && e.file_type().await?.is_file()
        {
            files.push(e.path());
        }
    }
    files.sort();
    let excess = files.len().saturating_sub(7);
    for path in files.into_iter().take(excess) {
        tokio::fs::remove_file(path).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// Las dos preferencias de ventana antes vivían solo en el navegador.
#[derive(Serialize, Deserialize)]
pub struct WindowPreferences {
    auto_start: bool,
    minimize_to_tray: bool,
}
#[tauri::command]
pub async fn backup_window_preferences(
    app: AppHandle,
    state: State<'_, AppState>,
    preferences: Option<WindowPreferences>,
) -> std::result::Result<Option<WindowPreferences>, String> {
    let result = async {
        let pool = db::get_pool(&app).await?;
        if let Some(preferences) = preferences {
            db::set_setting(
                &pool,
                "window_preferences",
                &serde_json::to_string(&preferences)?,
            )
            .await?;
            Ok(Some(preferences))
        } else {
            Ok(db::get_setting(&pool, "window_preferences")
                .await?
                .map(|s| serde_json::from_str(&s))
                .transpose()?)
        }
    }
    .await;
    localized(&state, result)
}
