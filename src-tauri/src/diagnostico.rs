//! Diagnóstico local de solo lectura; nunca incluye valores ni errores externos.
use crate::{
    crypto, db, i18n,
    state::{AppState, Config},
};
use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Correct,
    Warning,
    Failed,
    NotChecked,
}

#[derive(Debug, Serialize)]
pub struct Check {
    name: &'static str,
    status: Status,
    explanation: String,
    checked_at: String,
}

fn check(name: &'static str, status: Status, explanation: &str, language: &str) -> Check {
    Check {
        name,
        status,
        explanation: i18n::tr(language, explanation),
        checked_at: Utc::now().to_rfc3339(),
    }
}

async fn run(pool: Option<&SqlitePool>, keyring_available: bool, language: &str) -> Vec<Check> {
    let accessible = match pool {
        Some(pool) => matches!(
            tokio::time::timeout(
                Duration::from_secs(3),
                sqlx::query("SELECT 1").execute(pool)
            )
            .await,
            Ok(Ok(_))
        ),
        None => false,
    };
    let database = if accessible {
        check(
            "database",
            Status::Correct,
            "Database is accessible.",
            language,
        )
    } else {
        check(
            "database",
            Status::Failed,
            "Database is unavailable. Check access to the application database and restart ARGUS.",
            language,
        )
    };
    let config = if accessible {
        match tokio::time::timeout(
            Duration::from_secs(3),
            db::get_config(pool.unwrap(), "config"),
        )
        .await
        {
            Ok(Ok(Some(value))) => match serde_json::from_str::<Config>(&value) {
                Ok(config) if config.validate_ai_time_limits().is_ok() => check(
                    "configuration",
                    Status::Correct,
                    "Saved configuration is valid.",
                    language,
                ),
                _ => check(
                    "configuration",
                    Status::Failed,
                    "Saved configuration is invalid. Review and save the application settings.",
                    language,
                ),
            },
            Ok(Ok(None)) => check(
                "configuration",
                Status::Warning,
                "No saved configuration exists; defaults are in use.",
                language,
            ),
            _ => check(
                "configuration",
                Status::Failed,
                "Configuration could not be read. Check the application database.",
                language,
            ),
        }
    } else {
        check(
            "configuration",
            Status::NotChecked,
            "Configuration could not be checked because the database is unavailable.",
            language,
        )
    };
    let keyring = if keyring_available {
        check(
            "keyring",
            Status::Correct,
            "The system keyring was available when encryption was initialized.",
            language,
        )
    } else {
        check("keyring", Status::Warning, "The system keyring was unavailable when encryption was initialized. Unlock it and restart ARGUS.", language)
    };
    vec![config, database, keyring]
}

#[tauri::command]
pub async fn diagnostico_local(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Check>, String> {
    let language = state
        .config
        .lock()
        .map(|config| config.ui_language.clone())
        .unwrap_or_else(|_| "en".into());
    let pool = app
        .try_state::<SqlitePool>()
        .map(|pool| pool.inner().clone());
    Ok(run(pool.as_ref(), crypto::keyring_available(), &language).await)
}

// The timeout covers all REST calls together; dropping the future cancels them.
async fn discord_checks(
    token: &str,
    base_url: Option<&str>,
    bot: &str,
    guild: &str,
    gateway_up: bool,
    language: &str,
    mut cancel: tokio::sync::broadcast::Receiver<()>,
    timeout: Duration,
) -> Vec<Check> {
    let gateway = if gateway_up {
        check(
            "gateway",
            Status::Correct,
            "The active bot Gateway is connected.",
            language,
        )
    } else {
        check(
            "gateway",
            Status::Failed,
            "The active bot Gateway is disconnected. Start or reconnect the bot.",
            language,
        )
    };
    let permissions = tokio::select! {
        biased;
        _ = cancel.recv() => check("build_permissions", Status::NotChecked, "Permission check cancelled.", language),
        result = tokio::time::timeout(timeout, crate::discord::fetch_bot_permissions(token, base_url, guild, bot)) => {
            match result {
                Ok(Ok(permissions)) => {
                    let missing: Vec<&str> = [("Ver canales", "View Channels"), ("Gestionar canales", "Manage Channels"), ("Gestionar roles", "Manage Roles")]
                        .into_iter().filter_map(|(internal, display)| (!permissions.iter().any(|p| p == internal)).then_some(display)).collect();
                    if missing.is_empty() {
                        check("build_permissions", Status::Correct, "The bot has the server permissions required to build.", language)
                    } else {
                        let names = missing.into_iter().map(|name| i18n::tr(language, name)).collect::<Vec<_>>().join(", ");
                        let mut c = check("build_permissions", Status::Warning, "Missing building permissions. Grant the bot:", language);
                        c.explanation.push_str(&format!(" {names}"));
                        c
                    }
                }
                Ok(Err(_)) => check("build_permissions", Status::Failed, "Could not read bot permissions. Check the network and server access.", language),
                Err(_) => check("build_permissions", Status::Failed, "Permission check timed out. Check the network and retry.", language),
            }
        }
    };
    vec![gateway, permissions]
}

#[tauri::command]
pub async fn diagnostico_discord(state: State<'_, AppState>) -> Result<Vec<Check>, String> {
    let (bot, guild, language) = {
        let config = state
            .config
            .lock()
            .map_err(|_| "Configuration unavailable")?;
        let bot = config
            .active_bot_id
            .as_ref()
            .and_then(|id| config.bots.iter().find(|b| &b.id == id))
            .cloned();
        (
            bot,
            config.active_guild_id.clone(),
            config.ui_language.clone(),
        )
    };
    let bot = bot.ok_or_else(|| i18n::tr(&language, "No bot selected"))?;
    let guild = guild.ok_or_else(|| i18n::tr(&language, "No server selected"))?;
    let gateway_up = state
        .gateway_health
        .lock()
        .map(|health| health.as_ref().is_some_and(|(id, up)| id == &bot.id && *up))
        .unwrap_or(false)
        && state
            .gateway_task
            .lock()
            .map(|task| task.as_ref().is_some_and(|task| !task.is_finished()))
            .unwrap_or(false);
    let cancel = state.diagnostic_cancel.subscribe();
    let token = match crypto::decrypt(&bot.token_ciphertext) {
        Ok(token) => token,
        Err(_) => {
            return Ok(vec![
                check(
                    "gateway",
                    if gateway_up {
                        Status::Correct
                    } else {
                        Status::Failed
                    },
                    if gateway_up {
                        "The active bot Gateway is connected."
                    } else {
                        "The active bot Gateway is disconnected. Start or reconnect the bot."
                    },
                    &language,
                ),
                check(
                    "build_permissions",
                    Status::Failed,
                    "Could not access the bot credential. Check the keyring.",
                    &language,
                ),
            ])
        }
    };
    Ok(discord_checks(
        &token,
        None,
        &bot.id,
        &guild,
        gateway_up,
        &language,
        cancel,
        Duration::from_secs(10),
    )
    .await)
}

#[tauri::command]
pub fn cancelar_diagnostico_discord(state: State<'_, AppState>) {
    let _ = state.diagnostic_cancel.send(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn discord_fake_gateway_down_and_missing_permission_only_reads() {
        let server = MockServer::start().await;
        for (route, body) in [
            ("/api/v10/guilds/555", serde_json::json!({"owner_id":"9"})),
            (
                "/api/v10/guilds/555/roles",
                serde_json::json!([{"id":"555","name":"everyone","color":0,"hoist":false,"flags":0,"position":0,"permissions":((1u64<<10)|(1<<4)).to_string(),"managed":false,"mentionable":false}]),
            ),
            (
                "/api/v10/guilds/555/members/123",
                serde_json::json!({"user":{"id":"123","username":"bot","discriminator":"0","avatar":null,"bot":true},"nick":null,"roles":[],"joined_at":"2024-01-02T03:04:05.000000+00:00","deaf":false,"mute":false,"flags":0}),
            ),
        ] {
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .expect(1)
                .mount(&server)
                .await;
        }
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        let checks = discord_checks(
            "fake",
            Some(&server.address().to_string()),
            "123",
            "555",
            false,
            "en",
            rx,
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(checks[0].status, Status::Failed);
        assert_eq!(checks[1].status, Status::Warning);
        assert!(checks[1].explanation.contains("Manage Roles"));
        assert!(server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method.as_str() == "GET"));
        drop(tx);
    }

    #[tokio::test]
    async fn discord_offline_timeout_and_cancellation() {
        let server = MockServer::start().await;
        let addr = server.address().to_string();
        drop(server);
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        let checks = discord_checks(
            "fake",
            Some(&addr),
            "123",
            "555",
            false,
            "en",
            rx,
            Duration::from_millis(200),
        )
        .await;
        assert_eq!(checks[1].status, Status::Failed);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
            .mount(&server)
            .await;
        let addr = server.address().to_string();
        let checks = discord_checks(
            "fake",
            Some(&addr),
            "123",
            "555",
            false,
            "en",
            tx.subscribe(),
            Duration::from_millis(30),
        )
        .await;
        assert!(checks[1].explanation.contains("timed out"));
        let rx = tx.subscribe();
        tx.send(()).unwrap();
        let checks = discord_checks(
            "fake",
            Some(&addr),
            "123",
            "555",
            false,
            "en",
            rx,
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(checks[1].status, Status::NotChecked);
    }
    #[tokio::test]
    async fn inaccessible_database_and_missing_keyring_are_reported_without_secrets() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.close().await;
        let checks = run(Some(&pool), false, "en").await;
        assert_eq!(checks[0].status, Status::NotChecked);
        assert_eq!(checks[1].status, Status::Failed);
        assert_eq!(checks[2].status, Status::Warning);
        assert!(checks.iter().all(|c| !c.explanation.is_empty()
            && chrono::DateTime::parse_from_rfc3339(&c.checked_at).is_ok()));
    }
    #[tokio::test]
    async fn configuration_is_checked_without_exposing_its_contents() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE config (key TEXT, value TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO config VALUES ('config', ?)")
            .bind("secret-password-token")
            .execute(&pool)
            .await
            .unwrap();
        let checks = run(Some(&pool), true, "es").await;
        assert_eq!(checks[0].status, Status::Failed);
        assert_eq!(checks[1].status, Status::Correct);
        assert_eq!(checks[2].status, Status::Correct);
        assert!(!serde_json::to_string(&checks)
            .unwrap()
            .contains("secret-password-token"));
        assert!(checks[0].explanation.contains("configuración"));
    }
}
