//! Cliente de Discord (REST) sobre twilight-http con URL base configurable.

use twilight_http::client::ClientBuilder;
use twilight_http::error::ErrorType;
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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
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
