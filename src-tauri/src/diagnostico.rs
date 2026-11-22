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

// These commands inspect local CLI status only; none invokes a model or starts login.
async fn process_check(
    path: &std::path::Path,
    args: &[&str],
    input: Option<&str>,
    cancel: &mut tokio::sync::broadcast::Receiver<()>,
    timeout: Duration,
) -> Result<std::process::Output, &'static str> {
    use tokio::io::AsyncWriteExt;
    let mut command = tokio::process::Command::new(path);
    command
        .args(args)
        .stdin(if input.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| "failed")?;
    let work = async {
        if let Some(input) = input {
            let mut stdin = child.stdin.take().ok_or("failed")?;
            stdin
                .write_all(input.as_bytes())
                .await
                .map_err(|_| "failed")?;
            drop(stdin);
        }
        child.wait_with_output().await.map_err(|_| "failed")
    };
    tokio::select! {
        biased;
        _ = cancel.recv() => Err("cancelled"),
        result = tokio::time::timeout(timeout, work) => result.unwrap_or(Err("timeout")),
    }
}

// The MCP handshake and ping share one process and one bounded lifetime.
async fn mcp_checks(
    path: &std::path::Path,
    cancel: &mut tokio::sync::broadcast::Receiver<()>,
    timeout: Duration,
) -> (Result<bool, &'static str>, Result<bool, &'static str>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut child = match tokio::process::Command::new(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return (Err("failed"), Err("failed")),
    };
    let mut session = false;
    let work = async {
        let mut stdin = child.stdin.take().ok_or("failed")?;
        let mut lines = BufReader::new(child.stdout.take().ok_or("failed")?).lines();
        stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"argus-diagnostic\",\"version\":\"1\"}}}\n").await.map_err(|_| "failed")?;
        while let Some(line) = lines.next_line().await.map_err(|_| "failed")? {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if value["id"] == 1 {
                    if !value["result"].is_object() {
                        return Ok(false);
                    }
                    session = true;
                    break;
                }
            }
        }
        if !session {
            return Ok(false);
        }
        stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n").await.map_err(|_| "failed")?;
        while let Some(line) = lines.next_line().await.map_err(|_| "failed")? {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if value["id"] == 2 {
                    return Ok(value["result"].is_object());
                }
            }
        }
        Ok(false)
    };
    let response = tokio::select! {
        biased;
        _ = cancel.recv() => Err("cancelled"),
        result = tokio::time::timeout(timeout, work) => result.unwrap_or(Err("timeout")),
    };
    let session_result = if session {
        Ok(true)
    } else {
        match response {
            Err(reason) => Err(reason),
            Ok(_) => Ok(false),
        }
    };
    (session_result, response)
}

async fn agent_checks(
    name: &'static str,
    path: Option<std::path::PathBuf>,
    language: &str,
    cancel: &mut tokio::sync::broadcast::Receiver<()>,
    timeout: Duration,
) -> Vec<Check> {
    let names = match name {
        "claude" => ["claude_binary", "claude_session", "claude_response"],
        "codex" => ["codex_binary", "codex_session", "codex_response"],
        "opencode" => ["opencode_binary", "opencode_session", "opencode_response"],
        _ => ["mcp_binary", "mcp_session", "mcp_response"],
    };
    let Some(path) = path else {
        return vec![
            check(
                names[0],
                Status::Failed,
                "Binary not found. Install or configure it.",
                language,
            ),
            check(
                names[1],
                Status::NotChecked,
                "Session was not checked.",
                language,
            ),
            check(
                names[2],
                Status::NotChecked,
                "Response was not checked.",
                language,
            ),
        ];
    };
    let binary = check(names[0], Status::Correct, "Binary found.", language);
    if name == "mcp" {
        let (session, response) = mcp_checks(&path, cancel, timeout).await;
        let result = |name, outcome| match outcome {
            Ok(true) => check(
                name,
                Status::Correct,
                if name == names[1] {
                    "Session available."
                } else {
                    "Process responded."
                },
                language,
            ),
            Ok(false) => check(
                name,
                Status::Failed,
                if name == names[1] {
                    "MCP session unavailable. Check its initialization."
                } else {
                    "Process did not respond. Check its installation."
                },
                language,
            ),
            Err(reason) => process_failure(name, reason, language),
        };
        return vec![
            binary,
            result(names[1], session),
            result(names[2], response),
        ];
    }
    let session = {
        let args: &[&str] = match name {
            "claude" => &["auth", "status", "--json"],
            "codex" => &["login", "status"],
            _ => &["auth", "list"],
        };
        match process_check(&path, args, None, cancel, timeout).await {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                let available = output.status.success()
                    && match name {
                        "claude" => serde_json::from_slice::<serde_json::Value>(&output.stdout)
                            .ok()
                            .and_then(|v| v["loggedIn"].as_bool())
                            .unwrap_or(false),
                        "opencode" => {
                            !text.to_lowercase().contains("0 credentials")
                                && !text.trim().is_empty()
                        }
                        _ => true,
                    };
                check(
                    names[1],
                    if available {
                        Status::Correct
                    } else {
                        Status::Warning
                    },
                    if available {
                        "Session available."
                    } else {
                        "No session available. Sign in separately if needed."
                    },
                    language,
                )
            }
            Err(reason) => process_failure(names[1], reason, language),
        }
    };
    if session.status == Status::NotChecked {
        return vec![
            binary,
            session,
            check(
                names[2],
                Status::NotChecked,
                "Process check cancelled.",
                language,
            ),
        ];
    }
    let response = process_check(&path, &["--version"], None, cancel, timeout)
        .await
        .map(|o| o.status.success() && !o.stdout.is_empty());
    let response = match response {
        Ok(true) => check(names[2], Status::Correct, "Process responded.", language),
        Ok(false) => check(
            names[2],
            Status::Failed,
            "Process did not respond. Check its installation.",
            language,
        ),
        Err(reason) => process_failure(names[2], reason, language),
    };
    vec![binary, session, response]
}

fn process_failure(name: &'static str, reason: &str, language: &str) -> Check {
    match reason {
        "cancelled" => check(
            name,
            Status::NotChecked,
            "Process check cancelled.",
            language,
        ),
        "timeout" => check(
            name,
            Status::Failed,
            "Process check timed out. Retry or check the installation.",
            language,
        ),
        _ => check(
            name,
            Status::Failed,
            "Process could not be started. Check its installation.",
            language,
        ),
    }
}

#[tauri::command]
pub async fn diagnostico_agentes(state: State<'_, AppState>) -> Result<Vec<Check>, String> {
    let language = state
        .config
        .lock()
        .map(|c| c.ui_language.clone())
        .unwrap_or_else(|_| "en".into());
    let dirs = crate::asistente::carpetas_busqueda();
    let mut cancel = state.diagnostic_cancel.subscribe();
    let mut checks = Vec::new();
    for name in ["claude", "codex", "opencode", "mcp"] {
        let path = if name == "mcp" {
            crate::commands::ruta_del_mcp()
                .ok()
                .map(std::path::PathBuf::from)
                .filter(|p| p.is_file())
        } else {
            crate::asistente::buscar_binario(name, &dirs)
        };
        let result = agent_checks(name, path, &language, &mut cancel, Duration::from_secs(5)).await;
        let cancelled = result
            .iter()
            .any(|c| c.explanation == i18n::tr(&language, "Process check cancelled."));
        checks.extend(result);
        if cancelled {
            break;
        }
    }
    Ok(checks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[cfg(unix)]
    #[tokio::test]
    async fn fake_agents_missing_session_binary_hang_and_cancel() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("argus-diagnostic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent");
        std::fs::write(&path, "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 1; else echo '{\"loggedIn\":false}'; fi\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (tx, mut rx) = tokio::sync::broadcast::channel(4);
        let checks = agent_checks(
            "claude",
            Some(path.clone()),
            "en",
            &mut rx,
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(
            [&checks[0].status, &checks[1].status, &checks[2].status],
            [&Status::Correct, &Status::Warning, &Status::Correct]
        );
        let checks = agent_checks("codex", None, "en", &mut rx, Duration::from_secs(1)).await;
        assert_eq!(
            [&checks[0].status, &checks[1].status, &checks[2].status],
            [&Status::Failed, &Status::NotChecked, &Status::NotChecked]
        );
        std::fs::write(&path, "#!/bin/sh\nexec sleep 10\n").unwrap();
        let checks = agent_checks(
            "opencode",
            Some(path.clone()),
            "en",
            &mut rx,
            Duration::from_millis(30),
        )
        .await;
        assert!(checks[1].explanation.contains("timed out"));
        assert!(checks[2].explanation.contains("timed out"));
        tx.send(()).unwrap();
        let checks = agent_checks("codex", Some(path), "en", &mut rx, Duration::from_secs(2)).await;
        assert_eq!(checks[1].status, Status::NotChecked);
        assert_eq!(checks[2].status, Status::NotChecked);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fake_mcp_distinguishes_session_response_and_timeout() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("argus-mcp-diagnostic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp");
        let write_fake = |script: &str| {
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let (_tx, mut rx) = tokio::sync::broadcast::channel(4);
        write_fake(
            "#!/bin/sh
read line
echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}'
read line
read line
echo '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}'
",
        );
        let checks = agent_checks(
            "mcp",
            Some(path.clone()),
            "en",
            &mut rx,
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(
            [&checks[0].status, &checks[1].status, &checks[2].status],
            [&Status::Correct, &Status::Correct, &Status::Correct]
        );
        write_fake(
            "#!/bin/sh
read line
echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{}}'
",
        );
        let checks = agent_checks(
            "mcp",
            Some(path.clone()),
            "en",
            &mut rx,
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(checks[1].status, Status::Failed);
        assert_eq!(checks[2].status, Status::Failed);
        write_fake(
            "#!/bin/sh
read line
echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}'
exec sleep 10
",
        );
        let checks =
            agent_checks("mcp", Some(path), "en", &mut rx, Duration::from_millis(40)).await;
        assert_eq!(checks[1].status, Status::Correct);
        assert!(checks[2].explanation.contains("timed out"));
        std::fs::remove_dir_all(dir).unwrap();
    }

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
            Duration::from_secs(20),
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
