//! Servicio sin ventana (opcional, desactivado por defecto). `argus --service`
//! arranca ARGUS sin interfaz ni bandeja y solo conecta el bot activo si el
//! usuario habilitó el servicio. La garantía de una sola conexión por bot (lock
//! de archivo del sistema) vive en `conexion.rs` y la comparten servicio e interfaz.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager, State};

use crate::state::AppState;

pub use crate::conexion::ESPERA_PARADA;

/// Clave de `settings` que habilita el servicio sin ventana.
pub const AJUSTE_SERVICIO: &str = "service_enabled";

/// Carpeta de los locks: la de la base de datos.
pub fn carpeta_locks(app: &AppHandle) -> Result<PathBuf, String> {
    let ruta = crate::db::ruta_efectiva(&crate::db::ruta_bd(app).map_err(|e| e.to_string())?);
    ruta.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Could not find the data folder".to_string())
}

pub async fn habilitado(pool: &sqlx::SqlitePool) -> bool {
    matches!(
        crate::db::get_setting(pool, AJUSTE_SERVICIO).await,
        Ok(Some(v)) if v == "true"
    )
}

pub async fn habilitar(pool: &sqlx::SqlitePool, activo: bool) -> Result<(), String> {
    crate::db::set_setting(pool, AJUSTE_SERVICIO, if activo { "true" } else { "false" })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_service_enabled(app: AppHandle) -> Result<bool, String> {
    let pool = crate::db::get_pool(&app).await.map_err(|e| e.to_string())?;
    Ok(habilitado(&pool).await)
}

#[tauri::command]
pub async fn set_service_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    let pool = crate::db::get_pool(&app).await.map_err(|e| e.to_string())?;
    let resultado = habilitar(&pool, enabled).await.map(|_| enabled);
    crate::i18n::localizar_error(&state, resultado)
}

/// Arranque del modo sin ventana: solo conecta si el servicio está habilitado.
/// Termina el proceso si no lo está o si otro proceso ya posee el bot.
pub fn arrancar_sin_ventana(handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let pool = match crate::db::get_pool(&handle).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Service: database unavailable: {e}");
                handle.exit(1);
                return;
            }
        };
        if !habilitado(&pool).await {
            tracing::info!("Service disabled: enable it in ARGUS first. Exiting.");
            handle.exit(0);
            return;
        }
        let state = handle.state::<AppState>();
        // Canal local con la interfaz: el estado y los avisos viven aquí.
        let buzon = crate::canal::Buzon::new();
        let canal = match carpeta_locks(&handle) {
            Ok(c) => crate::canal::Servidor::iniciar(&c, buzon.clone())
                .await
                .ok(),
            Err(_) => None,
        };
        match crate::commands::conectar_bot_servicio(&handle, &state).await {
            Ok(st) => {
                buzon.fijar_estado(
                    serde_json::json!({ "running": true, "connected": st.connected }),
                );
                buzon.publicar("aviso", "Service connected");
            }
            Err(e) => {
                tracing::error!("Service could not connect: {e}");
                if let Some(c) = canal {
                    c.detener();
                }
                handle.exit(1);
                return;
            }
        }
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Service stopping");
        buzon.fijar_estado(serde_json::json!({ "running": false, "connected": false }));
        crate::commands::detener_conexion(&state).await;
        if let Some(c) = canal {
            c.detener();
        }
        handle.exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn el_servicio_esta_desactivado_por_defecto() {
        let dir = std::env::temp_dir().join(format!("argus-svc-{}", uuid::Uuid::new_v4()));
        let pool = crate::db::abrir_bd(&dir.join("argus.db")).await.unwrap();
        assert!(!habilitado(&pool).await);
        habilitar(&pool, true).await.unwrap();
        assert!(habilitado(&pool).await);
        habilitar(&pool, false).await.unwrap();
        assert!(!habilitado(&pool).await);
        std::fs::remove_dir_all(dir).ok();
    }
}
