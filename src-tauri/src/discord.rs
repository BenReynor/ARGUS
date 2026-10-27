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
}
