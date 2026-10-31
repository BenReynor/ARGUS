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

/// Modo del chat: en los dos el agente solo tiene herramientas de lectura; la
/// diferencia está en el prompt (Execute pide una propuesta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoAsistente {
    Read,
    Execute,
}

/// Herramientas nativas de claude que nunca se le dan al agente.
const CLAUDE_PROHIBIDAS: [&str; 7] = [
    "Bash",
    "Edit",
    "Write",
    "NotebookEdit",
    "WebFetch",
    "WebSearch",
    "Task",
];

/// Herramientas nativas de opencode desactivadas.
const OPENCODE_PROHIBIDAS: [&str; 4] = ["bash", "edit", "write", "webfetch"];

/// Orden de lanzamiento de un agente: qué ejecutar, sin ejecutarlo.
#[derive(Debug, Clone, PartialEq)]
pub struct OrdenAgente {
    pub programa: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub stdin: String,
    pub directorio: PathBuf,
}

/// Carpeta de trabajo propia del agente (no se crea aquí).
pub fn directorio_agente() -> PathBuf {
    std::env::temp_dir().join("argus-asistente")
}

/// Construye la orden para (agente, modelo, modo, sesión previa, prompt).
/// `ruta_agente` es el binario del agente y `ruta_mcp` el de `argus-mcp`. El token
/// del bot nunca aparece: argus-mcp lo lee de la BD.
pub fn orden_agente(
    agente: &str,
    ruta_agente: &Path,
    ruta_mcp: &Path,
    modelo: &str,
    _modo: ModoAsistente,
    sesion: Option<&str>,
    prompt: &str,
) -> Result<OrdenAgente, String> {
    let ruta_mcp_s = ruta_mcp.to_string_lossy().into_owned();
    let (args, env) = match agente {
        "claude" => {
            let mcp = serde_json::json!({
                "mcpServers": { "argus": { "command": ruta_mcp_s, "args": [] } }
            });
            let permitidas: Vec<String> = crate::mcp::HERRAMIENTAS_LECTURA
                .iter()
                .map(|h| format!("mcp__argus__{h}"))
                .collect();
            let mut a: Vec<String> = [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--strict-mcp-config",
                "--mcp-config",
            ]
            .map(String::from)
            .to_vec();
            a.push(mcp.to_string());
            a.push("--model".into());
            a.push(modelo.into());
            a.push("--allowedTools".into());
            a.push(permitidas.join(","));
            a.push("--disallowedTools".into());
            a.push(CLAUDE_PROHIBIDAS.join(","));
            if let Some(id) = sesion {
                a.push("--resume".into());
                a.push(id.into());
            }
            (a, Vec::new())
        }
        "opencode" => {
            let mut tools = serde_json::Map::new();
            for t in OPENCODE_PROHIBIDAS {
                tools.insert(t.into(), false.into());
            }
            for t in crate::mcp::HERRAMIENTAS_ESCRITURA {
                tools.insert(format!("argus_{t}"), false.into());
            }
            let cfg = serde_json::json!({
                "mcp": { "argus": { "type": "local", "command": [ruta_mcp_s], "enabled": true } },
                "tools": tools,
            });
            let mut a: Vec<String> = ["run", "--format", "json", "--model"]
                .map(String::from)
                .to_vec();
            a.push(modelo.into());
            if let Some(id) = sesion {
                a.push("--session".into());
                a.push(id.into());
            }
            (
                a,
                vec![("OPENCODE_CONFIG_CONTENT".to_string(), cfg.to_string())],
            )
        }
        otro => return Err(format!("Unknown AI agent: {otro}")),
    };
    Ok(OrdenAgente {
        programa: ruta_agente.to_path_buf(),
        args,
        env,
        stdin: prompt.to_string(),
        directorio: directorio_agente(),
    })
}

#[cfg(test)]
mod tests_orden {
    use super::*;
    use crate::mcp::{HERRAMIENTAS_ESCRITURA, HERRAMIENTAS_LECTURA};
    use serde_json::Value;

    fn orden(agente: &str, sesion: Option<&str>) -> OrdenAgente {
        orden_agente(
            agente,
            Path::new("/bin/agente"),
            Path::new("/opt/argus-mcp"),
            "mod",
            ModoAsistente::Execute,
            sesion,
            "hola",
        )
        .unwrap()
    }

    fn valor(o: &OrdenAgente, flag: &str) -> String {
        let i = o.args.iter().position(|a| a == flag).unwrap();
        o.args[i + 1].clone()
    }

    #[test]
    fn claude_sin_sesion() {
        let o = orden("claude", None);
        assert_eq!(
            &o.args[..6],
            [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--strict-mcp-config",
                "--mcp-config"
            ]
        );
        assert_eq!(valor(&o, "--model"), "mod");
        assert!(!o.args.contains(&"--resume".to_string()));
        assert_eq!(o.stdin, "hola");
        assert!(o.env.is_empty());
        let mcp: Value = serde_json::from_str(&valor(&o, "--mcp-config")).unwrap();
        let servidores = mcp["mcpServers"].as_object().unwrap();
        assert_eq!(servidores.len(), 1);
        assert_eq!(servidores["argus"]["command"], "/opt/argus-mcp");
        let permitidas = valor(&o, "--allowedTools");
        let lista: Vec<&str> = permitidas.split(',').collect();
        assert_eq!(lista.len(), HERRAMIENTAS_LECTURA.len());
        assert!(lista.iter().all(|t| t.starts_with("mcp__argus__")));
        for e in HERRAMIENTAS_ESCRITURA {
            assert!(!lista.contains(&format!("mcp__argus__{e}").as_str()));
        }
        let prohibidas = valor(&o, "--disallowedTools");
        for t in [
            "Bash",
            "Edit",
            "Write",
            "NotebookEdit",
            "WebFetch",
            "WebSearch",
            "Task",
        ] {
            assert!(prohibidas.split(',').any(|x| x == t), "{t}");
        }
        assert!(o.directorio.starts_with(std::env::temp_dir()));
    }

    #[test]
    fn claude_con_sesion() {
        let o = orden("claude", Some("abc-123"));
        assert_eq!(valor(&o, "--resume"), "abc-123");
    }

    #[test]
    fn opencode_con_y_sin_sesion() {
        let o = orden("opencode", None);
        assert_eq!(o.args, ["run", "--format", "json", "--model", "mod"]);
        let o2 = orden("opencode", Some("ses_1"));
        assert_eq!(valor(&o2, "--session"), "ses_1");
        assert_eq!(o2.stdin, "hola");
        assert_eq!(o2.env.len(), 1);
        let (k, v) = &o2.env[0];
        assert_eq!(k, "OPENCODE_CONFIG_CONTENT");
        let cfg: Value = serde_json::from_str(v).unwrap();
        let mcp = cfg["mcp"].as_object().unwrap();
        assert_eq!(mcp.len(), 1);
        assert_eq!(mcp["argus"]["type"], "local");
        assert_eq!(mcp["argus"]["command"][0], "/opt/argus-mcp");
        for t in ["bash", "edit", "write", "webfetch"] {
            assert_eq!(cfg["tools"][t], false, "{t}");
        }
        for e in HERRAMIENTAS_ESCRITURA {
            assert_eq!(cfg["tools"][format!("argus_{e}")], false, "{e}");
        }
        for l in HERRAMIENTAS_LECTURA {
            assert!(cfg["tools"].get(format!("argus_{l}")).is_none());
        }
    }

    #[test]
    fn sin_token_y_agente_desconocido() {
        for ag in ["claude", "opencode"] {
            let o = orden(ag, Some("s"));
            let todo = format!("{:?}{:?}", o.args, o.env).to_lowercase();
            assert!(!todo.contains("token"));
        }
        assert!(orden_agente(
            "otro",
            Path::new("x"),
            Path::new("y"),
            "m",
            ModoAsistente::Read,
            None,
            ""
        )
        .is_err());
    }
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
