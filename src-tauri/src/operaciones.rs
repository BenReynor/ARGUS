//! Registro persistente de propuestas. Nunca guarda argumentos ni respuestas de Discord.
use sqlx::{Row, SqlitePool};

/// Conserva categorías de error conocidas, nunca texto arbitrario de Discord o
/// argumentos, que pueden incluir contenido de mensajes o credenciales.
pub fn motivo_seguro(error: &str) -> String {
    if let Some(resto) = error.strip_prefix("Discord responded with an error (") {
        if let Some((codigo, _)) = resto.split_once(')') {
            if codigo.len() == 3 && codigo.bytes().all(|b| b.is_ascii_digit()) {
                return format!("Discord rejected the request (HTTP {codigo})");
            }
        }
    }
    let lower = error.to_ascii_lowercase();
    if lower.contains("response body couldn't be deserialized") {
        "Discord returned an unreadable response; the outcome may be unknown".into()
    } else if lower.contains("could not reach discord") {
        "Could not reach Discord; the outcome may be unknown".into()
    } else if lower.contains("permission") || lower.contains("permiso") {
        "The bot lacks permission for this action".into()
    } else if lower.contains("rate limit") {
        "Discord rate limited the request".into()
    } else if lower.contains("not found") || lower.contains("no encontrado") {
        "The requested resource was not found or is inaccessible".into()
    } else if lower.contains("invalid token") {
        "Discord rejected the bot credential".into()
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "The request timed out; its outcome may be unknown".into()
    } else if lower.contains("invalid") || lower.contains("not valid") {
        "The action contains invalid parameters".into()
    } else if lower.contains("hierarchy") || lower.contains("jerarqu") {
        "The bot's role hierarchy does not allow this action".into()
    } else {
        "The action failed; its cause could not be safely recorded".into()
    }
}

#[derive(Debug, serde::Serialize)]
pub struct Operacion {
    pub id: String,
    pub bot_id: String,
    pub guild_id: String,
    pub destination: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub steps: Vec<Paso>,
}
#[derive(Debug, serde::Serialize)]
pub struct Paso {
    pub step_index: i64,
    pub action: String,
    pub destination: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

// Persist only Discord numeric identifiers, never arbitrary argument text (which
// may contain credentials). Keep all identifiers for multi-target actions.
fn destino(accion: &crate::propuesta::Accion, guild: &str) -> Option<String> {
    let mut ids = serde_json::Map::new();
    for key in [
        "channel_id",
        "user_id",
        "role_id",
        "target_id",
        "message_id",
        "webhook_id",
        "categoria_id",
    ] {
        if let Some(value) = accion.args.get(key).and_then(|v| v.as_str()) {
            if !value.is_empty() && value.len() <= 20 && value.bytes().all(|b| b.is_ascii_digit()) {
                ids.insert(key.to_string(), value.into());
            }
        }
    }
    if let Some(values) = accion.args.get("message_ids").and_then(|v| v.as_array()) {
        let ids_validos: Vec<_> = values
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|id| !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit()))
            .collect();
        if !ids_validos.is_empty() {
            ids.insert("message_ids".into(), serde_json::json!(ids_validos));
        }
    }
    if ids.is_empty()
        && matches!(
            accion.herramienta.as_str(),
            "editar_servidor"
                | "crear_canal"
                | "crear_rol"
                | "ordenar_canales"
                | "ordenar_roles"
                | "aplicar_restauracion"
        )
    {
        ids.insert("guild_id".into(), guild.into());
    }
    if ids.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(ids).to_string())
    }
}

pub async fn iniciar(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
    propuesta: &crate::propuesta::Propuesta,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let destinos: Vec<_> = propuesta
        .acciones
        .iter()
        .map(|a| destino(a, guild))
        .collect();
    let destino_unico = destinos
        .first()
        .and_then(|d| d.as_ref())
        .filter(|d| destinos.iter().all(|otro| otro.as_ref() == Some(*d)));
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO operations(id,bot_id,guild_id,destination,status) VALUES(?,?,?,?, 'in_progress')")
        .bind(&id).bind(bot).bind(guild).bind(destino_unico).execute(&mut *tx).await?;
    for (i, a) in propuesta.acciones.iter().enumerate() {
        // Only recognized tool names are persisted, never free-form user text.
        let action = if crate::mcp::esquema_args(&a.herramienta).is_some() {
            a.herramienta.as_str()
        } else {
            "unknown"
        };
        sqlx::query("INSERT INTO operation_steps(operation_id,step_index,action,destination,status) VALUES(?,?,?,?,'pending')")
            .bind(&id).bind(i as i64).bind(action).bind(&destinos[i]).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(id)
}

pub async fn paso(
    pool: &SqlitePool,
    id: &str,
    index: usize,
    status: &str,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE operation_steps SET status=?, reason=?, started_at=COALESCE(started_at,strftime('%Y-%m-%dT%H:%M:%fZ','now')), finished_at=CASE WHEN ? IN ('in_progress','verifying','pending') THEN NULL ELSE strftime('%Y-%m-%dT%H:%M:%fZ','now') END WHERE operation_id=? AND step_index=?")
        .bind(status).bind(reason).bind(status).bind(id).bind(index as i64).execute(pool).await?;
    Ok(())
}
pub async fn terminar(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    reason: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE operation_steps SET status='unverified',reason='Outcome was not recorded',finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE operation_id=? AND status IN ('in_progress','verifying')").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE operation_steps SET status='cancelled',reason='Not attempted because the operation stopped',finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE operation_id=? AND status='pending'").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE operations SET status=?,reason=?,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?").bind(status).bind(reason).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
/// Registra el inicio de una comprobación posterior para un paso aceptado.
pub async fn iniciar_verificacion(
    pool: &SqlitePool,
    id: &str,
    index: usize,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let changed = sqlx::query("UPDATE operation_steps SET status='verifying', reason='Verification in progress', finished_at=NULL WHERE operation_id=? AND step_index=? AND status='unverified'")
        .bind(id).bind(index as i64).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(sqlx::Error::RowNotFound);
    }
    sqlx::query("UPDATE operations SET status='verifying', reason='Verification in progress', finished_at=NULL WHERE id=?")
        .bind(id).execute(&mut *tx).await?;
    tx.commit().await
}

/// Solo se invoca cuando la comprobación efectiva confirma el resultado.
pub async fn confirmar_verificacion(
    pool: &SqlitePool,
    id: &str,
    index: usize,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let changed = sqlx::query("UPDATE operation_steps SET status='correct', reason='Result verified', finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE operation_id=? AND step_index=? AND status='verifying'")
        .bind(id).bind(index as i64).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(sqlx::Error::RowNotFound);
    }
    sqlx::query("UPDATE operations SET status=CASE WHEN EXISTS(SELECT 1 FROM operation_steps WHERE operation_id=? AND status='verifying') THEN 'verifying' WHEN EXISTS(SELECT 1 FROM operation_steps WHERE operation_id=? AND status IN ('failed','skipped','cancelled')) THEN 'partial' WHEN EXISTS(SELECT 1 FROM operation_steps WHERE operation_id=? AND status!='correct') THEN 'unverified' ELSE 'correct' END WHERE id=?")
        .bind(id).bind(id).bind(id).bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE operations SET reason=CASE status WHEN 'correct' THEN 'All results verified' WHEN 'partial' THEN 'Some steps failed or were skipped' WHEN 'unverified' THEN 'Some results have not been verified' ELSE 'Verification in progress' END, finished_at=CASE WHEN status='verifying' THEN NULL ELSE strftime('%Y-%m-%dT%H:%M:%fZ','now') END WHERE id=?")
        .bind(id).execute(&mut *tx).await?;
    tx.commit().await
}

/// Llamar solo al arrancar la aplicación, no al abrir conexiones MCP adicionales.
pub async fn recuperar(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE operation_steps SET status='unverified',reason='Interrupted before the outcome was recorded',finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status IN ('in_progress','verifying') AND operation_id IN (SELECT id FROM operations WHERE status IN ('pending','in_progress','verifying'))").execute(&mut *tx).await?;
    sqlx::query("UPDATE operation_steps SET status='cancelled',reason='Not attempted before interruption',finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status='pending' AND operation_id IN (SELECT id FROM operations WHERE status IN ('pending','in_progress','verifying'))").execute(&mut *tx).await?;
    sqlx::query("UPDATE operations SET status='interrupted',reason='Interrupted before completion; it was not restarted',finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status IN ('pending','in_progress','verifying')").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn listar(
    pool: &SqlitePool,
    bot: &str,
    guild: &str,
) -> Result<Vec<Operacion>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT * FROM operations WHERE bot_id=? AND guild_id=? ORDER BY started_at DESC",
    )
    .bind(bot)
    .bind(guild)
    .fetch_all(pool)
    .await?;
    let mut result = Vec::new();
    for r in rows {
        let id: String = r.get("id");
        let steps =
            sqlx::query("SELECT * FROM operation_steps WHERE operation_id=? ORDER BY step_index")
                .bind(&id)
                .fetch_all(pool)
                .await?
                .into_iter()
                .map(|s| Paso {
                    step_index: s.get("step_index"),
                    action: s.get("action"),
                    destination: s.get("destination"),
                    status: s.get("status"),
                    reason: s.get("reason"),
                    started_at: s.get("started_at"),
                    finished_at: s.get("finished_at"),
                })
                .collect();
        result.push(Operacion {
            id,
            bot_id: r.get("bot_id"),
            guild_id: r.get("guild_id"),
            destination: r.get("destination"),
            status: r.get("status"),
            reason: r.get("reason"),
            started_at: r.get("started_at"),
            finished_at: r.get("finished_at"),
            steps,
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn listar_operaciones(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    guild_id: String,
) -> Result<Vec<Operacion>, String> {
    let bot = state
        .get_active_bot()
        .ok_or_else(|| "No bot selected".to_string())?;
    let pool = crate::db::get_pool(&app).await.map_err(|e| e.to_string())?;
    listar(&pool, &bot.id, &guild_id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propuesta::{Accion, Propuesta};

    #[test]
    fn motivos_explican_la_causa_sin_copiar_datos_privados() {
        for (error, esperado) in [
            (
                "Discord responded with an error (400): secret message",
                "Discord rejected the request (HTTP 400)",
            ),
            (
                "The bot is missing a permission: secret",
                "The bot lacks permission for this action",
            ),
            (
                "Not found: secret",
                "The requested resource was not found or is inaccessible",
            ),
            (
                "Invalid token secret",
                "Discord rejected the bot credential",
            ),
            (
                "unexpected secret",
                "The action failed; its cause could not be safely recorded",
            ),
        ] {
            assert_eq!(motivo_seguro(error), esperado);
        }
    }

    #[tokio::test]
    async fn destinos_de_cada_paso_se_capturan_sin_contenido_privado() {
        let dir = std::env::temp_dir().join(format!("argus-destinations-{}", uuid::Uuid::new_v4()));
        let pool = crate::db::abrir_bd(&dir.join("history.db")).await.unwrap();
        let acciones = vec![
            Accion {
                herramienta: "enviar_mensaje".into(),
                args: serde_json::json!({"channel_id":"123", "contenido":"secret"}),
                descripcion: String::new(),
            },
            Accion {
                herramienta: "dar_rol".into(),
                args: serde_json::json!({"user_id":"456", "role_id":"789", "token":"secret"}),
                descripcion: String::new(),
            },
            Accion {
                herramienta: "borrar_mensajes".into(),
                args: serde_json::json!({"channel_id":"123", "message_ids":["11", "22", "secret"], "contenido":"secret"}),
                descripcion: String::new(),
            },
        ];
        let propuesta = Propuesta {
            resumen: String::new(),
            acciones,
        };
        iniciar(&pool, "bot", "guild", &propuesta).await.unwrap();
        let registros = listar(&pool, "bot", "guild").await.unwrap();
        assert_eq!(registros[0].destination, None);
        assert_eq!(
            registros[0].steps[0].destination.as_deref(),
            Some(r#"{"channel_id":"123"}"#)
        );
        assert_eq!(
            registros[0].steps[1].destination.as_deref(),
            Some(r#"{"role_id":"789","user_id":"456"}"#)
        );
        assert_eq!(
            registros[0].steps[2].destination.as_deref(),
            Some(r#"{"channel_id":"123","message_ids":["11","22"]}"#)
        );
        assert!(!serde_json::to_string(&registros)
            .unwrap()
            .contains("secret"));
        pool.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn verificacion_efectiva_persiste_ambas_transiciones() {
        let dir = std::env::temp_dir().join(format!("argus-verification-{}", uuid::Uuid::new_v4()));
        let path = dir.join("history.db");
        let pool = crate::db::abrir_bd(&path).await.unwrap();
        let propuesta = Propuesta {
            resumen: String::new(),
            acciones: vec![Accion {
                herramienta: "crear_canal".into(),
                args: serde_json::json!({}),
                descripcion: String::new(),
            }],
        };
        let id = iniciar(&pool, "bot", "guild", &propuesta).await.unwrap();
        paso(&pool, &id, 0, "unverified", Some("Accepted"))
            .await
            .unwrap();
        terminar(&pool, &id, "unverified", "Accepted")
            .await
            .unwrap();
        assert!(confirmar_verificacion(&pool, &id, 0).await.is_err());
        iniciar_verificacion(&pool, &id, 0).await.unwrap();
        pool.close().await;
        let pool = crate::db::abrir_bd(&path).await.unwrap();
        let registro = listar(&pool, "bot", "guild").await.unwrap();
        assert_eq!(registro[0].status, "verifying");
        assert_eq!(registro[0].steps[0].status, "verifying");
        confirmar_verificacion(&pool, &id, 0).await.unwrap();
        pool.close().await;
        let pool = crate::db::abrir_bd(&path).await.unwrap();
        let registro = listar(&pool, "bot", "guild").await.unwrap();
        assert_eq!(registro[0].status, "correct");
        assert_eq!(registro[0].steps[0].status, "correct");
        pool.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn aislamiento_y_recuperacion_tras_reabrir() {
        let dir = std::env::temp_dir().join(format!("argus-operations-{}", uuid::Uuid::new_v4()));
        let path = dir.join("history.db");
        let pool = crate::db::abrir_bd(&path).await.unwrap();
        let propuesta = Propuesta {
            resumen: "private".into(),
            acciones: vec![Accion {
                herramienta: "crear_canal".into(),
                args: serde_json::json!({"token":"secret"}),
                descripcion: "private".into(),
            }],
        };
        let id = iniciar(&pool, "bot-a", "guild-a", &propuesta)
            .await
            .unwrap();
        let otro = iniciar(&pool, "bot-b", "guild-a", &propuesta)
            .await
            .unwrap();
        iniciar(&pool, "bot-a", "guild-b", &propuesta)
            .await
            .unwrap();
        paso(&pool, &id, 0, "in_progress", None).await.unwrap();
        terminar(&pool, &otro, "unverified", "Accepted")
            .await
            .unwrap();
        pool.close().await;
        let pool = crate::db::abrir_bd(&path).await.unwrap();
        recuperar(&pool).await.unwrap();
        let lista = listar(&pool, "bot-a", "guild-a").await.unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, id);
        assert_eq!(lista[0].status, "interrupted");
        assert_eq!(lista[0].steps[0].status, "unverified");
        assert!(!serde_json::to_string(&lista).unwrap().contains("secret"));
        assert_eq!(
            listar(&pool, "bot-b", "guild-a").await.unwrap()[0].status,
            "unverified"
        );
        assert_eq!(
            listar(&pool, "bot-a", "guild-b").await.unwrap()[0].status,
            "interrupted"
        );
        pool.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }
}
