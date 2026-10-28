//! Cliente de Discord (REST) sobre twilight-http con URL base configurable.

use twilight_http::client::ClientBuilder;
use twilight_http::error::ErrorType;
use twilight_http::request::AuditLogReason;
use twilight_http::Client;

/// Construye un cliente de Discord a partir de un token.
///
/// Si `base_url` es `Some("host:puerto")`, las peticiones van por HTTP a ese
/// proxy (Discord falso en tests) y se desactiva el rate-limiter local.
/// Con `None` se usa la API oficial de Discord.
pub fn build_client(token: &str, base_url: Option<&str>) -> Client {
    let builder = ClientBuilder::new().token(token.to_string());
    match base_url {
        Some(url) => builder
            .proxy(url.to_string(), true)
            .ratelimiter(None)
            .build(),
        None => builder.build(),
    }
}

/// Obtiene el usuario del token (`GET /users/@me`). Un 401 se traduce a
/// "Token inválido".
pub async fn fetch_current_user(
    token: &str,
    base_url: Option<&str>,
) -> Result<twilight_model::user::CurrentUser, String> {
    let client = build_client(token, base_url);
    let response = client.current_user().await.map_err(traducir_error)?;
    response
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))
}

/// Obtiene la aplicación del bot (`GET /applications/@me`; twilight usa su
/// alias `/oauth2/applications/@me`), de donde sale el id de aplicación para el
/// enlace de invitación.
pub async fn fetch_current_application(
    token: &str,
    base_url: Option<&str>,
) -> Result<twilight_model::oauth::Application, String> {
    let client = build_client(token, base_url);
    let response = client
        .current_user_application()
        .await
        .map_err(traducir_error)?;
    response
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))
}

/// Aviso (no error) cuando la aplicación no tiene activo el intent de miembros.
pub const AVISO_INTENT_MIEMBROS: &str =
    "Activa «Server Members Intent» en el Developer Portal → Bot";

/// Comprueba los `flags` de la aplicación: GATEWAY_GUILD_MEMBERS (1<<14) o
/// GATEWAY_GUILD_MEMBERS_LIMITED (1<<15). Sin `flags` en la respuesta no hay
/// base para avisar.
pub fn aviso_intent_miembros(application: &twilight_model::oauth::Application) -> Option<String> {
    let flags = application.flags?.bits();
    if flags & ((1 << 14) | (1 << 15)) == 0 {
        Some(AVISO_INTENT_MIEMBROS.to_string())
    } else {
        None
    }
}

/// Permisos que ARGUS pide al invitar al bot a un servidor (bits de Discord):
/// ver canales (1<<10), enviar mensajes (1<<11), leer historial (1<<16),
/// gestionar canales (1<<4), gestionar roles (1<<28), moderar miembros
/// —timeout— (1<<40), expulsar (1<<1) y banear (1<<2).
pub const INVITE_PERMISSIONS: u64 =
    (1 << 10) | (1 << 11) | (1 << 16) | (1 << 4) | (1 << 28) | (1 << 40) | (1 << 1) | (1 << 2);

/// Enlace OAuth2 para invitar al bot (`scope=bot applications.commands`).
pub fn build_invite_url(application_id: &str) -> String {
    format!(
        "https://discord.com/oauth2/authorize?client_id={application_id}&scope=bot%20applications.commands&permissions={INVITE_PERMISSIONS}"
    )
}

/// Lista los servidores donde está el bot (`GET /users/@me/guilds`).
pub async fn fetch_current_user_guilds(
    token: &str,
    base_url: Option<&str>,
) -> Result<Vec<twilight_model::user::CurrentUserGuild>, String> {
    let client = build_client(token, base_url);
    let response = client.current_user_guilds().await.map_err(traducir_error)?;
    response
        .models()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))
}

/// Nombre en español de un permiso de Discord (nombre de la API en mayúsculas).
fn nombre_permiso(api: &str) -> String {
    match api {
        "ADMINISTRATOR" => "Administrador",
        "VIEW_CHANNEL" => "Ver canales",
        "SEND_MESSAGES" => "Enviar mensajes",
        "READ_MESSAGE_HISTORY" => "Leer el historial de mensajes",
        "MANAGE_CHANNELS" => "Gestionar canales",
        "MANAGE_ROLES" => "Gestionar roles",
        "MANAGE_GUILD" => "Gestionar el servidor",
        "MANAGE_MESSAGES" => "Gestionar mensajes",
        "KICK_MEMBERS" => "Expulsar miembros",
        "BAN_MEMBERS" => "Banear miembros",
        "MODERATE_MEMBERS" => "Moderar miembros",
        otro => return otro.to_string(),
    }
    .to_string()
}

/// Busca en el texto de Discord un nombre de permiso (`MANAGE_ROLES`…).
fn permiso_en_texto(texto: &str) -> Option<String> {
    texto
        .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
        .find(|p| (p.len() > 4 && p.contains('_')) || *p == "ADMINISTRATOR")
        .map(nombre_permiso)
}

/// Único conversor de respuestas de error de Discord a mensajes en español.
/// `cuerpo` es el JSON de error de Discord (puede venir vacío o no ser JSON).
pub fn traducir_respuesta(status: u16, cuerpo: &[u8]) -> String {
    let json: Option<serde_json::Value> = serde_json::from_slice(cuerpo).ok();
    let texto = json
        .as_ref()
        .and_then(|j| j.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    match status {
        401 => "Token inválido".to_string(),
        403 => match permiso_en_texto(texto) {
            Some(p) => format!("Al bot le falta el permiso «{p}»"),
            None => "Al bot le falta un permiso para hacer esto en el servidor".to_string(),
        },
        404 => {
            "No encontrado: el servidor, canal, rol o miembro no existe o el bot no tiene acceso"
                .to_string()
        }
        429 => {
            let espera = json
                .as_ref()
                .and_then(|j| j.get("retry_after"))
                .and_then(|r| r.as_f64());
            match espera {
                Some(s) => format!(
                    "Discord limitó las peticiones; vuelve a intentarlo en {:.0} s",
                    s.ceil()
                ),
                None => "Discord limitó las peticiones; vuelve a intentarlo en unos segundos"
                    .to_string(),
            }
        }
        _ if texto.is_empty() => format!("Discord respondió con un error ({status})"),
        _ => format!("Discord respondió con un error ({status}): {texto}"),
    }
}

/// Convierte un error de twilight-http en un mensaje en español. Lo usan todas
/// las funciones de este módulo, y con ellas todos los comandos de Discord.
pub fn traducir_error(e: twilight_http::Error) -> String {
    match e.kind() {
        ErrorType::Response { status, body, .. } => traducir_respuesta(status.get(), body),
        ErrorType::Unauthorized => traducir_respuesta(401, b""),
        _ => format!("No se pudo contactar con Discord: {e}"),
    }
}

/// Convierte un canal de Discord al tipo `Channel` de ARGUS, con sus overwrites.
pub fn convertir_canal(c: twilight_model::channel::Channel) -> crate::state::Channel {
    use twilight_model::channel::permission_overwrite::PermissionOverwriteType;
    let overwrites = c.permission_overwrites.map(|v| {
        v.into_iter()
            .map(|o| crate::state::Overwrite {
                id: o.id.to_string(),
                type_: match o.kind {
                    PermissionOverwriteType::Member => 1,
                    _ => 0,
                },
                allow: o.allow.bits().to_string(),
                deny: o.deny.bits().to_string(),
            })
            .collect()
    });
    crate::state::Channel {
        id: c.id.to_string(),
        name: c.name.unwrap_or_default(),
        channel_type: u8::from(c.kind),
        parent_id: c.parent_id.map(|p| p.to_string()),
        position: c.position.unwrap_or(0),
        overwrites,
        // `GET /guilds/{id}/channels` no trae permisos calculados del bot.
        permissions_bitfield: None,
    }
}

/// Convierte un rol de Discord al tipo `Role` de ARGUS. Discord no informa de
/// cuántos miembros tiene cada rol, así que `member_count` queda en 0.
pub fn convertir_rol(r: twilight_model::guild::Role) -> crate::state::Role {
    let permisos = r.permissions.bits().to_string();
    crate::state::Role {
        id: r.id.to_string(),
        name: r.name,
        color: r.color,
        position: r.position as i32,
        permissions: permisos.clone(),
        permissions_bitfield: Some(permisos),
        member_count: 0,
        mentionable: r.mentionable,
        hoist: r.hoist,
    }
}

fn parse_guild_id(
    guild_id: &str,
) -> Result<twilight_model::id::Id<twilight_model::id::marker::GuildMarker>, String> {
    guild_id
        .parse::<u64>()
        .ok()
        .and_then(twilight_model::id::Id::new_checked)
        .ok_or_else(|| "ID de servidor inválido".to_string())
}

/// Lee los canales de un servidor (`GET /guilds/{id}/channels`).
pub async fn fetch_guild_channels(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<Vec<crate::state::Channel>, String> {
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    let response = client.guild_channels(gid).await.map_err(traducir_error)?;
    let canales = response
        .models()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(canales.into_iter().map(convertir_canal).collect())
}

/// Lee los roles de un servidor (`GET /guilds/{id}/roles`).
pub async fn fetch_guild_roles(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<Vec<crate::state::Role>, String> {
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    let response = client.roles(gid).await.map_err(traducir_error)?;
    let roles = response
        .models()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(roles.into_iter().map(convertir_rol).collect())
}

/// Convierte un miembro de Discord al tipo `Member` de ARGUS.
pub fn convertir_miembro(m: twilight_model::guild::Member) -> crate::state::Member {
    crate::state::Member {
        user_id: m.user.id.to_string(),
        username: m.user.name,
        nickname: m.nick,
        roles: m.roles.iter().map(|r| r.to_string()).collect(),
        bot: m.user.bot,
        joined_at: Some(m.joined_at.iso_8601().to_string()),
        avatar: m.avatar.or(m.user.avatar).map(|a| a.to_string()),
        flags: Some(m.flags.bits()),
    }
}

/// Tamaño de página al listar miembros (máximo que admite Discord).
const PAGINA_MIEMBROS: u16 = 1000;

/// Lee todos los miembros de un servidor paginando
/// `GET /guilds/{id}/members?limit=1000&after=<último id>` hasta agotar.
pub async fn fetch_guild_members(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<Vec<crate::state::Member>, String> {
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    let mut todos = Vec::new();
    let mut despues: Option<twilight_model::id::Id<twilight_model::id::marker::UserMarker>> = None;
    loop {
        let mut peticion = client
            .guild_members(gid)
            .limit(PAGINA_MIEMBROS)
            .map_err(|e| format!("Petición inválida: {e}"))?;
        if let Some(id) = despues {
            peticion = peticion.after(id);
        }
        let pagina = peticion
            .await
            .map_err(traducir_error)?
            .models()
            .await
            .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
        let completa = pagina.len() >= PAGINA_MIEMBROS as usize;
        despues = pagina.last().map(|m| m.user.id);
        todos.extend(pagina.into_iter().map(convertir_miembro));
        if !completa || despues.is_none() {
            break;
        }
    }
    Ok(todos)
}

/// Lee un miembro (`GET /guilds/{id}/members/{user_id}`).
pub async fn fetch_guild_member(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    user_id: &str,
) -> Result<crate::state::Member, String> {
    let gid = parse_guild_id(guild_id)?;
    let uid = user_id
        .parse::<u64>()
        .ok()
        .and_then(twilight_model::id::Id::new_checked)
        .ok_or_else(|| "ID de usuario inválido".to_string())?;
    let client = build_client(token, base_url);
    let miembro = client
        .guild_member(gid, uid)
        .await
        .map_err(traducir_error)?
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(convertir_miembro(miembro))
}

/// Longitud máxima de un mensaje de Discord (caracteres).
pub const MAX_LONGITUD_MENSAJE: usize = 2000;

/// Valida el contenido antes de llamar a Discord: no vacío y ≤ 2000 caracteres.
pub fn validar_contenido(content: &str) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("El mensaje no puede estar vacío".to_string());
    }
    let n = content.chars().count();
    if n > MAX_LONGITUD_MENSAJE {
        return Err(format!(
            "El mensaje es demasiado largo ({n} caracteres; el máximo es {MAX_LONGITUD_MENSAJE})"
        ));
    }
    Ok(())
}

fn parse_id<M>(texto: &str, que: &str) -> Result<twilight_model::id::Id<M>, String> {
    texto
        .parse::<u64>()
        .ok()
        .and_then(twilight_model::id::Id::new_checked)
        .ok_or_else(|| format!("ID de {que} inválido"))
}

/// Envía un mensaje a un canal (`POST /channels/{id}/messages`) y devuelve el
/// id del mensaje creado.
pub async fn send_channel_message(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    content: &str,
) -> Result<String, String> {
    validar_contenido(content)?;
    let cid = parse_id(channel_id, "canal")?;
    let client = build_client(token, base_url);
    let mensaje = client
        .create_message(cid)
        .content(content)
        .map_err(|e| format!("Mensaje inválido: {e}"))?
        .await
        .map_err(traducir_error)?
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(mensaje.id.to_string())
}

/// Abre (o recupera) el DM con un usuario (`POST /users/@me/channels`) y le
/// envía el mensaje. Devuelve el id del mensaje creado.
pub async fn send_direct_message(
    token: &str,
    base_url: Option<&str>,
    user_id: &str,
    content: &str,
) -> Result<String, String> {
    validar_contenido(content)?;
    let uid = parse_id(user_id, "usuario")?;
    let client = build_client(token, base_url);
    let canal = client
        .create_private_channel(uid)
        .await
        .map_err(traducir_error)?
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    send_channel_message(token, base_url, &canal.id.to_string(), content).await
}

/// Añade (`PUT`) o quita (`DELETE`) un rol a un miembro:
/// `/guilds/{id}/members/{user_id}/roles/{role_id}`.
pub async fn set_member_role(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    user_id: &str,
    role_id: &str,
    anadir: bool,
) -> Result<(), String> {
    let gid = parse_guild_id(guild_id)?;
    let uid = parse_id(user_id, "usuario")?;
    let rid = parse_id(role_id, "rol")?;
    let client = build_client(token, base_url);
    if anadir {
        client
            .add_guild_member_role(gid, uid, rid)
            .await
            .map_err(traducir_error)?;
    } else {
        client
            .remove_guild_member_role(gid, uid, rid)
            .await
            .map_err(traducir_error)?;
    }
    Ok(())
}

/// Máximo que admite Discord para un timeout: 28 días.
pub const TIMEOUT_MAX_MINUTOS: u32 = 28 * 24 * 60;

/// Silencia a un miembro (`PATCH /guilds/{id}/members/{user_id}` con
/// `communication_disabled_until`). 0 minutos quita el timeout.
pub async fn set_member_timeout(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    user_id: &str,
    minutos: u32,
) -> Result<(), String> {
    if minutos > TIMEOUT_MAX_MINUTOS {
        return Err("El silencio no puede superar los 28 días (40320 minutos)".to_string());
    }
    let gid = parse_guild_id(guild_id)?;
    let uid = parse_id(user_id, "usuario")?;
    let hasta = if minutos == 0 {
        None
    } else {
        let secs = chrono::Utc::now().timestamp() + i64::from(minutos) * 60;
        Some(
            twilight_model::util::Timestamp::from_secs(secs)
                .map_err(|_| "Fecha de silencio inválida".to_string())?,
        )
    };
    let client = build_client(token, base_url);
    client
        .update_guild_member(gid, uid)
        .communication_disabled_until(hasta)
        .map_err(|_| "Duración de silencio inválida".to_string())?
        .await
        .map_err(traducir_error)?;
    Ok(())
}

/// Id del dueño del servidor (`GET /guilds/{id}`).
pub async fn fetch_guild_owner_id(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<Option<String>, String> {
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    let cuerpo = client
        .guild(gid)
        .await
        .map_err(traducir_error)?
        .bytes()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(serde_json::from_slice::<serde_json::Value>(&cuerpo)
        .ok()
        .and_then(|v| v.get("owner_id")?.as_str().map(str::to_string)))
}

/// Máximo de caracteres del motivo de auditoría que admite Discord.
const MAX_MOTIVO: usize = 512;

fn validar_motivo(motivo: Option<&str>) -> Result<Option<&str>, String> {
    let motivo = motivo.map(str::trim).filter(|m| !m.is_empty());
    if motivo.is_some_and(|m| m.chars().count() > MAX_MOTIVO) {
        return Err(format!(
            "El motivo no puede superar los {MAX_MOTIVO} caracteres"
        ));
    }
    Ok(motivo)
}

/// Expulsa a un miembro (`DELETE /guilds/{id}/members/{user_id}`); el motivo
/// viaja en la cabecera `X-Audit-Log-Reason`.
pub async fn kick_guild_member(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    user_id: &str,
    motivo: Option<&str>,
) -> Result<(), String> {
    let motivo = validar_motivo(motivo)?;
    let gid = parse_guild_id(guild_id)?;
    let uid = parse_id(user_id, "usuario")?;
    let client = build_client(token, base_url);
    let mut peticion = client.remove_guild_member(gid, uid);
    if let Some(m) = motivo {
        peticion = peticion
            .reason(m)
            .map_err(|_| "Motivo inválido".to_string())?;
    }
    peticion.await.map_err(traducir_error)?;
    Ok(())
}

/// Motivo codificado como lo espera `X-Audit-Log-Reason` (porcentaje UTF-8).
fn codificar_motivo(motivo: &str) -> String {
    let mut s = String::new();
    for b in motivo.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

/// Máximo de días de mensajes que se pueden borrar al banear.
pub const BAN_DIAS_MAX: u8 = 7;

/// Banea a un miembro (`PUT /guilds/{id}/bans/{user_id}`) borrando sus mensajes
/// de los últimos `dias` (0–7); el motivo va en `X-Audit-Log-Reason`.
pub async fn ban_guild_member(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    user_id: &str,
    motivo: Option<&str>,
    dias: u8,
) -> Result<(), String> {
    if dias > BAN_DIAS_MAX {
        return Err("Los días de mensajes a borrar deben estar entre 0 y 7".to_string());
    }
    let motivo = validar_motivo(motivo)?;
    let gid = parse_guild_id(guild_id)?;
    let uid = parse_id::<twilight_model::id::marker::UserMarker>(user_id, "usuario")?;
    // twilight 0.15 manda `delete_message_seconds` en la query, pero Discord lo
    // lee del cuerpo JSON: se arma la petición a mano (con la cabecera de motivo).
    let cuerpo = serde_json::json!({"delete_message_seconds": u32::from(dias) * 86_400});
    let mut constructor =
        twilight_http::request::Request::builder(&twilight_http::routing::Route::CreateBan {
            delete_message_seconds: None,
            guild_id: gid.get(),
            user_id: uid.get(),
        })
        .json(&cuerpo)
        .map_err(|e| format!("Petición inválida: {e}"))?;
    if let Some(m) = motivo {
        // Los tipos de cabecera son los de la versión de `http` de twilight:
        // se obtienen por inferencia con `parse`.
        let nombre = "x-audit-log-reason"
            .parse()
            .map_err(|_| "Motivo inválido".to_string())?;
        let valor = codificar_motivo(m)
            .parse()
            .map_err(|_| "Motivo inválido".to_string())?;
        constructor = constructor.headers(std::iter::once((nombre, valor)));
    }
    build_client(token, base_url)
        .request::<twilight_http::response::marker::EmptyBody>(constructor.build())
        .await
        .map_err(traducir_error)?;
    Ok(())
}

/// Quita un ban (`DELETE /guilds/{id}/bans/{user_id}`).
pub async fn unban_guild_member(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    user_id: &str,
    motivo: Option<&str>,
) -> Result<(), String> {
    let motivo = validar_motivo(motivo)?;
    let gid = parse_guild_id(guild_id)?;
    let uid = parse_id(user_id, "usuario")?;
    let client = build_client(token, base_url);
    let mut peticion = client.delete_ban(gid, uid);
    if let Some(m) = motivo {
        peticion = peticion
            .reason(m)
            .map_err(|_| "Motivo inválido".to_string())?;
    }
    peticion.await.map_err(traducir_error)?;
    Ok(())
}

/// Un ban del servidor, listo para la UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ban {
    pub user_id: String,
    pub username: String,
    pub reason: Option<String>,
}

/// Lista los bans (`GET /guilds/{id}/bans`).
pub async fn fetch_guild_bans(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<Vec<Ban>, String> {
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    let bans = client
        .bans(gid)
        .await
        .map_err(traducir_error)?
        .models()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(bans
        .into_iter()
        .map(|b| Ban {
            user_id: b.user.id.to_string(),
            username: b.user.name,
            reason: b.reason,
        })
        .collect())
}

/// Cambia el apodo de un miembro (`PATCH /guilds/{id}/members/{user_id}`);
/// `None` o vacío lo borra.
pub async fn set_member_nickname(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    user_id: &str,
    apodo: Option<&str>,
    motivo: Option<&str>,
) -> Result<(), String> {
    let apodo = apodo.map(str::trim).filter(|a| !a.is_empty());
    if apodo.is_some_and(|a| a.chars().count() > 32) {
        return Err("El apodo no puede superar los 32 caracteres".to_string());
    }
    let motivo = validar_motivo(motivo)?;
    let gid = parse_guild_id(guild_id)?;
    let uid = parse_id(user_id, "usuario")?;
    let client = build_client(token, base_url);
    let mut peticion = client
        .update_guild_member(gid, uid)
        .nick(apodo)
        .map_err(|_| "Apodo inválido".to_string())?;
    if let Some(m) = motivo {
        peticion = peticion
            .reason(m)
            .map_err(|_| "Motivo inválido".to_string())?;
    }
    peticion.await.map_err(traducir_error)?;
    Ok(())
}

/// Tipos de canal que ARGUS sabe crear: texto, voz, categoría, anuncios y foro.
pub const TIPOS_DE_CANAL: [u8; 5] = [0, 2, 4, 5, 15];
const MAX_SLOWMODE_SEGUNDOS: u16 = 21600;

/// Campos editables de un canal. En `update_channel` solo se envían los `Some`.
/// `parent_id` vacío (`""`) saca el canal de su categoría.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DatosCanal {
    pub name: Option<String>,
    pub channel_type: Option<u8>,
    pub parent_id: Option<String>,
    pub topic: Option<String>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<u16>,
}

fn validar_datos_canal(datos: &DatosCanal) -> Result<(), String> {
    if let Some(nombre) = &datos.name {
        let largo = nombre.trim().chars().count();
        if largo == 0 || largo > 100 {
            return Err("El nombre del canal debe tener entre 1 y 100 caracteres".to_string());
        }
    }
    if let Some(tipo) = datos.channel_type {
        if !TIPOS_DE_CANAL.contains(&tipo) {
            return Err("Tipo de canal no admitido".to_string());
        }
    }
    if datos
        .rate_limit_per_user
        .is_some_and(|s| s > MAX_SLOWMODE_SEGUNDOS)
    {
        return Err("El modo lento no puede superar las 6 horas (21600 s)".to_string());
    }
    if datos
        .topic
        .as_ref()
        .is_some_and(|t| t.chars().count() > 1024)
    {
        return Err("El tema del canal no puede superar los 1024 caracteres".to_string());
    }
    Ok(())
}

/// Crea un canal o categoría (`POST /guilds/{id}/channels`).
pub async fn create_guild_channel(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    datos: &DatosCanal,
) -> Result<crate::state::Channel, String> {
    use twilight_model::channel::ChannelType;
    validar_datos_canal(datos)?;
    let nombre = datos
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or("El canal necesita un nombre")?;
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    let mut peticion = client
        .create_guild_channel(gid, nombre)
        .map_err(|_| "Nombre de canal inválido".to_string())?;
    let tipo = datos.channel_type.unwrap_or(0);
    peticion = peticion.kind(ChannelType::from(tipo));
    if let Some(padre) = datos.parent_id.as_deref().filter(|p| !p.is_empty()) {
        if tipo == 4 {
            return Err("Una categoría no puede estar dentro de otra categoría".to_string());
        }
        peticion = peticion.parent_id(parse_id(padre, "categoría")?);
    }
    if let Some(tema) = datos.topic.as_deref() {
        peticion = peticion
            .topic(tema)
            .map_err(|_| "Tema de canal inválido".to_string())?;
    }
    if let Some(nsfw) = datos.nsfw {
        peticion = peticion.nsfw(nsfw);
    }
    if let Some(segundos) = datos.rate_limit_per_user {
        peticion = peticion
            .rate_limit_per_user(segundos)
            .map_err(|_| "Modo lento inválido".to_string())?;
    }
    let canal = peticion
        .await
        .map_err(traducir_error)?
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(convertir_canal(canal))
}

/// Edita un canal (`PATCH /channels/{id}`), incluido moverlo de categoría.
pub async fn update_channel(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    datos: &DatosCanal,
) -> Result<crate::state::Channel, String> {
    use twilight_model::channel::ChannelType;
    validar_datos_canal(datos)?;
    let cid = parse_id(channel_id, "canal")?;
    let client = build_client(token, base_url);
    let mut peticion = client.update_channel(cid);
    if let Some(nombre) = datos.name.as_deref() {
        peticion = peticion
            .name(nombre.trim())
            .map_err(|_| "Nombre de canal inválido".to_string())?;
    }
    if let Some(tipo) = datos.channel_type {
        peticion = peticion.kind(ChannelType::from(tipo));
    }
    if let Some(padre) = datos.parent_id.as_deref() {
        let padre = if padre.is_empty() {
            None
        } else {
            Some(parse_id(padre, "categoría")?)
        };
        peticion = peticion.parent_id(padre);
    }
    if let Some(tema) = datos.topic.as_deref() {
        peticion = peticion
            .topic(tema)
            .map_err(|_| "Tema de canal inválido".to_string())?;
    }
    if let Some(nsfw) = datos.nsfw {
        peticion = peticion.nsfw(nsfw);
    }
    if let Some(segundos) = datos.rate_limit_per_user {
        peticion = peticion
            .rate_limit_per_user(segundos)
            .map_err(|_| "Modo lento inválido".to_string())?;
    }
    let canal = peticion
        .await
        .map_err(traducir_error)?
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(convertir_canal(canal))
}

/// Borra un canal o categoría (`DELETE /channels/{id}`).
pub async fn delete_channel(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
) -> Result<(), String> {
    let cid = parse_id(channel_id, "canal")?;
    build_client(token, base_url)
        .delete_channel(cid)
        .await
        .map_err(traducir_error)?;
    Ok(())
}

fn parse_bits_permisos(
    texto: &str,
    que: &str,
) -> Result<twilight_model::guild::Permissions, String> {
    texto
        .trim()
        .parse::<u64>()
        .ok()
        .and_then(twilight_model::guild::Permissions::from_bits)
        .ok_or_else(|| format!("Los permisos {que} no son válidos"))
}

/// Crea o cambia un overwrite de canal (`PUT /channels/{id}/permissions/{target}`).
/// `es_miembro` distingue miembro (`true`) de rol (`false`); `allow` y `deny`
/// son bits como texto.
pub async fn set_channel_overwrite(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    target_id: &str,
    es_miembro: bool,
    allow: &str,
    deny: &str,
) -> Result<(), String> {
    use twilight_model::http::permission_overwrite::{
        PermissionOverwrite, PermissionOverwriteType,
    };
    let cid = parse_id(channel_id, "canal")?;
    let permitidos = parse_bits_permisos(allow, "permitidos")?;
    let denegados = parse_bits_permisos(deny, "denegados")?;
    if permitidos.intersects(denegados) {
        return Err("Un mismo permiso no puede estar permitido y denegado a la vez".to_string());
    }
    let overwrite = PermissionOverwrite {
        allow: Some(permitidos),
        deny: Some(denegados),
        id: parse_id(target_id, if es_miembro { "miembro" } else { "rol" })?,
        kind: if es_miembro {
            PermissionOverwriteType::Member
        } else {
            PermissionOverwriteType::Role
        },
    };
    build_client(token, base_url)
        .update_channel_permission(cid, &overwrite)
        .await
        .map_err(traducir_error)?;
    Ok(())
}

/// Quita un overwrite de canal (`DELETE /channels/{id}/permissions/{target}`).
pub async fn delete_channel_overwrite(
    token: &str,
    base_url: Option<&str>,
    channel_id: &str,
    target_id: &str,
    es_miembro: bool,
) -> Result<(), String> {
    let cid = parse_id(channel_id, "canal")?;
    let client = build_client(token, base_url);
    let peticion = client.delete_channel_permission(cid);
    if es_miembro {
        peticion
            .member(parse_id(target_id, "miembro")?)
            .await
            .map_err(traducir_error)?;
    } else {
        peticion
            .role(parse_id(target_id, "rol")?)
            .await
            .map_err(traducir_error)?;
    }
    Ok(())
}

/// Cambia las posiciones de varios canales
/// (`PATCH /guilds/{id}/channels`). Cada par es (id del canal, posición).
pub async fn reorder_guild_channels(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    posiciones: &[(String, u64)],
) -> Result<(), String> {
    if posiciones.is_empty() {
        return Err("No hay posiciones que cambiar".to_string());
    }
    let gid = parse_guild_id(guild_id)?;
    let pares = posiciones
        .iter()
        .map(|(id, pos)| {
            Ok(
                twilight_http::request::guild::update_guild_channel_positions::Position::from((
                    parse_id(id, "canal")?,
                    *pos,
                )),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    build_client(token, base_url)
        .update_guild_channel_positions(gid, &pares)
        .await
        .map_err(traducir_error)?;
    Ok(())
}

/// Campos editables de un rol. En `update_role` solo se envían los `Some`.
/// `permissions` es el texto de bits de Discord (p. ej. `"8"`).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DatosRol {
    pub name: Option<String>,
    pub color: Option<u32>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    pub permissions: Option<String>,
}

fn validar_datos_rol(datos: &DatosRol) -> Result<(), String> {
    if let Some(nombre) = &datos.name {
        let largo = nombre.trim().chars().count();
        if largo == 0 || largo > 100 {
            return Err("El nombre del rol debe tener entre 1 y 100 caracteres".to_string());
        }
    }
    if datos.color.is_some_and(|c| c > 0xFF_FFFF) {
        return Err("El color del rol no es válido".to_string());
    }
    parse_permisos(datos)?;
    Ok(())
}

fn parse_permisos(datos: &DatosRol) -> Result<Option<twilight_model::guild::Permissions>, String> {
    datos
        .permissions
        .as_deref()
        .map(|t| {
            t.trim()
                .parse::<u64>()
                .ok()
                .and_then(twilight_model::guild::Permissions::from_bits)
                .ok_or_else(|| "Los permisos del rol no son válidos".to_string())
        })
        .transpose()
}

/// Crea un rol (`POST /guilds/{id}/roles`).
pub async fn create_guild_role(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    datos: &DatosRol,
) -> Result<crate::state::Role, String> {
    validar_datos_rol(datos)?;
    let nombre = datos
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or("El rol necesita un nombre")?;
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    let mut peticion = client.create_role(gid).name(nombre);
    if let Some(color) = datos.color {
        peticion = peticion.color(color);
    }
    if let Some(separado) = datos.hoist {
        peticion = peticion.hoist(separado);
    }
    if let Some(mencionable) = datos.mentionable {
        peticion = peticion.mentionable(mencionable);
    }
    if let Some(permisos) = parse_permisos(datos)? {
        peticion = peticion.permissions(permisos);
    }
    let rol = peticion
        .await
        .map_err(traducir_error)?
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(convertir_rol(rol))
}

/// Edita un rol (`PATCH /guilds/{id}/roles/{role_id}`).
pub async fn update_guild_role(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    role_id: &str,
    datos: &DatosRol,
) -> Result<crate::state::Role, String> {
    validar_datos_rol(datos)?;
    let gid = parse_guild_id(guild_id)?;
    let rid = parse_id(role_id, "rol")?;
    let client = build_client(token, base_url);
    let mut peticion = client.update_role(gid, rid);
    if let Some(nombre) = datos.name.as_deref() {
        peticion = peticion.name(Some(nombre.trim()));
    }
    if let Some(color) = datos.color {
        peticion = peticion.color(Some(color));
    }
    if let Some(separado) = datos.hoist {
        peticion = peticion.hoist(separado);
    }
    if let Some(mencionable) = datos.mentionable {
        peticion = peticion.mentionable(mencionable);
    }
    if let Some(permisos) = parse_permisos(datos)? {
        peticion = peticion.permissions(permisos);
    }
    let rol = peticion
        .await
        .map_err(traducir_error)?
        .model()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    Ok(convertir_rol(rol))
}

/// Borra un rol (`DELETE /guilds/{id}/roles/{role_id}`).
pub async fn delete_guild_role(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    role_id: &str,
) -> Result<(), String> {
    let gid = parse_guild_id(guild_id)?;
    let rid = parse_id(role_id, "rol")?;
    build_client(token, base_url)
        .delete_role(gid, rid)
        .await
        .map_err(traducir_error)?;
    Ok(())
}

/// Cambia las posiciones de varios roles (`PATCH /guilds/{id}/roles`).
/// Cada par es (id del rol, posición).
pub async fn reorder_guild_roles(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    posiciones: &[(String, u64)],
) -> Result<(), String> {
    if posiciones.is_empty() {
        return Err("No hay posiciones que cambiar".to_string());
    }
    let gid = parse_guild_id(guild_id)?;
    // twilight 0.15 serializa estos pares como `["id", pos]`, pero Discord
    // espera objetos `{"id", "position"}`: se arma el cuerpo a mano.
    let cuerpo = posiciones
        .iter()
        .map(|(id, pos)| {
            parse_id::<twilight_model::id::marker::RoleMarker>(id, "rol")?;
            Ok(serde_json::json!({"id": id, "position": pos}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let peticion = twilight_http::request::Request::builder(
        &twilight_http::routing::Route::UpdateRolePositions {
            guild_id: gid.get(),
        },
    )
    .json(&cuerpo)
    .map_err(|e| format!("Petición inválida: {e}"))?
    .build();
    build_client(token, base_url)
        .request::<twilight_http::response::marker::EmptyBody>(peticion)
        .await
        .map_err(traducir_error)?;
    Ok(())
}

/// Permisos de Discord: (posición del bit, nombre en español).
const PERMISOS_DISCORD: &[(u32, &str)] = &[
    (0, "Crear invitaciones"),
    (1, "Expulsar miembros"),
    (2, "Banear miembros"),
    (3, "Administrador"),
    (4, "Gestionar canales"),
    (5, "Gestionar servidor"),
    (6, "Añadir reacciones"),
    (7, "Ver registro de auditoría"),
    (8, "Voz prioritaria"),
    (9, "Transmitir vídeo"),
    (10, "Ver canales"),
    (11, "Enviar mensajes"),
    (12, "Enviar mensajes de texto a voz"),
    (13, "Gestionar mensajes"),
    (14, "Insertar enlaces"),
    (15, "Adjuntar archivos"),
    (16, "Leer el historial de mensajes"),
    (17, "Mencionar a @everyone"),
    (18, "Usar emojis externos"),
    (19, "Ver estadísticas del servidor"),
    (20, "Conectar a voz"),
    (21, "Hablar en voz"),
    (22, "Silenciar miembros en voz"),
    (23, "Ensordecer miembros"),
    (24, "Mover miembros de voz"),
    (25, "Usar detección de voz"),
    (26, "Cambiar apodo"),
    (27, "Gestionar apodos"),
    (28, "Gestionar roles"),
    (29, "Gestionar webhooks"),
    (30, "Gestionar expresiones"),
    (31, "Usar comandos de aplicación"),
    (32, "Pedir hablar"),
    (33, "Gestionar eventos"),
    (34, "Gestionar hilos"),
    (35, "Crear hilos públicos"),
    (36, "Crear hilos privados"),
    (37, "Usar stickers externos"),
    (38, "Enviar mensajes en hilos"),
    (39, "Usar actividades"),
    (40, "Moderar miembros"),
    (41, "Ver analíticas de monetización"),
    (42, "Usar el panel de sonidos"),
    (43, "Crear expresiones"),
    (44, "Crear eventos"),
    (45, "Usar sonidos externos"),
    (46, "Enviar mensajes de voz"),
    (49, "Crear encuestas"),
    (50, "Usar aplicaciones externas"),
];

const BIT_ADMINISTRADOR: u64 = 1 << 3;

/// Todos los permisos que ARGUS conoce.
fn todos_los_permisos() -> u64 {
    PERMISOS_DISCORD
        .iter()
        .fold(0, |acc, (bit, _)| acc | (1 << bit))
}

/// Permisos efectivos de un miembro en un servidor (sin overwrites de canal):
/// los del rol @everyone (su id es el del servidor) más los de sus roles.
/// El dueño o quien tenga Administrator lo tiene todo.
pub fn calcular_permisos(
    guild_id: &str,
    owner_id: Option<&str>,
    user_id: &str,
    roles_servidor: &[crate::state::Role],
    roles_miembro: &[String],
) -> u64 {
    if owner_id == Some(user_id) {
        return todos_los_permisos();
    }
    let bits = roles_servidor
        .iter()
        .filter(|r| r.id == guild_id || roles_miembro.contains(&r.id))
        .filter_map(|r| r.permissions.parse::<u64>().ok())
        .fold(0, |acc, p| acc | p);
    if bits & BIT_ADMINISTRADOR != 0 {
        todos_los_permisos()
    } else {
        bits
    }
}

/// Nombres en español de los permisos activos en `bits`.
pub fn nombres_de_permisos(bits: u64) -> Vec<String> {
    PERMISOS_DISCORD
        .iter()
        .filter(|(bit, _)| bits & (1 << bit) != 0)
        .map(|(_, nombre)| nombre.to_string())
        .collect()
}

/// Datos del servidor con recuento aproximado de miembros
/// (`GET /guilds/{id}?with_counts=true`).
pub async fn fetch_guild_info(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
) -> Result<crate::state::Guild, String> {
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    let cuerpo = client
        .guild(gid)
        .with_counts(true)
        .await
        .map_err(traducir_error)?
        .bytes()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    let json: serde_json::Value = serde_json::from_slice(&cuerpo)
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    let texto = |k: &str| json.get(k).and_then(|v| v.as_str()).map(str::to_string);
    Ok(crate::state::Guild {
        id: texto("id").unwrap_or_else(|| guild_id.to_string()),
        name: texto("name").unwrap_or_default(),
        icon: texto("icon"),
        member_count: json
            .get("approximate_member_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as _,
        joined_at: None,
    })
}

/// Permisos del bot en un servidor: `GET /guilds/{id}` (dueño),
/// `GET /guilds/{id}/roles` y `GET /guilds/{id}/members/{bot_id}`.
pub async fn fetch_bot_permissions(
    token: &str,
    base_url: Option<&str>,
    guild_id: &str,
    bot_id: &str,
) -> Result<Vec<String>, String> {
    let gid = parse_guild_id(guild_id)?;
    let client = build_client(token, base_url);
    // Solo hace falta `owner_id`: no se deserializa el servidor completo.
    let cuerpo = client
        .guild(gid)
        .await
        .map_err(traducir_error)?
        .bytes()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))?;
    let owner_id = serde_json::from_slice::<serde_json::Value>(&cuerpo)
        .ok()
        .and_then(|v| v.get("owner_id")?.as_str().map(str::to_string));
    let roles = fetch_guild_roles(token, base_url, guild_id).await?;
    let miembro = fetch_guild_member(token, base_url, guild_id, bot_id).await?;
    let bits = calcular_permisos(
        guild_id,
        owner_id.as_deref(),
        bot_id,
        &roles,
        &miembro.roles,
    );
    Ok(nombres_de_permisos(bits))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};

    fn rol(id: &str, permisos: u64) -> crate::state::Role {
        crate::state::Role {
            id: id.to_string(),
            name: format!("rol-{id}"),
            color: 0,
            position: 0,
            permissions: permisos.to_string(),
            permissions_bitfield: Some(permisos.to_string()),
            member_count: 0,
            mentionable: false,
            hoist: false,
        }
    }

    #[test]
    fn permisos_sin_roles_son_los_de_everyone() {
        let roles = vec![rol("555", (1 << 10) | (1 << 11)), rol("600", 1 << 28)];
        let bits = calcular_permisos("555", Some("1"), "2", &roles, &[]);
        assert_eq!(
            nombres_de_permisos(bits),
            vec!["Ver canales", "Enviar mensajes"]
        );
        assert_eq!(calcular_permisos("555", None, "2", &[], &[]), 0);
    }

    #[test]
    fn permisos_con_rol_que_concede() {
        let roles = vec![rol("555", 1 << 10), rol("600", 1 << 28), rol("601", 1 << 2)];
        let bits = calcular_permisos("555", Some("1"), "2", &roles, &["600".to_string()]);
        assert_eq!(
            nombres_de_permisos(bits),
            vec!["Ver canales", "Gestionar roles"]
        );
    }

    #[test]
    fn administrator_y_dueno_lo_tienen_todo() {
        let roles = vec![rol("555", 1 << 10), rol("600", BIT_ADMINISTRADOR)];
        let admin = calcular_permisos("555", Some("1"), "2", &roles, &["600".to_string()]);
        assert_eq!(admin, todos_los_permisos());
        assert_eq!(nombres_de_permisos(admin).len(), PERMISOS_DISCORD.len());
        let dueno = calcular_permisos("555", Some("2"), "2", &roles, &[]);
        assert_eq!(dueno, todos_los_permisos());
    }
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn obtiene_usuario_actual_desde_discord_falso() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me"))
            .and(header("authorization", "Bot token-falso"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "123456789",
                "username": "bot-de-prueba",
                "discriminator": "0",
                "avatar": null,
                "bot": true,
                "mfa_enabled": false
            })))
            .expect(1)
            .mount(&server)
            .await;

        let addr = server.address().to_string();
        let client = build_client("token-falso", Some(&addr));
        let user = client
            .current_user()
            .await
            .expect("petición fallida")
            .model()
            .await
            .expect("respuesta inválida");

        assert_eq!(user.name, "bot-de-prueba");
        assert_eq!(user.id.get(), 123456789);
    }

    #[tokio::test]
    async fn obtiene_aplicacion_desde_discord_falso() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/oauth2/applications/@me"))
            .and(header("authorization", "Bot token-falso"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "987654321",
                "name": "app-de-prueba",
                "description": "",
                "bot_public": true,
                "bot_require_code_grant": false,
                "summary": "",
                "rpc_origins": [],
                "verify_key": "abc",
                "icon": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let addr = server.address().to_string();
        let app = fetch_current_application("token-falso", Some(&addr))
            .await
            .expect("aplicación");
        assert_eq!(app.id.get(), 987654321);
    }

    #[test]
    fn traduce_cada_codigo_de_error() {
        assert_eq!(traducir_respuesta(401, b"{}"), "Token inválido");
        assert_eq!(
            traducir_respuesta(
                403,
                br#"{"code":50013,"message":"Missing Permissions: MANAGE_ROLES"}"#
            ),
            "Al bot le falta el permiso «Gestionar roles»"
        );
        assert!(
            traducir_respuesta(403, br#"{"message":"Missing Permissions"}"#)
                .contains("le falta un permiso")
        );
        assert!(traducir_respuesta(404, b"").starts_with("No encontrado"));
        assert!(traducir_respuesta(429, br#"{"retry_after":1.5}"#).contains("en 2 s"));
        assert!(traducir_respuesta(429, b"").contains("limitó las peticiones"));
        assert!(traducir_respuesta(500, b"").contains("(500)"));
    }

    async fn error_de_estado(status: u16, cuerpo: serde_json::Value) -> String {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me"))
            .respond_with(ResponseTemplate::new(status).set_body_json(cuerpo))
            .mount(&server)
            .await;
        let addr = server.address().to_string();
        fetch_current_user("token-falso", Some(&addr))
            .await
            .expect_err("debía fallar")
    }

    #[tokio::test]
    async fn errores_http_del_discord_falso_salen_en_espanol() {
        assert_eq!(
            error_de_estado(
                401,
                serde_json::json!({"code":0,"message":"401: Unauthorized"})
            )
            .await,
            "Token inválido"
        );
        assert_eq!(
            error_de_estado(
                403,
                serde_json::json!({"code":50013,"message":"Missing Permissions: BAN_MEMBERS"})
            )
            .await,
            "Al bot le falta el permiso «Banear miembros»"
        );
        assert!(error_de_estado(
            404,
            serde_json::json!({"code":10004,"message":"Unknown Guild"})
        )
        .await
        .starts_with("No encontrado"));
        let limite = error_de_estado(
            429,
            serde_json::json!({"message":"You are being rate limited.","retry_after":3.0,"global":false}),
        )
        .await;
        assert!(limite.contains("limitó las peticiones"), "{limite}");
    }

    #[test]
    fn enlace_de_invitacion_tiene_cliente_scope_y_permisos() {
        use twilight_model::guild::Permissions;
        let esperado = Permissions::VIEW_CHANNEL
            | Permissions::SEND_MESSAGES
            | Permissions::READ_MESSAGE_HISTORY
            | Permissions::MANAGE_CHANNELS
            | Permissions::MANAGE_ROLES
            | Permissions::MODERATE_MEMBERS
            | Permissions::KICK_MEMBERS
            | Permissions::BAN_MEMBERS;
        assert_eq!(INVITE_PERMISSIONS, esperado.bits());
        assert_eq!(
            build_invite_url("42"),
            format!(
                "https://discord.com/oauth2/authorize?client_id=42&scope=bot%20applications.commands&permissions={}",
                esperado.bits()
            )
        );
    }
}
