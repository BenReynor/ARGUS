use chrono::Utc;
use tauri::{AppHandle, Emitter, State};

use crate::crypto;
use crate::db::{self, BotConfig as DbBotConfig, CliAgentConfig, VigilanceConfig};
use crate::discord::{DatosCanal, DatosRol};
use crate::state::{
    AppState, BotConfig, BotStatus, Channel, Config, Guild, Member, Role, ServerMap,
};

pub fn require_active_guild(state: &AppState) -> Result<String, String> {
    state
        .get_active_guild()
        .ok_or_else(|| "No server selected".to_string())
}

#[tauri::command]
pub async fn start_bot(app: AppHandle, state: State<'_, AppState>) -> Result<BotStatus, String> {
    let st = state.clone();
    let resultado = start_bot_en(app, state).await;
    crate::i18n::localizar_estado(&st, resultado)
}

async fn start_bot_en(app: AppHandle, state: State<'_, AppState>) -> Result<BotStatus, String> {
    let status = conectar_bot(Some(&app), &state, None).await?;
    lanzar_sincronizacion(app, &state);
    Ok(status)
}

/// Sincroniza el servidor activo en segundo plano (si hay uno elegido).
fn lanzar_sincronizacion(app: AppHandle, state: &AppState) {
    let (Some(guild_id), Some(bot)) = (state.get_active_guild(), state.get_active_bot()) else {
        return;
    };
    let maps = state.server_maps.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = sincronizar_y_guardar(&app, &maps, &bot, &guild_id).await {
            tracing::error!("Falló la sincronización del servidor: {e}");
        }
    });
}

/// Repite la sincronización completa: carga el mapa por REST, lo guarda en
/// `server_maps`, emite `sync-progress` y al terminar `map-updated`.
async fn sincronizar_y_guardar(
    app: &AppHandle,
    maps: &std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, ServerMap>>>,
    bot: &DbBotConfig,
    guild_id: &str,
) -> Result<ServerMap, String> {
    let token = crypto::decrypt(&bot.token_ciphertext)
        .map_err(|e| format!("Error decrypting the token: {e}"))?;
    let mapa = crate::sync::sincronizar_servidor(&token, None, guild_id, |p| {
        crate::sync::emitir_progreso(app, &p)
    })
    .await?;
    maps.lock()
        .unwrap()
        .insert(guild_id.to_string(), mapa.clone());
    if let Err(e) = app.emit(crate::gateway::EVENTO_MAPA, ()) {
        tracing::warn!("No se pudo emitir map-updated: {e}");
    }
    Ok(mapa)
}

/// Botón «Resincronizar mapa»: repite la sincronización completa del servidor
/// activo y devuelve el mapa nuevo.
#[tauri::command]
pub async fn resync_map(app: AppHandle, state: State<'_, AppState>) -> Result<ServerMap, String> {
    let st = state.clone();
    let resultado = resync_map_en(app, state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn resync_map_en(app: AppHandle, state: State<'_, AppState>) -> Result<ServerMap, String> {
    let guild_id = require_active_guild(&state)?;
    let bot = state
        .get_active_bot()
        .ok_or_else(|| "No active bot. Choose one in Settings.".to_string())?;
    let maps = state.server_maps.clone();
    sincronizar_y_guardar(&app, &maps, &bot, &guild_id).await
}

/// Descifra el token del bot activo, lo valida con `GET /users/@me` y deja
/// `bot_status` como conectado con el nombre oficial. Sin procesos externos.
async fn conectar_bot(
    app: Option<&AppHandle>,
    state: &AppState,
    base_url: Option<&str>,
) -> Result<BotStatus, String> {
    let active_bot = state.get_active_bot().ok_or("No bot selected")?;

    let token = crypto::decrypt(&active_bot.token_ciphertext)
        .map_err(|e| format!("Error decrypting the token: {}", e))?;

    let resultado = crate::discord::fetch_current_user(&token, base_url).await;
    // El intent de miembros es un aviso, nunca un error: si la aplicación no se
    // puede leer, simplemente no se avisa.
    let warning = match &resultado {
        Ok(_) => crate::discord::fetch_current_application(&token, base_url)
            .await
            .ok()
            .and_then(|a| crate::discord::aviso_intent_miembros(&a)),
        Err(_) => None,
    };
    // Los tests pasan una URL base falsa: sin red real no hay Gateway.
    if let (true, true, Some(app)) = (resultado.is_ok(), base_url.is_none(), app) {
        let pool = db::get_pool(app).await.ok();
        let tarea = crate::gateway::lanzar(
            app.clone(),
            token,
            state.server_maps.clone(),
            state.bot_status.clone(),
            pool,
        );
        let anterior = state.gateway_task.lock().unwrap().replace(tarea);
        if let Some(anterior) = anterior {
            anterior.abort();
        }
    }
    let (nuevo, salida) = {
        let mut status = state.bot_status.lock().unwrap();
        match resultado {
            Ok(user) => {
                status.running = true;
                status.connected = true;
                status.bot_name = Some(user.name);
                status.last_error = None;
                status.warning = warning;
                (status.clone(), Ok(status.clone()))
            }
            Err(e) => {
                status.running = false;
                status.connected = false;
                status.bot_name = None;
                status.last_error = Some(e.clone());
                status.warning = None;
                (status.clone(), Err(e))
            }
        }
    };
    if let Some(app) = app {
        crate::gateway::emitir_estado(app, &nuevo);
    }
    salida
}

#[tauri::command]
pub async fn stop_bot(app: AppHandle, state: State<'_, AppState>) -> Result<BotStatus, String> {
    let st = state.clone();
    let resultado = stop_bot_en(app, state).await;
    crate::i18n::localizar_estado(&st, resultado)
}

async fn stop_bot_en(app: AppHandle, state: State<'_, AppState>) -> Result<BotStatus, String> {
    let tarea = state.gateway_task.lock().unwrap().take();
    if let Some(tarea) = tarea {
        tarea.abort();
    }
    let nuevo = {
        let mut status = state.bot_status.lock().unwrap();
        status.running = false;
        status.connected = false;
        status.bot_name = None;
        status.clone()
    };
    crate::gateway::emitir_estado(&app, &nuevo);
    Ok(nuevo)
}

#[tauri::command]
pub async fn get_bot_status(state: State<'_, AppState>) -> Result<BotStatus, String> {
    let st = state.clone();
    let resultado = get_bot_status_en(state).await;
    crate::i18n::localizar_estado(&st, resultado)
}

async fn get_bot_status_en(state: State<'_, AppState>) -> Result<BotStatus, String> {
    Ok(state.bot_status.lock().unwrap().clone())
}

// Guild management
#[tauri::command]
pub async fn list_guilds(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Guild>, String> {
    let st = state.clone();
    let resultado = list_guilds_en(app, state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_guilds_en(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Guild>, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let guilds = sincronizar_guilds(&pool, &state, None).await?;
    state.config.lock().unwrap().guilds_cache = guilds.clone();
    Ok(guilds)
}

/// Pide los servidores del bot activo (`GET /users/@me/guilds`), actualiza la
/// tabla `guilds` y devuelve id, nombre e icono.
pub async fn sincronizar_guilds(
    pool: &sqlx::SqlitePool,
    state: &AppState,
    base_url: Option<&str>,
) -> Result<Vec<Guild>, String> {
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    let token = crypto::decrypt(&bot.token_ciphertext)
        .map_err(|e| format!("Error decrypting the token: {}", e))?;
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
    let st = state.clone();
    let resultado = get_active_guild_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_active_guild_en(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.get_active_guild())
}

#[tauri::command]
pub async fn set_active_guild(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = set_active_guild_en(state, guild_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn set_active_guild_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<String, String> {
    state.set_active_guild(&guild_id);
    Ok("Active server changed".to_string())
}

// Channels
#[tauri::command]
pub async fn list_channels(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<Channel>, String> {
    let st = state.clone();
    let resultado = list_channels_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_channels_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<Channel>, String> {
    leer_canales(&state, &guild_id, None).await
}

const PERMISO_GESTIONAR_CANALES: &str = "Gestionar canales";

/// Comprueba (sin tocar nada) que el bot activo tiene `permiso` en el servidor;
/// si no, devuelve el error en español sin haber llamado a la acción.
pub async fn exigir_permiso(
    state: &AppState,
    guild_id: &str,
    permiso: &str,
    base_url: Option<&str>,
) -> Result<(), String> {
    let permisos = permisos_del_bot(state, guild_id, base_url).await?;
    if permisos.iter().any(|p| p == permiso) {
        Ok(())
    } else {
        Err(format!("The bot is missing the permission «{permiso}»"))
    }
}

/// Tras cambiar canales, vuelve a leerlos y actualiza el mapa guardado (si hay
/// uno) para que la UI lo vea sin esperar al Gateway. Devuelve `true` si cambió.
pub async fn refrescar_canales_del_mapa(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<bool, String> {
    if state.get_map(guild_id).is_none() {
        return Ok(false);
    }
    let canales = leer_canales(state, guild_id, base_url).await?;
    let mut maps = state.server_maps.lock().unwrap();
    let Some(mapa) = maps.get_mut(guild_id) else {
        return Ok(false);
    };
    mapa.channels = canales;
    crate::mapa::reconstruir_categorias(mapa);
    mapa.last_updated = Utc::now().to_rfc3339();
    Ok(true)
}

fn avisar_mapa_actualizado(app: &AppHandle) {
    if let Err(e) = app.emit(crate::gateway::EVENTO_MAPA, ()) {
        tracing::warn!("No se pudo emitir map-updated: {e}");
    }
}

#[tauri::command]
pub async fn create_channel(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    datos: DatosCanal,
) -> Result<Channel, String> {
    let st = state.clone();
    let resultado = create_channel_en(app, state, guild_id, datos).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn create_channel_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    datos: DatosCanal,
) -> Result<Channel, String> {
    let canal = crear_canal(&state, &guild_id, &datos, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(canal)
}

pub async fn crear_canal(
    state: &AppState,
    guild_id: &str,
    datos: &DatosCanal,
    base_url: Option<&str>,
) -> Result<Channel, String> {
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_CANALES, base_url).await?;
    let token = token_del_bot_activo(state)?;
    let canal = crate::discord::create_guild_channel(&token, base_url, guild_id, datos).await?;
    refrescar_canales_del_mapa(state, guild_id, base_url).await?;
    Ok(canal)
}

#[tauri::command]
pub async fn update_channel(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    datos: DatosCanal,
) -> Result<Channel, String> {
    let st = state.clone();
    let resultado = update_channel_en(app, state, guild_id, channel_id, datos).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn update_channel_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    datos: DatosCanal,
) -> Result<Channel, String> {
    let canal = editar_canal(&state, &guild_id, &channel_id, &datos, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(canal)
}

pub async fn editar_canal(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    datos: &DatosCanal,
    base_url: Option<&str>,
) -> Result<Channel, String> {
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_CANALES, base_url).await?;
    let token = token_del_bot_activo(state)?;
    let canal = crate::discord::update_channel(&token, base_url, channel_id, datos).await?;
    refrescar_canales_del_mapa(state, guild_id, base_url).await?;
    Ok(canal)
}

#[tauri::command]
pub async fn delete_channel(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = delete_channel_en(app, state, guild_id, channel_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn delete_channel_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
) -> Result<String, String> {
    let mensaje = borrar_canal(&state, &guild_id, &channel_id, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(mensaje)
}

pub async fn borrar_canal(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_CANALES, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::delete_channel(&token, base_url, channel_id).await?;
    refrescar_canales_del_mapa(state, guild_id, base_url).await?;
    Ok(format!("Channel {channel_id} deleted"))
}

/// Cada par es (id del canal, nueva posición).
#[tauri::command]
pub async fn reorder_channels(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    positions: Vec<(String, u64)>,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = reorder_channels_en(app, state, guild_id, positions).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn reorder_channels_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    positions: Vec<(String, u64)>,
) -> Result<String, String> {
    let mensaje = reordenar_canales(&state, &guild_id, &positions, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(mensaje)
}

pub async fn reordenar_canales(
    state: &AppState,
    guild_id: &str,
    posiciones: &[(String, u64)],
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_CANALES, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::reorder_guild_channels(&token, base_url, guild_id, posiciones).await?;
    refrescar_canales_del_mapa(state, guild_id, base_url).await?;
    Ok(format!("{} channels reordered", posiciones.len()))
}

const PERMISO_GESTIONAR_ROLES: &str = "Gestionar roles";
const ERROR_JERARQUIA: &str =
    "That role is above the bot's role: move it up in Server settings → Roles";

/// Orden de un rol en la jerarquía, como lo calcula Discord: gana la posición
/// mayor y, con la misma posición (pasa al crear roles), el ID menor (el más
/// antiguo). Comparar solo `position` rechazaba roles que el bot sí puede tocar.
pub fn rango_rol(r: &Role) -> (i32, std::cmp::Reverse<u64>) {
    (
        r.position,
        std::cmp::Reverse(r.id.parse().unwrap_or(u64::MAX)),
    )
}

/// Rango del rol más alto de un miembro (`None` si solo tiene @everyone).
fn rango_mas_alto(roles: &[Role], m: &Member) -> Option<(i32, std::cmp::Reverse<u64>)> {
    roles
        .iter()
        .filter(|r| m.roles.contains(&r.id))
        .map(rango_rol)
        .max()
}

/// Comprueba la jerarquía: el bot solo puede tocar roles por debajo de su rol
/// más alto. Cada objetivo es (id del rol, posición de destino si se
/// reordena); el destino también debe quedar por debajo. El dueño del servidor
/// no cuenta: quien actúa es el bot.
pub async fn exigir_jerarquia(
    state: &AppState,
    guild_id: &str,
    objetivos: &[(&str, Option<u64>)],
    base_url: Option<&str>,
) -> Result<(), String> {
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    let roles = leer_roles(state, guild_id, base_url).await?;
    let miembro = leer_miembro(state, guild_id, &bot.id, base_url).await?;
    let Some(tope) = rango_mas_alto(&roles, &miembro) else {
        return Err(ERROR_JERARQUIA.to_string());
    };
    for (id, nueva) in objetivos {
        let rol = roles
            .iter()
            .find(|r| r.id == *id)
            .ok_or_else(|| format!("Role {id} does not exist on the server"))?;
        if rango_rol(rol) >= tope || nueva.is_some_and(|n| n >= tope.0.max(0) as u64) {
            return Err(ERROR_JERARQUIA.to_string());
        }
    }
    Ok(())
}

const ERROR_DUENO: &str = "That cannot be done to the server owner";
const ERROR_JERARQUIA_MIEMBRO: &str =
    "That member has a role equal to or above the bot's: move the bot's role up in Server settings → Roles";

/// Comprueba que el bot puede moderar a `user_id`: no es el dueño y su rol más
/// alto queda por debajo del del bot. Con `permitir_ausente`, un usuario que no
/// está en el servidor (p. ej. banear por id) solo pasa la comprobación del dueño.
pub async fn exigir_jerarquia_miembro(
    state: &AppState,
    guild_id: &str,
    user_id: &str,
    permitir_ausente: bool,
    base_url: Option<&str>,
) -> Result<(), String> {
    let token = token_del_bot_activo(state)?;
    let dueno = crate::discord::fetch_guild_owner_id(&token, base_url, guild_id).await?;
    if dueno.as_deref() == Some(user_id) {
        return Err(ERROR_DUENO.to_string());
    }
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    let objetivo = match leer_miembro(state, guild_id, user_id, base_url).await {
        Ok(m) => m,
        Err(_) if permitir_ausente => return Ok(()),
        Err(e) => return Err(e),
    };
    let roles = leer_roles(state, guild_id, base_url).await?;
    let del_bot = leer_miembro(state, guild_id, &bot.id, base_url).await?;
    // `None` (solo @everyone) queda por debajo de cualquier rol.
    if rango_mas_alto(&roles, &objetivo) >= rango_mas_alto(&roles, &del_bot) {
        return Err(ERROR_JERARQUIA_MIEMBRO.to_string());
    }
    Ok(())
}

/// Datos para que la UI deshabilite la moderación imposible: dueño del servidor
/// y posición del rol más alto del bot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContextoModeracion {
    pub owner_id: Option<String>,
    pub bot_id: String,
    pub bot_top_position: i64,
}

#[tauri::command]
pub async fn moderation_context(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<ContextoModeracion, String> {
    let st = state.clone();
    let resultado = moderation_context_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn moderation_context_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<ContextoModeracion, String> {
    let token = token_del_bot_activo(&state)?;
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    let owner_id = crate::discord::fetch_guild_owner_id(&token, None, &guild_id).await?;
    let roles = leer_roles(&state, &guild_id, None).await?;
    let del_bot = leer_miembro(&state, &guild_id, &bot.id, None).await?;
    let bot_top_position = roles
        .iter()
        .filter(|r| del_bot.roles.contains(&r.id))
        .map(|r| r.position as i64)
        .max()
        .unwrap_or(0);
    Ok(ContextoModeracion {
        owner_id,
        bot_id: bot.id.clone(),
        bot_top_position,
    })
}

#[tauri::command]
pub async fn kick_member(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    reason: Option<String>,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = kick_member_en(state, guild_id, user_id, reason).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn kick_member_en(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    reason: Option<String>,
) -> Result<String, String> {
    expulsar_miembro(&state, &guild_id, &user_id, reason.as_deref(), None).await
}

pub async fn expulsar_miembro(
    state: &AppState,
    guild_id: &str,
    user_id: &str,
    motivo: Option<&str>,
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, "Expulsar miembros", base_url).await?;
    exigir_jerarquia_miembro(state, guild_id, user_id, false, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::kick_guild_member(&token, base_url, guild_id, user_id, motivo).await?;
    Ok(format!("User {user_id} kicked"))
}

#[tauri::command]
pub async fn ban_member(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    reason: Option<String>,
    delete_message_days: Option<u8>,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = ban_member_en(state, guild_id, user_id, reason, delete_message_days).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn ban_member_en(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    reason: Option<String>,
    delete_message_days: Option<u8>,
) -> Result<String, String> {
    banear_miembro(
        &state,
        &guild_id,
        &user_id,
        reason.as_deref(),
        delete_message_days.unwrap_or(0),
        None,
    )
    .await
}

pub async fn banear_miembro(
    state: &AppState,
    guild_id: &str,
    user_id: &str,
    motivo: Option<&str>,
    dias: u8,
    base_url: Option<&str>,
) -> Result<String, String> {
    if dias > crate::discord::BAN_DIAS_MAX {
        return Err("The days of messages to delete must be between 0 and 7".to_string());
    }
    exigir_permiso(state, guild_id, "Banear miembros", base_url).await?;
    exigir_jerarquia_miembro(state, guild_id, user_id, true, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::ban_guild_member(&token, base_url, guild_id, user_id, motivo, dias).await?;
    Ok(format!("User {user_id} banned"))
}

#[tauri::command]
pub async fn unban_member(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    reason: Option<String>,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = unban_member_en(state, guild_id, user_id, reason).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn unban_member_en(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    reason: Option<String>,
) -> Result<String, String> {
    quitar_ban(&state, &guild_id, &user_id, reason.as_deref(), None).await
}

pub async fn quitar_ban(
    state: &AppState,
    guild_id: &str,
    user_id: &str,
    motivo: Option<&str>,
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, "Banear miembros", base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::unban_guild_member(&token, base_url, guild_id, user_id, motivo).await?;
    Ok(format!("Ban of {user_id} removed"))
}

#[tauri::command]
pub async fn list_bans(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<crate::discord::Ban>, String> {
    let st = state.clone();
    let resultado = list_bans_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_bans_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<crate::discord::Ban>, String> {
    listar_bans(&state, &guild_id, None).await
}

pub async fn listar_bans(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<crate::discord::Ban>, String> {
    exigir_permiso(state, guild_id, "Banear miembros", base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_guild_bans(&token, base_url, guild_id).await
}

#[tauri::command]
pub async fn set_nickname(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    nickname: Option<String>,
    reason: Option<String>,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = set_nickname_en(state, guild_id, user_id, nickname, reason).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn set_nickname_en(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    nickname: Option<String>,
    reason: Option<String>,
) -> Result<String, String> {
    cambiar_apodo(
        &state,
        &guild_id,
        &user_id,
        nickname.as_deref(),
        reason.as_deref(),
        None,
    )
    .await
}

pub async fn cambiar_apodo(
    state: &AppState,
    guild_id: &str,
    user_id: &str,
    apodo: Option<&str>,
    motivo: Option<&str>,
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, "Gestionar apodos", base_url).await?;
    let es_el_bot = state.get_active_bot().is_some_and(|b| b.id == user_id);
    if !es_el_bot {
        exigir_jerarquia_miembro(state, guild_id, user_id, false, base_url).await?;
    }
    let token = token_del_bot_activo(state)?;
    crate::discord::set_member_nickname(&token, base_url, guild_id, user_id, apodo, motivo).await?;
    Ok(format!("Nickname of {user_id} updated"))
}

#[tauri::command]
pub async fn get_guild_settings_discord(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<crate::discord::AjustesServidor, String> {
    let st = state.clone();
    let resultado = get_guild_settings_discord_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_guild_settings_discord_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<crate::discord::AjustesServidor, String> {
    leer_ajustes_servidor(&state, &guild_id, None).await
}

pub async fn leer_ajustes_servidor(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<crate::discord::AjustesServidor, String> {
    exigir_permiso(state, guild_id, "Gestionar servidor", base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_guild_settings(&token, base_url, guild_id).await
}

#[tauri::command]
pub async fn update_guild_discord(
    state: State<'_, AppState>,
    guild_id: String,
    settings: crate::discord::AjustesServidor,
) -> Result<crate::discord::AjustesServidor, String> {
    let st = state.clone();
    let resultado = update_guild_discord_en(state, guild_id, settings).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn update_guild_discord_en(
    state: State<'_, AppState>,
    guild_id: String,
    settings: crate::discord::AjustesServidor,
) -> Result<crate::discord::AjustesServidor, String> {
    guardar_ajustes_servidor(&state, &guild_id, &settings, None).await
}

pub async fn guardar_ajustes_servidor(
    state: &AppState,
    guild_id: &str,
    ajustes: &crate::discord::AjustesServidor,
    base_url: Option<&str>,
) -> Result<crate::discord::AjustesServidor, String> {
    exigir_permiso(state, guild_id, "Gestionar servidor", base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::update_guild_settings(&token, base_url, guild_id, ajustes).await
}

#[tauri::command]
pub async fn audit_log(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<crate::discord::EntradaAuditoria>, String> {
    let st = state.clone();
    let resultado = audit_log_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn audit_log_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<crate::discord::EntradaAuditoria>, String> {
    leer_auditoria(&state, &guild_id, None).await
}

pub async fn leer_auditoria(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<crate::discord::EntradaAuditoria>, String> {
    exigir_permiso(state, guild_id, "Ver registro de auditoría", base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_audit_log(&token, base_url, guild_id).await
}

// Mensajes, invitaciones y webhooks: cada uno exige su permiso antes de llamar.
const PERMISO_HISTORIAL: &str = "Leer el historial de mensajes";
const PERMISO_MENSAJES: &str = "Gestionar mensajes";
const PERMISO_CREAR_INVITACION: &str = "Crear invitaciones";
const PERMISO_SERVIDOR: &str = "Gestionar servidor";
const PERMISO_WEBHOOKS: &str = "Gestionar webhooks";

#[tauri::command]
pub async fn channel_messages(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
) -> Result<Vec<crate::mensajes::MensajeCanal>, String> {
    let st = state.clone();
    let resultado = channel_messages_en(state, guild_id, channel_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn channel_messages_en(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
) -> Result<Vec<crate::mensajes::MensajeCanal>, String> {
    leer_mensajes(&state, &guild_id, &channel_id, None).await
}

pub async fn leer_mensajes(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<crate::mensajes::MensajeCanal>, String> {
    exigir_permiso(state, guild_id, PERMISO_HISTORIAL, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::mensajes::fetch_channel_messages(&token, base_url, channel_id).await
}

#[tauri::command]
pub async fn delete_message(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    message_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = delete_message_en(state, guild_id, channel_id, message_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn delete_message_en(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    message_id: String,
) -> Result<String, String> {
    borrar_mensaje(&state, &guild_id, &channel_id, &message_id, None).await
}

pub async fn borrar_mensaje(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    message_id: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    let sin_permiso = exigir_permiso(state, guild_id, PERMISO_MENSAJES, base_url)
        .await
        .err();
    let token = token_del_bot_activo(state)?;
    match crate::mensajes::delete_message(&token, base_url, channel_id, message_id).await {
        Ok(()) => Ok(format!("Message {message_id} deleted")),
        // Sin «Gestionar mensajes» el bot aún puede borrar sus propios mensajes:
        // se intenta igual y, si Discord lo rechaza, se explica el permiso.
        Err(_) if sin_permiso.is_some() => Err(sin_permiso.unwrap_or_default()),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn bulk_delete_messages(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    message_ids: Vec<String>,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = bulk_delete_messages_en(state, guild_id, channel_id, message_ids).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn bulk_delete_messages_en(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    message_ids: Vec<String>,
) -> Result<String, String> {
    borrar_mensajes_en_masa(&state, &guild_id, &channel_id, &message_ids, None).await
}

pub async fn borrar_mensajes_en_masa(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    message_ids: &[String],
    base_url: Option<&str>,
) -> Result<String, String> {
    // Valida antes de gastar llamadas de permisos.
    crate::mensajes::validar_borrado_masivo(message_ids, Utc::now().timestamp_millis())?;
    exigir_permiso(state, guild_id, PERMISO_MENSAJES, base_url).await?;
    let token = token_del_bot_activo(state)?;
    let n =
        crate::mensajes::bulk_delete_messages(&token, base_url, channel_id, message_ids).await?;
    Ok(format!("{n} messages deleted"))
}

#[tauri::command]
pub async fn pin_message(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    message_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = pin_message_en(state, guild_id, channel_id, message_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn pin_message_en(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    message_id: String,
) -> Result<String, String> {
    fijar_mensaje(&state, &guild_id, &channel_id, &message_id, true, None).await
}

#[tauri::command]
pub async fn unpin_message(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    message_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = unpin_message_en(state, guild_id, channel_id, message_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn unpin_message_en(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    message_id: String,
) -> Result<String, String> {
    fijar_mensaje(&state, &guild_id, &channel_id, &message_id, false, None).await
}

pub async fn fijar_mensaje(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    message_id: &str,
    fijar: bool,
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, PERMISO_MENSAJES, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::mensajes::set_pin(&token, base_url, channel_id, message_id, fijar).await?;
    Ok(if fijar {
        format!("Message {message_id} pinned")
    } else {
        format!("Message {message_id} unpinned")
    })
}

#[tauri::command]
pub async fn create_invite(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    max_age: u32,
    max_uses: u32,
) -> Result<crate::mensajes::Invitacion, String> {
    let st = state.clone();
    let resultado = create_invite_en(state, guild_id, channel_id, max_age, max_uses).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn create_invite_en(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    max_age: u32,
    max_uses: u32,
) -> Result<crate::mensajes::Invitacion, String> {
    crear_invitacion(&state, &guild_id, &channel_id, max_age, max_uses, None).await
}

pub async fn crear_invitacion(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    max_age: u32,
    max_uses: u32,
    base_url: Option<&str>,
) -> Result<crate::mensajes::Invitacion, String> {
    exigir_permiso(state, guild_id, PERMISO_CREAR_INVITACION, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::mensajes::create_invite(&token, base_url, channel_id, max_age, max_uses).await
}

#[tauri::command]
pub async fn list_invites(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<crate::mensajes::Invitacion>, String> {
    let st = state.clone();
    let resultado = list_invites_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_invites_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<crate::mensajes::Invitacion>, String> {
    leer_invitaciones(&state, &guild_id, None).await
}

pub async fn leer_invitaciones(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<crate::mensajes::Invitacion>, String> {
    exigir_permiso(state, guild_id, PERMISO_SERVIDOR, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::mensajes::fetch_invites(&token, base_url, guild_id).await
}

#[tauri::command]
pub async fn delete_invite(
    state: State<'_, AppState>,
    guild_id: String,
    code: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = delete_invite_en(state, guild_id, code).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn delete_invite_en(
    state: State<'_, AppState>,
    guild_id: String,
    code: String,
) -> Result<String, String> {
    borrar_invitacion(&state, &guild_id, &code, None).await
}

pub async fn borrar_invitacion(
    state: &AppState,
    guild_id: &str,
    code: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, PERMISO_SERVIDOR, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::mensajes::delete_invite(&token, base_url, code).await?;
    Ok(format!("Invite {code} deleted"))
}

#[tauri::command]
pub async fn list_webhooks(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<crate::mensajes::Webhook>, String> {
    let st = state.clone();
    let resultado = list_webhooks_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_webhooks_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<crate::mensajes::Webhook>, String> {
    leer_webhooks(&state, &guild_id, None).await
}

pub async fn leer_webhooks(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<crate::mensajes::Webhook>, String> {
    exigir_permiso(state, guild_id, PERMISO_WEBHOOKS, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::mensajes::fetch_webhooks(&token, base_url, guild_id).await
}

#[tauri::command]
pub async fn create_webhook(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    name: String,
) -> Result<crate::mensajes::Webhook, String> {
    let st = state.clone();
    let resultado = create_webhook_en(state, guild_id, channel_id, name).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn create_webhook_en(
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    name: String,
) -> Result<crate::mensajes::Webhook, String> {
    crear_webhook(&state, &guild_id, &channel_id, &name, None).await
}

pub async fn crear_webhook(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    name: &str,
    base_url: Option<&str>,
) -> Result<crate::mensajes::Webhook, String> {
    exigir_permiso(state, guild_id, PERMISO_WEBHOOKS, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::mensajes::create_webhook(&token, base_url, channel_id, name).await
}

#[tauri::command]
pub async fn delete_webhook(
    state: State<'_, AppState>,
    guild_id: String,
    webhook_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = delete_webhook_en(state, guild_id, webhook_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn delete_webhook_en(
    state: State<'_, AppState>,
    guild_id: String,
    webhook_id: String,
) -> Result<String, String> {
    borrar_webhook(&state, &guild_id, &webhook_id, None).await
}

pub async fn borrar_webhook(
    state: &AppState,
    guild_id: &str,
    webhook_id: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, PERMISO_WEBHOOKS, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::mensajes::delete_webhook(&token, base_url, webhook_id).await?;
    Ok(format!("Webhook {webhook_id} deleted"))
}

/// Tras cambiar roles, vuelve a leerlos y actualiza el mapa guardado (si hay uno).
pub async fn refrescar_roles_del_mapa(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<(), String> {
    if state.get_map(guild_id).is_none() {
        return Ok(());
    }
    let roles = leer_roles(state, guild_id, base_url).await?;
    if let Some(mapa) = state.server_maps.lock().unwrap().get_mut(guild_id) {
        mapa.roles = roles;
        mapa.last_updated = Utc::now().to_rfc3339();
    }
    Ok(())
}

#[tauri::command]
pub async fn create_role(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    datos: DatosRol,
) -> Result<Role, String> {
    let st = state.clone();
    let resultado = create_role_en(app, state, guild_id, datos).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn create_role_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    datos: DatosRol,
) -> Result<Role, String> {
    let rol = crear_rol(&state, &guild_id, &datos, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(rol)
}

pub async fn crear_rol(
    state: &AppState,
    guild_id: &str,
    datos: &DatosRol,
    base_url: Option<&str>,
) -> Result<Role, String> {
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_ROLES, base_url).await?;
    let token = token_del_bot_activo(state)?;
    let rol = crate::discord::create_guild_role(&token, base_url, guild_id, datos).await?;
    refrescar_roles_del_mapa(state, guild_id, base_url).await?;
    Ok(rol)
}

#[tauri::command]
pub async fn update_role(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    role_id: String,
    datos: DatosRol,
) -> Result<Role, String> {
    let st = state.clone();
    let resultado = update_role_en(app, state, guild_id, role_id, datos).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn update_role_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    role_id: String,
    datos: DatosRol,
) -> Result<Role, String> {
    let rol = editar_rol(&state, &guild_id, &role_id, &datos, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(rol)
}

pub async fn editar_rol(
    state: &AppState,
    guild_id: &str,
    role_id: &str,
    datos: &DatosRol,
    base_url: Option<&str>,
) -> Result<Role, String> {
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_ROLES, base_url).await?;
    exigir_jerarquia(state, guild_id, &[(role_id, None)], base_url).await?;
    let token = token_del_bot_activo(state)?;
    let rol = crate::discord::update_guild_role(&token, base_url, guild_id, role_id, datos).await?;
    refrescar_roles_del_mapa(state, guild_id, base_url).await?;
    Ok(rol)
}

#[tauri::command]
pub async fn delete_role(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    role_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = delete_role_en(app, state, guild_id, role_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn delete_role_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    role_id: String,
) -> Result<String, String> {
    let mensaje = borrar_rol(&state, &guild_id, &role_id, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(mensaje)
}

pub async fn borrar_rol(
    state: &AppState,
    guild_id: &str,
    role_id: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_ROLES, base_url).await?;
    exigir_jerarquia(state, guild_id, &[(role_id, None)], base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::delete_guild_role(&token, base_url, guild_id, role_id).await?;
    refrescar_roles_del_mapa(state, guild_id, base_url).await?;
    Ok(format!("Role {role_id} deleted"))
}

/// Cada par es (id del rol, nueva posición).
#[tauri::command]
pub async fn reorder_roles(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    positions: Vec<(String, u64)>,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = reorder_roles_en(app, state, guild_id, positions).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn reorder_roles_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    positions: Vec<(String, u64)>,
) -> Result<String, String> {
    let mensaje = reordenar_roles(&state, &guild_id, &positions, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(mensaje)
}

pub async fn reordenar_roles(
    state: &AppState,
    guild_id: &str,
    posiciones: &[(String, u64)],
    base_url: Option<&str>,
) -> Result<String, String> {
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_ROLES, base_url).await?;
    let objetivos: Vec<(&str, Option<u64>)> = posiciones
        .iter()
        .map(|(id, pos)| (id.as_str(), Some(*pos)))
        .collect();
    exigir_jerarquia(state, guild_id, &objetivos, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::reorder_guild_roles(&token, base_url, guild_id, posiciones).await?;
    refrescar_roles_del_mapa(state, guild_id, base_url).await?;
    Ok(format!("{} roles reordered", posiciones.len()))
}

/// `kind`: 0 = rol, 1 = miembro (igual que `Overwrite::type_`).
#[tauri::command]
pub async fn set_channel_permission(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    target_id: String,
    kind: u8,
    allow: String,
    deny: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = set_channel_permission_en(
        app, state, guild_id, channel_id, target_id, kind, allow, deny,
    )
    .await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn set_channel_permission_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    target_id: String,
    kind: u8,
    allow: String,
    deny: String,
) -> Result<String, String> {
    let mensaje = fijar_permiso_canal(
        &state,
        &guild_id,
        &channel_id,
        &target_id,
        kind,
        &allow,
        &deny,
        None,
    )
    .await?;
    avisar_mapa_actualizado(&app);
    Ok(mensaje)
}

#[allow(clippy::too_many_arguments)]
pub async fn fijar_permiso_canal(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    target_id: &str,
    kind: u8,
    allow: &str,
    deny: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    if kind > 1 {
        return Err("The overwrite type must be 0 (role) or 1 (member)".to_string());
    }
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_ROLES, base_url).await?;
    if kind == 0 {
        exigir_jerarquia(state, guild_id, &[(target_id, None)], base_url).await?;
    }
    let token = token_del_bot_activo(state)?;
    crate::discord::set_channel_overwrite(
        &token,
        base_url,
        channel_id,
        target_id,
        kind == 1,
        allow,
        deny,
    )
    .await?;
    refrescar_canales_del_mapa(state, guild_id, base_url).await?;
    Ok(format!(
        "Permissions of {target_id} updated in channel {channel_id}"
    ))
}

#[tauri::command]
pub async fn delete_channel_permission(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    target_id: String,
    kind: u8,
) -> Result<String, String> {
    let st = state.clone();
    let resultado =
        delete_channel_permission_en(app, state, guild_id, channel_id, target_id, kind).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn delete_channel_permission_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    channel_id: String,
    target_id: String,
    kind: u8,
) -> Result<String, String> {
    let mensaje =
        quitar_permiso_canal(&state, &guild_id, &channel_id, &target_id, kind, None).await?;
    avisar_mapa_actualizado(&app);
    Ok(mensaje)
}

pub async fn quitar_permiso_canal(
    state: &AppState,
    guild_id: &str,
    channel_id: &str,
    target_id: &str,
    kind: u8,
    base_url: Option<&str>,
) -> Result<String, String> {
    if kind > 1 {
        return Err("The overwrite type must be 0 (role) or 1 (member)".to_string());
    }
    exigir_permiso(state, guild_id, PERMISO_GESTIONAR_ROLES, base_url).await?;
    if kind == 0 {
        exigir_jerarquia(state, guild_id, &[(target_id, None)], base_url).await?;
    }
    let token = token_del_bot_activo(state)?;
    crate::discord::delete_channel_overwrite(&token, base_url, channel_id, target_id, kind == 1)
        .await?;
    refrescar_canales_del_mapa(state, guild_id, base_url).await?;
    Ok(format!(
        "Permissions of {target_id} removed from channel {channel_id}"
    ))
}

/// Descifra el token del bot activo.
pub fn token_del_bot_activo(state: &AppState) -> Result<String, String> {
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    crypto::decrypt(&bot.token_ciphertext).map_err(|e| format!("Error decrypting the token: {}", e))
}

/// `GET /guilds/{id}/channels` convertido a `Channel` (con overwrites).
pub async fn leer_canales(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<Channel>, String> {
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_guild_channels(&token, base_url, guild_id).await
}

/// `GET /guilds/{id}/roles` convertido a `Role`.
pub async fn leer_roles(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<Role>, String> {
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_guild_roles(&token, base_url, guild_id).await
}

// Members
#[tauri::command]
pub async fn list_members(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<Member>, String> {
    let st = state.clone();
    let resultado = list_members_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_members_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<Member>, String> {
    leer_miembros(&state, &guild_id, None).await
}

/// `GET /guilds/{id}/members` paginado, convertido a `Member`.
pub async fn leer_miembros(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<Member>, String> {
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_guild_members(&token, base_url, guild_id).await
}

/// `GET /guilds/{id}/members/{user_id}` convertido a `Member`.
pub async fn leer_miembro(
    state: &AppState,
    guild_id: &str,
    user_id: &str,
    base_url: Option<&str>,
) -> Result<Member, String> {
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_guild_member(&token, base_url, guild_id, user_id).await
}

/// Permisos efectivos del bot activo en un servidor, con nombres en español.
#[tauri::command]
pub async fn bot_permissions(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<String>, String> {
    let st = state.clone();
    let resultado = bot_permissions_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn bot_permissions_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<String>, String> {
    permisos_del_bot(&state, &guild_id, None).await
}

/// Nombre, icono y miembros aproximados del servidor.
#[tauri::command]
pub async fn guild_info(state: State<'_, AppState>, guild_id: String) -> Result<Guild, String> {
    let st = state.clone();
    let resultado = guild_info_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn guild_info_en(state: State<'_, AppState>, guild_id: String) -> Result<Guild, String> {
    informacion_del_servidor(&state, &guild_id, None).await
}

pub async fn informacion_del_servidor(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Guild, String> {
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_guild_info(&token, base_url, guild_id).await
}

pub async fn permisos_del_bot(
    state: &AppState,
    guild_id: &str,
    base_url: Option<&str>,
) -> Result<Vec<String>, String> {
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    let token = token_del_bot_activo(state)?;
    crate::discord::fetch_bot_permissions(&token, base_url, guild_id, &bot.id).await
}

// Roles
#[tauri::command]
pub async fn list_roles(state: State<'_, AppState>, guild_id: String) -> Result<Vec<Role>, String> {
    let st = state.clone();
    let resultado = list_roles_en(state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_roles_en(state: State<'_, AppState>, guild_id: String) -> Result<Vec<Role>, String> {
    leer_roles(&state, &guild_id, None).await
}

// Discord operations
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    channel_id: String,
    content: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = send_message_en(state, channel_id, content).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn send_message_en(
    state: State<'_, AppState>,
    channel_id: String,
    content: String,
) -> Result<String, String> {
    enviar_a_canal(&state, &channel_id, &content, None).await
}

/// Valida la longitud y envía a `POST /channels/{id}/messages`.
pub async fn enviar_a_canal(
    state: &AppState,
    channel_id: &str,
    content: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    crate::discord::validar_contenido(content)?;
    let token = token_del_bot_activo(state)?;
    let id = crate::discord::send_channel_message(&token, base_url, channel_id, content).await?;
    Ok(format!("Message sent to channel {channel_id} (id {id})"))
}

#[tauri::command]
pub async fn send_dm(
    state: State<'_, AppState>,
    user_id: String,
    content: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = send_dm_en(state, user_id, content).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn send_dm_en(
    state: State<'_, AppState>,
    user_id: String,
    content: String,
) -> Result<String, String> {
    enviar_dm(&state, &user_id, &content, None).await
}

/// Valida la longitud, abre el DM (`POST /users/@me/channels`) y envía.
pub async fn enviar_dm(
    state: &AppState,
    user_id: &str,
    content: &str,
    base_url: Option<&str>,
) -> Result<String, String> {
    crate::discord::validar_contenido(content)?;
    let token = token_del_bot_activo(state)?;
    let id = crate::discord::send_direct_message(&token, base_url, user_id, content).await?;
    Ok(format!("DM sent to user {user_id} (id {id})"))
}

#[tauri::command]
pub async fn get_member_info(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
) -> Result<Member, String> {
    let st = state.clone();
    let resultado = get_member_info_en(state, guild_id, user_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_member_info_en(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
) -> Result<Member, String> {
    leer_miembro(&state, &guild_id, &user_id, None).await
}

#[tauri::command]
pub async fn add_role(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    role_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = add_role_en(state, guild_id, user_id, role_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn add_role_en(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    role_id: String,
) -> Result<String, String> {
    cambiar_rol(&state, &guild_id, &user_id, &role_id, true, None).await
}

#[tauri::command]
pub async fn remove_role(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    role_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = remove_role_en(state, guild_id, user_id, role_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn remove_role_en(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    role_id: String,
) -> Result<String, String> {
    cambiar_rol(&state, &guild_id, &user_id, &role_id, false, None).await
}

/// `PUT`/`DELETE /guilds/{id}/members/{user_id}/roles/{role_id}`.
pub async fn cambiar_rol(
    state: &AppState,
    guild_id: &str,
    user_id: &str,
    role_id: &str,
    anadir: bool,
    base_url: Option<&str>,
) -> Result<String, String> {
    let token = token_del_bot_activo(state)?;
    crate::discord::set_member_role(&token, base_url, guild_id, user_id, role_id, anadir).await?;
    Ok(if anadir {
        format!("Role {role_id} added to user {user_id}")
    } else {
        format!("Role {role_id} removed from user {user_id}")
    })
}

#[tauri::command]
pub async fn timeout_member(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    duration_minutes: u32,
    reason: Option<String>,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = timeout_member_en(state, guild_id, user_id, duration_minutes, reason).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn timeout_member_en(
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
    duration_minutes: u32,
    reason: Option<String>,
) -> Result<String, String> {
    silenciar_miembro(
        &state,
        &guild_id,
        &user_id,
        duration_minutes,
        reason.as_deref(),
        None,
    )
    .await
}

/// `PATCH /guilds/{id}/members/{user_id}`; 0 minutos quita el timeout. Exige
/// «Moderar miembros» y que el objetivo no sea el dueño ni tenga un rol igual o
/// superior al del bot; el motivo va al registro de auditoría.
pub async fn silenciar_miembro(
    state: &AppState,
    guild_id: &str,
    user_id: &str,
    minutos: u32,
    motivo: Option<&str>,
    base_url: Option<&str>,
) -> Result<String, String> {
    if minutos > crate::discord::TIMEOUT_MAX_MINUTOS {
        return Err(crate::discord::ERROR_TIMEOUT_MAX.to_string());
    }
    exigir_permiso(state, guild_id, "Moderar miembros", base_url).await?;
    exigir_jerarquia_miembro(state, guild_id, user_id, false, base_url).await?;
    let token = token_del_bot_activo(state)?;
    crate::discord::set_member_timeout(&token, base_url, guild_id, user_id, minutos, motivo)
        .await?;
    Ok(if minutos == 0 {
        format!("Timeout removed from user {user_id}")
    } else {
        format!("User {user_id} timed out for {minutos} minutes")
    })
}

// Config
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    let st = state.clone();
    let resultado = get_config_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_config_en(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub async fn get_config_full(state: State<'_, AppState>) -> Result<Config, String> {
    let st = state.clone();
    let resultado = get_config_full_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_config_full_en(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = save_config_en(app, state, config).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn save_config_en(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
) -> Result<String, String> {
    *state.config.lock().unwrap() = config;
    state
        .save_config_to_db(&app)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Settings saved".to_string())
}

#[tauri::command]
pub async fn save_config_full(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = save_config_full_en(app, state, config).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn save_config_full_en(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
) -> Result<String, String> {
    *state.config.lock().unwrap() = config;
    state
        .save_config_to_db(&app)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Full settings saved".to_string())
}

#[tauri::command]
pub async fn get_active_guild_cmd(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let st = state.clone();
    let resultado = get_active_guild_cmd_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_active_guild_cmd_en(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.get_active_guild())
}

#[tauri::command]
pub async fn set_active_guild_cmd(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = set_active_guild_cmd_en(state, guild_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn set_active_guild_cmd_en(
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<String, String> {
    state.set_active_guild(&guild_id);
    Ok("Active server changed".to_string())
}

// Server map
#[tauri::command]
pub async fn get_server_map(state: State<'_, AppState>) -> Result<Option<ServerMap>, String> {
    let st = state.clone();
    let resultado = get_server_map_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_server_map_en(state: State<'_, AppState>) -> Result<Option<ServerMap>, String> {
    let guild_id = require_active_guild(&state)?;
    Ok(state.get_map(&guild_id))
}

#[tauri::command]
pub async fn refresh_server_map(state: State<'_, AppState>) -> Result<ServerMap, String> {
    let st = state.clone();
    let resultado = refresh_server_map_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn refresh_server_map_en(state: State<'_, AppState>) -> Result<ServerMap, String> {
    let guild_id = require_active_guild(&state)?;
    let channels = list_channels(state.clone(), guild_id.clone()).await?;
    let roles = list_roles(state.clone(), guild_id.clone()).await?;
    // Sin el intent de miembros Discord rechaza la lista: el resto del mapa
    // sigue siendo útil, así que no se aborta (el aviso del intent ya lo explica).
    let members = match list_members(state.clone(), guild_id.clone()).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("No se pudieron leer los miembros: {e}");
            Vec::new()
        }
    };
    // Nombre e icono reales (de la lista de servidores del bot), no inventados.
    let conocido = state
        .config
        .lock()
        .unwrap()
        .guilds_cache
        .iter()
        .find(|g| g.id == guild_id)
        .cloned();
    let guild = Guild {
        id: guild_id.clone(),
        name: conocido
            .as_ref()
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "Servidor".to_string()),
        member_count: members.len() as i64,
        icon: conocido.and_then(|g| g.icon),
        joined_at: None,
    };

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
    let st = state.clone();
    let resultado = list_bots_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_bots_en(state: State<'_, AppState>) -> Result<Vec<BotConfig>, String> {
    let config = state.config.lock().unwrap();
    Ok(config.bots.clone())
}

#[tauri::command]
pub async fn add_bot(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<BotRegistrado, String> {
    let st = state.clone();
    let resultado = add_bot_en(app, state, token).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn add_bot_en(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<BotRegistrado, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let (bot, warning) = registrar_bot(&pool, &token, None).await?;

    // Scoped, not drop(): the guard must not be live across an .await.
    {
        let mut config = state.config.lock().unwrap();
        config.bots.retain(|b| b.id != bot.id);
        config.bots.push(bot.clone());
    }
    Ok(BotRegistrado { bot, warning })
}

/// Bot recién registrado más el aviso opcional (p. ej. intent de miembros).
#[derive(Debug, Clone, serde::Serialize)]
pub struct BotRegistrado {
    #[serde(flatten)]
    pub bot: BotConfig,
    pub warning: Option<String>,
}

/// Valida el token con `GET /users/@me` y guarda el bot (token cifrado) con
/// los datos oficiales de Discord. Un token rechazado no guarda nada.
pub async fn registrar_bot(
    pool: &sqlx::SqlitePool,
    token: &str,
    base_url: Option<&str>,
) -> Result<(BotConfig, Option<String>), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("Invalid token".to_string());
    }
    let user = crate::discord::fetch_current_user(token, base_url).await?;
    let application = crate::discord::fetch_current_application(token, base_url).await?;
    let token_ciphertext =
        crypto::encrypt(token).map_err(|e| format!("Error encrypting the token: {}", e))?;
    let now = Utc::now().to_rfc3339();
    let bot = BotConfig {
        id: user.id.to_string(),
        name: user.name.clone(),
        token_ciphertext,
        avatar: user.avatar.map(|a| a.to_string()),
        application_id: Some(application.id.to_string()),
        allowed_guilds: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    db::upsert_bot(pool, &bot)
        .await
        .map_err(|e| e.to_string())?;
    Ok((bot, crate::discord::aviso_intent_miembros(&application)))
}

/// Enlace para invitar al bot activo a un servidor. Si el bot se registró antes
/// de guardar su id de aplicación, se pide a Discord y se guarda.
#[tauri::command]
pub async fn invite_url(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let st = state.clone();
    let resultado = invite_url_en(app, state).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn invite_url_en(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    enlace_de_invitacion(&pool, &state, None).await
}

/// Ruta del binario `argus-mcp`, que se instala junto al ejecutable de la app.
pub fn ruta_del_mcp() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not locate ARGUS: {e}"))?;
    let nombre = if cfg!(windows) {
        "argus-mcp.exe"
    } else {
        "argus-mcp"
    };
    let dir = exe.parent().ok_or("Could not locate the ARGUS folder")?;
    Ok(dir.join(nombre).to_string_lossy().into_owned())
}

#[tauri::command]
pub fn ruta_mcp(state: State<'_, AppState>) -> Result<String, String> {
    crate::i18n::localizar_error(&state, ruta_del_mcp())
}

pub async fn enlace_de_invitacion(
    pool: &sqlx::SqlitePool,
    state: &AppState,
    base_url: Option<&str>,
) -> Result<String, String> {
    let mut bot = state
        .get_active_bot()
        .ok_or_else(|| "No bot selected".to_string())?;
    if bot.application_id.is_none() {
        let token = crypto::decrypt(&bot.token_ciphertext)
            .map_err(|e| format!("Error decrypting the token: {}", e))?;
        let application = crate::discord::fetch_current_application(&token, base_url).await?;
        bot.application_id = Some(application.id.to_string());
        db::upsert_bot(pool, &bot)
            .await
            .map_err(|e| e.to_string())?;
        // Scoped: el guard no debe vivir a través de un .await.
        {
            let mut config = state.config.lock().unwrap();
            if let Some(b) = config.bots.iter_mut().find(|b| b.id == bot.id) {
                b.application_id = bot.application_id.clone();
            }
        }
    }
    let id = bot.application_id.unwrap_or_default();
    Ok(crate::discord::build_invite_url(&id))
}

#[tauri::command]
pub async fn update_bot(
    app: AppHandle,
    state: State<'_, AppState>,
    bot: BotConfig,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = update_bot_en(app, state, bot).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn update_bot_en(
    app: AppHandle,
    state: State<'_, AppState>,
    mut bot: BotConfig,
) -> Result<String, String> {
    // Encrypt token if not already encrypted
    if !bot.token_ciphertext.starts_with("encrypted:") {
        bot.token_ciphertext = crypto::encrypt(&bot.token_ciphertext)
            .map_err(|e| format!("Error encrypting the token: {}", e))?;
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
        application_id: bot.application_id.clone(),
        allowed_guilds: bot.allowed_guilds.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    db::upsert_bot(&pool, &bot_db)
        .await
        .map_err(|e| e.to_string())?;

    Ok("Bot updated".to_string())
}

#[tauri::command]
pub async fn delete_bot(
    app: AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = delete_bot_en(app, state, bot_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn delete_bot_en(
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

    Ok("Bot deleted".to_string())
}

#[tauri::command]
pub async fn set_active_bot(
    app: AppHandle,
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = set_active_bot_en(app, state, bot_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn set_active_bot_en(
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
        return Err("The bot is not registered".to_string());
    }
    state.set_active_bot(&bot_id);
    state
        .save_config_to_db(&app)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Active bot changed".to_string())
}

// Templates
#[tauri::command]
pub async fn list_templates(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<db::Template>, String> {
    let st = state.clone();
    let resultado = list_templates_en(app, state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_templates_en(
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
    let st = state.clone();
    let resultado = save_template_en(app, state, template).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn save_template_en(
    app: AppHandle,
    state: State<'_, AppState>,
    template: db::Template,
) -> Result<String, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::save_template(&pool, &template)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Template saved".to_string())
}

#[tauri::command]
pub async fn delete_template(
    app: AppHandle,
    state: State<'_, AppState>,
    template_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = delete_template_en(app, state, template_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn delete_template_en(
    app: AppHandle,
    state: State<'_, AppState>,
    template_id: String,
) -> Result<String, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::delete_template(&pool, &template_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Template deleted".to_string())
}

#[tauri::command]
pub async fn apply_template(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    template_id: String,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = apply_template_en(app, state, guild_id, template_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn apply_template_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    template_id: String,
) -> Result<String, String> {
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let template = db::get_template(&pool, &template_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Template not found")?;

    // TODO: Implement actual template application via Discord API
    // For now, just return success
    Ok(format!(
        "Template '{}' applied to server {}",
        template.name, guild_id
    ))
}

// Vigilance config
#[tauri::command]
pub async fn get_vigilance_config(state: State<'_, AppState>) -> Result<VigilanceConfig, String> {
    let st = state.clone();
    let resultado = get_vigilance_config_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_vigilance_config_en(state: State<'_, AppState>) -> Result<VigilanceConfig, String> {
    let config = state.config.lock().unwrap();
    Ok(config.vigilance.clone())
}

#[tauri::command]
pub async fn save_vigilance_config(
    app: AppHandle,
    state: State<'_, AppState>,
    vigilance: VigilanceConfig,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = save_vigilance_config_en(app, state, vigilance).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn save_vigilance_config_en(
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
    Ok("Monitoring settings saved".to_string())
}

// Ajustes propios de cada servidor (del bot activo)
#[tauri::command]
pub async fn get_guild_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<db::GuildSettings, String> {
    let st = state.clone();
    let resultado = get_guild_settings_en(app, state, guild_id).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn get_guild_settings_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
) -> Result<db::GuildSettings, String> {
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::get_guild_settings(&pool, &bot.id, &guild_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_guild_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    settings: db::GuildSettings,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = save_guild_settings_en(app, state, guild_id, settings).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn save_guild_settings_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    settings: db::GuildSettings,
) -> Result<String, String> {
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    let pool = db::get_pool(&app).await.map_err(|e| e.to_string())?;
    db::save_guild_settings(&pool, &bot.id, &guild_id, &settings)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Server settings saved".to_string())
}

// CLI Agents
#[tauri::command]
pub async fn list_cli_agents(state: State<'_, AppState>) -> Result<Vec<CliAgentConfig>, String> {
    let st = state.clone();
    let resultado = list_cli_agents_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn list_cli_agents_en(state: State<'_, AppState>) -> Result<Vec<CliAgentConfig>, String> {
    let config = state.config.lock().unwrap();
    Ok(config.cli_agents.values().cloned().collect())
}

#[tauri::command]
pub async fn save_cli_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    agent: CliAgentConfig,
) -> Result<String, String> {
    let st = state.clone();
    let resultado = save_cli_agent_en(app, state, agent).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn save_cli_agent_en(
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
    Ok("CLI agent saved".to_string())
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
    let st = state.clone();
    let resultado =
        search_messages_en(app, state, query, guild_id, channel_id, user_id, limit).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn search_messages_en(
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
    let st = state.clone();
    let resultado = cleanup_context_en(app, state, guild_id, user_id).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn cleanup_context_en(
    app: AppHandle,
    state: State<'_, AppState>,
    guild_id: String,
    user_id: String,
) -> Result<String, String> {
    state
        .cleanup_user_context(&app, &guild_id, &user_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Context cleared".to_string())
}

// Backup
#[tauri::command]
pub async fn export_all_data(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    let st = state.clone();
    let resultado = export_all_data_en(state).await;
    crate::i18n::localizar_error(&st, resultado)
}

async fn export_all_data_en(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    // TODO: Implement export to ZIP
    Ok(vec![])
}

#[tauri::command]
pub async fn import_data(state: State<'_, AppState>, _data: Vec<u8>) -> Result<String, String> {
    let st = state.clone();
    let resultado = import_data_en(state, _data).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn import_data_en(state: State<'_, AppState>, _data: Vec<u8>) -> Result<String, String> {
    // TODO: Implement import from ZIP
    Ok("Imported".to_string())
}

#[tauri::command]
pub async fn uninstall_wipe(state: State<'_, AppState>) -> Result<String, String> {
    let st = state.clone();
    let resultado = uninstall_wipe_en(state).await;
    crate::i18n::localizar_texto(&st, resultado)
}

async fn uninstall_wipe_en(state: State<'_, AppState>) -> Result<String, String> {
    // TODO: Implement full wipe (DB, config, logs, backups, shortcuts)
    Ok("Uninstall complete".to_string())
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
            application_id: None,
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

    async fn mock_aplicacion(server: &MockServer, auth: &str, id: &str) {
        mock_aplicacion_flags(server, auth, id, 1 << 14).await;
    }

    async fn mock_aplicacion_flags(server: &MockServer, auth: &str, id: &str, flags: u64) {
        Mock::given(method("GET"))
            .and(path("/api/v10/oauth2/applications/@me"))
            .and(header("authorization", auth))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": id,
                "name": "app",
                "description": "",
                "bot_public": true,
                "bot_require_code_grant": false,
                "summary": "",
                "rpc_origins": [],
                "verify_key": "abc",
                "icon": null,
                "flags": flags
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    #[test]
    fn ruta_del_mcp_apunta_a_argus_mcp_junto_al_ejecutable() {
        let ruta = ruta_del_mcp().unwrap();
        assert!(ruta.contains("argus-mcp"), "{ruta}");
    }

    #[tokio::test]
    async fn invite_url_usa_el_id_de_aplicacion_del_bot_activo() {
        let server = MockServer::start().await;
        mock_aplicacion(&server, "Bot token-inv", "777888999").await;

        let pool = pool_en_memoria().await;
        let state = estado_con_bot_activo("token-inv");
        let addr = server.address().to_string();
        let url = enlace_de_invitacion(&pool, &state, Some(&addr))
            .await
            .expect("enlace");

        assert!(url.starts_with(
            "https://discord.com/oauth2/authorize?client_id=777888999&scope=bot%20applications.commands&permissions="
        ));
        // Queda guardado para no volver a pedirlo.
        assert_eq!(
            state.get_active_bot().unwrap().application_id.as_deref(),
            Some("777888999")
        );
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
        mock_aplicacion(&server, "Bot token-valido", "555000111").await;

        let pool = pool_en_memoria().await;
        let addr = server.address().to_string();
        let (bot, aviso) = registrar_bot(&pool, "token-valido", Some(&addr))
            .await
            .expect("registro");
        assert_eq!(aviso, None);

        assert_eq!(bot.id, "123456789");
        assert_eq!(bot.name, "bot-oficial");
        assert_eq!(bot.application_id.as_deref(), Some("555000111"));
        assert!(bot.avatar.is_some());
        assert_ne!(bot.token_ciphertext, "token-valido");

        let guardado = db::get_bot(&pool, "123456789")
            .await
            .unwrap()
            .expect("bot guardado");
        assert_eq!(guardado.name, "bot-oficial");
        assert_eq!(guardado.avatar, bot.avatar);
        assert_eq!(guardado.application_id.as_deref(), Some("555000111"));
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

        assert_eq!(err, "Invalid token");
        assert!(db::list_bots(&pool).await.unwrap().is_empty());
    }

    async fn mock_usuario(server: &MockServer, auth: &str) {
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me"))
            .and(header("authorization", auth))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "123456789",
                "username": "bot-oficial",
                "discriminator": "0",
                "avatar": null,
                "bot": true,
                "mfa_enabled": false
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn registrar_bot_avisa_sin_intent_de_miembros() {
        let server = MockServer::start().await;
        mock_usuario(&server, "Bot token-sin").await;
        mock_aplicacion_flags(&server, "Bot token-sin", "5", 1 << 12).await;
        let pool = pool_en_memoria().await;
        let addr = server.address().to_string();
        let (_, aviso) = registrar_bot(&pool, "token-sin", Some(&addr))
            .await
            .expect("el aviso no es un error");
        assert_eq!(
            aviso.as_deref(),
            Some("Enable «Server Members Intent» in the Developer Portal → Bot")
        );
        assert_eq!(db::list_bots(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn registrar_bot_no_avisa_con_intent_limitado() {
        let server = MockServer::start().await;
        mock_usuario(&server, "Bot token-lim").await;
        mock_aplicacion_flags(&server, "Bot token-lim", "5", 1 << 15).await;
        let pool = pool_en_memoria().await;
        let addr = server.address().to_string();
        let (_, aviso) = registrar_bot(&pool, "token-lim", Some(&addr))
            .await
            .unwrap();
        assert_eq!(aviso, None);
    }

    #[tokio::test]
    async fn start_bot_avisa_sin_intent_y_no_avisa_con_intent() {
        for (flags, esperado) in [(0u64, true), (1 << 14, false)] {
            let server = MockServer::start().await;
            mock_usuario(&server, "Bot token-flags").await;
            mock_aplicacion_flags(&server, "Bot token-flags", "5", flags).await;
            let state = estado_con_bot_activo("token-flags");
            let addr = server.address().to_string();
            let status = conectar_bot(None, &state, Some(&addr))
                .await
                .expect("conexión");
            assert!(status.connected);
            assert_eq!(status.warning.is_some(), esperado, "flags {flags}");
            assert_eq!(state.bot_status.lock().unwrap().warning.is_some(), esperado);
        }
    }

    fn estado_con_bot_activo(token: &str) -> AppState {
        let state = AppState::new();
        let now = Utc::now().to_rfc3339();
        let bot = BotConfig {
            id: "123456789".to_string(),
            name: "nombre-viejo".to_string(),
            token_ciphertext: crypto::encrypt(token).unwrap(),
            avatar: None,
            application_id: None,
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
        let err = conectar_bot(None, &state, None)
            .await
            .expect_err("debe fallar");
        assert_eq!(err, "No bot selected");
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
        let status = conectar_bot(None, &state, Some(&addr))
            .await
            .expect("conexión");

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
        let err = conectar_bot(None, &state, Some(&addr))
            .await
            .expect_err("debe fallar");

        assert_eq!(err, "Invalid token");
        let status = state.bot_status.lock().unwrap();
        assert!(!status.connected && !status.running);
        assert_eq!(status.last_error.as_deref(), Some("Invalid token"));
    }

    #[tokio::test]
    async fn lee_canales_y_roles_del_discord_falso() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(header("authorization", "Bot token-cr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "10", "type": 4, "name": "Categoría", "position": 0,
                 "permission_overwrites": []},
                {"id": "11", "type": 0, "name": "general", "position": 2, "parent_id": "10",
                 "permission_overwrites": [
                    {"id": "555", "type": 0, "allow": "0", "deny": "1024"},
                    {"id": "77", "type": 1, "allow": "2048", "deny": "0"}
                 ]}
            ])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(header("authorization", "Bot token-cr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "555", "name": "@everyone", "color": 0, "hoist": false, "flags": 0,
                 "position": 0, "permissions": "1024", "managed": false, "mentionable": false},
                {"id": "600", "name": "Admin", "color": 16711680, "hoist": true, "flags": 0,
                 "position": 3, "permissions": "8", "managed": false, "mentionable": true}
            ])))
            .expect(1)
            .mount(&server)
            .await;

        let state = estado_con_bot_activo("token-cr");
        let addr = server.address().to_string();

        let canales = leer_canales(&state, "555", Some(&addr))
            .await
            .expect("canales");
        assert_eq!(canales.len(), 2);
        assert_eq!(canales[0].channel_type, 4);
        assert_eq!(canales[0].parent_id, None);
        assert_eq!(canales[1].name, "general");
        assert_eq!(canales[1].position, 2);
        assert_eq!(canales[1].parent_id.as_deref(), Some("10"));
        let ow = canales[1].overwrites.as_ref().expect("overwrites");
        assert_eq!(ow.len(), 2);
        assert_eq!((ow[0].id.as_str(), ow[0].type_), ("555", 0));
        assert_eq!(ow[0].deny, "1024");
        assert_eq!((ow[1].id.as_str(), ow[1].type_), ("77", 1));
        assert_eq!(ow[1].allow, "2048");

        let roles = leer_roles(&state, "555", Some(&addr)).await.expect("roles");
        assert_eq!(roles.len(), 2);
        assert_eq!(roles[1].name, "Admin");
        assert_eq!(roles[1].color, 16711680);
        assert_eq!(roles[1].position, 3);
        assert_eq!(roles[1].permissions, "8");
        assert_eq!(roles[1].permissions_bitfield.as_deref(), Some("8"));
        assert!(roles[1].hoist && roles[1].mentionable);
    }

    fn miembro_json(id: u64, nick: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "user": {"id": id.to_string(), "username": format!("u{id}"),
                     "discriminator": "0", "avatar": null},
            "nick": nick,
            "roles": ["600"],
            "joined_at": "2024-01-02T03:04:05.000000+00:00",
            "deaf": false, "mute": false, "flags": 0
        })
    }

    #[tokio::test]
    async fn lee_miembros_paginando_dos_paginas_del_discord_falso() {
        use wiremock::matchers::{query_param, query_param_is_missing};
        let server = MockServer::start().await;
        let pagina1: Vec<_> = (1..=1000).map(|i| miembro_json(i, None)).collect();
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/members"))
            .and(header("authorization", "Bot token-mi"))
            .and(query_param("limit", "1000"))
            .and(query_param_is_missing("after"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pagina1))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/members"))
            .and(query_param("limit", "1000"))
            .and(query_param("after", "1000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![
                miembro_json(1001, Some("apodo")),
                miembro_json(1002, None),
            ]))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/members/1001"))
            .and(header("authorization", "Bot token-mi"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(miembro_json(1001, Some("apodo"))),
            )
            .expect(1)
            .mount(&server)
            .await;

        let state = estado_con_bot_activo("token-mi");
        let addr = server.address().to_string();

        let miembros = leer_miembros(&state, "555", Some(&addr))
            .await
            .expect("miembros");
        assert_eq!(miembros.len(), 1002);
        assert_eq!(miembros[0].user_id, "1");
        assert_eq!(miembros[1001].user_id, "1002");
        assert_eq!(miembros[1000].nickname.as_deref(), Some("apodo"));
        assert_eq!(miembros[0].roles, vec!["600".to_string()]);

        let uno = leer_miembro(&state, "555", "1001", Some(&addr))
            .await
            .expect("miembro");
        assert_eq!(uno.username, "u1001");
        assert_eq!(uno.nickname.as_deref(), Some("apodo"));
        assert!(!uno.bot);
        assert!(uno.joined_at.is_some());
    }

    #[tokio::test]
    async fn permisos_del_bot_con_el_discord_falso() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555"))
            .and(header("authorization", "Bot token-pe"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "555", "name": "Servidor", "owner_id": "9",
                "roles": [], "emojis": [], "features": [], "afk_timeout": 300,
                "default_message_notifications": 0, "explicit_content_filter": 0,
                "mfa_level": 0, "nsfw_level": 0, "premium_tier": 0,
                "system_channel_flags": 0, "verification_level": 0
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "555", "name": "@everyone", "color": 0, "hoist": false, "flags": 0,
                 "position": 0, "permissions": "1024", "managed": false, "mentionable": false},
                {"id": "600", "name": "Mod", "color": 0, "hoist": false, "flags": 0,
                 "position": 1, "permissions": "268435456", "managed": false,
                 "mentionable": false}
            ])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/members/123456789"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user": {"id": "123456789", "username": "bot", "discriminator": "0",
                         "avatar": null, "bot": true},
                "nick": null, "roles": ["600"],
                "joined_at": "2024-01-02T03:04:05.000000+00:00",
                "deaf": false, "mute": false, "flags": 0
            })))
            .expect(1)
            .mount(&server)
            .await;

        let state = estado_con_bot_activo("token-pe");
        let addr = server.address().to_string();
        let permisos = permisos_del_bot(&state, "555", Some(&addr))
            .await
            .expect("permisos");
        assert_eq!(permisos, vec!["Ver canales", "Gestionar roles"]);
    }

    #[tokio::test]
    async fn informacion_del_servidor_con_recuento_aproximado() {
        use wiremock::matchers::query_param;
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555"))
            .and(query_param("with_counts", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "555", "name": "Servidor", "icon": "abc123", "owner_id": "9",
                "approximate_member_count": 42, "approximate_presence_count": 7
            })))
            .expect(1)
            .mount(&server)
            .await;

        let state = estado_con_bot_activo("token-pe");
        let addr = server.address().to_string();
        let g = informacion_del_servidor(&state, "555", Some(&addr))
            .await
            .expect("guild");
        assert_eq!(g.name, "Servidor");
        assert_eq!(g.icon.as_deref(), Some("abc123"));
        assert_eq!(g.member_count, 42);
    }

    #[tokio::test]
    async fn permisos_del_bot_sin_bot_activo_da_error() {
        let state = AppState::new();
        let err = permisos_del_bot(&state, "555", None).await.unwrap_err();
        assert_eq!(err, "No bot selected");
    }

    #[tokio::test]
    async fn leer_canales_sin_bot_activo_da_error() {
        let state = AppState::new();
        let err = leer_canales(&state, "555", None).await.unwrap_err();
        assert_eq!(err, "No bot selected");
    }

    fn mensaje_json(id: &str, canal: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "channel_id": canal, "content": "hola", "timestamp": "2026-10-27T10:00:00.000000+00:00",
            "edited_timestamp": null, "tts": false, "mention_everyone": false, "mentions": [],
            "mention_roles": [], "attachments": [], "embeds": [], "pinned": false, "type": 0,
            "author": {"id": "1", "username": "bot", "discriminator": "0", "avatar": null}
        })
    }

    #[tokio::test]
    async fn envia_mensaje_a_canal_con_el_discord_falso() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/42/messages"))
            .and(header("authorization", "Bot token-msg"))
            .and(wiremock::matchers::body_partial_json(
                serde_json::json!({"content": "hola"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(mensaje_json("900", "42")))
            .expect(1)
            .mount(&server)
            .await;
        let state = estado_con_bot_activo("token-msg");
        let addr = server.address().to_string();
        let r = enviar_a_canal(&state, "42", "hola", Some(&addr))
            .await
            .expect("envío");
        assert!(r.contains("900"), "{r}");
    }

    #[tokio::test]
    async fn envia_dm_abriendo_el_canal_privado() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v10/users/@me/channels"))
            .and(wiremock::matchers::body_partial_json(
                serde_json::json!({"recipient_id": "77"}),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": "4242", "type": 1})),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/4242/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mensaje_json("901", "4242")))
            .expect(1)
            .mount(&server)
            .await;
        let state = estado_con_bot_activo("token-dm");
        let addr = server.address().to_string();
        let r = enviar_dm(&state, "77", "hola", Some(&addr))
            .await
            .expect("dm");
        assert!(r.contains("901"), "{r}");
    }

    #[tokio::test]
    async fn mensaje_demasiado_largo_no_llama_a_discord() {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::any())
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let state = estado_con_bot_activo("token-largo");
        let addr = server.address().to_string();
        let largo = "a".repeat(2001);
        let e1 = enviar_a_canal(&state, "42", &largo, Some(&addr))
            .await
            .unwrap_err();
        assert!(e1.contains("too long"), "{e1}");
        let e2 = enviar_dm(&state, "77", &largo, Some(&addr))
            .await
            .unwrap_err();
        assert!(e2.contains("too long"), "{e2}");
        let e3 = enviar_a_canal(&state, "42", "  ", Some(&addr))
            .await
            .unwrap_err();
        assert!(e3.contains("empty"), "{e3}");
        // 2000 caracteres exactos sí pasan la validación.
        assert!(crate::discord::validar_contenido(&"ñ".repeat(2000)).is_ok());
    }

    #[tokio::test]
    async fn anade_y_quita_rol_con_el_discord_falso() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/guilds/555/members/1001/roles/7"))
            .and(header("authorization", "Bot token-rol"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/v10/guilds/555/members/1001/roles/7"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let state = estado_con_bot_activo("token-rol");
        let addr = server.address().to_string();
        let r = cambiar_rol(&state, "555", "1001", "7", true, Some(&addr))
            .await
            .expect("añadir");
        assert!(r.contains("added"), "{r}");
        let r = cambiar_rol(&state, "555", "1001", "7", false, Some(&addr))
            .await
            .expect("quitar");
        assert!(r.contains("removed"), "{r}");
    }

    #[tokio::test]
    async fn rol_sin_permiso_o_id_invalido_da_error_en_espanol() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(
                    serde_json::json!({"code":50013,"message":"Missing Permissions"}),
                ),
            )
            .mount(&server)
            .await;
        let state = estado_con_bot_activo("token-rol403");
        let addr = server.address().to_string();
        let e = cambiar_rol(&state, "555", "1001", "7", true, Some(&addr))
            .await
            .unwrap_err();
        assert!(e.contains("missing a permission"), "{e}");
        let e = cambiar_rol(&state, "555", "abc", "7", true, Some(&addr))
            .await
            .unwrap_err();
        assert!(e.contains("Invalid"), "{e}");
    }

    #[tokio::test]
    async fn timeout_mayor_de_28_dias_no_llama_a_discord() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let state = estado_con_bot_activo("token-to2");
        let addr = server.address().to_string();
        let e = silenciar_miembro(&state, "555", "1001", 28 * 24 * 60 + 1, None, Some(&addr))
            .await
            .unwrap_err();
        assert!(e.contains("28 days"), "{e}");
    }

    #[tokio::test]
    async fn enviar_sin_permiso_da_error_en_espanol() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/42/messages"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(
                    serde_json::json!({"code":50013,"message":"Missing Permissions"}),
                ),
            )
            .mount(&server)
            .await;
        let state = estado_con_bot_activo("token-403");
        let addr = server.address().to_string();
        let e = enviar_a_canal(&state, "42", "hola", Some(&addr))
            .await
            .unwrap_err();
        assert!(e.contains("missing a permission"), "{e}");
    }

    /// Mismo error, mismo Discord falso: inglés por defecto y español con `ui_language = "es"`.
    #[tokio::test]
    async fn los_errores_salen_en_el_idioma_de_la_config() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(serde_json::json!({"message": "401: Unauthorized", "code": 0})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/42/messages"))
            .respond_with(ResponseTemplate::new(403).set_body_json(
                serde_json::json!({"code": 50013, "message": "Missing Permissions"}),
            ))
            .mount(&server)
            .await;
        let addr = server.address().to_string();
        let state = estado_con_bot_activo("token-malo");
        let pool = pool_en_memoria().await;

        let token = registrar_bot(&pool, "token-malo", Some(&addr))
            .await
            .unwrap_err();
        let permiso = enviar_a_canal(&state, "42", "hola", Some(&addr))
            .await
            .unwrap_err();

        // Por defecto (config sin tocar): inglés.
        assert_eq!(state.config.lock().unwrap().ui_language, "en");
        let en = |r: Result<(), String>| crate::i18n::localizar_error(&state, r).unwrap_err();
        assert_eq!(en(Err(token.clone())), "Invalid token");
        assert_eq!(en(Err(permiso.clone())), permiso);

        state.config.lock().unwrap().ui_language = "es".to_string();
        let es = |r: Result<(), String>| crate::i18n::localizar_error(&state, r).unwrap_err();
        assert_eq!(es(Err(token)), "Token inválido");
        assert_ne!(es(Err(permiso.clone())), permiso);
        assert!(es(Err(permiso)).starts_with("Al bot le falta"));

        // Los mensajes de éxito y el estado del bot también.
        let ok = crate::i18n::localizar_texto(&state, Ok("Channel 7 deleted".into()));
        assert_eq!(ok.unwrap(), "Canal 7 borrado");
        let estado = BotStatus {
            last_error: Some("Invalid token".into()),
            ..BotStatus::default()
        };
        let estado = crate::i18n::localizar_estado(&state, Ok(estado)).unwrap();
        assert_eq!(estado.last_error.as_deref(), Some("Token inválido"));
        state.config.lock().unwrap().ui_language = "en".to_string();
        let sin_catalogo = crate::i18n::localizar_texto(&state, Ok("Brand new text".into()));
        assert_eq!(sin_catalogo.unwrap(), "Brand new text");
    }
}
