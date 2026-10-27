use chrono::Utc;
use std::process::Stdio;
use tauri::{AppHandle, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::crypto;
use crate::db::{self, BotConfig as DbBotConfig, CliAgentConfig, VigilanceConfig};
use crate::state::{
    AppState, BotConfig, BotStatus, Channel, Config, Guild, Member, Role, ServerMap,
};

#[tauri::command]
pub async fn start_bot(app: AppHandle, state: State<'_, AppState>) -> Result<BotStatus, String> {
    let active_bot = state
        .get_active_bot()
        .ok_or("No hay bot activo configurado")?;

    let token = crypto::decrypt(&active_bot.token_ciphertext)
        .map_err(|e| format!("Error descifrando token: {}", e))?;

    let guild_id = state
        .get_active_guild()
        .unwrap_or_else(|| "1541513296375717921".to_string());

    let status_guard = state.bot_status.lock().unwrap();
    if status_guard.running {
        return Ok(status_guard.clone());
    }
    drop(status_guard);

    let mut process_guard = state.bot_process.lock().unwrap();
    if process_guard.is_some() {
        return Err("El bot ya está ejecutándose".to_string());
    }

    {
        let mut status = state.bot_status.lock().unwrap();
        status.running = true;
        status.last_error = None;
    }

    let npx_cmd = if cfg!(target_os = "windows") {
        "npx.cmd"
    } else {
        "npx"
    };

    let mut child = Command::new(npx_cmd)
        .args(["-y", "@pasympa/discord-mcp"])
        .env("DISCORD_TOKEN", &token)
        .env("DISCORD_ALLOWED_GUILDS", &guild_id)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let mut status = state.bot_status.lock().unwrap();
            status.running = false;
            status.last_error = Some(format!("Error al iniciar: {}", e));
            "Error al iniciar proceso".to_string()
        })?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let app_handle = app.clone();

    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if line.contains("READY") || line.contains("connected") {
                let state_ref = app_handle.state::<AppState>();
                let mut s = state_ref.bot_status.lock().unwrap();
                s.connected = true;
                s.guild_name = Some("ISD Internal Security Department".to_string());
                s.guild_id = Some(guild_id.clone());
            }
            println!("[BOT STDOUT] {}", line);
        }
    });

    let app_handle2 = app.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if line.contains("Error") || line.contains("error") || line.contains("invalid token") {
                let state_ref = app_handle2.state::<AppState>();
                let mut s = state_ref.bot_status.lock().unwrap();
                s.last_error = Some(line.clone());
                s.connected = false;
            }
            eprintln!("[BOT STDERR] {}", line);
        }
    });

    *process_guard = Some(child);

    let app_handle3 = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        let state_ref = app_handle3.state::<AppState>();
        let mut s = state_ref.bot_status.lock().unwrap();
        s.running = true;
    });

    Ok(state.bot_status.lock().unwrap().clone())
}

#[tauri::command]
pub async fn stop_bot(state: State<'_, AppState>) -> Result<BotStatus, String> {
    let child = {
        let mut process_guard = state.bot_process.lock().unwrap();
        process_guard.take()
    };

    if let Some(mut child) = child {
        child.kill().await.map_err(|e| e.to_string())?;
    }

    let mut status = state.bot_status.lock().unwrap();
    status.running = false;
    status.connected = false;

    Ok(status.clone())
}

#[tauri::command]
pub async fn get_bot_status(state: State<'_, AppState>) -> Result<BotStatus, String> {
    Ok(state.bot_status.lock().unwrap().clone())
}

// Guild management
#[tauri::command]
pub async fn list_guilds(state: State<'_, AppState>) -> Result<Vec<Guild>, String> {
    let config = state.config.lock().unwrap();
    Ok(config.guilds_cache.clone())
}

#[tauri::command]
pub async fn get_active_guild(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.get_active_guild())
}

#[tauri::command]
pub async fn set_active_guild(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<String, String> {
    state.set_active_guild(&guild_id);
    Ok("Guild activa cambiada".to_string())
}

// Channels
#[tauri::command]
pub async fn list_channels(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<Channel>, String> {
    let map = state.get_map(&guild_id).ok_or("Mapa no disponible")?;
    Ok(map.channels)
}

// Members
#[tauri::command]
pub async fn list_members(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<Member>, String> {
    let map = state.get_map(&guild_id).ok_or("Mapa no disponible")?;
    Ok(map.members)
}

// Roles
#[tauri::command]
pub async fn list_roles(state: State<'_, AppState>, guild_id: String) -> Result<Vec<Role>, String> {
    let map = state.get_map(&guild_id).ok_or("Mapa no disponible")?;
    Ok(map.roles)
}

// Discord operations
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    channel_id: String,
    content: String,
) -> Result<String, String> {
    println!("Enviando a canal {}: {}", channel_id, content);
    Ok(format!("Mensaje enviado a canal {}", channel_id))
}

#[tauri::command]
pub async fn send_dm(
    state: State<'_, AppState>,
    user_id: String,
    content: String,
) -> Result<String, String> {
    println!("Enviando DM a {}: {}", user_id, content);
    Ok(format!("DM enviado a usuario {}", user_id))
}

#[tauri::command]
pub async fn get_member_info(
    state: State<'_, AppState>,
    _guild_id: String,
    user_id: String,
) -> Result<Member, String> {
    Ok(Member {
        user_id,
        username: "Usuario".to_string(),
        nickname: None,
        roles: vec![],
        bot: false,
        joined_at: None,
        avatar: None,
        flags: None,
    })
}

#[tauri::command]
pub async fn add_role(
    state: State<'_, AppState>,
    _guild_id: String,
    user_id: String,
    role_id: String,
) -> Result<String, String> {
    Ok(format!("Rol {} añadido a usuario {}", role_id, user_id))
}

#[tauri::command]
pub async fn remove_role(
    state: State<'_, AppState>,
    _guild_id: String,
    user_id: String,
    role_id: String,
) -> Result<String, String> {
    Ok(format!("Rol {} quitado a usuario {}", role_id, user_id))
}

#[tauri::command]
pub async fn timeout_member(
    state: State<'_, AppState>,
    _guild_id: String,
    user_id: String,
    duration_minutes: u32,
) -> Result<String, String> {
    Ok(format!(
        "Usuario {} muteado por {} minutos",
        user_id, duration_minutes
    ))
}

// Config
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub async fn get_config_full(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
) -> Result<String, String> {
    *state.config.lock().unwrap() = config;
    state
        .save_config_to_db(&app)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Configuración guardada".to_string())
}

#[tauri::command]
pub async fn save_config_full(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
) -> Result<String, String> {
    *state.config.lock().unwrap() = config;
    state
        .save_config_to_db(&app)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Configuración completa guardada".to_string())
}

#[tauri::command]
pub async fn get_active_guild_cmd(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.get_active_guild())
}

#[tauri::command]
pub async fn set_active_guild_cmd(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<String, String> {
    state.set_active_guild(&guild_id);
    Ok("Guild activa cambiada".to_string())
}

// Server map
#[tauri::command]
pub async fn get_server_map(state: State<'_, AppState>) -> Result<Option<ServerMap>, String> {
    let guild_id = state
        .get_active_guild()
        .unwrap_or_else(|| "1541513296375717921".to_string());
    Ok(state.get_map(&guild_id))
}

#[tauri::command]
pub async fn refresh_server_map(state: State<'_, AppState>) -> Result<ServerMap, String> {
    let guild_id = state
        .get_active_guild()
        .unwrap_or_else(|| "1541513296375717921".to_string());
    let guild = Guild {
        id: guild_id.clone(),
        name: "ISD Internal Security Department | Site-14".to_string(),
        member_count: 20,
        icon: None,
        joined_at: Some(chrono::Utc::now().to_rfc3339()),
    };

    let channels = list_channels(state.clone(), guild_id.clone()).await?;
    let members = list_members(state.clone(), guild_id.clone()).await?;
    let roles = list_roles(state.clone(), guild_id.clone()).await?;

    let mut categories = std::collections::HashMap::new();
    for channel in &channels {
        if let Some(parent) = &channel.parent_id {
            categories
                .entry(parent.clone())
                .or_insert_with(Vec::new)
                .push(channel.clone());
        }
    }

    let map = ServerMap {
        guild,
        channels,
        members,
        roles,
        categories,
        last_updated: Utc::now().to_rfc3339(),
    };

    state.set_map(guild_id, map.clone());
    Ok(map)
}

// Bot management (multi-bot)
#[tauri::command]
pub async fn list_bots(state: State<'_, AppState>) -> Result<Vec<BotConfig>, String> {
    let config = state.config.lock().unwrap();
    Ok(config.bots.clone())
}

#[tauri::command]
pub async fn add_bot(
    app: AppHandle,
    state: State<'_, AppState>,
    mut bot: BotConfig,
) -> Result<String, String> {
    if bot.id.is_empty() {
        bot.id = uuid::Uuid::new_v4().to_string();
    }
    // Encrypt token
    bot.token_ciphertext = crypto::encrypt(&bot.token_ciphertext)
        .map_err(|e| format!("Error cifrando token: {}", e))?;

    // Scoped, not drop(): the guard must not be live across the .await below.
    {
        let mut config = state.config.lock().unwrap();
        config.bots.push(bot.clone());
    }

    // Save to DB
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    let bot_db = db::BotConfig {
        id: bot.id.clone(),
        name: bot.name.clone(),
        token_ciphertext: bot.token_ciphertext.clone(),
        allowed_guilds: bot.allowed_guilds.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    db::upsert_bot(&pool, &bot_db)
        .await
        .map_err(|e| e.to_string())?;

    Ok("Bot añadido".to_string())
}

#[tauri::command]
pub async fn update_bot(
    app: AppHandle,
    state: State<'_, AppState>,
    mut bot: BotConfig,
) -> Result<String, String> {
    // Encrypt token if not already encrypted
    if !bot.token_ciphertext.starts_with("encrypted:") {
        bot.token_ciphertext = crypto::encrypt(&bot.token_ciphertext)
            .map_err(|e| format!("Error cifrando token: {}", e))?;
    }

    // Scoped, not drop(): the guard must not be live across the .await below.
    {
        let mut config = state.config.lock().unwrap();
        if let Some(idx) = config.bots.iter().position(|b| b.id == bot.id) {
            config.bots[idx] = bot.clone();
        }
    }

    // Save to DB
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    let bot_db = db::BotConfig {
        id: bot.id.clone(),
        name: bot.name.clone(),
        token_ciphertext: bot.token_ciphertext.clone(),
        allowed_guilds: bot.allowed_guilds.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    db::upsert_bot(&pool, &bot_db)
        .await
        .map_err(|e| e.to_string())?;

    Ok("Bot actualizado".to_string())
}

#[tauri::command]
pub async fn delete_bot(
    app: AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<String, String> {
    // Scoped, not drop(): the guard must not be live across the .await below.
    {
        let mut config = state.config.lock().unwrap();
        config.bots.retain(|b| b.id != bot_id);
        if config.active_bot_id.as_ref() == Some(&bot_id) {
            config.active_bot_id = config.bots.first().map(|b| b.id.clone());
        }
    }

    // Delete from DB
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::delete_bot(&pool, &bot_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok("Bot eliminado".to_string())
}

#[tauri::command]
pub async fn set_active_bot(state: State<'_, AppState>, bot_id: String) -> Result<String, String> {
    state.set_active_bot(&bot_id);
    Ok("Bot activo cambiado".to_string())
}

// Templates
#[tauri::command]
pub async fn list_templates(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<db::Template>, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::list_templates(&pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_template(
    app: AppHandle,
    state: State<'_, AppState>,
    template: db::Template,
) -> Result<String, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::save_template(&pool, &template)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Plantilla guardada".to_string())
}

#[tauri::command]
pub async fn delete_template(
    app: AppHandle,
    state: State<'_, AppState>,
    template_id: String,
) -> Result<String, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::delete_template(&pool, &template_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Plantilla eliminada".to_string())
}

#[tauri::command]
pub async fn apply_template(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    template_id: String,
) -> Result<String, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let template = db::get_template(&pool, &template_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Plantilla no encontrada")?;

    // TODO: Implement actual template application via Discord API
    // For now, just return success
    Ok(format!(
        "Plantilla '{}' aplicada a guild {}",
        template.name, guild_id
    ))
}

// Vigilance config
#[tauri::command]
pub async fn get_vigilance_config(state: State<'_, AppState>) -> Result<VigilanceConfig, String> {
    let config = state.config.lock().unwrap();
    Ok(config.vigilance.clone())
}

#[tauri::command]
pub async fn save_vigilance_config(
    app: AppHandle,
    state: State<'_, AppState>,
    vigilance: VigilanceConfig,
) -> Result<String, String> {
    // Scoped, not drop(): the guard must not be live across the .await below.
    {
        let mut config = state.config.lock().unwrap();
        config.vigilance = vigilance.clone();
    }

    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::save_vigilance(&pool, &vigilance)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Configuración de vigilancia guardada".to_string())
}

// CLI Agents
#[tauri::command]
pub async fn list_cli_agents(state: State<'_, AppState>) -> Result<Vec<CliAgentConfig>, String> {
    let config = state.config.lock().unwrap();
    Ok(config.cli_agents.values().cloned().collect())
}

#[tauri::command]
pub async fn save_cli_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    agent: CliAgentConfig,
) -> Result<String, String> {
    // Scoped, not drop(): the guard must not be live across the .await below.
    {
        let mut config = state.config.lock().unwrap();
        config.cli_agents.insert(agent.id.clone(), agent.clone());
    }

    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::upsert_cli_agent(&pool, &agent)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Agente CLI guardado".to_string())
}

// Messages / Context
#[tauri::command]
pub async fn search_messages(
    app: AppHandle,
    state: State<'_, AppState>,
    query: String,
    guild_id: Option<String>,
    channel_id: Option<String>,
    user_id: Option<String>,
    limit: i64,
) -> Result<Vec<db::Message>, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::search_messages_fts(
        &pool,
        &query,
        guild_id.as_deref(),
        channel_id.as_deref(),
        user_id.as_deref(),
        limit,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cleanup_context(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
) -> Result<String, String> {
    state
        .cleanup_user_context(&app, &guild_id, &user_id)
        .map_err(|e| e.to_string())?;
    Ok("Contexto limpiado".to_string())
}

// Backup
#[tauri::command]
pub async fn export_all_data(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    // TODO: Implement export to ZIP
    Ok(vec![])
}

#[tauri::command]
pub async fn import_data(state: State<'_, AppState>, _data: Vec<u8>) -> Result<String, String> {
    // TODO: Implement import from ZIP
    Ok("Importado".to_string())
}

#[tauri::command]
pub async fn uninstall_wipe(state: State<'_, AppState>) -> Result<String, String> {
    // TODO: Implement full wipe (DB, config, logs, backups, shortcuts)
    Ok("Desinstalación completa".to_string())
}
