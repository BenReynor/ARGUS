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
    let response = client.current_user().await.map_err(|e| match e.kind() {
        ErrorType::Response { status, .. } if status.get() == 401 => "Token inválido".to_string(),
        ErrorType::Unauthorized => "Token inválido".to_string(),
        _ => format!("No se pudo contactar con Discord: {e}"),
    })?;
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
        .map_err(|e| match e.kind() {
            ErrorType::Response { status, .. } if status.get() == 401 => {
                "Token inválido".to_string()
            }
            ErrorType::Unauthorized => "Token inválido".to_string(),
            _ => format!("No se pudo contactar con Discord: {e}"),
        })?;
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
    let response = client
        .current_user_guilds()
        .await
        .map_err(|e| match e.kind() {
            ErrorType::Response { status, .. } if status.get() == 401 => {
                "Token inválido".to_string()
            }
            ErrorType::Unauthorized => "Token inválido".to_string(),
            _ => format!("No se pudo contactar con Discord: {e}"),
        })?;
    response
        .models()
        .await
        .map_err(|e| format!("Respuesta de Discord inválida: {e}"))
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
