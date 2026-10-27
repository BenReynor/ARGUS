use chrono::Utc;
use tauri::{AppHandle, State};

use crate::crypto;
use crate::db::{self, BotConfig as DbBotConfig, CliAgentConfig, VigilanceConfig};
use crate::state::{
    AppState, BotConfig, BotStatus, Channel, Config, Guild, Member, Role, ServerMap,
};

fn require_active_guild(state: &AppState) -> Result<String, String> {
    state
        .get_active_guild()
        .ok_or_else(|| "No hay servidor seleccionado".to_string())
}

#[tauri::command]
pub async fn start_bot(state: State<'_, AppState>) -> Result<BotStatus, String> {
    conectar_bot(&state, None).await
}

/// Descifra el token del bot activo, lo valida con `GET /users/@me` y deja
/// `bot_status` como conectado con el nombre oficial. Sin procesos externos.
async fn conectar_bot(state: &AppState, base_url: Option<&str>) -> Result<BotStatus, String> {
    let active_bot = state.get_active_bot().ok_or("No hay bot seleccionado")?;

    let token = crypto::decrypt(&active_bot.token_ciphertext)
        .map_err(|e| format!("Error descifrando token: {}", e))?;

    let resultado = crate::discord::fetch_current_user(&token, base_url).await;
    let mut status = state.bot_status.lock().unwrap();
    match resultado {
        Ok(user) => {
            status.running = true;
            status.connected = true;
            status.bot_name = Some(user.name);
            status.last_error = None;
            Ok(status.clone())
        }
        Err(e) => {
            status.running = false;
            status.connected = false;
            status.bot_name = None;
            status.last_error = Some(e.clone());
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn stop_bot(state: State<'_, AppState>) -> Result<BotStatus, String> {
    let mut status = state.bot_status.lock().unwrap();
    status.running = false;
    status.connected = false;
    status.bot_name = None;

    Ok(status.clone())
}

#[tauri::command]
pub async fn get_bot_status(state: State<'_, AppState>) -> Result<BotStatus, String> {
    Ok(state.bot_status.lock().unwrap().clone())
}

// Guild management
#[tauri::command]
pub async fn list_guilds(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Guild>, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let guilds = sincronizar_guilds(&pool, &state, None).await?;
    state.config.lock().unwrap().guilds_cache = guilds.clone();
    Ok(guilds)
}

/// Pide los servidores del bot activo (`GET /users/@me/guilds`), actualiza la
/// tabla `guilds` y devuelve id, nombre e icono.
async fn sincronizar_guilds(
    pool: &sqlx::SqlitePool,
    state: &AppState,
    base_url: Option<&str>,
) -> Result<Vec<Guild>, String> {
    let bot = state.get_active_bot().ok_or("No hay bot seleccionado")?;
    let token = crypto::decrypt(&bot.token_ciphertext)
        .map_err(|e| format!("Error descifrando token: {}", e))?;
    let remotos = crate::discord::fetch_current_user_guilds(&token, base_url).await?;
    let guilds: Vec<Guild> = remotos
        .into_iter()
        .map(|g| Guild {
            id: g.id.to_string(),
            name: g.name,
            icon: g.icon.map(|i| i.to_string()),
            member_count: 0,
            joined_at: None,
        })
        .collect();
    db::replace_guilds(pool, &guilds)
        .await
        .map_err(|e| e.to_string())?;
    Ok(guilds)
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
    let guild_id = require_active_guild(&state)?;
    Ok(state.get_map(&guild_id))
}

#[tauri::command]
pub async fn refresh_server_map(state: State<'_, AppState>) -> Result<ServerMap, String> {
    let guild_id = require_active_guild(&state)?;
    let guild = Guild {
        id: guild_id.clone(),
        name: format!("Servidor {}", guild_id),
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
    token: String,
) -> Result<BotConfig, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let bot = registrar_bot(&pool, &token, None).await?;

    // Scoped, not drop(): the guard must not be live across an .await.
    {
        let mut config = state.config.lock().unwrap();
        config.bots.retain(|b| b.id != bot.id);
        config.bots.push(bot.clone());
    }
    Ok(bot)
}

/// Valida el token con `GET /users/@me` y guarda el bot (token cifrado) con
/// los datos oficiales de Discord. Un token rechazado no guarda nada.
async fn registrar_bot(
    pool: &sqlx::SqlitePool,
    token: &str,
    base_url: Option<&str>,
) -> Result<BotConfig, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("Token inválido".to_string());
    }
    let user = crate::discord::fetch_current_user(token, base_url).await?;
    let token_ciphertext =
        crypto::encrypt(token).map_err(|e| format!("Error cifrando token: {}", e))?;
    let now = Utc::now().to_rfc3339();
    let bot = BotConfig {
        id: user.id.to_string(),
        name: user.name.clone(),
        token_ciphertext,
        avatar: user.avatar.map(|a| a.to_string()),
        allowed_guilds: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    db::upsert_bot(pool, &bot)
        .await
        .map_err(|e| e.to_string())?;
    Ok(bot)
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
        avatar: bot.avatar.clone(),
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
    state
        .save_config_to_db(&app)
        .await
        .map_err(|e| e.to_string())?;

    Ok("Bot eliminado".to_string())
}

#[tauri::command]
pub async fn set_active_bot(
    app: AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<String, String> {
    let existe = state
        .config
        .lock()
        .unwrap()
        .bots
        .iter()
        .any(|b| b.id == bot_id);
    if !existe {
        return Err("El bot no está registrado".to_string());
    }
    state.set_active_bot(&bot_id);
    state
        .save_config_to_db(&app)
        .await
        .map_err(|e| e.to_string())?;
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
        .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn pool_en_memoria() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("BD en memoria");
        db::run_migrations(&pool).await.expect("migraciones");
        pool
    }

    #[tokio::test]
    async fn lista_guilds_del_bot_activo_y_actualiza_la_tabla() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me/guilds"))
            .and(header("authorization", "Bot token-guilds"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "111", "name": "Uno", "icon": "abcdef0123456789abcdef0123456789",
                 "owner": false, "permissions": "0", "features": []},
                {"id": "222", "name": "Dos", "icon": null,
                 "owner": true, "permissions": "8", "features": []}
            ])))
            .expect(1)
            .mount(&server)
            .await;

        let pool = pool_en_memoria().await;
        // Un servidor viejo en caché debe desaparecer tras sincronizar.
        db::upsert_guild(
            &pool,
            &db::GuildCache {
                id: "999".into(),
                name: "Viejo".into(),
                icon: None,
                member_count: 0,
                joined_at: None,
            },
        )
        .await
        .unwrap();

        let state = AppState::new();
        let bot = BotConfig {
            id: "1".into(),
            name: "bot".into(),
            token_ciphertext: crypto::encrypt("token-guilds").unwrap(),
            avatar: None,
            allowed_guilds: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        };
        {
            let mut c = state.config.lock().unwrap();
            c.bots.push(bot);
            c.active_bot_id = Some("1".into());
        }

        let addr = server.address().to_string();
        let guilds = sincronizar_guilds(&pool, &state, Some(&addr))
            .await
            .expect("sincronizar");

        assert_eq!(guilds.len(), 2);
        assert_eq!(guilds[0].id, "111");
        assert_eq!(guilds[0].name, "Uno");
        assert!(guilds[0].icon.is_some());
        assert_eq!(guilds[1].name, "Dos");
        assert!(guilds[1].icon.is_none());

        let en_bd = db::list_guilds_cached(&pool).await.unwrap();
        assert_eq!(en_bd.len(), 2);
        assert!(en_bd.iter().all(|g| g.id != "999"));
    }

    #[tokio::test]
    async fn registra_bot_con_datos_oficiales_y_token_cifrado() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me"))
            .and(header("authorization", "Bot token-valido"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "123456789",
                "username": "bot-oficial",
                "discriminator": "0",
                "avatar": "abcdef0123456789abcdef0123456789",
                "bot": true,
                "mfa_enabled": false
            })))
            .expect(1)
            .mount(&server)
            .await;

        let pool = pool_en_memoria().await;
        let addr = server.address().to_string();
        let bot = registrar_bot(&pool, "token-valido", Some(&addr))
            .await
            .expect("registro");

        assert_eq!(bot.id, "123456789");
        assert_eq!(bot.name, "bot-oficial");
        assert!(bot.avatar.is_some());
        assert_ne!(bot.token_ciphertext, "token-valido");

        let guardado = db::get_bot(&pool, "123456789")
            .await
            .unwrap()
            .expect("bot guardado");
        assert_eq!(guardado.name, "bot-oficial");
        assert_eq!(guardado.avatar, bot.avatar);
        assert_eq!(
            crypto::decrypt(&guardado.token_ciphertext).unwrap(),
            "token-valido"
        );
    }

    #[tokio::test]
    async fn token_rechazado_401_no_guarda_nada() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "401: Unauthorized",
                "code": 0
            })))
            .expect(1)
            .mount(&server)
            .await;

        let pool = pool_en_memoria().await;
        let addr = server.address().to_string();
        let err = registrar_bot(&pool, "token-malo", Some(&addr))
            .await
            .expect_err("debe fallar");

        assert_eq!(err, "Token inválido");
        assert!(db::list_bots(&pool).await.unwrap().is_empty());
    }

    fn estado_con_bot_activo(token: &str) -> AppState {
        let state = AppState::new();
        let now = Utc::now().to_rfc3339();
        let bot = BotConfig {
            id: "123456789".to_string(),
            name: "nombre-viejo".to_string(),
            token_ciphertext: crypto::encrypt(token).unwrap(),
            avatar: None,
            allowed_guilds: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        };
        {
            let mut config = state.config.lock().unwrap();
            config.bots.push(bot);
        }
        state.set_active_bot("123456789");
        state
    }

    #[tokio::test]
    async fn start_bot_sin_bot_activo_falla() {
        let state = AppState::new();
        let err = conectar_bot(&state, None).await.expect_err("debe fallar");
        assert_eq!(err, "No hay bot seleccionado");
    }

    #[tokio::test]
    async fn start_bot_valida_token_y_queda_conectado() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me"))
            .and(header("authorization", "Bot token-valido"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "123456789",
                "username": "bot-oficial",
                "discriminator": "0",
                "avatar": null,
                "bot": true,
                "mfa_enabled": false
            })))
            .expect(1)
            .mount(&server)
            .await;

        let state = estado_con_bot_activo("token-valido");
        let addr = server.address().to_string();
        let status = conectar_bot(&state, Some(&addr)).await.expect("conexión");

        assert!(status.running && status.connected);
        assert_eq!(status.bot_name.as_deref(), Some("bot-oficial"));
        assert!(state.bot_status.lock().unwrap().connected);
    }

    #[tokio::test]
    async fn start_bot_con_token_rechazado_no_conecta() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "401: Unauthorized",
                "code": 0
            })))
            .mount(&server)
            .await;

        let state = estado_con_bot_activo("token-malo");
        let addr = server.address().to_string();
        let err = conectar_bot(&state, Some(&addr))
            .await
            .expect_err("debe fallar");

        assert_eq!(err, "Token inválido");
        let status = state.bot_status.lock().unwrap();
        assert!(!status.connected && !status.running);
        assert_eq!(status.last_error.as_deref(), Some("Token inválido"));
    }
}
