pub use crate::db::{BotConfig, CliAgentConfig, GuildCache as Guild, VigilanceConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::broadcast;

pub type DynError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponderTriggers {
    pub mention: bool,
    pub slash_command: bool,
    pub prefix: Option<String>,
    pub allowed_channels: Vec<String>,
    pub blocked_channels: Vec<String>,
    pub respond_to_bots: bool,
    pub respond_in_dms: bool,
}

impl Default for ResponderTriggers {
    fn default() -> Self {
        Self {
            mention: true,
            slash_command: true,
            prefix: Some("!".to_string()),
            allowed_channels: vec![],
            blocked_channels: vec![],
            respond_to_bots: false,
            respond_in_dms: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsConfig {
    pub enabled: bool,
    pub sound: bool,
    pub only_when_unfocused: bool,
    pub telegram_enabled: bool,
    pub telegram_events: Vec<String>,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sound: true,
            only_when_unfocused: true,
            telegram_enabled: false,
            telegram_events: vec![
                "critical".to_string(),
                "ban".to_string(),
                "role_change".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub enabled: bool,
    pub interval_hours: u32,
    pub retention_count: u32,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_hours: 24,
            retention_count: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bots: Vec<BotConfig>,
    pub active_bot_id: Option<String>,
    pub active_guild_id: Option<String>,
    pub guilds_cache: Vec<Guild>,
    pub multi_guild_mode: bool,

    // IA
    pub llm_chat_provider: String,
    pub llm_chat_model: String,
    pub llm_chat_url: String,
    pub llm_chat_api_key_ciphertext: String,
    pub plan_provider: String,
    pub ask_provider: String,
    pub cli_agents: HashMap<String, CliAgentConfig>,

    // Contexto/Memoria
    pub context_limit_per_user: u32,
    pub context_limit_per_channel: u32,
    pub auto_cleanup_enabled: bool,
    pub archive_retention_months: u32,

    // Vigilancia
    pub vigilance: VigilanceConfig,

    // Backup
    pub backup: BackupConfig,

    // General
    pub language: String,
    pub auto_update: bool,
    pub telemetry_opt_in: bool,
    pub anti_spam_cooldown_secs: u32,
    pub responder_triggers: ResponderTriggers,
    pub alerts: AlertsConfig,
}

impl Default for Config {
    fn default() -> Self {
        let mut cli_agents = HashMap::new();
        cli_agents.insert(
            "opencode".to_string(),
            CliAgentConfig {
                id: "opencode".to_string(),
                name: "opencode".to_string(),
                mode: "auto".to_string(),
                binary_path: None,
                model: Some("auto".to_string()),
                config: serde_json::json!({}),
                enabled: true,
            },
        );
        cli_agents.insert(
            "codex".to_string(),
            CliAgentConfig {
                id: "codex".to_string(),
                name: "codex".to_string(),
                mode: "auto".to_string(),
                binary_path: None,
                model: None,
                config: serde_json::json!({}),
                enabled: true,
            },
        );
        cli_agents.insert(
            "claude".to_string(),
            CliAgentConfig {
                id: "claude".to_string(),
                name: "claude".to_string(),
                mode: "auto".to_string(),
                binary_path: None,
                model: Some("sonnet".to_string()),
                config: serde_json::json!({}),
                enabled: true,
            },
        );

        Self {
            bots: vec![],
            active_bot_id: None,
            active_guild_id: Some("1541513296375717921".to_string()),
            guilds_cache: vec![],
            multi_guild_mode: false,

            llm_chat_provider: "nemotron".to_string(),
            llm_chat_model: "nvidia/nemotron-3-ultra".to_string(),
            llm_chat_url: "https://openrouter.ai/api/v1".to_string(),
            llm_chat_api_key_ciphertext: String::new(),
            plan_provider: "auto".to_string(),
            ask_provider: "auto".to_string(),
            cli_agents,

            context_limit_per_user: 10,
            context_limit_per_channel: 50,
            auto_cleanup_enabled: true,
            archive_retention_months: 12,

            vigilance: VigilanceConfig::default(),

            backup: BackupConfig::default(),

            language: "es".to_string(),
            auto_update: true,
            telemetry_opt_in: false,
            anti_spam_cooldown_secs: 30,
            responder_triggers: ResponderTriggers::default(),
            alerts: AlertsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotStatus {
    pub running: bool,
    pub connected: bool,
    pub guild_name: Option<String>,
    pub guild_id: Option<String>,
    pub member_count: Option<u32>,
    pub latency_ms: Option<u64>,
    pub last_error: Option<String>,
}

impl Default for BotStatus {
    fn default() -> Self {
        Self {
            running: false,
            connected: false,
            guild_name: None,
            guild_id: None,
            member_count: None,
            latency_ms: None,
            last_error: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub channel_type: u8,
    pub parent_id: Option<String>,
    pub position: i32,
    pub overwrites: Option<Vec<Overwrite>>,
    pub permissions_bitfield: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overwrite {
    pub id: String,
    pub type_: u8, // 0 = role, 1 = member
    pub allow: String,
    pub deny: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub username: String,
    pub nickname: Option<String>,
    pub roles: Vec<String>,
    pub bot: bool,
    pub joined_at: Option<String>,
    pub avatar: Option<String>,
    pub flags: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub color: u32,
    pub position: i32,
    pub permissions: String,
    pub permissions_bitfield: Option<String>,
    pub member_count: u32,
    pub mentionable: bool,
    pub hoist: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMap {
    pub guild: Guild,
    pub channels: Vec<Channel>,
    pub members: Vec<Member>,
    pub roles: Vec<Role>,
    pub categories: HashMap<String, Vec<Channel>>,
    pub last_updated: String,
}

// Event types for Tauri ↔ bot_core communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BotCoreEvent {
    MapUpdated {
        guild_id: String,
    },
    LogEvent {
        level: String,
        message: String,
    },
    BotStatusChange {
        status: BotStatus,
    },
    ExecProgress {
        action: String,
        step: u32,
        total: u32,
        status: String,
    },
    IncidentNew {
        incident: crate::db::Incident,
    },
    ChatMessage {
        message: crate::db::Message,
    },
}

pub struct AppState {
    pub config: Arc<Mutex<Config>>,
    pub bot_status: Arc<Mutex<BotStatus>>,
    pub bot_process: Arc<Mutex<Option<tokio::process::Child>>>,
    pub server_maps: Arc<Mutex<HashMap<String, ServerMap>>>,
    pub bot_core_tx: Arc<Mutex<Option<broadcast::Sender<BotCoreEvent>>>>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self {
            config: Arc::new(Mutex::new(Config::default())),
            bot_status: Arc::new(Mutex::new(BotStatus::default())),
            bot_process: Arc::new(Mutex::new(None)),
            server_maps: Arc::new(Mutex::new(HashMap::new())),
            bot_core_tx: Arc::new(Mutex::new(Some(tx))),
        }
    }

    pub async fn load_config_from_db(&self, app: &AppHandle) -> Result<(), DynError> {
        // Read everything first: a std MutexGuard must not live across an
        // .await (the future would not be Send) nor be taken twice (deadlock).
        let pool = crate::db::get_pool(app).await?;
        let stored = crate::db::get_config(&pool, "config").await?;
        let bots = crate::db::list_bots(&pool).await?;
        let guilds = crate::db::list_guilds_cached(&pool).await?;
        let vigilance = crate::db::get_vigilance(&pool).await?;
        let lang = crate::db::get_setting(&pool, "language").await?;

        let mut config = self.config.lock().unwrap();
        if let Some(parsed) = stored.and_then(|json| serde_json::from_str(&json).ok()) {
            *config = parsed;
        }
        config.active_bot_id = bots.first().map(|b| b.id.clone());
        config.bots = bots;
        config.guilds_cache = guilds;
        config.vigilance = vigilance;
        if let Some(lang) = lang {
            config.language = lang;
        }
        Ok(())
    }

    pub async fn save_config_to_db(&self, app: &AppHandle) -> Result<(), DynError> {
        let pool = crate::db::get_pool(app).await?;
        let config = self.config.lock().unwrap().clone();

        crate::db::set_config(&pool, "config", &serde_json::to_string(&config)?).await?;
        crate::db::set_setting(&pool, "language", &config.language).await?;
        crate::db::save_vigilance(&pool, &config.vigilance).await?;
        Ok(())
    }

    pub fn get_active_bot(&self) -> Option<BotConfig> {
        let config = self.config.lock().unwrap();
        config
            .active_bot_id
            .as_ref()
            .and_then(|id| config.bots.iter().find(|b| b.id == *id).cloned())
    }

    pub fn set_active_bot(&self, bot_id: &str) {
        self.config.lock().unwrap().active_bot_id = Some(bot_id.to_string());
    }

    pub fn set_active_guild(&self, guild_id: &str) {
        self.config.lock().unwrap().active_guild_id = Some(guild_id.to_string());
    }

    pub fn get_active_guild(&self) -> Option<String> {
        self.config.lock().unwrap().active_guild_id.clone()
    }

    pub fn get_map(&self, guild_id: &str) -> Option<ServerMap> {
        self.server_maps.lock().unwrap().get(guild_id).cloned()
    }

    pub fn set_map(&self, guild_id: String, map: ServerMap) {
        self.server_maps.lock().unwrap().insert(guild_id, map);
    }

    pub fn cleanup_user_context(
        &self,
        app: &AppHandle,
        guild_id: &str,
        user_id: &str,
    ) -> Result<(), DynError> {
        let config = self.config.lock().unwrap();
        let limit = config.context_limit_per_user as i64;
        drop(config);

        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let pool = crate::db::get_pool(app).await?;
            crate::db::cleanup_user_context(&pool, guild_id, user_id, limit).await?;
            Ok::<(), DynError>(())
        })
    }
}
