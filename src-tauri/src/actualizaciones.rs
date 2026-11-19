#[cfg(target_os = "linux")]
use tauri::Manager;

/// Linux solo permite sustituir una AppImage; los paquetes del sistema se
/// descargan manualmente. No se inspeccionan variables con credenciales.
#[tauri::command]
pub fn updater_install_supported(app: tauri::AppHandle) -> bool {
    #[cfg(target_os = "linux")]
    {
        app.env().appimage.is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        true
    }
}
