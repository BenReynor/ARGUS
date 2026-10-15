use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub discord_token: String,
    pub server_id: String,
    pub auto_start: bool,
    pub minimize_to_tray: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            discord_token: String::new(),
            server_id: "1541513296375717921".to_string(), // ISD server
            auto_start: false,
            minimize_to_tray: true,
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
pub struct Guild {
    pub id: String,
    pub name: String,
    pub member_count: u32,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub channel_type: u8,
    pub parent_id: Option<String>,
    pub position: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub username: String,
    pub nickname: Option<String>,
    pub roles: Vec<String>,
    pub bot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub color: u32,
    pub position: i32,
    pub permissions: String,
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

pub struct AppState {
    pub config: Arc<Mutex<Config>>,
    pub bot_status: Arc<Mutex<BotStatus>>,
    pub bot_process: Arc<Mutex<Option<tokio::process::Child>>>,
    pub server_map: Arc<Mutex<Option<ServerMap>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Arc::new(Mutex::new(Config::default())),
            bot_status: Arc::new(Mutex::new(BotStatus::default())),
            bot_process: Arc::new(Mutex::new(None)),
            server_map: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn load_config(&self, app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(store) = app.store("config.json") {
            if let Ok(value) = store.get("config") {
                if let Ok(config) = serde_json::from_value(value) {
                    *self.config.lock().unwrap() = config;
                }
            }
        }
        Ok(())
    }

    pub async fn save_config(&self, app: &tauri::AppHandle, config: Config) -> Result<(), Box<dyn std::error::Error>> {
        *self.config.lock().unwrap() = config.clone();
        if let Some(store) = app.store("config.json") {
            store.set("config", serde_json::to_value(config)?)?;
            store.save()?;
        }
        Ok(())
    }
}