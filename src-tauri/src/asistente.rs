//! Asistente IA: detección de los agentes de línea de comandos (`claude`,
//! `opencode`) y de los modelos que ofrece cada uno.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use crate::i18n::localizar_error;
use crate::state::AppState;

/// Agentes soportados, en el orden en que se listan.
pub const AGENTES: [&str; 2] = ["claude", "opencode"];

/// Alias de modelo que acepta `claude --model`.
const MODELOS_CLAUDE: [&str; 3] = ["opus", "sonnet", "haiku"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgenteIa {
    pub agente: String,
    pub ruta: String,
    pub modelos: Vec<String>,
}

/// Caché en memoria de los modelos de opencode, por ruta del binario.
fn cache_modelos() -> &'static Mutex<HashMap<PathBuf, Vec<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Vec<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Carpetas donde buscar: primero el PATH y después las rutas habituales
/// (una app abierta desde un acceso directo no hereda el PATH de la terminal).
pub fn carpetas_busqueda() -> Vec<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    carpetas_con(
        &path,
        var("HOME").or_else(|| var("USERPROFILE")).as_deref(),
        var("APPDATA").as_deref(),
        var("USERPROFILE").as_deref(),
    )
}

fn carpetas_con(
    path: &std::ffi::OsStr,
    home: Option<&Path>,
    appdata: Option<&Path>,
    userprofile: Option<&Path>,
) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::env::split_paths(path).collect();
    if let Some(h) = home {
        v.push(h.join(".local").join("bin"));
        v.push(h.join(".claude").join("local"));
        v.push(h.join(".opencode").join("bin"));
        v.push(h.join(".npm-global").join("bin"));
    }
    if let Some(a) = appdata {
        v.push(a.join("npm"));
    }
    if let Some(u) = userprofile {
        v.push(u.join(".local").join("bin"));
    }
    let mut vistas = std::collections::HashSet::new();
    v.retain(|p| !p.as_os_str().is_empty() && vistas.insert(p.clone()));
    v
}

fn es_ejecutable(p: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Primera coincidencia ejecutable de `nombre` en `carpetas`.
pub fn buscar_binario(nombre: &str, carpetas: &[PathBuf]) -> Option<PathBuf> {
    let nombres: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|e| format!("{nombre}.{e}"))
            .collect()
    } else {
        vec![nombre.to_string()]
    };
    carpetas
        .iter()
        .flat_map(|c| nombres.iter().map(move |n| c.join(n)))
        .find(|p| es_ejecutable(p))
}

/// Una línea `proveedor/modelo` por modelo; ignora el resto de la salida.
fn parsear_modelos(salida: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for l in salida.lines().map(str::trim) {
        let valido = l.split_once('/').is_some_and(|(p, m)| {
            !p.is_empty() && !m.is_empty() && !l.contains(char::is_whitespace)
        });
        if valido && !v.iter().any(|x| x == l) {
            v.push(l.to_string());
        }
    }
    v
}

fn modelos_opencode(ruta: &Path) -> Vec<String> {
    if let Some(m) = cache_modelos().lock().unwrap().get(ruta) {
        return m.clone();
    }
    let salida = Command::new(ruta)
        .arg("models")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let modelos = match salida {
        Ok(o) if o.status.success() => parsear_modelos(&String::from_utf8_lossy(&o.stdout)),
        Ok(o) => {
            tracing::warn!("`opencode models` terminó con {}", o.status);
            Vec::new()
        }
        Err(e) => {
            tracing::warn!("no se pudo ejecutar `opencode models`: {e}");
            Vec::new()
        }
    };
    // Solo se cachea un resultado útil: un fallo puntual se reintenta.
    if !modelos.is_empty() {
        cache_modelos()
            .lock()
            .unwrap()
            .insert(ruta.to_path_buf(), modelos.clone());
    }
    modelos
}

/// Agentes encontrados en `carpetas`, con sus modelos.
pub fn agentes_en(carpetas: &[PathBuf]) -> Vec<AgenteIa> {
    AGENTES
        .iter()
        .filter_map(|&agente| {
            let ruta = buscar_binario(agente, carpetas)?;
            let modelos = if agente == "claude" {
                MODELOS_CLAUDE.iter().map(|m| m.to_string()).collect()
            } else {
                modelos_opencode(&ruta)
            };
            Some(AgenteIa {
                agente: agente.to_string(),
                ruta: ruta.to_string_lossy().into_owned(),
                modelos,
            })
        })
        .collect()
}

/// Agentes instalados (PATH + rutas habituales). Lista vacía si no hay ninguno.
pub fn agentes_disponibles() -> Vec<AgenteIa> {
    agentes_en(&carpetas_busqueda())
}

#[tauri::command]
pub async fn listar_agentes_ia(state: tauri::State<'_, AppState>) -> Result<Vec<AgenteIa>, String> {
    let r = tokio::task::spawn_blocking(agentes_disponibles)
        .await
        .map_err(|e| format!("Could not detect AI agents: {e}"));
    localizar_error(&state, r)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn dir_temporal(nombre: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("argus_asistente_{nombre}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn falso(dir: &Path, nombre: &str, script: &str) {
        let p = dir.join(nombre);
        std::fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn encuentra_binarios_falsos_y_sus_modelos() {
        let d = dir_temporal("con");
        falso(&d, "claude", "exit 0");
        falso(
            &d,
            "opencode",
            "echo 'opencode/big-pickle'\necho 'anthropic/claude-sonnet'",
        );
        let r = agentes_en(&[d.clone()]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].agente, "claude");
        assert_eq!(r[0].modelos, ["opus", "sonnet", "haiku"]);
        assert_eq!(r[0].ruta, d.join("claude").to_string_lossy());
        assert_eq!(r[1].agente, "opencode");
        assert_eq!(
            r[1].modelos,
            ["opencode/big-pickle", "anthropic/claude-sonnet"]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sin_agentes_devuelve_lista_vacia() {
        let d = dir_temporal("vacio");
        assert!(agentes_en(&[d.clone(), d.join("no_existe")]).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ignora_ficheros_sin_permiso_de_ejecucion() {
        let d = dir_temporal("noexec");
        std::fs::write(d.join("claude"), "x").unwrap();
        assert!(buscar_binario("claude", &[d.clone()]).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn modelos_de_opencode_se_cachean() {
        let d = dir_temporal("cache");
        falso(&d, "opencode", "echo 'a/uno'");
        assert_eq!(agentes_en(&[d.clone()])[0].modelos, ["a/uno"]);
        // Si el binario cambia, la caché sigue devolviendo lo primero.
        falso(&d, "opencode", "echo 'b/dos'");
        assert_eq!(agentes_en(&[d.clone()])[0].modelos, ["a/uno"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rutas_habituales_incluidas() {
        let v = carpetas_con(
            std::ffi::OsStr::new(""),
            Some(Path::new("/h")),
            Some(Path::new("/ad")),
            Some(Path::new("/up")),
        );
        for esperado in [
            "/h/.local/bin",
            "/h/.claude/local",
            "/h/.opencode/bin",
            "/h/.npm-global/bin",
            "/ad/npm",
            "/up/.local/bin",
        ] {
            assert!(v.contains(&PathBuf::from(esperado)), "falta {esperado}");
        }
    }

    #[test]
    fn parsea_solo_lineas_proveedor_modelo() {
        let m = parsear_modelos("a/b\n\nruido con espacios\n  c/d  \na/b\n/x\n");
        assert_eq!(m, ["a/b", "c/d"]);
    }
}
