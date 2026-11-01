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

/// «Text file busy» (ETXTBSY): el binario se acaba de escribir (p. ej. opencode
/// actualizándose) y otro proceso aún lo tiene abierto un instante. Es
/// transitorio, así que lanzar el agente se reintenta unas pocas veces.
fn es_texto_ocupado(e: &std::io::Error) -> bool {
    cfg!(unix) && e.raw_os_error() == Some(26)
}

const REINTENTOS_LANZAR: u64 = 5;

/// `cmd.spawn()` con reintentos ante ETXTBSY.
pub(crate) async fn lanzar_agente(
    cmd: &mut tokio::process::Command,
) -> std::io::Result<tokio::process::Child> {
    let mut intento = 0;
    loop {
        match cmd.spawn() {
            Err(e) if es_texto_ocupado(&e) && intento < REINTENTOS_LANZAR => {
                intento += 1;
                tokio::time::sleep(std::time::Duration::from_millis(20 * intento)).await;
            }
            r => return r,
        }
    }
}

/// `cmd.output()` con reintentos ante ETXTBSY (versión síncrona).
fn salida_con_reintentos(cmd: &mut Command) -> std::io::Result<std::process::Output> {
    let mut intento = 0;
    loop {
        match cmd.output() {
            Err(e) if es_texto_ocupado(&e) && intento < REINTENTOS_LANZAR => {
                intento += 1;
                std::thread::sleep(std::time::Duration::from_millis(20 * intento));
            }
            r => return r,
        }
    }
}

fn modelos_opencode(ruta: &Path) -> Vec<String> {
    if let Some(m) = cache_modelos().lock().unwrap().get(ruta) {
        return m.clone();
    }
    let salida = salida_con_reintentos(
        Command::new(ruta)
            .arg("models")
            .stdin(Stdio::null())
            .stderr(Stdio::null()),
    );
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
    /// Escribe el plan estructurado completo (bloque ```plan) a partir del plan del editor.
    Plan,
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
    /// Archivos que se escriben en `directorio` antes de lanzar (nombre, contenido):
    /// opencode v2 ignora OPENCODE_CONFIG_CONTENT y lee su `opencode.json` de ahí.
    pub archivos: Vec<(String, String)>,
}

/// Agente de opencode que ARGUS define para el asistente y el chat.
pub const OPENCODE_AGENTE_ASISTENTE: &str = "argus-asistente";

/// Escribe los archivos de la orden en su carpeta, cada uno de forma atómica
/// (temporal + renombrar): varias respuestas del chat pueden lanzarse a la vez.
pub(crate) fn preparar_directorio(orden: &OrdenAgente) -> Result<(), String> {
    std::fs::create_dir_all(&orden.directorio)
        .map_err(|e| format!("Could not prepare the AI agent folder: {e}"))?;
    for (nombre, contenido) in &orden.archivos {
        let destino = orden.directorio.join(nombre);
        let temporal = orden
            .directorio
            .join(format!(".{nombre}.{}.tmp", std::process::id()));
        std::fs::write(&temporal, contenido)
            .and_then(|_| std::fs::rename(&temporal, &destino))
            .map_err(|e| format!("Could not prepare the AI agent folder: {e}"))?;
    }
    Ok(())
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
    let mut archivos = Vec::new();
    let (args, env) = match agente {
        "claude" => {
            let mcp = serde_json::json!({
                "mcpServers": { "argus": { "command": ruta_mcp_s, "args": [] } }
            });
            let permitidas: Vec<String> = crate::mcp::HERRAMIENTAS_LECTURA
                .iter()
                .map(|h| format!("mcp__argus__{h}"))
                .collect();
            // `--tools ""` quita TODAS las integradas (también las que Claude Code
            // añada en el futuro: Cron, RemoteTrigger, SendMessage…) y
            // `--disable-slash-commands` las skills; quedan solo las MCP permitidas.
            let mut a: Vec<String> = [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--tools",
                "",
                "--disable-slash-commands",
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
            // Además de las integradas, las de escritura de argus-mcp: así el
            // modelo ni siquiera las ve (no solo se le deniegan al usarlas).
            let prohibidas: Vec<String> = CLAUDE_PROHIBIDAS
                .iter()
                .map(|h| h.to_string())
                .chain(
                    crate::mcp::HERRAMIENTAS_ESCRITURA
                        .iter()
                        .map(|h| format!("mcp__argus__{h}")),
                )
                .collect();
            a.push("--disallowedTools".into());
            a.push(prohibidas.join(","));
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
            // Todo denegado y, después, solo las de lectura de ARGUS (en opencode
            // gana la última regla que coincide). Cubre las integradas (read,
            // grep, execute…) y los MCP del usuario, que opencode mezcla.
            let mut permisos = serde_json::Map::new();
            permisos.insert("*".into(), "deny".into());
            for h in crate::mcp::HERRAMIENTAS_LECTURA {
                permisos.insert(format!("argus_{h}"), "allow".into());
            }
            let cfg = serde_json::json!({
                "mcp": { "argus": { "type": "local", "command": [ruta_mcp_s], "enabled": true } },
                "tools": tools,
                "agent": { OPENCODE_AGENTE_ASISTENTE: {
                    "description": "ARGUS assistant: reads the Discord server, never changes it",
                    "mode": "primary",
                    "permission": permisos,
                } },
            });
            archivos.push(("opencode.json".to_string(), cfg.to_string()));
            let mut a: Vec<String> = ["run", "--format", "json", "--agent"]
                .map(String::from)
                .to_vec();
            a.push(OPENCODE_AGENTE_ASISTENTE.into());
            a.push("--model".into());
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
        archivos,
    })
}

/// Evento que el chat recibe como `asistente-evento`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EventoAsistente {
    /// Texto parcial de la respuesta.
    Texto { texto: String },
    /// Herramienta usada: `ok` es `None` mientras se ejecuta.
    Herramienta {
        id: String,
        nombre: String,
        ok: Option<bool>,
    },
    /// Error legible (ya clasificado; en inglés hasta que el comando lo traduce).
    Error { mensaje: String },
    /// Fin de la respuesta, con el id de sesión para reanudar.
    Fin {
        sesion: Option<String>,
        cancelado: bool,
    },
}

pub const EVENTO_ASISTENTE: &str = "asistente-evento";

/// Convierte la salida de error de un agente en un mensaje claro.
pub fn clasificar_error(crudo: &str) -> String {
    let m = crudo.to_lowercase();
    let tiene = |ps: &[&str]| ps.iter().any(|p| m.contains(p));
    if tiene(&[
        "not logged in",
        "please run /login",
        "invalid api key",
        "authentication",
        "unauthorized",
        "not authenticated",
        "credentials",
        "opencode auth login",
    ]) {
        "The AI agent is not signed in. Sign in with its command line and try again".into()
    } else if tiene(&[
        "quota",
        "rate limit",
        "usage limit",
        "credit balance",
        "insufficient",
        "billing",
        "limit reached",
    ]) {
        "The AI agent has no quota left. Wait for it to reset or pick another model".into()
    } else if tiene(&[
        "model not found",
        "modelnotfound",
        "invalid model",
        "unknown model",
        "selected model",
        "model_not_found",
        "no such model",
        "may not exist",
    ]) {
        "The selected model is not valid for this agent".into()
    } else {
        format!("AI agent error: {}", crudo.trim())
    }
}

/// Estado de una lectura de salida de agente (traduce líneas JSON a eventos).
#[derive(Default)]
struct Lector {
    sesion: Option<String>,
    herramientas: HashMap<String, String>,
    error: bool,
}

impl Lector {
    fn linea(&mut self, agente: &str, linea: &str) -> Vec<EventoAsistente> {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(linea) else {
            return Vec::new();
        };
        if agente == "claude" {
            self.claude(&v)
        } else {
            self.opencode(&v)
        }
    }

    fn texto(t: &str) -> Vec<EventoAsistente> {
        if t.is_empty() {
            Vec::new()
        } else {
            vec![EventoAsistente::Texto { texto: t.into() }]
        }
    }

    fn claude(&mut self, v: &serde_json::Value) -> Vec<EventoAsistente> {
        let mut out = Vec::new();
        if let Some(s) = v["session_id"].as_str() {
            self.sesion = Some(s.to_string());
        }
        match v["type"].as_str() {
            Some("assistant") | Some("user") => {
                for c in v["message"]["content"].as_array().into_iter().flatten() {
                    match c["type"].as_str() {
                        Some("text") => out.extend(Self::texto(c["text"].as_str().unwrap_or(""))),
                        Some("tool_use") => {
                            let id = c["id"].as_str().unwrap_or("").to_string();
                            let nombre = c["name"].as_str().unwrap_or("").to_string();
                            let nombre = nombre
                                .strip_prefix("mcp__argus__")
                                .unwrap_or(&nombre)
                                .to_string();
                            self.herramientas.insert(id.clone(), nombre.clone());
                            out.push(EventoAsistente::Herramienta {
                                id,
                                nombre,
                                ok: None,
                            });
                        }
                        Some("tool_result") => {
                            let id = c["tool_use_id"].as_str().unwrap_or("").to_string();
                            let nombre = self.herramientas.get(&id).cloned().unwrap_or_default();
                            let ok = !c["is_error"].as_bool().unwrap_or(false);
                            out.push(EventoAsistente::Herramienta {
                                id,
                                nombre,
                                ok: Some(ok),
                            });
                        }
                        _ => {}
                    }
                }
            }
            Some("result") if v["is_error"].as_bool().unwrap_or(false) => {
                let crudo = v["result"].as_str().unwrap_or("unknown error");
                self.error = true;
                out.push(EventoAsistente::Error {
                    mensaje: clasificar_error(crudo),
                });
            }
            _ => {}
        }
        out
    }

    fn opencode(&mut self, v: &serde_json::Value) -> Vec<EventoAsistente> {
        let mut out = Vec::new();
        if let Some(s) = v["sessionID"].as_str() {
            self.sesion = Some(s.to_string());
        }
        let parte = &v["part"];
        match v["type"].as_str() {
            Some("text") => out.extend(Self::texto(parte["text"].as_str().unwrap_or(""))),
            Some("tool_use") => {
                let id = parte["callID"].as_str().unwrap_or("").to_string();
                let tool = parte["tool"].as_str().unwrap_or("");
                let nombre = tool.strip_prefix("argus_").unwrap_or(tool).to_string();
                let ok = match parte["state"]["status"].as_str() {
                    Some("completed") => Some(true),
                    Some("error") => Some(false),
                    _ => None,
                };
                out.push(EventoAsistente::Herramienta { id, nombre, ok });
            }
            Some("error") => {
                let e = &v["error"];
                let crudo = e["data"]["message"]
                    .as_str()
                    .or_else(|| e["message"].as_str())
                    .or_else(|| e["name"].as_str())
                    .unwrap_or("unknown error");
                self.error = true;
                out.push(EventoAsistente::Error {
                    mensaje: clasificar_error(crudo),
                });
            }
            _ => {}
        }
        out
    }
}

/// Mata el proceso y todo su grupo.
pub(crate) fn matar_grupo(pid: u32) {
    #[cfg(unix)]
    let r = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .stderr(Stdio::null())
        .status();
    #[cfg(windows)]
    let r = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if let Err(e) = r {
        tracing::warn!("no se pudo matar el grupo del agente: {e}");
    }
}

/// Ejecuta la orden, emite los eventos según llegan y devuelve el id de sesión.
/// `cancelar` mata el proceso y su grupo.
pub async fn ejecutar_orden(
    orden: &OrdenAgente,
    agente: &str,
    cancelar: &tokio::sync::Notify,
    mut emitir: impl FnMut(EventoAsistente),
) -> Result<Option<String>, String> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    preparar_directorio(orden)?;
    let mut cmd = tokio::process::Command::new(&orden.programa);
    cmd.args(&orden.args)
        .envs(orden.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&orden.directorio)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut hijo = lanzar_agente(&mut cmd)
        .await
        .map_err(|e| format!("Could not start the AI agent: {e}"))?;
    let pid = hijo.id();
    if let Some(mut stdin) = hijo.stdin.take() {
        let texto = orden.stdin.clone();
        tokio::spawn(async move {
            let _ = stdin.write_all(texto.as_bytes()).await;
        });
    }
    let mut stderr = hijo.stderr.take().expect("stderr piped");
    let errores = tokio::spawn(async move {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s).await;
        s
    });
    let mut lineas = BufReader::new(hijo.stdout.take().expect("stdout piped")).lines();
    let mut lector = Lector::default();
    let mut cancelado = false;
    loop {
        tokio::select! {
            l = lineas.next_line() => match l {
                Ok(Some(l)) => {
                    for e in lector.linea(agente, &l) {
                        emitir(e);
                    }
                }
                _ => break,
            },
            _ = cancelar.notified() => {
                cancelado = true;
                if let Some(p) = pid {
                    matar_grupo(p);
                }
                let _ = hijo.start_kill();
                break;
            }
        }
    }
    let estado = hijo.wait().await;
    let stderr = errores.await.unwrap_or_default();
    if !cancelado && !lector.error {
        let fallo = !matches!(&estado, Ok(s) if s.success());
        if fallo && !stderr.trim().is_empty() {
            emitir(EventoAsistente::Error {
                mensaje: clasificar_error(&stderr),
            });
            lector.error = true;
        } else if fallo {
            emitir(EventoAsistente::Error {
                mensaje: "The AI agent stopped unexpectedly".into(),
            });
            lector.error = true;
        }
    }
    emitir(EventoAsistente::Fin {
        sesion: lector.sesion.clone(),
        cancelado,
    });
    Ok(lector.sesion)
}

/// Lanza el agente constructor sin interfaz y devuelve todo el texto de su
/// respuesta. Pasado `limite` mata el proceso y su grupo. No usa el cerrojo de
/// la app: el que limita los constructores es el del llamador.
pub async fn texto_del_constructor(
    orden: &OrdenAgente,
    agente: &str,
    limite: std::time::Duration,
) -> Result<String, String> {
    let cancelar = tokio::sync::Notify::new();
    let mut texto = String::new();
    let mut error: Option<String> = None;
    {
        let ejecucion = ejecutar_orden(orden, agente, &cancelar, |e| match e {
            EventoAsistente::Texto { texto: t } => {
                texto.push_str(&t);
                texto.push('\n');
            }
            EventoAsistente::Error { mensaje } => error = Some(mensaje),
            _ => {}
        });
        tokio::pin!(ejecucion);
        tokio::select! {
            r = &mut ejecucion => { r?; }
            _ = tokio::time::sleep(limite) => {
                cancelar.notify_one();
                let _ = ejecucion.await;
                return Err("The AI agent took too long to reply".into());
            }
        }
    }
    if let Some(e) = error {
        return Err(e);
    }
    if texto.trim().is_empty() {
        return Err("The AI agent returned an empty reply".into());
    }
    Ok(texto)
}

/// Ejecución en curso: sirve de cerrojo (un agente a la vez) y de cancelación.
fn en_curso() -> &'static Mutex<Option<std::sync::Arc<tokio::sync::Notify>>> {
    static EN_CURSO: OnceLock<Mutex<Option<std::sync::Arc<tokio::sync::Notify>>>> = OnceLock::new();
    EN_CURSO.get_or_init(|| Mutex::new(None))
}

/// Cancela el agente en curso (si lo hay): mata su proceso y su grupo.
pub fn cancelar_agente() {
    if let Some(n) = en_curso().lock().unwrap().as_ref() {
        n.notify_one();
    }
}

struct Turno;

impl Turno {
    fn tomar(n: std::sync::Arc<tokio::sync::Notify>) -> Result<Self, String> {
        let mut g = en_curso().lock().unwrap();
        if g.is_some() {
            return Err("An AI agent is already running".into());
        }
        *g = Some(n);
        Ok(Turno)
    }
}

impl Drop for Turno {
    fn drop(&mut self) {
        *en_curso().lock().unwrap() = None;
    }
}

/// Ejemplo de propuesta del prompt: rol + categoría + canal con tema + permiso por nombre.
const EJEMPLO_PROPUESTA: &str = r##"{
  "resumen": "A moderators role and a private staff category with one channel",
  "acciones": [
    {
      "herramienta": "crear_rol",
      "args": { "nombre": "Moderators", "color": "#3498DB", "mencionable": true, "permisos": ["ManageMessages", "KickMembers"] },
      "descripcion": "Create the Moderators role"
    },
    {
      "herramienta": "crear_canal",
      "args": { "nombre": "Staff", "tipo": "categoria" },
      "descripcion": "Create the Staff category"
    },
    {
      "herramienta": "crear_canal",
      "args": { "nombre": "mod-chat", "tipo": "texto", "categoria_id": "Staff", "tema": "Private chat for the moderation team" },
      "descripcion": "Create #mod-chat inside Staff"
    },
    {
      "herramienta": "permiso_canal",
      "args": { "channel_id": "mod-chat", "target_id": "Moderators", "tipo": "rol", "permitir": ["ViewChannel", "SendMessages"] },
      "descripcion": "Let Moderators see and write in #mod-chat"
    }
  ]
}"##;

/// Manual resumido de la sintaxis del plan estructurado (lo que entiende `plan_estructurado`).
pub const MANUAL_PLAN: &str = r##"### Plan syntax (one item per line; anything else is free text that ARGUS will not build)
- `## Role: Name | color: #RRGGBB | permissions: ManageMessages, KickMembers | hoist | mentionable: no`
  (roles are created in the order written, highest first; permission names are Discord's)
- `## Category: Name | description: "text" | view: Role1, Role2 | write: Role | read only: Role | hide from: @everyone`
- `- text: name | description: "text" | view: ... | write: ... | read only: ... | hide from: ...`
  (channel types: text, voice, announcements, forum; channels belong to the last category above them,
  so write them right after it)
- Ranges: `- voice: Table-{01..05}` creates Table-01 … Table-05; `{n}` in a quoted description is the number.
- Description: quoted text, or `description: generate: short hint` so ARGUS asks an AI to write it later.
  A category description is the default for its channels.
- Variables: `@var name = value`, used as `$name` in `##` and `-` lines.
- Comments: `// comment`. Use `|` only to separate attributes (inside quotes it is literal).
- The names in view/write/read only/hide from are role names; `@everyone` is allowed."##;

/// Máximo de elementos por lista en los resúmenes compactos del prompt.
const MAX_RESUMEN: usize = 40;
/// Máximo de caracteres de cada resumen compacto.
const MAX_RESUMEN_CHARS: usize = 3000;

fn recortar_texto(mut s: String, max: usize) -> String {
    if s.chars().count() > max {
        s = s.chars().take(max).collect();
        s.push('…');
    }
    s
}

fn lista_recortada<'a>(nombres: impl Iterator<Item = &'a str>) -> String {
    let todos: Vec<&str> = nombres.collect();
    let mut s = todos
        .iter()
        .take(MAX_RESUMEN)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if todos.len() > MAX_RESUMEN {
        s.push_str(&format!(", +{} more", todos.len() - MAX_RESUMEN));
    }
    s
}

/// Resumen compacto del mapa: categorías con sus canales y roles (recortado).
pub fn resumen_mapa(mapa: &crate::state::ServerMap) -> String {
    let mut ordenados: Vec<&crate::state::Channel> = mapa.channels.iter().collect();
    ordenados.sort_by_key(|c| c.position);
    let categorias: Vec<&crate::state::Channel> = ordenados
        .iter()
        .copied()
        .filter(|c| c.channel_type == 4)
        .collect();
    let mut s = String::new();
    for c in categorias.iter().take(MAX_RESUMEN) {
        let hijos = ordenados
            .iter()
            .filter(|h| h.channel_type != 4 && h.parent_id.as_deref() == Some(c.id.as_str()))
            .map(|h| h.name.as_str());
        s.push_str(&format!("- {}: {}\n", c.name, lista_recortada(hijos)));
    }
    if categorias.len() > MAX_RESUMEN {
        s.push_str(&format!(
            "- (+{} more categories)\n",
            categorias.len() - MAX_RESUMEN
        ));
    }
    let sueltos = ordenados.iter().filter(|c| {
        c.channel_type != 4
            && c.parent_id
                .as_deref()
                .is_none_or(|p| !categorias.iter().any(|k| k.id == p))
    });
    let sueltos = lista_recortada(sueltos.map(|c| c.name.as_str()));
    if !sueltos.is_empty() {
        s.push_str(&format!("- (no category): {sueltos}\n"));
    }
    let mut roles: Vec<&crate::state::Role> = mapa.roles.iter().collect();
    roles.sort_by_key(|r| std::cmp::Reverse(r.position));
    s.push_str(&format!(
        "Roles: {}\n",
        lista_recortada(roles.iter().map(|r| r.name.as_str()))
    ));
    recortar_texto(s, MAX_RESUMEN_CHARS)
}

/// Resumen compacto del plan ya analizado (roles, categorías con canales y texto libre).
pub fn resumen_plan(plan: &crate::plan_estructurado::PlanEstructurado) -> String {
    let mut s = String::new();
    if !plan.roles.is_empty() {
        s.push_str(&format!(
            "Roles: {}\n",
            lista_recortada(plan.roles.iter().map(|r| r.nombre.as_str()))
        ));
    }
    for c in plan.categorias.iter().take(MAX_RESUMEN) {
        s.push_str(&format!(
            "- {}: {}\n",
            c.nombre,
            lista_recortada(c.canales.iter().map(|k| k.nombre.as_str()))
        ));
    }
    if !plan.texto_libre.is_empty() {
        s.push_str("Free text (not structured):\n");
        for l in plan.texto_libre.iter().take(MAX_RESUMEN) {
            s.push_str(&format!("  {}\n", l.trim()));
        }
    }
    recortar_texto(s, MAX_RESUMEN_CHARS)
}

/// Tipo de conversación del asistente: construir el servidor o moderarlo.
/// Cada tipo tiene su propia conversación y sesión por servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoAsistente {
    Construccion,
    Moderacion,
}

impl TipoAsistente {
    /// Valor guardado en la BD.
    pub fn as_str(self) -> &'static str {
        match self {
            TipoAsistente::Construccion => "construccion",
            TipoAsistente::Moderacion => "moderacion",
        }
    }

    /// `None` (o vacío) = construcción, para compatibilidad con el frontend previo.
    pub fn desde(s: Option<&str>) -> Result<Self, String> {
        match s.map(str::trim) {
            None | Some("") | Some("construccion") => Ok(TipoAsistente::Construccion),
            Some("moderacion") => Ok(TipoAsistente::Moderacion),
            Some(otro) => Err(format!("Unknown assistant type: {otro}")),
        }
    }
}

/// Mensajes por canal que lee el asistente de moderación si no se indica otra cosa.
pub const LIMITE_POR_CANAL: u32 = 100;

/// Acciones que puede proponer el asistente de moderación (y solo esas).
pub const ACCIONES_MODERACION: &[&str] = &[
    "avisar",
    "silenciar",
    "expulsar",
    "banear",
    "desbanear",
    "borrar_mensajes",
    "dar_rol",
    "quitar_rol",
    "cambiar_apodo",
];

/// Error si la propuesta contiene alguna acción que no es de moderación.
pub fn comprobar_acciones_moderacion(p: &crate::propuesta::Propuesta) -> Result<(), String> {
    match p
        .acciones
        .iter()
        .find(|a| !ACCIONES_MODERACION.contains(&a.herramienta.as_str()))
    {
        Some(a) => Err(format!(
            "The moderation assistant can only propose moderation actions, not {herramienta}",
            herramienta = a.herramienta
        )),
        None => Ok(()),
    }
}

/// Prompt según el tipo de conversación. El de construcción es
/// `prompt_asistente_con_mapa`; el de moderación ignora el plan y el mapa.
#[allow(clippy::too_many_arguments)]
pub fn prompt_asistente_tipo(
    tipo: TipoAsistente,
    modo: ModoAsistente,
    servidor: (&str, &str),
    plan: &str,
    contexto: &str,
    idioma: &str,
    mensaje: &str,
    mapa: &str,
    limite_por_canal: u32,
) -> String {
    match tipo {
        TipoAsistente::Construccion => {
            prompt_asistente_con_mapa(modo, servidor, plan, contexto, idioma, mensaje, mapa)
        }
        TipoAsistente::Moderacion => {
            prompt_moderacion(modo, servidor, contexto, idioma, mensaje, limite_por_canal)
        }
    }
}

/// Prompt del asistente de moderación: miembros, mensajes y sanciones.
pub fn prompt_moderacion(
    modo: ModoAsistente,
    servidor: (&str, &str),
    contexto: &str,
    idioma: &str,
    mensaje: &str,
    limite_por_canal: u32,
) -> String {
    let (id, nombre) = servidor;
    let lengua = if crate::i18n::normalizar(idioma) == "es" {
        "Spanish"
    } else {
        "English"
    };
    let contexto = if contexto.trim().is_empty() {
        "(The user has not written a context for this server yet.)"
    } else {
        contexto
    };
    let limite = limite_por_canal.clamp(1, 500);
    let mut p = format!(
        "You are the moderation assistant built into ARGUS, a desktop app to administer Discord \
servers with the user's own bot. You reach the server through the ARGUS MCP tools. You focus on \
members, their messages and sanctions; you do not build or restructure the server.\n\n\
## Active server\n\
The active server is \"{nombre}\" (id {id}). Pass guild_id \"{id}\" in every tool call.\n\n\
## Rules\n\
- You cannot modify anything: you only have read tools. Never try to sanction anyone yourself.\n\
- Always reply in {lengua}, the language of the ARGUS interface.\n\
- Tools to use: `buscar_mensajes` (find who said something), `leer_mensajes` (read a channel), \
`listar_avisos` (warnings a member already has), plus the member and role read tools.\n\
- Read at most {limite} messages per channel (`limite_por_canal` = {limite}) unless the user asks \
for more.\n\
- Base every conclusion on evidence. For each relevant message cite the author (name and id), \
the date and the message link. If the evidence is not enough, say so clearly instead of guessing.\n\n\
## Server context\n<context>\n{contexto}\n</context>\n"
    );
    if modo == ModoAsistente::Execute {
        p.push_str(&format!(
            "\n## Execute mode\n\
Investigate with your read tools, then explain briefly what you found and END your answer with \
ONE ```json block containing a `Propuesta`. ARGUS validates it, shows it to the user and, only if \
the user approves, executes exactly those actions.\n\n\
Format: {{\"resumen\": string, \"acciones\": [{{\"herramienta\": string, \"args\": object, \
\"descripcion\": string}}]}}\n\
- `herramienta` must be one of these moderation tools and nothing else: {acciones}. Never propose \
changes to the structure of the server (channels, categories, roles, permissions, settings).\n\
- Do not include `guild_id` and never use `elegir_servidor`: the active server is used.\n\
- In `descripcion` name the member and give the reason, citing the message when there is one.\n\
- Only propose what the user asked for or what the evidence clearly supports.\n\
- Use exactly these arguments for each tool (required ones must be present):\n{firmas}\n\n\
Example of a valid answer block (warn a member and delete a message):\n```json\n{ejemplo}\n```\n",
            acciones = ACCIONES_MODERACION.join(", "),
            firmas = crate::mcp::firmas_de(ACCIONES_MODERACION),
            ejemplo = EJEMPLO_MODERACION,
        ));
    } else {
        p.push_str(
            "\n## Read mode\nAnswer with an analysis of the members and messages the user asks \
about, citing the evidence (author, date and link of each relevant message) and saying when there \
is not enough proof. Do not produce a JSON proposal.\n",
        );
    }
    p.push_str(&format!("\n## User message\n{mensaje}\n"));
    p
}

const EJEMPLO_MODERACION: &str = r#"{"resumen":"Warn Ana and delete her spam","acciones":[{"herramienta":"avisar","args":{"user_id":"Ana","motivo":"Spam in #general","mensaje_id":"123456789012345678"},"descripcion":"Warn Ana: spam in #general"},{"herramienta":"borrar_mensajes","args":{"channel_id":"987654321098765432","message_ids":["123456789012345678","123456789012345679"]},"descripcion":"Delete Ana's two spam messages"}]}"#;

/// Prompt que se le da al agente. `servidor` es (id, nombre) del servidor activo.
pub fn prompt_asistente(
    modo: ModoAsistente,
    servidor: (&str, &str),
    plan: &str,
    contexto: &str,
    idioma: &str,
    mensaje: &str,
) -> String {
    prompt_asistente_con_mapa(modo, servidor, plan, contexto, idioma, mensaje, "")
}

/// Como `prompt_asistente`; en modo Execute añade el plan ya analizado y, si
/// `mapa` no está vacío, el resumen compacto del mapa del servidor.
pub fn prompt_asistente_con_mapa(
    modo: ModoAsistente,
    servidor: (&str, &str),
    plan: &str,
    contexto: &str,
    idioma: &str,
    mensaje: &str,
    mapa: &str,
) -> String {
    let (id, nombre) = servidor;
    let contexto = if contexto.trim().is_empty() {
        "(The user has not written a context for this server yet.)"
    } else {
        contexto
    };
    let lengua = if crate::i18n::normalizar(idioma) == "es" {
        "Spanish"
    } else {
        "English"
    };
    let plan = if plan.trim().is_empty() {
        "(The user has not written a plan for this server yet.)"
    } else {
        plan
    };
    let mut p = format!(
        "You are the assistant built into ARGUS, a desktop app to administer Discord servers \
with the user's own bot. You reach the server through the ARGUS MCP tools.\n\n\
## Active server\n\
The active server is \"{nombre}\" (id {id}). Pass guild_id \"{id}\" in every tool \
call.\n\n\
## Rules\n\
- You cannot modify anything: you only have read tools (list channels, roles, members, \
permissions, settings, audit log, messages...). Never try to change the server yourself.\n\
- Always reply in {lengua}, the language of the ARGUS interface.\n\n\
## Server context (theme, tone, style; follow it in names and descriptions)\n\
<context>\n{contexto}\n</context>\n\n\
## Server plan (written by the user)\n\
<plan>\n{plan}\n</plan>\n"
    );
    if modo == ModoAsistente::Plan {
        if !mapa.trim().is_empty() {
            p.push_str(&format!(
                "\n## Server map (compact snapshot of what exists now)\n<server_map>\n{}\n</server_map>\n",
                mapa.trim_end()
            ));
        }
        p.push_str(&format!(
            "\n## Plan mode\n\
Your job is to write the user's server plan. The text inside <plan> above is the CURRENT \
content of the user's editor (they may have edited it by hand): start from it, keep everything \
the user did not ask you to change, and apply the request. Use your read tools only if you need \
details of the real server that are missing from the map. Do not propose actions.\n\n\
Explain briefly what you changed, then END your answer with ONE ```plan block containing the \
COMPLETE new plan (the whole document, not a fragment or a diff). ARGUS shows the user the \
changes against their editor and builds from the plan they accept.\n\n\
{manual}\n",
            manual = MANUAL_PLAN,
        ));
    } else if modo == ModoAsistente::Execute {
        let analizado = crate::plan_estructurado::analizar(plan);
        if analizado.tiene_estructura() || !analizado.texto_libre.is_empty() {
            p.push_str(&format!(
                "\n## Parsed plan (ARGUS already analysed the plan text; no need to re-read it)\n\
<parsed_plan>\n{}</parsed_plan>\n",
                resumen_plan(&analizado)
            ));
        }
        if !mapa.trim().is_empty() {
            p.push_str(&format!(
                "\n## Server map (compact snapshot of what exists now; use your read tools only \
for details missing here)\n<server_map>\n{}</server_map>\n",
                mapa.trim_end().to_string() + "\n"
            ));
        }
        p.push_str(&format!(
            "\n## Execute mode\n\
Study the server with your read tools, compare it with the plan and work out what has to \
change. Explain it briefly and END your answer with ONE ```json block containing a \
`Propuesta`. ARGUS will validate it, show it to the user and, only if the user approves, \
execute exactly those actions in a safe order (roles, categories, channels, channel \
permissions, member roles).\n\n\
Format: {{\"resumen\": string, \"acciones\": [{{\"herramienta\": string, \"args\": object, \
\"descripcion\": string}}]}}\n\
- `herramienta` is the name of a write tool: {escritura}.\n\
- Do not include `guild_id` and never use `elegir_servidor`: the active server is used.\n\
- Where an id of a role, channel or category goes (`categoria_id`, `channel_id`, `role_id`, \
`target_id` with tipo \"rol\"), you may write its name instead; it can be something an earlier \
action creates.\n\
- Permission names are Discord's (e.g. \"ViewChannel\", \"SendMessages\", \
\"ReadMessageHistory\", \"Connect\", \"Speak\", \"ManageMessages\", \"ModerateMembers\", \
\"Administrator\"). Create roles in the order the plan lists them (highest first); do not use \
`ordenar_roles` or `ordenar_canales` unless the plan asks to reorder something.\n\
- Only propose what is needed: do not recreate what already exists, and never delete, move \
or rename anything the plan does not explicitly ask for.\n\
- Use exactly these arguments for each tool (required ones must be present):\n{firmas}\n\n\
Example of a valid answer block (a role, a category, a channel with topic and a permission by \
name):\n```json\n{ejemplo}\n```\n",
            escritura = crate::mcp::HERRAMIENTAS_ESCRITURA.join(", "),
            firmas = crate::mcp::firmas_escritura(),
            ejemplo = EJEMPLO_PROPUESTA,
        ));
    } else {
        p.push_str(
            "\n## Read mode\nConverse with the user and answer questions about the server and \
the plan. Do not produce a JSON proposal.\n",
        );
    }
    p.push_str(&format!("\n## User message\n{mensaje}\n"));
    p
}

/// Extrae la `Propuesta` del último bloque ```json de la respuesta del agente.
pub fn extraer_propuesta(respuesta: &str) -> Result<crate::propuesta::Propuesta, String> {
    let minus = respuesta.to_ascii_lowercase();
    let inicio = minus
        .rfind("```json")
        .ok_or("The assistant did not include a proposal in its answer")?;
    let resto = &respuesta[inicio + "```json".len()..];
    let cuerpo = resto.find("```").map_or(resto, |i| &resto[..i]);
    let propuesta: crate::propuesta::Propuesta = serde_json::from_str(cuerpo.trim())
        .map_err(|e| format!("The assistant's proposal is not valid JSON: {e}"))?;
    if propuesta.acciones.is_empty() {
        return Err("The assistant's proposal has no actions".into());
    }
    Ok(propuesta)
}

/// Extrae el plan completo del último bloque ```plan de la respuesta del agente.
pub fn extraer_plan(respuesta: &str) -> Result<String, String> {
    let minus = respuesta.to_ascii_lowercase();
    let inicio = minus
        .rfind("```plan")
        .ok_or("The assistant did not include a plan in its answer")?;
    let resto = &respuesta[inicio + "```plan".len()..];
    // Lo que queda de la línea de apertura se descarta.
    let resto = resto.split_once('\n').map_or("", |(_, r)| r);
    let cuerpo = resto.find("```").map_or(resto, |i| &resto[..i]);
    let cuerpo = cuerpo.trim_matches('\n').trim_end();
    if cuerpo.trim().is_empty() {
        return Err("The assistant's plan is empty".into());
    }
    Ok(cuerpo.to_string())
}

/// Una línea de la vista de cambios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineaDiff {
    /// `same`, `added` o `removed`.
    pub tipo: &'static str,
    pub texto: String,
}

/// Diferencias por líneas (LCS) entre `antes` y `despues`.
pub fn diferencias_lineas(antes: &str, despues: &str) -> Vec<LineaDiff> {
    let a: Vec<&str> = antes.lines().collect();
    let b: Vec<&str> = despues.lines().collect();
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf]
    {
        suf += 1;
    }
    let ma = &a[pre..a.len() - suf];
    let mb = &b[pre..b.len() - suf];
    let (n, m) = (ma.len(), mb.len());
    // tabla[i][j] = longitud de la subsecuencia común de ma[i..] y mb[j..]
    let mut tabla = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            tabla[i][j] = if ma[i] == mb[j] {
                tabla[i + 1][j + 1] + 1
            } else {
                tabla[i + 1][j].max(tabla[i][j + 1])
            };
        }
    }
    let mk = |tipo, t: &str| LineaDiff {
        tipo,
        texto: t.to_string(),
    };
    let mut out: Vec<LineaDiff> = a[..pre].iter().map(|t| mk("same", t)).collect();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && ma[i] == mb[j] {
            out.push(mk("same", ma[i]));
            i += 1;
            j += 1;
        } else if i < n && (j >= m || tabla[i + 1][j] >= tabla[i][j + 1]) {
            out.push(mk("removed", ma[i]));
            i += 1;
        } else {
            out.push(mk("added", mb[j]));
            j += 1;
        }
    }
    out.extend(a[a.len() - suf..].iter().map(|t| mk("same", t)));
    out
}

/// Vista de cambios del plan propuesto frente al del editor.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CambiosPlan {
    pub lineas: Vec<LineaDiff>,
    pub anadidas: usize,
    pub quitadas: usize,
    /// Avisos de `analizar` sobre la versión propuesta («línea: mensaje»).
    pub avisos: Vec<String>,
}

pub fn cambios_de_plan(actual: &str, propuesto: &str, idioma: &str) -> CambiosPlan {
    let lineas = diferencias_lineas(actual, propuesto);
    let cuenta = |t: &str| lineas.iter().filter(|l| l.tipo == t).count();
    let analizado = crate::plan_estructurado::analizar(propuesto);
    CambiosPlan {
        anadidas: cuenta("added"),
        quitadas: cuenta("removed"),
        avisos: analizado
            .avisos
            .iter()
            .map(|a| format!("{}: {}", a.linea, crate::i18n::tr(idioma, &a.mensaje)))
            .collect(),
        lineas,
    }
}

#[tauri::command]
pub fn comparar_plan(
    state: tauri::State<'_, AppState>,
    actual: String,
    propuesto: String,
) -> CambiosPlan {
    cambios_de_plan(&actual, &propuesto, &crate::i18n::idioma(&state))
}

pub const EVENTO_PLAN: &str = "asistente-plan";

/// Plan propuesto por la IA en modo Plan (se emite como `asistente-plan`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventoPlan {
    pub plan: String,
}

/// Propuesta ya validada que se emite como `asistente-propuesta`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventoPropuesta {
    pub propuesta: crate::propuesta::Propuesta,
    pub validacion: crate::propuesta::Validacion,
}

pub const EVENTO_PROPUESTA: &str = "asistente-propuesta";

/// Extrae y valida la propuesta de `respuesta`. `validar` es `validar_propuesta`.
pub fn propuesta_de_respuesta(
    respuesta: &str,
    validar: impl FnOnce(&crate::propuesta::Propuesta) -> crate::propuesta::Validacion,
) -> Result<EventoPropuesta, String> {
    let propuesta = extraer_propuesta(respuesta)?;
    let validacion = validar(&propuesta);
    if let Some(e) = validacion.errores.first() {
        let mas = validacion.errores.len() - 1;
        let detalle = format!("{}: {}", e.herramienta, e.mensaje);
        return Err(if mas == 0 {
            format!("The assistant's proposal is not valid: {detalle}")
        } else {
            format!("The assistant's proposal is not valid: {detalle} (and {mas} more)")
        });
    }
    Ok(EventoPropuesta {
        propuesta,
        validacion,
    })
}

/// Lanza el agente con `mensaje` y transmite la respuesta por `asistente-evento`.
/// En modo Execute emite además `asistente-propuesta` (o un error legible).
/// Devuelve el id de sesión. Solo un agente a la vez.
#[tauri::command]
pub async fn enviar_al_asistente(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mensaje: String,
    agente: String,
    modelo: String,
    modo: String,
    plan: Option<String>,
    tipo: Option<String>,
    limite_por_canal: Option<u32>,
) -> Result<Option<String>, String> {
    use tauri::Emitter;
    let r = async {
        let tipo = TipoAsistente::desde(tipo.as_deref())?;
        let modo = match modo.as_str() {
            "read" | "Read" => ModoAsistente::Read,
            "plan" | "Plan" => ModoAsistente::Plan,
            "execute" | "Execute" => ModoAsistente::Execute,
            _ => return Err(format!("Unknown assistant mode: {modo}")),
        };
        if tipo == TipoAsistente::Moderacion && modo == ModoAsistente::Plan {
            return Err("The moderation assistant has no Plan mode".to_string());
        }
        let gid = state
            .get_active_guild()
            .ok_or("No active server: choose one first")?;
        let nombre_servidor = {
            let mapas = state.server_maps.lock().unwrap();
            mapas.get(&gid).map(|m| m.guild.name.clone())
        }
        .unwrap_or_else(|| gid.clone());
        let idioma = crate::i18n::idioma(&state);
        let (bot_id, pool) = bot_y_pool(&app, &state).await?;
        let plan = match plan {
            Some(p) => p,
            None => crate::db::leer_plan(&pool, &bot_id, &gid)
                .await
                .map_err(|e| e.to_string())?,
        };
        let contexto = crate::db::leer_contexto(&pool, &bot_id, &gid)
            .await
            .map_err(|e| e.to_string())?;
        // Reanuda la sesión guardada; con otro agente se descarta.
        let previa =
            crate::db::leer_conversacion(&pool, &bot_id, &gid, tipo.as_str(), Some(&agente))
                .await
                .map_err(|e| e.to_string())?;
        let sesion = previa.as_ref().and_then(|c| c.session_id.clone());
        let mapa = {
            let mapas = state.server_maps.lock().unwrap();
            mapas.get(&gid).map(resumen_mapa).unwrap_or_default()
        };
        let prompt = prompt_asistente_tipo(
            tipo,
            modo,
            (&gid, &nombre_servidor),
            &plan,
            &contexto,
            &idioma,
            &mensaje,
            &mapa,
            limite_por_canal.unwrap_or(LIMITE_POR_CANAL),
        );
        let ruta = buscar_binario(&agente, &carpetas_busqueda())
            .ok_or_else(|| format!("{agente} is not installed"))?;
        let mcp = crate::commands::ruta_del_mcp()?;
        let orden = orden_agente(
            &agente,
            &ruta,
            Path::new(&mcp),
            &modelo,
            modo,
            sesion.as_deref(),
            &prompt,
        )?;
        let cancelar = std::sync::Arc::new(tokio::sync::Notify::new());
        let _turno = Turno::tomar(cancelar.clone())?;
        let emitir = |e: EventoAsistente| {
            let mut e = e;
            if let EventoAsistente::Error { mensaje } = &mut e {
                *mensaje = crate::i18n::tr(&idioma, mensaje);
            }
            if let Err(err) = app.emit(EVENTO_ASISTENTE, e) {
                tracing::warn!("no se pudo emitir {EVENTO_ASISTENTE}: {err}");
            }
        };
        let mut respuesta = String::new();
        let mut con_error = false;
        let nueva_sesion = ejecutar_orden(&orden, &agente, &cancelar, |e| {
            match &e {
                EventoAsistente::Texto { texto } => {
                    respuesta.push_str(texto);
                    respuesta.push('\n');
                }
                EventoAsistente::Error { .. } => con_error = true,
                EventoAsistente::Fin { cancelado, .. }
                    if modo == ModoAsistente::Execute && !cancelado && !con_error =>
                {
                    // La propuesta (o su error) sale antes del fin de la respuesta.
                    match propuesta_de_respuesta(&respuesta, |p| {
                        let mut v = crate::propuesta::validar_en_estado(&state, p);
                        if tipo == TipoAsistente::Moderacion {
                            if let Err(mensaje) = comprobar_acciones_moderacion(p) {
                                v.errores.push(crate::propuesta::ErrorValidacion {
                                    indice: 0,
                                    herramienta: "moderacion".into(),
                                    mensaje,
                                });
                            }
                        }
                        v
                    }) {
                        Ok(ev) => {
                            if let Err(err) = app.emit(EVENTO_PROPUESTA, ev) {
                                tracing::warn!("no se pudo emitir {EVENTO_PROPUESTA}: {err}");
                            }
                        }
                        Err(mensaje) => emitir(EventoAsistente::Error { mensaje }),
                    }
                }
                EventoAsistente::Fin { cancelado, .. }
                    if modo == ModoAsistente::Plan && !cancelado && !con_error =>
                {
                    match extraer_plan(&respuesta) {
                        Ok(plan) => {
                            if let Err(err) = app.emit(EVENTO_PLAN, EventoPlan { plan }) {
                                tracing::warn!("no se pudo emitir {EVENTO_PLAN}: {err}");
                            }
                        }
                        Err(mensaje) => emitir(EventoAsistente::Error { mensaje }),
                    }
                }
                _ => {}
            }
            emitir(e);
        })
        .await?;
        // Guarda el id de sesión para reanudarla en el siguiente turno.
        if let Some(id) = &nueva_sesion {
            let mut c = previa.unwrap_or_else(|| crate::db::ConversacionAsistente {
                agente: agente.clone(),
                modelo: modelo.clone(),
                session_id: None,
                mensajes: serde_json::json!([]),
            });
            c.session_id = Some(id.clone());
            c.modelo = modelo.clone();
            crate::db::guardar_conversacion(&pool, &bot_id, &gid, tipo.as_str(), &c)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(nueva_sesion)
    }
    .await;
    localizar_error(&state, r)
}

/// Qué estructura tiene un plan (para decidir si se ofrece «Build from plan»).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResumenPlan {
    pub roles: usize,
    pub categorias: usize,
    pub canales: usize,
    pub texto_libre: usize,
    pub tiene_estructura: bool,
    /// Avisos del análisis como «línea: motivo», ya traducidos.
    pub avisos: Vec<String>,
}

#[tauri::command]
pub fn resumir_plan(state: tauri::State<'_, AppState>, plan: String) -> ResumenPlan {
    let idioma = crate::i18n::idioma(&state);
    let p = crate::plan_estructurado::analizar(&plan);
    ResumenPlan {
        avisos: p
            .avisos
            .iter()
            .map(|a| format!("{}: {}", a.linea, crate::i18n::tr(&idioma, &a.mensaje)))
            .collect(),
        roles: p.roles.len(),
        categorias: p.categorias.len(),
        canales: p.categorias.iter().map(|c| c.canales.len()).sum(),
        texto_libre: p.texto_libre.len(),
        tiene_estructura: p.tiene_estructura(),
    }
}

/// Resultado de «Build from plan»: la propuesta compilada y de dónde salió.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResultadoConstruccion {
    pub propuesta: crate::propuesta::Propuesta,
    pub validacion: crate::propuesta::Validacion,
    /// Acciones que salieron de la estructura del plan.
    pub desde_estructura: usize,
    /// Descripciones de canal que escribió la IA.
    pub descripciones_ia: usize,
    /// Líneas de texto libre del plan (no estructuradas).
    pub texto_libre: usize,
    pub avisos: Vec<String>,
}

/// Compila la estructura del plan (en local), escribe las descripciones
/// pendientes con una llamada barata y devuelve la propuesta para aprobarla
/// con `aplicar_propuesta`.
#[tauri::command]
pub async fn construir_desde_plan(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    agente: String,
    modelo: String,
    plan: Option<String>,
) -> Result<ResultadoConstruccion, String> {
    let r = async {
        let gid = state
            .get_active_guild()
            .ok_or("No active server: choose one first")?;
        let idioma = crate::i18n::idioma(&state);
        let (bot_id, pool) = bot_y_pool(&app, &state).await?;
        let plan = match plan {
            Some(p) => p,
            None => crate::db::leer_plan(&pool, &bot_id, &gid)
                .await
                .map_err(|e| e.to_string())?,
        };
        let contexto = crate::db::leer_contexto(&pool, &bot_id, &gid)
            .await
            .map_err(|e| e.to_string())?;
        let analizado = crate::plan_estructurado::analizar(&plan);
        if !analizado.tiene_estructura() {
            return Err("The plan has no structure to build from".to_string());
        }
        let (roles, canales) = {
            let mapas = state.server_maps.lock().unwrap();
            mapas
                .get(&gid)
                .map(|m| (m.roles.clone(), m.channels.clone()))
                .unwrap_or_default()
        };
        let mut comp = crate::plan_compilar::compilar(
            &analizado,
            &crate::plan_compilar::Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        let desde_estructura = comp.propuesta.acciones.len();
        let mut descripciones_ia = 0;
        if !comp.pendientes.is_empty() {
            let ruta = buscar_binario(&agente, &carpetas_busqueda())
                .ok_or_else(|| format!("{agente} is not installed"))?;
            let cancelar = std::sync::Arc::new(tokio::sync::Notify::new());
            let _turno = Turno::tomar(cancelar)?;
            descripciones_ia = crate::plan_descripciones::escribir_descripciones(
                &mut comp,
                &contexto,
                &idioma,
                &agente,
                &ruta,
                &modelo,
                std::time::Duration::from_secs(300),
            )
            .await;
        }
        let validacion = crate::propuesta::validar_en_estado(&state, &comp.propuesta);
        let mut avisos: Vec<String> = analizado
            .avisos
            .iter()
            .map(|a| format!("{}: {}", a.linea, crate::i18n::tr(&idioma, &a.mensaje)))
            .collect();
        avisos.extend(comp.avisos.iter().map(|a| crate::i18n::tr(&idioma, a)));
        // El compilador escribe en inglés; la tarjeta se muestra en el idioma
        // de la app (solo cambian los textos visibles, no las acciones).
        comp.propuesta.resumen = crate::i18n::tr(&idioma, &comp.propuesta.resumen);
        for a in &mut comp.propuesta.acciones {
            a.descripcion = crate::i18n::tr(&idioma, &a.descripcion);
        }
        Ok(ResultadoConstruccion {
            propuesta: comp.propuesta,
            validacion,
            desde_estructura,
            descripciones_ia,
            texto_libre: analizado.texto_libre.len(),
            avisos,
        })
    }
    .await;
    localizar_error(&state, r)
}

async fn bot_y_pool(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<(String, sqlx::SqlitePool), String> {
    let bot = state.get_active_bot().ok_or("No bot selected")?;
    let pool = crate::db::get_pool(app).await.map_err(|e| e.to_string())?;
    Ok((bot.id, pool))
}

/// Aviso reciente de un miembro, con su recuento en el servidor.
#[derive(Debug, Clone, Serialize)]
pub struct AvisoReciente {
    pub user_id: String,
    pub total: i64,
    pub motivo: String,
    pub fecha: String,
}

/// Avisos recientes del servidor: un registro por usuario (último motivo y
/// cuántos avisos lleva), del más reciente al más antiguo.
#[tauri::command]
pub async fn listar_avisos_recientes(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    guild_id: String,
) -> Result<Vec<AvisoReciente>, String> {
    let r = async {
        let (_, pool) = bot_y_pool(&app, &state).await?;
        avisos_recientes(&pool, &guild_id)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    localizar_error(&state, r)
}

pub async fn avisos_recientes(
    pool: &sqlx::SqlitePool,
    guild_id: &str,
) -> anyhow::Result<Vec<AvisoReciente>> {
    let recuento: HashMap<String, i64> = crate::db::recuento_avisos(pool, guild_id, None)
        .await?
        .into_iter()
        .collect();
    let mut vistos = std::collections::HashSet::new();
    let mut salida = Vec::new();
    for a in crate::db::ultimos_avisos(pool, guild_id, None, 200).await? {
        if !vistos.insert(a.user_id.clone()) {
            continue;
        }
        salida.push(AvisoReciente {
            total: recuento.get(&a.user_id).copied().unwrap_or(1),
            user_id: a.user_id,
            motivo: a.motivo,
            fecha: a.fecha,
        });
        if salida.len() >= 20 {
            break;
        }
    }
    Ok(salida)
}

/// Plan (Markdown) del servidor para el bot activo; vacío si no hay.
#[tauri::command]
pub async fn leer_plan_servidor(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    guild_id: String,
) -> Result<String, String> {
    let r = async {
        let (bot, pool) = bot_y_pool(&app, &state).await?;
        crate::db::leer_plan(&pool, &bot, &guild_id)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    localizar_error(&state, r)
}

#[tauri::command]
pub async fn guardar_plan_servidor(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    guild_id: String,
    contenido: String,
) -> Result<(), String> {
    let r = async {
        let (bot, pool) = bot_y_pool(&app, &state).await?;
        crate::db::guardar_plan(&pool, &bot, &guild_id, &contenido)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    localizar_error(&state, r)
}

/// Contexto (Markdown) del servidor para el bot activo; vacío si no hay.
#[tauri::command]
pub async fn leer_contexto_servidor(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    guild_id: String,
) -> Result<String, String> {
    let r = async {
        let (bot, pool) = bot_y_pool(&app, &state).await?;
        crate::db::leer_contexto(&pool, &bot, &guild_id)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    localizar_error(&state, r)
}

#[tauri::command]
pub async fn guardar_contexto_servidor(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    guild_id: String,
    contenido: String,
) -> Result<(), String> {
    let r = async {
        let (bot, pool) = bot_y_pool(&app, &state).await?;
        crate::db::guardar_contexto(&pool, &bot, &guild_id, &contenido)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    localizar_error(&state, r)
}

/// Conversación guardada del servidor. Si `agente` es otro distinto del de la
/// conversación, esta se descarta y devuelve `None`.
#[tauri::command]
pub async fn leer_conversacion(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    guild_id: String,
    agente: Option<String>,
    tipo: Option<String>,
) -> Result<Option<crate::db::ConversacionAsistente>, String> {
    let r = async {
        let tipo = TipoAsistente::desde(tipo.as_deref())?;
        let (bot, pool) = bot_y_pool(&app, &state).await?;
        crate::db::leer_conversacion(&pool, &bot, &guild_id, tipo.as_str(), agente.as_deref())
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    localizar_error(&state, r)
}

/// Guarda los mensajes de la conversación conservando el id de sesión si el
/// agente es el mismo (con otro agente empieza una conversación nueva).
#[tauri::command]
pub async fn guardar_conversacion(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    guild_id: String,
    agente: String,
    modelo: String,
    mensajes: serde_json::Value,
    tipo: Option<String>,
) -> Result<(), String> {
    let r = async {
        let tipo = TipoAsistente::desde(tipo.as_deref())?;
        let (bot, pool) = bot_y_pool(&app, &state).await?;
        let previa =
            crate::db::leer_conversacion(&pool, &bot, &guild_id, tipo.as_str(), Some(&agente))
                .await
                .map_err(|e| e.to_string())?;
        let c = crate::db::ConversacionAsistente {
            session_id: previa.and_then(|p| p.session_id),
            agente,
            modelo,
            mensajes,
        };
        crate::db::guardar_conversacion(&pool, &bot, &guild_id, tipo.as_str(), &c)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    localizar_error(&state, r)
}

/// Empieza de cero: borra la conversación y su sesión.
#[tauri::command]
pub async fn nueva_conversacion(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    guild_id: String,
    tipo: Option<String>,
) -> Result<(), String> {
    let r = async {
        let tipo = TipoAsistente::desde(tipo.as_deref())?;
        let (bot, pool) = bot_y_pool(&app, &state).await?;
        crate::db::borrar_conversacion(&pool, &bot, &guild_id, tipo.as_str())
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    localizar_error(&state, r)
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
            &o.args[..9],
            [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--tools",
                "",
                "--disable-slash-commands",
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
        // Las de escritura de argus-mcp, prohibidas también: el modelo no las ve.
        for e in HERRAMIENTAS_ESCRITURA {
            let nombre = format!("mcp__argus__{e}");
            assert!(prohibidas.split(',').any(|x| x == nombre), "{nombre}");
        }
        assert!(o.directorio.starts_with(std::env::temp_dir()));
        // Ninguna integrada de Claude Code (ni las futuras) y sin skills.
        assert_eq!(valor(&o, "--tools"), "");
        assert!(o.args.contains(&"--disable-slash-commands".to_string()));
        assert!(o.archivos.is_empty());
    }

    #[test]
    fn claude_con_sesion() {
        let o = orden("claude", Some("abc-123"));
        assert_eq!(valor(&o, "--resume"), "abc-123");
    }

    #[test]
    fn opencode_con_y_sin_sesion() {
        let o = orden("opencode", None);
        assert_eq!(
            o.args,
            [
                "run",
                "--format",
                "json",
                "--agent",
                OPENCODE_AGENTE_ASISTENTE,
                "--model",
                "mod"
            ]
        );
        // opencode v2 ignora OPENCODE_CONFIG_CONTENT: el agente va en el
        // opencode.json de la carpeta. Todo denegado salvo las de lectura de ARGUS.
        assert_eq!(o.archivos.len(), 1);
        assert_eq!(o.archivos[0].0, "opencode.json");
        let archivo: Value = serde_json::from_str(&o.archivos[0].1).unwrap();
        let permisos = archivo["agent"][OPENCODE_AGENTE_ASISTENTE]["permission"]
            .as_object()
            .unwrap();
        assert_eq!(permisos.len(), HERRAMIENTAS_LECTURA.len() + 1);
        assert_eq!(
            permisos.keys().next().unwrap(),
            "*",
            "la denegación va primero"
        );
        assert_eq!(permisos["*"], "deny");
        for l in HERRAMIENTAS_LECTURA {
            assert_eq!(permisos[&format!("argus_{l}")], "allow", "{l}");
        }
        for e in HERRAMIENTAS_ESCRITURA {
            assert!(permisos.get(&format!("argus_{e}")).is_none(), "{e}");
        }
        assert_eq!(archivo["mcp"]["argus"]["command"][0], "/opt/argus-mcp");
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
mod tests_ejecucion {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn guion(nombre: &str, cuerpo: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("argus_ejec_{nombre}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("agente");
        std::fs::write(&p, format!("#!/bin/sh\ncat >/dev/null\n{cuerpo}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn orden(programa: PathBuf) -> OrdenAgente {
        OrdenAgente {
            directorio: programa.parent().unwrap().join("trabajo"),
            programa,
            args: Vec::new(),
            env: Vec::new(),
            stdin: "hola".into(),
            archivos: Vec::new(),
        }
    }

    async fn correr(
        nombre: &str,
        agente: &str,
        cuerpo: &str,
    ) -> (Option<String>, Vec<EventoAsistente>) {
        let o = orden(guion(nombre, cuerpo));
        let mut ev = Vec::new();
        let n = tokio::sync::Notify::new();
        let s = ejecutar_orden(&o, agente, &n, |e| ev.push(e))
            .await
            .unwrap();
        (s, ev)
    }

    fn herr(id: &str, nombre: &str, ok: Option<bool>) -> EventoAsistente {
        EventoAsistente::Herramienta {
            id: id.into(),
            nombre: nombre.into(),
            ok,
        }
    }

    #[tokio::test]
    async fn claude_falso_emite_texto_herramientas_y_sesion() {
        let (s, ev) = correr(
            "claude_ok",
            "claude",
            r#"cat <<'EOF'
{"type":"system","subtype":"init","session_id":"sess-1"}
{"type":"assistant","message":{"content":[{"type":"text","text":"Hola"},{"type":"tool_use","id":"t1","name":"mcp__argus__listar_canales","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":false}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"mcp__argus__info_rol","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","is_error":true}]}}
{"type":"result","subtype":"success","is_error":false,"result":"Hola","session_id":"sess-1"}
EOF"#,
        )
        .await;
        assert_eq!(s.as_deref(), Some("sess-1"));
        assert_eq!(
            ev,
            vec![
                EventoAsistente::Texto {
                    texto: "Hola".into()
                },
                herr("t1", "listar_canales", None),
                herr("t1", "listar_canales", Some(true)),
                herr("t2", "info_rol", None),
                herr("t2", "info_rol", Some(false)),
                EventoAsistente::Fin {
                    sesion: Some("sess-1".into()),
                    cancelado: false
                },
            ]
        );
    }

    #[tokio::test]
    async fn opencode_falso_emite_texto_herramientas_y_sesion() {
        let (s, ev) = correr(
            "opencode_ok",
            "opencode",
            r#"cat <<'EOF'
{"type":"step_start","sessionID":"ses_9","part":{"type":"step-start"}}
{"type":"tool_use","sessionID":"ses_9","part":{"type":"tool","callID":"c1","tool":"argus_listar_roles","state":{"status":"completed"}}}
{"type":"tool_use","sessionID":"ses_9","part":{"type":"tool","callID":"c2","tool":"argus_info_rol","state":{"status":"error"}}}
{"type":"text","sessionID":"ses_9","part":{"type":"text","text":"Listo"}}
{"type":"step_finish","sessionID":"ses_9","part":{"type":"step-finish"}}
EOF"#,
        )
        .await;
        assert_eq!(s.as_deref(), Some("ses_9"));
        assert_eq!(
            ev,
            vec![
                herr("c1", "listar_roles", Some(true)),
                herr("c2", "info_rol", Some(false)),
                EventoAsistente::Texto {
                    texto: "Listo".into()
                },
                EventoAsistente::Fin {
                    sesion: Some("ses_9".into()),
                    cancelado: false
                },
            ]
        );
    }

    #[tokio::test]
    async fn errores_se_clasifican() {
        let (_, ev) = correr(
            "err_login",
            "claude",
            r#"echo '{"type":"result","is_error":true,"result":"Invalid API key · Please run /login","session_id":"s"}'"#,
        )
        .await;
        assert!(ev.contains(&EventoAsistente::Error {
            mensaje: "The AI agent is not signed in. Sign in with its command line and try again"
                .into()
        }));
        let (_, ev) = correr(
            "err_modelo",
            "opencode",
            r#"echo '{"type":"error","error":{"name":"APIError","data":{"message":"Model not found: foo/bar"}}}'; exit 1"#,
        )
        .await;
        assert_eq!(
            ev[0],
            EventoAsistente::Error {
                mensaje: "The selected model is not valid for this agent".into()
            }
        );
        // Un solo error aunque el proceso termine con fallo.
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, EventoAsistente::Error { .. }))
                .count(),
            1
        );
        let (_, ev) = correr(
            "err_cuota",
            "claude",
            "echo 'usage limit reached' >&2; exit 1",
        )
        .await;
        assert!(matches!(&ev[0], EventoAsistente::Error { mensaje } if mensaje.contains("quota")));
    }

    #[tokio::test]
    async fn cancelar_mata_el_proceso_y_su_grupo() {
        let marca = std::env::temp_dir().join(format!("argus_nieto_{}", std::process::id()));
        let _ = std::fs::remove_file(&marca);
        let cuerpo = format!(
            "(sleep 2; touch {}) &\necho '{{\"type\":\"system\",\"session_id\":\"s\"}}'\nsleep 30",
            marca.display()
        );
        let o = orden(guion("cancel", &cuerpo));
        let n = std::sync::Arc::new(tokio::sync::Notify::new());
        let n2 = n.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            n2.notify_one();
        });
        let mut ev = Vec::new();
        let t = std::time::Instant::now();
        let s = ejecutar_orden(&o, "claude", &n, |e| ev.push(e))
            .await
            .unwrap();
        assert!(t.elapsed().as_secs() < 5);
        assert_eq!(s.as_deref(), Some("s"));
        assert_eq!(
            ev.last(),
            Some(&EventoAsistente::Fin {
                sesion: Some("s".into()),
                cancelado: true
            })
        );
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        assert!(!marca.exists(), "el nieto sobrevivió");
    }

    #[test]
    fn un_solo_agente_a_la_vez() {
        let n = std::sync::Arc::new(tokio::sync::Notify::new());
        let t = Turno::tomar(n.clone()).unwrap();
        assert!(Turno::tomar(n.clone()).is_err());
        drop(t);
        assert!(Turno::tomar(n).is_ok());
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

#[cfg(test)]
mod tests_prompt {
    use super::*;
    use crate::propuesta::{validar_acciones, Nombres, Propuesta};

    fn prompt(modo: ModoAsistente, idioma: &str) -> String {
        prompt_asistente(
            modo,
            ("123456789", "Mi Servidor"),
            "# Plan\n- categoría Staff\n- rol Moderators",
            "Tema: piratas",
            idioma,
            "Construye el servidor",
        )
    }

    fn validar(p: &Propuesta) -> crate::propuesta::Validacion {
        validar_acciones(p, &Nombres::default())
    }

    fn moderacion(modo: ModoAsistente) -> String {
        prompt_asistente_tipo(
            TipoAsistente::Moderacion,
            modo,
            ("123456789", "Mi Servidor"),
            "PLAN SECRETO DE CONSTRUCCION",
            "Tema: piratas",
            "es",
            "Expulsa a Ana",
            "MAPA",
            50,
        )
    }

    #[test]
    fn prompt_moderacion_read_pide_pruebas_y_no_propone() {
        let p = moderacion(ModoAsistente::Read);
        for t in ["buscar_mensajes", "leer_mensajes", "listar_avisos"] {
            assert!(p.contains(t), "{t}");
        }
        assert!(p.contains("123456789") && p.contains("Mi Servidor"));
        assert!(p.contains("at most 50 messages per channel"));
        assert!(p.contains("author") && p.contains("date") && p.contains("link"));
        assert!(p.contains("not enough"));
        assert!(p.contains("Spanish"));
        assert!(p.contains("Expulsa a Ana"));
        assert!(!p.contains("```json"));
        assert!(!p.contains("PLAN SECRETO"));
    }

    #[test]
    fn prompt_moderacion_execute_solo_acciones_de_moderacion() {
        let p = moderacion(ModoAsistente::Execute);
        assert!(p.contains(&ACCIONES_MODERACION.join(", ")));
        assert!(p.contains("Never propose changes to the structure"));
        assert!(p.contains("```json"));
        assert!(!p.contains("crear_canal"));
        assert!(!p.contains("ordenar_roles"));
        // el ejemplo del prompt es una propuesta válida y de moderación
        let ej: Propuesta = serde_json::from_str(EJEMPLO_MODERACION).unwrap();
        assert!(comprobar_acciones_moderacion(&ej).is_ok());
        let v = validar(&ej);
        assert!(v.errores.is_empty(), "{:?}", v.errores);
        // el prompt de construcción sigue siendo distinto
        let c = prompt(ModoAsistente::Execute, "en");
        assert!(c.contains("crear_canal") && !c.contains("moderation assistant"));
    }

    #[test]
    fn moderacion_rechaza_cambios_de_estructura() {
        let p: Propuesta = serde_json::from_str(
            r#"{"resumen":"x","acciones":[{"herramienta":"crear_canal","args":{"nombre":"a"},"descripcion":"d"}]}"#,
        )
        .unwrap();
        assert!(comprobar_acciones_moderacion(&p).is_err());
    }

    #[test]
    fn tipo_de_asistente_se_lee_del_texto() {
        assert_eq!(
            TipoAsistente::desde(None).unwrap(),
            TipoAsistente::Construccion
        );
        assert_eq!(
            TipoAsistente::desde(Some("moderacion")).unwrap(),
            TipoAsistente::Moderacion
        );
        assert!(TipoAsistente::desde(Some("otro")).is_err());
    }

    #[test]
    fn el_prompt_incluye_el_contexto_del_servidor() {
        let p = prompt(ModoAsistente::Execute, "en");
        assert!(p.contains("## Server context"));
        assert!(p.contains("Tema: piratas"));
        let vacio = prompt_asistente(ModoAsistente::Read, ("1", "S"), "", "", "en", "hola");
        assert!(vacio.contains("## Server context"));
    }

    #[test]
    fn prompt_read_explica_argus_servidor_plan_y_solo_lectura() {
        let p = prompt(ModoAsistente::Read, "en");
        assert!(p.contains("ARGUS"));
        assert!(p.contains("\"Mi Servidor\"") && p.contains("123456789"));
        assert!(p.contains("Pass guild_id \"123456789\" in every tool call"));
        assert!(p.contains("- rol Moderators"), "plan completo");
        assert!(p.contains("cannot modify anything"));
        assert!(p.contains("reply in English"));
        assert!(p.contains("Construye el servidor"));
        assert!(!p.contains("```json"), "Read no pide propuesta");
    }

    #[test]
    fn prompt_execute_documenta_la_propuesta() {
        let p = prompt(ModoAsistente::Execute, "es");
        assert!(p.contains("reply in Spanish"));
        assert!(p.contains("END your answer with ONE ```json block"));
        for h in crate::mcp::HERRAMIENTAS_ESCRITURA {
            assert!(p.contains(h), "falta la herramienta {h}");
        }
        for campo in [
            "crear_rol",
            "crear_canal",
            "permiso_canal",
            "tema",
            "permitir",
        ] {
            assert!(p.contains(campo), "falta {campo}");
        }
    }

    #[test]
    fn execute_recibe_el_plan_analizado_y_el_mapa_compacto() {
        let plan = "## Rol: Moderators\n## Categoría: Staff\n- texto: mod-chat\n";
        let p = prompt_asistente_con_mapa(
            ModoAsistente::Execute,
            ("1", "S"),
            plan,
            "",
            "en",
            "build",
            "- Lobby: general\nRoles: Admin\n",
        );
        assert!(p.contains("<parsed_plan>"));
        assert!(p.contains("<server_map>") && p.contains("- Lobby: general"));
        let leer = prompt_asistente_con_mapa(
            ModoAsistente::Read,
            ("1", "S"),
            plan,
            "",
            "en",
            "hi",
            "- Lobby: general\n",
        );
        assert!(!leer.contains("<server_map>") && !leer.contains("<parsed_plan>"));
    }

    #[test]
    fn el_resumen_del_mapa_se_recorta() {
        use crate::state::{Channel, Guild, ServerMap};
        let canal = |i: usize, t: u8, parent: Option<&str>| Channel {
            id: i.to_string(),
            name: format!("c{i}"),
            channel_type: t,
            parent_id: parent.map(String::from),
            position: i as i32,
            overwrites: None,
            permissions_bitfield: None,
        };
        let mut channels = vec![canal(1, 4, None)];
        channels.extend((2..300).map(|i| canal(i, 0, Some("1"))));
        let mapa = ServerMap {
            guild: serde_json::from_value::<Guild>(serde_json::json!({
                "id": "1", "name": "S", "icon": null, "member_count": 0, "joined_at": null
            }))
            .unwrap(),
            channels,
            members: vec![],
            roles: vec![],
            categories: Default::default(),
            last_updated: String::new(),
        };
        let r = resumen_mapa(&mapa);
        assert!(r.starts_with("- c1: c2, c3"));
        assert!(r.contains("more"));
        assert!(r.chars().count() <= MAX_RESUMEN_CHARS + 1);
    }

    #[test]
    fn prompt_plan_incluye_el_plan_actual_el_manual_y_el_mapa() {
        let p = prompt_asistente_con_mapa(
            ModoAsistente::Plan,
            ("1", "S"),
            "## Category: Tabernas\n- text: barra",
            "ctx",
            "en",
            "add 3 voice channels to Tabernas",
            "- Tabernas: barra\nRoles: Mod\n",
        );
        assert!(p.contains("<plan>\n## Category: Tabernas\n- text: barra\n</plan>"));
        assert!(p.contains("## Plan mode"));
        assert!(p.contains("```plan"));
        assert!(p.contains("Plan syntax"));
        assert!(p.contains("{01..05}"));
        assert!(p.contains("<server_map>"));
        assert!(p.contains("add 3 voice channels to Tabernas"));
        assert!(!p.contains("```json"), "Plan no pide propuesta de acciones");
        // La sintaxis del manual la entiende el analizador.
        let a = crate::plan_estructurado::analizar(
            "## Category: C | description: generate: x\n- voice: Table-{01..05} | description: \"Mesa {n}\"",
        );
        assert!(a.avisos.is_empty(), "{:?}", a.avisos);
        assert_eq!(a.categorias[0].canales.len(), 5);
    }

    #[test]
    fn extrae_el_ultimo_bloque_plan() {
        let r = "Hecho.\n```plan\nviejo\n```\nmejor:\n```plan\n## Role: A\n\n- text: x\n```\nfin";
        assert_eq!(extraer_plan(r).unwrap(), "## Role: A\n\n- text: x");
        // Sin cierre (respuesta cortada) toma hasta el final.
        assert_eq!(extraer_plan("```PLAN\na\nb").unwrap(), "a\nb");
    }

    #[test]
    fn extraer_plan_sin_bloque_o_vacio_da_error() {
        assert!(extraer_plan("hola")
            .unwrap_err()
            .contains("did not include a plan"));
        assert!(extraer_plan("```json\n{}\n```").is_err());
        assert!(extraer_plan("```plan\n\n```")
            .unwrap_err()
            .contains("empty"));
    }

    fn marcas(d: &[LineaDiff]) -> Vec<String> {
        d.iter()
            .map(|l| {
                let m = match l.tipo {
                    "added" => '+',
                    "removed" => '-',
                    _ => ' ',
                };
                format!("{m}{}", l.texto)
            })
            .collect()
    }

    #[test]
    fn diferencias_marcan_anadidas_y_quitadas() {
        let d = diferencias_lineas("a\nb\nc\nd", "a\nc\nx\nd\ny");
        assert_eq!(marcas(&d), [" a", "-b", " c", "+x", " d", "+y"]);
        assert_eq!(marcas(&diferencias_lineas("a\nb", "a\nb")), [" a", " b"]);
        assert_eq!(marcas(&diferencias_lineas("", "a\nb")), ["+a", "+b"]);
        assert_eq!(marcas(&diferencias_lineas("a\nb", "")), ["-a", "-b"]);
        assert!(diferencias_lineas("", "").is_empty());
    }

    #[test]
    fn cambios_cuentan_lineas_y_avisan_de_la_version_propuesta() {
        let c = cambios_de_plan(
            "## Category: A\n- text: x",
            "## Category: A\n- text: y | bogus: 1\n- text: z",
            "en",
        );
        assert_eq!((c.anadidas, c.quitadas), (2, 1));
        assert_eq!(c.avisos.len(), 1, "{:?}", c.avisos);
        assert!(c.avisos[0].starts_with("2: "), "{:?}", c.avisos);
    }

    #[test]
    fn el_ejemplo_del_prompt_es_una_propuesta_valida() {
        let propuesta = extraer_propuesta(&prompt(ModoAsistente::Execute, "en")).unwrap();
        assert_eq!(propuesta.acciones.len(), 4);
        let v = validar(&propuesta);
        assert!(v.errores.is_empty(), "{:?}", v.errores);
    }

    #[test]
    fn extrae_bloque_valido_tras_texto() {
        let r = "He revisado el servidor. Propongo:\n\n```json\n{\"resumen\":\"x\",\"acciones\":[{\"herramienta\":\"crear_rol\",\"args\":{\"nombre\":\"VIP\"},\"descripcion\":\"Rol VIP\"}]}\n```\n";
        let ev = propuesta_de_respuesta(r, validar).unwrap();
        assert_eq!(ev.propuesta.acciones[0].herramienta, "crear_rol");
        assert!(!ev.validacion.cambios.is_empty());
    }

    #[test]
    fn usa_el_ultimo_bloque() {
        let r = "```json\n{\"acciones\":[]}\n```\ny al final:\n```json\n{\"acciones\":[{\"herramienta\":\"crear_rol\",\"args\":{\"nombre\":\"A\"}}]}\n```";
        assert_eq!(extraer_propuesta(r).unwrap().acciones.len(), 1);
    }

    #[test]
    fn sin_bloque_da_error_legible() {
        let e = propuesta_de_respuesta("Todo está ya como dice el plan.", validar).unwrap_err();
        assert_eq!(e, "The assistant did not include a proposal in its answer");
        let e = extraer_propuesta("```\n{}\n```").unwrap_err();
        assert!(e.contains("did not include"));
    }

    #[test]
    fn json_roto_da_error_legible() {
        let e = extraer_propuesta("```json\n{\"acciones\": [ {\"herramienta\": \n```").unwrap_err();
        assert!(
            e.starts_with("The assistant's proposal is not valid JSON:"),
            "{e}"
        );
        let e = extraer_propuesta("```json\n{\"acciones\":[]}\n```").unwrap_err();
        assert_eq!(e, "The assistant's proposal has no actions");
    }

    #[test]
    fn propuesta_con_acciones_invalidas_da_error() {
        let r = "```json\n{\"acciones\":[{\"herramienta\":\"borrar_todo\",\"args\":{}}]}\n```";
        let e = propuesta_de_respuesta(r, validar).unwrap_err();
        assert!(
            e.starts_with("The assistant's proposal is not valid: borrar_todo"),
            "{e}"
        );
    }
}
