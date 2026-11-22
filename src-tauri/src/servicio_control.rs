//! Controles del servicio sin ventana desde la interfaz: iniciar, detener,
//! estado, último error y arranque con la sesión del usuario (opcional,
//! desactivado por defecto).
//!
//! El arranque con el sistema es **por usuario y sin privilegios**: en Linux un
//! archivo `.desktop` en `~/.config/autostart`; en Windows la clave
//! `HKCU\...\Run`. No se instala ningún servicio del sistema (systemd de
//! sistema, Servicio de Windows) ni se pide administrador. Las funciones de
//! archivo reciben la carpeta como parámetro para probarlas sin tocar el
//! arranque real del equipo.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::canal::{Cliente, NO_ACTIVO};
use crate::state::AppState;

/// Nombre del archivo de autoarranque (Linux) y del valor del registro (Windows).
pub const NOMBRE_AUTOARRANQUE: &str = "argus-service";
/// Último error del servicio (sobrevive a que el proceso termine).
pub const ARCHIVO_ERROR: &str = "service-last-error.json";
const CONSUMIDOR_ESTADO: &str = "status";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct ErrorGuardado {
    error: String,
    at: String,
}

/// Guarda el último error del servicio (lo llama el proceso sin ventana).
pub fn guardar_error(carpeta: &Path, error: &str) {
    let e = ErrorGuardado {
        error: error.to_string(),
        at: chrono::Utc::now().to_rfc3339(),
    };
    if let Ok(txt) = serde_json::to_string(&e) {
        let _ = std::fs::create_dir_all(carpeta);
        let _ = std::fs::write(carpeta.join(ARCHIVO_ERROR), txt);
    }
}

/// Borra el último error (arranque correcto).
pub fn limpiar_error(carpeta: &Path) {
    let _ = std::fs::remove_file(carpeta.join(ARCHIVO_ERROR));
}

fn leer_error(carpeta: &Path) -> Option<String> {
    let txt = std::fs::read_to_string(carpeta.join(ARCHIVO_ERROR)).ok()?;
    serde_json::from_str::<ErrorGuardado>(&txt)
        .ok()
        .map(|e| e.error)
}

// ---------- Arranque con la sesión del usuario ----------

/// Rutas con caracteres que romperían el `Exec` del `.desktop` se rechazan.
fn ruta_segura(exe: &Path) -> Result<String, String> {
    let s = exe.to_str().ok_or("The executable path is not valid")?;
    if s.chars()
        .any(|c| matches!(c, '"' | '\\' | '`' | '$' | '%' | '\n' | '\r'))
    {
        return Err("The executable path contains unsupported characters".into());
    }
    Ok(s.to_string())
}

/// Contenido del `.desktop` de autoarranque.
pub fn contenido_desktop(exe: &Path) -> Result<String, String> {
    let ruta = ruta_segura(exe)?;
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=ARGUS service\nExec=\"{ruta}\" --service\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    ))
}

/// Comando que se registra en `HKCU\...\Run` de Windows.
pub fn comando_windows(exe: &Path) -> Result<String, String> {
    Ok(format!("\"{}\" --service", ruta_segura(exe)?))
}

fn archivo_desktop(base: &Path) -> PathBuf {
    base.join("autostart")
        .join(format!("{NOMBRE_AUTOARRANQUE}.desktop"))
}

/// Linux: crea o borra el `.desktop` dentro de `base/autostart` (`base` es
/// `~/.config`). No toca nada fuera de esa carpeta.
pub fn autoarranque_archivo(base: &Path, exe: &Path, activar: bool) -> Result<(), String> {
    let ruta = archivo_desktop(base);
    if activar {
        let contenido = contenido_desktop(exe)?;
        std::fs::create_dir_all(ruta.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&ruta, contenido).map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&ruta) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

pub fn autoarranque_archivo_activo(base: &Path) -> bool {
    archivo_desktop(base).is_file()
}

#[cfg(windows)]
const CLAVE_RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Arranque real del equipo (solo lo usan los comandos, nunca los tests).
fn autoarranque_sistema(app: &AppHandle, exe: &Path, activar: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let _ = app;
        let mut cmd = std::process::Command::new("reg");
        if activar {
            cmd.args([
                "add",
                CLAVE_RUN,
                "/v",
                NOMBRE_AUTOARRANQUE,
                "/t",
                "REG_SZ",
                "/d",
            ])
            .arg(comando_windows(exe)?)
            .arg("/f");
        } else {
            cmd.args(["delete", CLAVE_RUN, "/v", NOMBRE_AUTOARRANQUE, "/f"]);
        }
        let s = cmd.output().map_err(|e| e.to_string())?;
        // Borrar un valor que no existe no es un error.
        if s.status.success() || !activar {
            Ok(())
        } else {
            Err("Could not change the startup setting".into())
        }
    }
    #[cfg(target_os = "linux")]
    {
        let base = app.path().config_dir().map_err(|e| e.to_string())?;
        autoarranque_archivo(&base, exe, activar)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (app, exe, activar);
        Err("Starting with the system is only available on Linux and Windows".into())
    }
}

fn autoarranque_sistema_activo(app: &AppHandle) -> bool {
    #[cfg(windows)]
    {
        let _ = app;
        std::process::Command::new("reg")
            .args(["query", CLAVE_RUN, "/v", NOMBRE_AUTOARRANQUE])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        app.path()
            .config_dir()
            .map(|b| autoarranque_archivo_activo(&b))
            .unwrap_or(false)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = app;
        false
    }
}

// ---------- Estado ----------

#[derive(Debug, Serialize, PartialEq)]
pub struct EstadoServicio {
    /// El usuario habilitó el servicio (ajuste `service_enabled`).
    pub enabled: bool,
    /// El proceso sin ventana responde por el canal local.
    pub running: bool,
    /// Gateway del servicio conectado (`None` si no se sabe).
    pub gateway_connected: Option<bool>,
    /// Arranque con la sesión del usuario activado.
    pub autostart: bool,
    pub last_error: Option<String>,
}

/// Estado del servicio a partir de la carpeta de datos (sin efectos).
pub async fn estado_en(carpeta: &Path, enabled: bool, autostart: bool) -> EstadoServicio {
    let lectura = Cliente::new(carpeta, CONSUMIDOR_ESTADO).leer().await;
    let (running, gateway_connected) = match &lectura {
        Ok(l) => (true, l.estado.get("connected").and_then(|v| v.as_bool())),
        Err(_) => (false, None),
    };
    EstadoServicio {
        enabled,
        running,
        gateway_connected,
        autostart,
        last_error: leer_error(carpeta),
    }
}

/// Orden con la que se lanza el servicio (se separa para probarla sin lanzar).
pub fn comando_servicio(exe: &Path) -> std::process::Command {
    let mut c = std::process::Command::new(exe);
    c.arg("--service")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Grupo propio: cerrar la interfaz no mata el servicio.
        c.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NO_WINDOW
        c.creation_flags(0x0000_0008 | 0x0800_0000);
    }
    c
}

async fn estado_completo(app: &AppHandle) -> Result<EstadoServicio, String> {
    let pool = crate::db::get_pool(app).await.map_err(|e| e.to_string())?;
    let enabled = crate::servicio::habilitado(&pool).await;
    let carpeta = crate::servicio::carpeta_locks(app)?;
    Ok(estado_en(&carpeta, enabled, autoarranque_sistema_activo(app)).await)
}

#[tauri::command]
pub async fn servicio_estado(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<EstadoServicio, String> {
    let mut r = estado_completo(&app).await;
    if let Ok(e) = r.as_mut() {
        let lang = crate::i18n::idioma(&state);
        e.last_error = e.last_error.take().map(|m| crate::i18n::tr(&lang, &m));
    }
    crate::i18n::localizar_error(&state, r)
}

#[tauri::command]
pub async fn servicio_iniciar(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let r = async {
        let pool = crate::db::get_pool(&app).await.map_err(|e| e.to_string())?;
        if !crate::servicio::habilitado(&pool).await {
            return Err("Enable the service before starting it".to_string());
        }
        let carpeta = crate::servicio::carpeta_locks(&app)?;
        if Cliente::new(&carpeta, CONSUMIDOR_ESTADO)
            .leer()
            .await
            .is_ok()
        {
            return Ok(true); // ya está en marcha: no se lanza otro
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut hijo = comando_servicio(&exe).spawn().map_err(|e| e.to_string())?;
        // Recoge el proceso si termina mientras la interfaz sigue abierta.
        std::thread::spawn(move || {
            let _ = hijo.wait();
        });
        Ok(true)
    }
    .await;
    crate::i18n::localizar_error(&state, r)
}

#[tauri::command]
pub async fn servicio_detener(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let r = async {
        let carpeta = crate::servicio::carpeta_locks(&app)?;
        match Cliente::new(&carpeta, CONSUMIDOR_ESTADO).parar().await {
            Ok(()) => Ok(true),
            Err(e) if e == NO_ACTIVO => Ok(false), // ya estaba detenido
            Err(e) => Err(e),
        }
    }
    .await;
    crate::i18n::localizar_error(&state, r)
}

#[tauri::command]
pub async fn servicio_set_autoarranque(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    let r = (|| {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        autoarranque_sistema(&app, &exe, enabled).map(|_| enabled)
    })();
    crate::i18n::localizar_error(&state, r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canal::{Buzon, Servidor};

    fn dir() -> PathBuf {
        std::env::temp_dir().join(format!("argus-ctl-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn el_autoarranque_en_archivo_se_activa_y_desactiva_en_una_carpeta_de_prueba() {
        let base = dir();
        let exe = Path::new("/opt/argus app/argus");
        assert!(
            !autoarranque_archivo_activo(&base),
            "desactivado por defecto"
        );
        autoarranque_archivo(&base, exe, true).unwrap();
        assert!(autoarranque_archivo_activo(&base));
        let txt = std::fs::read_to_string(archivo_desktop(&base)).unwrap();
        assert!(txt.contains("Exec=\"/opt/argus app/argus\" --service"));
        autoarranque_archivo(&base, exe, false).unwrap();
        assert!(!autoarranque_archivo_activo(&base));
        // Desactivar dos veces no falla.
        autoarranque_archivo(&base, exe, false).unwrap();
        std::fs::remove_dir_all(base).ok();
    }

    #[test]
    fn rechaza_rutas_que_romperian_el_comando() {
        assert!(contenido_desktop(Path::new("/tmp/a\"b")).is_err());
        assert!(comando_windows(Path::new("C:\\x\\a$b.exe")).is_err());
        assert_eq!(
            comando_windows(Path::new("C:/Argus/argus.exe")).unwrap(),
            "\"C:/Argus/argus.exe\" --service"
        );
    }

    #[test]
    fn el_comando_lanza_el_modo_sin_ventana_sin_ejecutarse() {
        let c = comando_servicio(Path::new("/ruta/argus"));
        let args: Vec<_> = c.get_args().collect();
        assert_eq!(args, ["--service"]);
    }

    #[test]
    fn el_ultimo_error_se_guarda_y_se_limpia() {
        let d = dir();
        assert_eq!(leer_error(&d), None);
        guardar_error(&d, "boom");
        assert_eq!(leer_error(&d).as_deref(), Some("boom"));
        limpiar_error(&d);
        assert_eq!(leer_error(&d), None);
        std::fs::remove_dir_all(d).ok();
    }

    #[tokio::test]
    async fn el_estado_distingue_detenido_y_en_marcha_y_la_parada_llega_por_el_canal() {
        let d = dir();
        let detenido = estado_en(&d, true, false).await;
        assert!(!detenido.running && detenido.gateway_connected.is_none());

        let b = Buzon::new();
        b.fijar_estado(serde_json::json!({ "running": true, "connected": false }));
        let s = Servidor::iniciar(&d, b.clone()).await.unwrap();
        guardar_error(&d, "Gateway failed");
        let e = estado_en(&d, true, false).await;
        assert!(e.running && e.enabled && !e.autostart);
        assert_eq!(e.gateway_connected, Some(false));
        assert_eq!(e.last_error.as_deref(), Some("Gateway failed"));

        Cliente::new(&d, CONSUMIDOR_ESTADO).parar().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), b.esperar_parada())
            .await
            .expect("el servicio recibe la orden de parada");
        s.detener();
        let tras = estado_en(&d, true, false).await;
        assert!(!tras.running);
        std::fs::remove_dir_all(d).ok();
    }
}
