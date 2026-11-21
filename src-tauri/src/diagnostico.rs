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

#[cfg(test)]
mod tests {
    use super::*;
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
