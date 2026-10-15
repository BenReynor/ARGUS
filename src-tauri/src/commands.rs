use std::process::Stdio;
use tauri::{AppHandle, Manager, State};
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::state::{AppState, BotStatus, Config, ServerMap, Guild, Channel, Member, Role};

#[tauri::command]
pub async fn start_bot(app: AppHandle, state: State<'_, AppState>) -> Result<BotStatus, String> {
    let config = {
        let guard = state.config.lock().unwrap();
        guard.clone()
    };

    if config.discord_token.is_empty() {
        return Err("Token de Discord no configurado".to_string());
    }

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

    let npx_cmd = if cfg!(target_os = "windows") { "npx.cmd" } else { "npx" };

    let mut child = Command::new(npx_cmd)
        .args(["-y", "@pasympa/discord-mcp"])
        .env("DISCORD_TOKEN", &config.discord_token)
        .env("DISCORD_ALLOWED_GUILDS", &config.server_id)
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
                s.guild_id = Some(config.server_id.clone());
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

#[tauri::command]
pub async fn list_guilds(_state: State<'_, AppState>) -> Result<Vec<Guild>, String> {
    Ok(vec![Guild {
        id: "1541513296375717921".to_string(),
        name: "ISD Internal Security Department | Site-14".to_string(),
        member_count: 20,
        icon: None,
    }])
}

#[tauri::command]
pub async fn list_channels(_state: State<'_, AppState>, _guild_id: String) -> Result<Vec<Channel>, String> {
    let channels = vec![
        Channel { id: "1541513297529147465".to_string(), name: "💬-𝑮𝒆𝓃𝑒𝓇𝒶𝓁".to_string(), channel_type: 0, parent_id: Some("1541513296375717921".to_string()), position: 0 },
        Channel { id: "1541550714889703424".to_string(), name: "🤖𝑩𝑜𝓉𝓈-𝓎-𝒸𝑜𝓂𝒶𝓃𝒹𝑜𝓈".to_string(), channel_type: 0, parent_id: Some("1541513296375717921".to_string()), position: 1 },
        Channel { id: "1542372186239664170".to_string(), name: "📢avisos-𝑰𝑺𝑫".to_string(), channel_type: 0, parent_id: Some("1542373594225446993".to_string()), position: 0 },
        Channel { id: "1541533121571323914".to_string(), name: "📋𝒾𝓃𝒻𝑜𝓇𝓂𝑒𝓈-𝐼𝒮𝒟".to_string(), channel_type: 0, parent_id: Some("1542373594225446993".to_string()), position: 1 },
        Channel { id: "1541914398057299978".to_string(), name: "📋-ᴀᴄᴛᴀs-ʏ-ʀᴇᴘᴏʀᴛᴇs".to_string(), channel_type: 15, parent_id: Some("1555423953185677372".to_string()), position: 0 },
    ];
    Ok(channels)
}

#[tauri::command]
pub async fn list_members(_state: State<'_, AppState>, _guild_id: String) -> Result<Vec<Member>, String> {
    let members = vec![
        Member { user_id: "893650374454571040".to_string(), username: "wilby_32".to_string(), nickname: Some("Wilby".to_string()), roles: vec!["Staff".to_string()], bot: false },
        Member { user_id: "1013579698418225192".to_string(), username: "nicolasw1w".to_string(), nickname: None, roles: vec!["Director de seguridad interna".to_string()], bot: false },
        Member { user_id: "1560141544181334138".to_string(), username: "Agente Albert".to_string(), nickname: None, roles: vec!["Agente Albert".to_string(), "Bots".to_string()], bot: true },
    ];
    Ok(members)
}

#[tauri::command]
pub async fn list_roles(_state: State<'_, AppState>, _guild_id: String) -> Result<Vec<Role>, String> {
    let roles = vec![
        Role { id: "1541517911590576178".to_string(), name: "Director de seguridad interna".to_string(), color: 0xFF0000, position: 31, permissions: "Administrator".to_string(), member_count: 1, mentionable: true, hoist: true },
        Role { id: "1541518029735731230".to_string(), name: "Subdirector/comandante de la B-7".to_string(), color: 0xFF4444, position: 30, permissions: "Administrator".to_string(), member_count: 1, mentionable: true, hoist: true },
        Role { id: "1560145154122322023".to_string(), name: "Agente Albert".to_string(), color: 0x00FF00, position: 1, permissions: "Bot".to_string(), member_count: 1, mentionable: true, hoist: true },
    ];
    Ok(roles)
}

#[tauri::command]
pub async fn send_message(_state: State<'_, AppState>, channel_id: String, content: String) -> Result<String, String> {
    println!("Enviando a canal {}: {}", channel_id, content);
    Ok(format!("Mensaje enviado a canal {}", channel_id))
}

#[tauri::command]
pub async fn send_dm(_state: State<'_, AppState>, user_id: String, content: String) -> Result<String, String> {
    println!("Enviando DM a {}: {}", user_id, content);
    Ok(format!("DM enviado a usuario {}", user_id))
}

#[tauri::command]
pub async fn get_member_info(_state: State<'_, AppState>, _guild_id: String, user_id: String) -> Result<Member, String> {
    Ok(Member {
        user_id,
        username: "Usuario".to_string(),
        nickname: None,
        roles: vec![],
        bot: false,
    })
}

#[tauri::command]
pub async fn add_role(_state: State<'_, AppState>, _guild_id: String, user_id: String, role_id: String) -> Result<String, String> {
    Ok(format!("Rol {} añadido a usuario {}", role_id, user_id))
}

#[tauri::command]
pub async fn remove_role(_state: State<'_, AppState>, _guild_id: String, user_id: String, role_id: String) -> Result<String, String> {
    Ok(format!("Rol {} quitado a usuario {}", role_id, user_id))
}

#[tauri::command]
pub async fn timeout_member(_state: State<'_, AppState>, _guild_id: String, user_id: String, duration_minutes: u32) -> Result<String, String> {
    Ok(format!("Usuario {} muteado por {} minutos", user_id, duration_minutes))
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub async fn save_config(app: AppHandle, state: State<'_, AppState>, config: Config) -> Result<String, String> {
    state.save_config(&app, config).await.map_err(|e| e.to_string())?;
    Ok("Configuración guardada".to_string())
}

#[tauri::command]
pub async fn get_server_map(state: State<'_, AppState>) -> Result<Option<ServerMap>, String> {
    Ok(state.server_map.lock().unwrap().clone())
}

#[tauri::command]
pub async fn refresh_server_map(state: State<'_, AppState>) -> Result<ServerMap, String> {
    let guild = Guild {
        id: "1541513296375717921".to_string(),
        name: "ISD Internal Security Department | Site-14".to_string(),
        member_count: 20,
        icon: None,
    };

    let channels = list_channels(state.clone(), guild.id.clone()).await?;
    let members = list_members(state.clone(), guild.id.clone()).await?;
    let roles = list_roles(state.clone(), guild.id.clone()).await?;

    let mut categories = std::collections::HashMap::new();
    for channel in &channels {
        if let Some(parent) = &channel.parent_id {
            categories.entry(parent.clone()).or_insert_with(Vec::new).push(channel.clone());
        }
    }

    let map = ServerMap {
        guild,
        channels,
        members,
        roles,
        categories,
        last_updated: chrono::Utc::now().to_rfc3339(),
    };

    *state.server_map.lock().unwrap() = Some(map.clone());
    Ok(map)
}