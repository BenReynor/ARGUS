use crate::crypto;
use anyhow::Result;
use sqlx::{Row, SqlitePool};
use tauri::AppHandle;
use tauri::Manager;

const DB_NAME: &str = "argus.db";

pub async fn get_pool(app: &AppHandle) -> Result<SqlitePool> {
    let pool = app
        .try_state::<sqlx::Pool<sqlx::Sqlite>>()
        .ok_or_else(|| anyhow::anyhow!("La base de datos no está abierta"))?;
    Ok(pool.inner().clone())
}

/// Ruta de la base de datos: `<config del sistema>/argus/argus.db`
/// (en Linux `~/.config/argus/argus.db`).
pub fn ruta_bd(app: &AppHandle) -> Result<std::path::PathBuf> {
    Ok(app.path().config_dir()?.join("argus").join(DB_NAME))
}

/// Abre (o crea) la base de datos en `ruta` y aplica las migraciones.
/// La carpeta queda con permisos 0700 y el archivo con 0600 en Unix.
pub async fn abrir_bd(ruta: &std::path::Path) -> Result<SqlitePool> {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    if let Some(dir) = ruta.parent() {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let opciones = SqliteConnectOptions::new()
        .filename(ruta)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opciones)
        .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(0o600))?;
    }
    run_migrations(&pool).await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

// Config table (key-value JSON)
pub async fn get_config(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT value FROM config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get("value")))
}

pub async fn set_config(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO config (key, value, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_all_config(pool: &SqlitePool) -> Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT key, value FROM config")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get("key"), r.get("value")))
        .collect())
}

// Bots table
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BotConfig {
    pub id: String,
    pub name: String,
    pub token_ciphertext: String,
    #[serde(default)]
    pub avatar: Option<String>,
    /// Id de aplicación de Discord (`GET /applications/@me`).
    #[serde(default)]
    pub application_id: Option<String>,
    pub allowed_guilds: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_bots(pool: &SqlitePool) -> Result<Vec<BotConfig>> {
    let rows = sqlx::query(
        "SELECT id, name, token_ciphertext, avatar, application_id, allowed_guilds, created_at, updated_at FROM bots",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| BotConfig {
            id: r.get("id"),
            name: r.get("name"),
            token_ciphertext: r.get("token_ciphertext"),
            avatar: r.get("avatar"),
            application_id: r.get("application_id"),
            allowed_guilds: serde_json::from_str(&r.get::<String, _>("allowed_guilds"))
                .unwrap_or_default(),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn get_bot(pool: &SqlitePool, id: &str) -> Result<Option<BotConfig>> {
    let row = sqlx::query("SELECT id, name, token_ciphertext, avatar, application_id, allowed_guilds, created_at, updated_at FROM bots WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| BotConfig {
        id: r.get("id"),
        name: r.get("name"),
        token_ciphertext: r.get("token_ciphertext"),
        avatar: r.get("avatar"),
        application_id: r.get("application_id"),
        allowed_guilds: serde_json::from_str(&r.get::<String, _>("allowed_guilds"))
            .unwrap_or_default(),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

pub async fn upsert_bot(pool: &SqlitePool, bot: &BotConfig) -> Result<()> {
    sqlx::query(
        "INSERT INTO bots (id, name, token_ciphertext, avatar, application_id, allowed_guilds, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, token_ciphertext = excluded.token_ciphertext, avatar = excluded.avatar, application_id = excluded.application_id, allowed_guilds = excluded.allowed_guilds, updated_at = CURRENT_TIMESTAMP"
    )
    .bind(&bot.id)
    .bind(&bot.name)
    .bind(&bot.token_ciphertext)
    .bind(&bot.avatar)
    .bind(&bot.application_id)
    .bind(serde_json::to_string(&bot.allowed_guilds)?)
    .bind(&bot.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_bot(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM bots WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// Guilds cache
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GuildCache {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub member_count: i64,
    pub joined_at: Option<String>,
}

pub async fn upsert_guild(pool: &SqlitePool, guild: &GuildCache) -> Result<()> {
    sqlx::query(
        "INSERT INTO guilds (id, name, icon, member_count, joined_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, icon = excluded.icon, member_count = excluded.member_count, joined_at = excluded.joined_at"
    )
    .bind(&guild.id)
    .bind(&guild.name)
    .bind(&guild.icon)
    .bind(guild.member_count)
    .bind(&guild.joined_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Deja la tabla `guilds` con exactamente los servidores dados.
pub async fn replace_guilds(pool: &SqlitePool, guilds: &[GuildCache]) -> Result<()> {
    sqlx::query("DELETE FROM guilds").execute(pool).await?;
    for g in guilds {
        upsert_guild(pool, g).await?;
    }
    Ok(())
}

pub async fn list_guilds_cached(pool: &SqlitePool) -> Result<Vec<GuildCache>> {
    let rows = sqlx::query("SELECT id, name, icon, member_count, joined_at FROM guilds")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| GuildCache {
            id: r.get("id"),
            name: r.get("name"),
            icon: r.get("icon"),
            member_count: r.get("member_count"),
            joined_at: r.get("joined_at"),
        })
        .collect())
}

// Server maps
pub async fn get_server_map(pool: &SqlitePool, guild_id: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT data FROM server_maps WHERE guild_id = ?")
        .bind(guild_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get("data")))
}

/// Todos los mapas guardados: (guild_id, JSON).
pub async fn list_server_maps(pool: &SqlitePool) -> Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT guild_id, data FROM server_maps")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get("guild_id"), r.get("data")))
        .collect())
}

pub async fn upsert_server_map(pool: &SqlitePool, guild_id: &str, data: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO server_maps (guild_id, data, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(guild_id) DO UPDATE SET data = excluded.data, updated_at = CURRENT_TIMESTAMP",
    )
    .bind(guild_id)
    .bind(data)
    .execute(pool)
    .await?;
    Ok(())
}

// Templates
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    pub data: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_templates(pool: &SqlitePool) -> Result<Vec<Template>> {
    let rows = sqlx::query("SELECT id, name, description, data, version, created_at, updated_at FROM templates ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| Template {
            id: r.get("id"),
            name: r.get("name"),
            description: r.get("description"),
            data: r.get("data"),
            version: r.get("version"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

pub async fn get_template(pool: &SqlitePool, id: &str) -> Result<Option<Template>> {
    let row = sqlx::query("SELECT id, name, description, data, version, created_at, updated_at FROM templates WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| Template {
        id: r.get("id"),
        name: r.get("name"),
        description: r.get("description"),
        data: r.get("data"),
        version: r.get("version"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }))
}

pub async fn save_template(pool: &SqlitePool, t: &Template) -> Result<()> {
    sqlx::query(
        "INSERT INTO templates (id, name, description, data, version, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, data = excluded.data, version = excluded.version, updated_at = CURRENT_TIMESTAMP"
    )
    .bind(&t.id)
    .bind(&t.name)
    .bind(&t.description)
    .bind(&t.data)
    .bind(t.version)
    .bind(&t.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_template(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM templates WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// CLI Agents
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CliAgentConfig {
    pub id: String,
    pub name: String,
    pub mode: String, // "auto" | "manual"
    pub binary_path: Option<String>,
    pub model: Option<String>,
    pub config: serde_json::Value,
    pub enabled: bool,
}

impl Default for CliAgentConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            mode: "auto".to_string(),
            binary_path: None,
            model: None,
            config: serde_json::Value::Object(serde_json::Map::new()),
            enabled: true,
        }
    }
}

pub async fn list_cli_agents(pool: &SqlitePool) -> Result<Vec<CliAgentConfig>> {
    let rows =
        sqlx::query("SELECT id, name, mode, binary_path, model, config, enabled FROM cli_agents")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|r| CliAgentConfig {
            id: r.get("id"),
            name: r.get("name"),
            mode: r.get("mode"),
            binary_path: r.get("binary_path"),
            model: r.get("model"),
            config: serde_json::from_str(&r.get::<String, _>("config")).unwrap_or_default(),
            enabled: r.get::<i64, _>("enabled") != 0,
        })
        .collect())
}

pub async fn upsert_cli_agent(pool: &SqlitePool, a: &CliAgentConfig) -> Result<()> {
    sqlx::query(
        "INSERT INTO cli_agents (id, name, mode, binary_path, model, config, enabled)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, mode = excluded.mode, binary_path = excluded.binary_path, model = excluded.model, config = excluded.config, enabled = excluded.enabled"
    )
    .bind(&a.id)
    .bind(&a.name)
    .bind(&a.mode)
    .bind(&a.binary_path)
    .bind(&a.model)
    .bind(serde_json::to_string(&a.config)?)
    .bind(if a.enabled { 1 } else { 0 })
    .execute(pool)
    .await?;
    Ok(())
}

// Vigilance config
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct VigilanceConfig {
    pub preset: String,
    pub enabled_events: Vec<String>,
    pub filters: serde_json::Value,
}

pub async fn get_vigilance(pool: &SqlitePool) -> Result<VigilanceConfig> {
    let row = sqlx::query("SELECT preset, enabled_events, filters FROM vigilance WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    Ok(row
        .map(|r| VigilanceConfig {
            preset: r.get("preset"),
            enabled_events: serde_json::from_str(&r.get::<String, _>("enabled_events"))
                .unwrap_or_default(),
            filters: serde_json::from_str(&r.get::<String, _>("filters")).unwrap_or_default(),
        })
        .unwrap_or_default())
}

pub async fn save_vigilance(pool: &SqlitePool, v: &VigilanceConfig) -> Result<()> {
    sqlx::query(
        "INSERT INTO vigilance (id, preset, enabled_events, filters) VALUES (1, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET preset = excluded.preset, enabled_events = excluded.enabled_events, filters = excluded.filters"
    )
    .bind(&v.preset)
    .bind(serde_json::to_string(&v.enabled_events)?)
    .bind(serde_json::to_string(&v.filters)?)
    .execute(pool)
    .await?;
    Ok(())
}

// Messages (chat history / context)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: i64,
    pub guild_id: String,
    pub channel_id: String,
    pub user_id: String,
    pub username: String,
    pub content: String,
    pub role: String, // "user" | "assistant" | "system"
    pub provider: Option<String>,
    pub timestamp: String,
    pub is_archived: bool,
}

pub async fn insert_message(pool: &SqlitePool, m: &Message) -> Result<i64> {
    let row = sqlx::query(
        "INSERT INTO messages (guild_id, channel_id, user_id, username, content, role, provider, timestamp, is_archived)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id"
    )
    .bind(&m.guild_id)
    .bind(&m.channel_id)
    .bind(&m.user_id)
    .bind(&m.username)
    .bind(&m.content)
    .bind(&m.role)
    .bind(&m.provider)
    .bind(&m.timestamp)
    .bind(if m.is_archived { 1 } else { 0 })
    .fetch_one(pool)
    .await?;
    Ok(row.get("id"))
}

pub async fn get_messages(
    pool: &SqlitePool,
    guild_id: &str,
    user_id: &str,
    limit: i64,
) -> Result<Vec<Message>> {
    let rows = sqlx::query(
        "SELECT id, guild_id, channel_id, user_id, username, content, role, provider, timestamp, is_archived
         FROM messages
         WHERE guild_id = ? AND user_id = ? AND is_archived = 0
         ORDER BY timestamp DESC
         LIMIT ?"
    )
    .bind(guild_id)
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Message {
            id: r.get("id"),
            guild_id: r.get("guild_id"),
            channel_id: r.get("channel_id"),
            user_id: r.get("user_id"),
            username: r.get("username"),
            content: r.get("content"),
            role: r.get("role"),
            provider: r.get("provider"),
            timestamp: r.get("timestamp"),
            is_archived: r.get::<i64, _>("is_archived") != 0,
        })
        .collect())
}

pub async fn cleanup_user_context(
    pool: &SqlitePool,
    guild_id: &str,
    user_id: &str,
    keep: i64,
) -> Result<u64> {
    let result = sqlx::query(
        "DELETE FROM messages
         WHERE guild_id = ? AND user_id = ? AND is_archived = 0
         AND id NOT IN (
             SELECT id FROM messages
             WHERE guild_id = ? AND user_id = ? AND is_archived = 0
             ORDER BY timestamp DESC
             LIMIT ?
         )",
    )
    .bind(guild_id)
    .bind(user_id)
    .bind(guild_id)
    .bind(user_id)
    .bind(keep)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn search_messages_fts(
    pool: &SqlitePool,
    query: &str,
    guild_id: Option<&str>,
    channel_id: Option<&str>,
    user_id: Option<&str>,
    limit: i64,
) -> Result<Vec<Message>> {
    let mut q = String::from(
        "SELECT m.id, m.guild_id, m.channel_id, m.user_id, m.username, m.content, m.role, m.provider, m.timestamp, m.is_archived
         FROM messages m
         JOIN messages_fts f ON m.id = f.rowid
         WHERE messages_fts MATCH ?
         AND m.is_archived = 0"
    );
    let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> =
        vec![Box::new(query.to_string())];

    if let Some(g) = guild_id {
        q.push_str(" AND m.guild_id = ?");
        params.push(Box::new(g.to_string()));
    }
    if let Some(c) = channel_id {
        q.push_str(" AND m.channel_id = ?");
        params.push(Box::new(c.to_string()));
    }
    if let Some(u) = user_id {
        q.push_str(" AND m.user_id = ?");
        params.push(Box::new(u.to_string()));
    }
    q.push_str(" ORDER BY m.timestamp DESC LIMIT ?");
    params.push(Box::new(limit.to_string()));

    // Note: dynamic query building with sqlx is complex; in practice use query_as with bind
    // For now, simplified version:
    let rows = sqlx::query(&q)
        .bind(query)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| Message {
            id: r.get("id"),
            guild_id: r.get("guild_id"),
            channel_id: r.get("channel_id"),
            user_id: r.get("user_id"),
            username: r.get("username"),
            content: r.get("content"),
            role: r.get("role"),
            provider: r.get("provider"),
            timestamp: r.get("timestamp"),
            is_archived: r.get::<i64, _>("is_archived") != 0,
        })
        .collect())
}

// Incidents
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Incident {
    pub id: i64,
    pub guild_id: Option<String>,
    pub type_: String,
    pub payload: String,
    pub created_at: String,
}

pub async fn insert_incident(pool: &SqlitePool, i: &Incident) -> Result<()> {
    sqlx::query("INSERT INTO incidents (guild_id, type, payload, created_at) VALUES (?, ?, ?, ?)")
        .bind(&i.guild_id)
        .bind(&i.type_)
        .bind(&i.payload)
        .bind(&i.created_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_incidents(
    pool: &SqlitePool,
    guild_id: Option<&str>,
    limit: i64,
) -> Result<Vec<Incident>> {
    let rows = if let Some(g) = guild_id {
        sqlx::query("SELECT id, guild_id, type, payload, created_at FROM incidents WHERE guild_id = ? ORDER BY created_at DESC LIMIT ?")
            .bind(g)
            .bind(limit)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query("SELECT id, guild_id, type, payload, created_at FROM incidents ORDER BY created_at DESC LIMIT ?")
            .bind(limit)
            .fetch_all(pool)
            .await?
    };
    Ok(rows
        .into_iter()
        .map(|r| Incident {
            id: r.get("id"),
            guild_id: r.get("guild_id"),
            type_: r.get("type"),
            payload: r.get("payload"),
            created_at: r.get("created_at"),
        })
        .collect())
}

// Settings
pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get("value")))
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

// Ajustes propios de cada servidor (por bot)
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GuildSettings {
    pub alias: Option<String>,
    pub notes: String,
    pub vigilance_enabled: bool,
    pub personality_enabled: bool,
    pub templates_enabled: bool,
}

pub async fn get_guild_settings(
    pool: &SqlitePool,
    bot_id: &str,
    guild_id: &str,
) -> Result<GuildSettings> {
    let row = sqlx::query("SELECT data FROM guild_settings WHERE bot_id = ? AND guild_id = ?")
        .bind(bot_id)
        .bind(guild_id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => {
            let data: String = r.get("data");
            Ok(serde_json::from_str(&data).unwrap_or_default())
        }
        None => Ok(GuildSettings::default()),
    }
}

pub async fn save_guild_settings(
    pool: &SqlitePool,
    bot_id: &str,
    guild_id: &str,
    settings: &GuildSettings,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO guild_settings (bot_id, guild_id, data, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(bot_id, guild_id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
    )
    .bind(bot_id)
    .bind(guild_id)
    .bind(serde_json::to_string(settings)?)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod guild_settings_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool_temporal(nombre: &str) -> (SqlitePool, std::path::PathBuf) {
        let ruta =
            std::env::temp_dir().join(format!("argus_test_{}_{}.db", nombre, std::process::id()));
        let _ = std::fs::remove_file(&ruta);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}?mode=rwc", ruta.display()))
            .await
            .expect("BD temporal");
        run_migrations(&pool).await.expect("migraciones");
        (pool, ruta)
    }

    #[tokio::test]
    async fn sin_ajustes_guardados_devuelve_valores_por_defecto() {
        let (pool, ruta) = pool_temporal("defecto").await;
        let s = get_guild_settings(&pool, "b1", "g1").await.unwrap();
        assert_eq!(s, GuildSettings::default());
        assert!(!s.vigilance_enabled && !s.personality_enabled && !s.templates_enabled);
        pool.close().await;
        let _ = std::fs::remove_file(ruta);
    }

    #[tokio::test]
    async fn guarda_y_lee_ajustes_separados_por_servidor_y_bot() {
        let (pool, ruta) = pool_temporal("guardar").await;
        let a = GuildSettings {
            alias: Some("Principal".into()),
            notes: "notas".into(),
            vigilance_enabled: true,
            ..Default::default()
        };
        save_guild_settings(&pool, "b1", "g1", &a).await.unwrap();
        assert_eq!(get_guild_settings(&pool, "b1", "g1").await.unwrap(), a);
        assert_eq!(
            get_guild_settings(&pool, "b1", "g2").await.unwrap(),
            GuildSettings::default()
        );
        assert_eq!(
            get_guild_settings(&pool, "b2", "g1").await.unwrap(),
            GuildSettings::default()
        );
        let b = GuildSettings {
            templates_enabled: true,
            ..a.clone()
        };
        save_guild_settings(&pool, "b1", "g1", &b).await.unwrap();
        assert_eq!(get_guild_settings(&pool, "b1", "g1").await.unwrap(), b);
        pool.close().await;
        let _ = std::fs::remove_file(ruta);
    }

    #[test]
    fn json_parcial_usa_valores_por_defecto() {
        let s: GuildSettings = serde_json::from_str("{\"notes\":\"x\"}").unwrap();
        assert_eq!(s.notes, "x");
        assert!(!s.personality_enabled);
    }
}

#[cfg(test)]
mod abrir_bd_tests {
    use super::*;

    #[tokio::test]
    async fn crea_la_bd_aplica_migraciones_y_se_reabre() {
        let dir = std::env::temp_dir().join(format!("argus_abrir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ruta = dir.join("argus").join(DB_NAME);

        let pool = abrir_bd(&ruta).await.expect("primera apertura");
        assert!(ruta.exists());
        assert!(list_bots(&pool).await.expect("tabla bots").is_empty());
        pool.close().await;

        // Reabrir una BD existente no falla (migraciones ya aplicadas)
        let pool = abrir_bd(&ruta).await.expect("segunda apertura");
        pool.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
