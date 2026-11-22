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

#[derive(Debug, Serialize)]
pub struct Check {
    name: &'static str,
    status: &'static str,
    explanation: String,
    checked_at: String,
}

fn check(name: &'static str, status: &'static str, explanation: &str, language: &str) -> Check {
    Check {
        name,
        status,
        explanation: i18n::tr(language, explanation),
        checked_at: Utc::now().to_rfc3339(),
    }
}

async fn run(
    pool: Option<&SqlitePool>,
    keyring_available: bool,
    language: &str,
    only: Option<&str>,
) -> Vec<Check> {
    if only == Some("keyring") {
        return vec![keyring_check(keyring_available, language)];
    }
    let accessible = if only == Some("configuration") {
        pool.is_some()
    } else {
        match pool {
            Some(pool) => matches!(
                tokio::time::timeout(
                    Duration::from_secs(3),
                    sqlx::query("SELECT 1").execute(pool)
                )
                .await,
                Ok(Ok(_))
            ),
            None => false,
        }
    };
    let database = if accessible {
        check(
            "database",
            crate::operaciones::CORRECT,
            "Database is accessible.",
            language,
        )
    } else {
        check(
            "database",
            crate::operaciones::FAILED,
            "Database is unavailable. Check access to the application database and restart ARGUS.",
            language,
        )
    };
    if only == Some("database") {
        return vec![database];
    }
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
                    crate::operaciones::CORRECT,
                    "Saved configuration is valid.",
                    language,
                ),
                _ => check(
                    "configuration",
                    crate::operaciones::FAILED,
                    "Saved configuration is invalid. Review and save the application settings.",
                    language,
                ),
            },
            Ok(Ok(None)) => check(
                "configuration",
                crate::operaciones::PARTIAL,
                "No saved configuration exists; defaults are in use.",
                language,
            ),
            _ => check(
                "configuration",
                crate::operaciones::FAILED,
                "Configuration could not be read. Check the application database.",
                language,
            ),
        }
    } else {
        check(
            "configuration",
            crate::operaciones::UNVERIFIED,
            "Configuration could not be checked because the database is unavailable.",
            language,
        )
    };
    if only == Some("configuration") {
        return vec![config];
    }
    vec![config, database, keyring_check(keyring_available, language)]
}

fn keyring_check(keyring_available: bool, language: &str) -> Check {
    if keyring_available {
        check(
            "keyring",
            crate::operaciones::CORRECT,
            "The system keyring was available when encryption was initialized.",
            language,
        )
    } else {
        check("keyring", crate::operaciones::PARTIAL, "The system keyring was unavailable when encryption was initialized. Unlock it and restart ARGUS.", language)
    }
}

#[tauri::command]
pub async fn diagnostico_local(
    app: AppHandle,
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<Vec<Check>, String> {
    let language = state
        .config
        .lock()
        .map(|config| config.ui_language.clone())
        .unwrap_or_else(|_| "en".into());
    let pool = app
        .try_state::<SqlitePool>()
        .map(|pool| pool.inner().clone());
    Ok(run(
        pool.as_ref(),
        crypto::keyring_available(),
        &language,
        name.as_deref(),
    )
    .await)
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
    only: Option<&str>,
) -> Vec<Check> {
    let gateway = if gateway_up {
        check(
            "gateway",
            crate::operaciones::CORRECT,
            "The active bot Gateway is connected.",
            language,
        )
    } else {
        check(
            "gateway",
            crate::operaciones::FAILED,
            "The active bot Gateway is disconnected. Start or reconnect the bot.",
            language,
        )
    };
    if only == Some("gateway") {
        return vec![gateway];
    }
    let permissions = tokio::select! {
        biased;
        _ = cancel.recv() => check("build_permissions", crate::operaciones::UNVERIFIED, "Permission check cancelled.", language),
        result = tokio::time::timeout(timeout, crate::discord::fetch_bot_permissions(token, base_url, guild, bot)) => {
            match result {
                Ok(Ok(permissions)) => {
                    let missing: Vec<&str> = [("Ver canales", "View Channels"), ("Gestionar canales", "Manage Channels"), ("Gestionar roles", "Manage Roles")]
                        .into_iter().filter_map(|(internal, display)| (!permissions.iter().any(|p| p == internal)).then_some(display)).collect();
                    if missing.is_empty() {
                        check("build_permissions", crate::operaciones::CORRECT, "The bot has the server permissions required to build.", language)
                    } else {
                        let names = missing.into_iter().map(|name| i18n::tr(language, name)).collect::<Vec<_>>().join(", ");
                        let mut c = check("build_permissions", crate::operaciones::PARTIAL, "Missing building permissions. Grant the bot:", language);
                        c.explanation.push_str(&format!(" {names}"));
                        c
                    }
                }
                Ok(Err(_)) => check("build_permissions", crate::operaciones::FAILED, "Could not read bot permissions. Check the network and server access.", language),
                Err(_) => check("build_permissions", crate::operaciones::FAILED, "Permission check timed out. Check the network and retry.", language),
            }
        }
    };
    if only == Some("build_permissions") {
        vec![permissions]
    } else {
        vec![gateway, permissions]
    }
}

#[tauri::command]
pub async fn diagnostico_discord(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<Vec<Check>, String> {
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
    if name.as_deref() == Some("gateway") {
        return Ok(vec![check(
            "gateway",
            if gateway_up {
                crate::operaciones::CORRECT
            } else {
                crate::operaciones::FAILED
            },
            if gateway_up {
                "The active bot Gateway is connected."
            } else {
                "The active bot Gateway is disconnected. Start or reconnect the bot."
            },
            &language,
        )]);
    }
    let cancel = state.diagnostic_cancel.subscribe();
    let token = match crypto::decrypt(&bot.token_ciphertext) {
        Ok(token) => token,
        Err(_) => {
            let checks = vec![
                check(
                    "gateway",
                    if gateway_up {
                        crate::operaciones::CORRECT
                    } else {
                        crate::operaciones::FAILED
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
                    crate::operaciones::FAILED,
                    "Could not access the bot credential. Check the keyring.",
                    &language,
                ),
            ];
            return Ok(checks
                .into_iter()
                .filter(|c| name.as_deref().is_none_or(|n| c.name == n))
                .collect());
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
        name.as_deref(),
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
    session_only: bool,
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
        if session_only {
            return Ok(true);
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
    only: Option<&str>,
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
                crate::operaciones::FAILED,
                "Binary not found. Install or configure it.",
                language,
            ),
            check(
                names[1],
                crate::operaciones::UNVERIFIED,
                "Session was not checked.",
                language,
            ),
            check(
                names[2],
                crate::operaciones::UNVERIFIED,
                "Response was not checked.",
                language,
            ),
        ]
        .into_iter()
        .filter(|c| only.is_none_or(|n| c.name == n))
        .collect();
    };
    let binary = check(
        names[0],
        crate::operaciones::CORRECT,
        "Binary found.",
        language,
    );
    if only == Some(names[0]) {
        return vec![binary];
    }
    if name == "mcp" {
        let (session, response) = mcp_checks(&path, cancel, timeout, only == Some(names[1])).await;
        let result = |name, outcome| match outcome {
            Ok(true) => check(
                name,
                crate::operaciones::CORRECT,
                if name == names[1] {
                    "Session available."
                } else {
                    "Process responded."
                },
                language,
            ),
            Ok(false) => check(
                name,
                crate::operaciones::FAILED,
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
        ]
        .into_iter()
        .filter(|c| only.is_none_or(|n| c.name == n))
        .collect();
    }
    let session = if only == Some(names[2]) {
        None
    } else {
        Some({
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
                            crate::operaciones::CORRECT
                        } else {
                            crate::operaciones::PARTIAL
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
        })
    };
    if only == Some(names[1]) {
        return vec![session.unwrap()];
    }
    if session
        .as_ref()
        .is_some_and(|s| s.status == crate::operaciones::UNVERIFIED)
    {
        return vec![
            binary,
            session.unwrap(),
            check(
                names[2],
                crate::operaciones::UNVERIFIED,
                "Process check cancelled.",
                language,
            ),
        ];
    }
    let response = process_check(&path, &["--version"], None, cancel, timeout)
        .await
        .map(|o| o.status.success() && !o.stdout.is_empty());
    let response = match response {
        Ok(true) => check(
            names[2],
            crate::operaciones::CORRECT,
            "Process responded.",
            language,
        ),
        Ok(false) => check(
            names[2],
            crate::operaciones::FAILED,
            "Process did not respond. Check its installation.",
            language,
        ),
        Err(reason) => process_failure(names[2], reason, language),
    };
    if only == Some(names[2]) {
        vec![response]
    } else {
        vec![binary, session.unwrap(), response]
    }
}

fn process_failure(name: &'static str, reason: &str, language: &str) -> Check {
    match reason {
        "cancelled" => check(
            name,
            crate::operaciones::UNVERIFIED,
            "Process check cancelled.",
            language,
        ),
        "timeout" => check(
            name,
            crate::operaciones::FAILED,
            "Process check timed out. Retry or check the installation.",
            language,
        ),
        _ => check(
            name,
            crate::operaciones::FAILED,
            "Process could not be started. Check its installation.",
            language,
        ),
    }
}

#[tauri::command]
pub async fn diagnostico_agentes(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<Vec<Check>, String> {
    let language = state
        .config
        .lock()
        .map(|c| c.ui_language.clone())
        .unwrap_or_else(|_| "en".into());
    let name_filter = name.clone();
    let dirs = crate::asistente::carpetas_busqueda();
    let mut cancel = state.diagnostic_cancel.subscribe();
    let mut checks = Vec::new();
    for agent in ["claude", "codex", "opencode", "mcp"] {
        if name
            .as_ref()
            .is_some_and(|target| !target.starts_with(&format!("{agent}_")))
        {
            continue;
        }
        let name = agent;
        let path = if name == "mcp" {
            crate::commands::ruta_del_mcp()
                .ok()
                .map(std::path::PathBuf::from)
                .filter(|p| p.is_file())
        } else {
            crate::asistente::buscar_binario(name, &dirs)
        };
        let result = agent_checks(
            name,
            path,
            &language,
            &mut cancel,
            Duration::from_secs(5),
            name_filter.as_deref(),
        )
        .await;
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
    async fn retry_runs_only_selected_agent_check() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("argus-retry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent");
        let log = dir.join("calls");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho \"$*\" >> '{}'{nl}echo ok{nl}",
                log.display(),
                nl = "\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (_tx, mut rx) = tokio::sync::broadcast::channel(4);
        for target in ["codex_binary", "codex_session", "codex_response"] {
            let checks = agent_checks(
                "codex",
                Some(path.clone()),
                "en",
                &mut rx,
                Duration::from_secs(1),
                Some(target),
            )
            .await;
            assert_eq!(checks.len(), 1);
            assert_eq!(checks[0].name, target);
        }
        assert_eq!(
            std::fs::read_to_string(log).unwrap(),
            "login status\n--version\n"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn retry_gateway_does_not_request_permissions() {
        let server = MockServer::start().await;
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        let checks = discord_checks(
            "fake",
            Some(&server.address().to_string()),
            "123",
            "555",
            true,
            "en",
            rx,
            Duration::from_secs(1),
            Some("gateway"),
        )
        .await;
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].name, "gateway");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

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
            None,
        )
        .await;
        assert_eq!(
            [&checks[0].status, &checks[1].status, &checks[2].status],
            [
                &crate::operaciones::CORRECT,
                &crate::operaciones::PARTIAL,
                &crate::operaciones::CORRECT
            ]
        );
        let checks = agent_checks("codex", None, "en", &mut rx, Duration::from_secs(1), None).await;
        assert_eq!(
            [&checks[0].status, &checks[1].status, &checks[2].status],
            [
                &crate::operaciones::FAILED,
                &crate::operaciones::UNVERIFIED,
                &crate::operaciones::UNVERIFIED
            ]
        );
        std::fs::write(&path, "#!/bin/sh\nexec sleep 10\n").unwrap();
        let checks = agent_checks(
            "opencode",
            Some(path.clone()),
            "en",
            &mut rx,
            Duration::from_millis(30),
            None,
        )
        .await;
        assert!(checks[1].explanation.contains("timed out"));
        assert!(checks[2].explanation.contains("timed out"));
        tx.send(()).unwrap();
        let checks = agent_checks(
            "codex",
            Some(path),
            "en",
            &mut rx,
            Duration::from_secs(2),
            None,
        )
        .await;
        assert_eq!(checks[1].status, crate::operaciones::UNVERIFIED);
        assert_eq!(checks[2].status, crate::operaciones::UNVERIFIED);
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
            None,
        )
        .await;
        assert_eq!(
            [&checks[0].status, &checks[1].status, &checks[2].status],
            [
                &crate::operaciones::CORRECT,
                &crate::operaciones::CORRECT,
                &crate::operaciones::CORRECT
            ]
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
            None,
        )
        .await;
        assert_eq!(checks[1].status, crate::operaciones::FAILED);
        assert_eq!(checks[2].status, crate::operaciones::FAILED);
        write_fake(
            "#!/bin/sh
read line
echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}'
exec sleep 10
",
        );
        let checks = agent_checks(
            "mcp",
            Some(path),
            "en",
            &mut rx,
            Duration::from_millis(40),
            None,
        )
        .await;
        assert_eq!(checks[1].status, crate::operaciones::CORRECT);
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
            None,
        )
        .await;
        assert_eq!(checks[0].status, crate::operaciones::FAILED);
        assert_eq!(checks[1].status, crate::operaciones::PARTIAL);
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
            None,
        )
        .await;
        assert_eq!(checks[1].status, crate::operaciones::FAILED);
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
            None,
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
            None,
        )
        .await;
        assert_eq!(checks[1].status, crate::operaciones::UNVERIFIED);
    }
    #[tokio::test]
    async fn inaccessible_database_and_missing_keyring_are_reported_without_secrets() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.close().await;
        let checks = run(Some(&pool), false, "en", None).await;
        assert_eq!(checks[0].status, crate::operaciones::UNVERIFIED);
        assert_eq!(checks[1].status, crate::operaciones::FAILED);
        assert_eq!(checks[2].status, crate::operaciones::PARTIAL);
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
        let checks = run(Some(&pool), true, "es", None).await;
        assert_eq!(checks[0].status, crate::operaciones::FAILED);
        assert_eq!(checks[1].status, crate::operaciones::CORRECT);
        assert_eq!(checks[2].status, crate::operaciones::CORRECT);
        assert!(!serde_json::to_string(&checks)
            .unwrap()
            .contains("secret-password-token"));
        assert!(checks[0].explanation.contains("configuración"));
    }
}
