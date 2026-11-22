//! Canal local entre el servicio sin ventana y la interfaz.
//!
//! Es un socket TCP solo en 127.0.0.1 (puerto efímero). El servicio escribe
//! `service-channel.json` (puerto y secreto del canal) con permisos de solo el
//! usuario (0600 en Unix; en Windows hereda la ACL del perfil). El secreto es
//! propio del canal y nunca es un token de bot: ningún token viaja por aquí.
//! La autenticación es por desafío: el servidor envía un nonce y el cliente
//! responde `sha256(nonce || secreto)`, así el secreto tampoco cruza el socket.
//!
//! Protocolo: una línea JSON por mensaje.
//! - `leer` devuelve el estado y los eventos (avisos y operaciones) aún no
//!   confirmados por ese consumidor; `ack` los confirma.
//! - Los eventos tienen un `seq` creciente y se conservan en el servicio hasta
//!   que el consumidor los confirma: cerrar la interfaz no los pierde, y tras
//!   confirmar no se repiten al reabrirla (el cursor vive en el servicio).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

pub const ARCHIVO_CANAL: &str = "service-channel.json";
/// Máximo de eventos retenidos para un consumidor ausente.
const MAX_EVENTOS: usize = 1000;
const MAX_LINEA: usize = 64 * 1024;
pub const NO_ACTIVO: &str = "The service is not running";
pub const AUTH_FALLIDA: &str = "Service authentication failed";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evento {
    pub seq: u64,
    /// "aviso" u "operacion".
    pub tipo: String,
    pub texto: String,
}

#[derive(Debug, Default)]
struct Interior {
    siguiente: u64,
    eventos: VecDeque<Evento>,
    estado: Value,
    cursores: HashMap<String, u64>,
}

/// Estado compartido del servicio: lo que la interfaz lee por el canal.
#[derive(Debug, Clone, Default)]
pub struct Buzon(Arc<Mutex<Interior>>, Arc<tokio::sync::Notify>);

impl Buzon {
    pub fn new() -> Self {
        Self::default()
    }

    /// Se completa cuando la interfaz pide detener el servicio (op `stop`).
    pub async fn esperar_parada(&self) {
        self.1.notified().await;
    }

    pub fn publicar(&self, tipo: &str, texto: &str) -> u64 {
        let mut g = self.0.lock().unwrap();
        g.siguiente += 1;
        let seq = g.siguiente;
        g.eventos.push_back(Evento {
            seq,
            tipo: tipo.into(),
            texto: texto.into(),
        });
        while g.eventos.len() > MAX_EVENTOS {
            g.eventos.pop_front();
        }
        seq
    }

    pub fn fijar_estado(&self, estado: Value) {
        self.0.lock().unwrap().estado = estado;
    }

    fn leer(&self, consumidor: &str) -> Value {
        let g = self.0.lock().unwrap();
        let cursor = g.cursores.get(consumidor).copied().unwrap_or(0);
        let eventos: Vec<&Evento> = g.eventos.iter().filter(|e| e.seq > cursor).collect();
        serde_json::json!({ "ok": true, "estado": g.estado, "eventos": eventos })
    }

    fn confirmar(&self, consumidor: &str, hasta: u64) {
        let mut g = self.0.lock().unwrap();
        let tope = g.siguiente;
        let c = g.cursores.entry(consumidor.to_string()).or_insert(0);
        // Nunca retrocede ni pasa de lo publicado.
        *c = (*c).max(hasta.min(tope));
    }
}

#[derive(Serialize, Deserialize)]
struct Descriptor {
    port: u16,
    secret: String,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn respuesta_desafio(nonce: &str, secreto: &str) -> String {
    let mut h = Sha256::new();
    h.update(nonce.as_bytes());
    h.update(secreto.as_bytes());
    hex(&h.finalize())
}

fn aleatorio() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|e| e.to_string())?;
    Ok(hex(&b))
}

fn iguales(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |r, (x, y)| r | (x ^ y)) == 0
}

fn escribir_privado(ruta: &Path, contenido: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::remove_file(ruta);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(ruta)
            .map_err(|e| e.to_string())?;
        f.write_all(contenido.as_bytes()).map_err(|e| e.to_string())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(ruta, contenido).map_err(|e| e.to_string())
    }
}

/// Servidor del canal (lado del servicio).
pub struct Servidor {
    pub puerto: u16,
    ruta: PathBuf,
    tarea: JoinHandle<()>,
}

impl Servidor {
    pub async fn iniciar(carpeta: &Path, buzon: Buzon) -> Result<Self, String> {
        std::fs::create_dir_all(carpeta).map_err(|e| e.to_string())?;
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|e| e.to_string())?;
        let puerto = listener.local_addr().map_err(|e| e.to_string())?.port();
        let secreto = aleatorio()?;
        let ruta = carpeta.join(ARCHIVO_CANAL);
        let desc = serde_json::to_string(&Descriptor {
            port: puerto,
            secret: secreto.clone(),
        })
        .map_err(|e| e.to_string())?;
        escribir_privado(&ruta, &desc)?;
        let tarea = tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let (b, s) = (buzon.clone(), secreto.clone());
                tokio::spawn(async move {
                    let _ = atender(socket, b, s).await;
                });
            }
        });
        Ok(Self {
            puerto,
            ruta,
            tarea,
        })
    }

    pub fn detener(self) {
        self.tarea.abort();
        let _ = std::fs::remove_file(&self.ruta);
    }
}

async fn linea(r: &mut BufReader<OwnedReadHalf>) -> Result<Option<String>, String> {
    let mut s = String::new();
    let n = tokio::time::timeout(std::time::Duration::from_secs(30), r.read_line(&mut s))
        .await
        .map_err(|_| "timeout".to_string())?
        .map_err(|e| e.to_string())?;
    if n == 0 || s.len() > MAX_LINEA {
        return Ok(None);
    }
    Ok(Some(s))
}

async fn atender(socket: TcpStream, buzon: Buzon, secreto: String) -> Result<(), String> {
    let (rd, mut wr) = socket.into_split();
    let mut rd = BufReader::new(rd);
    let nonce = aleatorio()?;
    let desafio = serde_json::json!({ "nonce": nonce }).to_string() + "\n";
    wr.write_all(desafio.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let Some(l) = linea(&mut rd).await? else {
        return Ok(());
    };
    let v: Value = serde_json::from_str(&l).map_err(|e| e.to_string())?;
    let dado = v.get("auth").and_then(Value::as_str).unwrap_or("");
    if !iguales(dado, &respuesta_desafio(&nonce, &secreto)) {
        let _ = wr.write_all(b"{\"ok\":false,\"error\":\"auth\"}\n").await;
        return Ok(());
    }
    wr.write_all(b"{\"ok\":true}\n")
        .await
        .map_err(|e| e.to_string())?;
    while let Some(l) = linea(&mut rd).await? {
        let v: Value = serde_json::from_str(&l).unwrap_or(Value::Null);
        let consumidor = v.get("consumidor").and_then(Value::as_str).unwrap_or("ui");
        let r = match v.get("op").and_then(Value::as_str) {
            Some("leer") => buzon.leer(consumidor),
            Some("ack") => {
                let hasta = v.get("hasta").and_then(Value::as_u64).unwrap_or(0);
                buzon.confirmar(consumidor, hasta);
                serde_json::json!({ "ok": true })
            }
            Some("stop") => {
                buzon.1.notify_one();
                serde_json::json!({ "ok": true })
            }
            _ => serde_json::json!({ "ok": false, "error": "op" }),
        };
        wr.write_all((r.to_string() + "\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Lo que la interfaz recibe al leer el canal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lectura {
    pub estado: Value,
    pub eventos: Vec<Evento>,
}

/// Cliente del canal (lado de la interfaz). Cada llamada abre una conexión
/// nueva, así que reconectar tras reiniciar el servicio o la interfaz no
/// requiere estado local: el cursor vive en el servicio.
pub struct Cliente {
    carpeta: PathBuf,
    consumidor: String,
}

impl Cliente {
    pub fn new(carpeta: &Path, consumidor: &str) -> Self {
        Self {
            carpeta: carpeta.to_path_buf(),
            consumidor: consumidor.into(),
        }
    }

    async fn sesion(&self) -> Result<(BufReader<OwnedReadHalf>, OwnedWriteHalf), String> {
        let txt = std::fs::read_to_string(self.carpeta.join(ARCHIVO_CANAL))
            .map_err(|_| NO_ACTIVO.to_string())?;
        let d: Descriptor = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
        let s = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            TcpStream::connect(("127.0.0.1", d.port)),
        )
        .await
        .map_err(|_| NO_ACTIVO.to_string())?
        .map_err(|_| NO_ACTIVO.to_string())?;
        let (rd, mut wr) = s.into_split();
        let mut rd = BufReader::new(rd);
        let l = linea(&mut rd).await?.ok_or(NO_ACTIVO)?;
        let v: Value = serde_json::from_str(&l).map_err(|e| e.to_string())?;
        let nonce = v.get("nonce").and_then(Value::as_str).unwrap_or("");
        let msg =
            serde_json::json!({ "auth": respuesta_desafio(nonce, &d.secret) }).to_string() + "\n";
        wr.write_all(msg.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let l = linea(&mut rd).await?.ok_or(AUTH_FALLIDA)?;
        let v: Value = serde_json::from_str(&l).map_err(|e| e.to_string())?;
        if v.get("ok") != Some(&Value::Bool(true)) {
            return Err(AUTH_FALLIDA.into());
        }
        Ok((rd, wr))
    }

    async fn pedir(&self, peticion: Value) -> Result<Value, String> {
        let (mut rd, mut wr) = self.sesion().await?;
        wr.write_all((peticion.to_string() + "\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let l = linea(&mut rd).await?.ok_or(NO_ACTIVO)?;
        serde_json::from_str(&l).map_err(|e| e.to_string())
    }

    /// Lee el estado y los eventos pendientes (sin confirmarlos).
    pub async fn leer(&self) -> Result<Lectura, String> {
        let v = self
            .pedir(serde_json::json!({ "op": "leer", "consumidor": self.consumidor }))
            .await?;
        let eventos: Vec<Evento> =
            serde_json::from_value(v.get("eventos").cloned().unwrap_or(Value::Null))
                .unwrap_or_default();
        Ok(Lectura {
            estado: v.get("estado").cloned().unwrap_or(Value::Null),
            eventos,
        })
    }

    /// Confirma lo ya mostrado: no volverá a entregarse.
    pub async fn confirmar(&self, hasta: u64) -> Result<(), String> {
        self.pedir(
            serde_json::json!({ "op": "ack", "consumidor": self.consumidor, "hasta": hasta }),
        )
        .await
        .map(|_| ())
    }

    /// Pide al servicio una parada ordenada.
    pub async fn parar(&self) -> Result<(), String> {
        self.pedir(serde_json::json!({ "op": "stop" }))
            .await
            .map(|_| ())
    }

    /// Lee y confirma de una vez lo entregado.
    pub async fn leer_y_confirmar(&self) -> Result<Lectura, String> {
        let l = self.leer().await?;
        if let Some(u) = l.eventos.last() {
            self.confirmar(u.seq).await?;
        }
        Ok(l)
    }
}

/// Comando de la interfaz: estado y eventos nuevos del servicio. Si el servicio
/// no está activo devuelve error y la interfaz sigue funcionando por su cuenta.
#[tauri::command]
pub async fn leer_canal_servicio(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Lectura, String> {
    let r = async {
        let carpeta = crate::servicio::carpeta_locks(&app)?;
        Cliente::new(&carpeta, "ui").leer_y_confirmar().await
    }
    .await;
    crate::i18n::localizar_error(&state, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        std::env::temp_dir().join(format!("argus-canal-{}", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn comparte_estado_y_eventos_sin_tokens() {
        let d = dir();
        let b = Buzon::new();
        b.fijar_estado(serde_json::json!({"connected": true}));
        b.publicar("aviso", "hola");
        let s = Servidor::iniciar(&d, b).await.unwrap();
        let l = Cliente::new(&d, "ui").leer().await.unwrap();
        assert_eq!(l.estado["connected"], true);
        assert_eq!(l.eventos.len(), 1);
        // El descriptor solo guarda puerto y secreto del canal.
        let txt = std::fs::read_to_string(d.join(ARCHIVO_CANAL)).unwrap();
        assert!(txt.contains("port") && txt.contains("secret") && !txt.contains("token"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let m = std::fs::metadata(d.join(ARCHIVO_CANAL))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(m & 0o777, 0o600);
        }
        s.detener();
        std::fs::remove_dir_all(d).ok();
    }

    #[tokio::test]
    async fn rechaza_secreto_incorrecto() {
        let d = dir();
        let s = Servidor::iniciar(&d, Buzon::new()).await.unwrap();
        let mut desc: Descriptor =
            serde_json::from_str(&std::fs::read_to_string(d.join(ARCHIVO_CANAL)).unwrap()).unwrap();
        desc.secret = "otro".into();
        std::fs::write(d.join(ARCHIVO_CANAL), serde_json::to_string(&desc).unwrap()).unwrap();
        assert_eq!(
            Cliente::new(&d, "ui").leer().await.unwrap_err(),
            AUTH_FALLIDA
        );
        s.detener();
        std::fs::remove_dir_all(d).ok();
    }

    #[tokio::test]
    async fn reconexion_no_pierde_ni_repite() {
        let d = dir();
        let b = Buzon::new();
        let s = Servidor::iniciar(&d, b.clone()).await.unwrap();
        // La interfaz está cerrada mientras el servicio acumula eventos.
        b.publicar("operacion", "uno");
        b.publicar("aviso", "dos");
        // La interfaz se abre: recibe ambos, una vez.
        let ui = Cliente::new(&d, "ui");
        let l = ui.leer_y_confirmar().await.unwrap();
        assert_eq!(l.eventos.iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2]);
        // Se cierra y se vuelve a abrir (cliente nuevo): no hay repeticiones.
        drop(ui);
        b.publicar("aviso", "tres");
        let l = Cliente::new(&d, "ui").leer_y_confirmar().await.unwrap();
        assert_eq!(l.eventos.len(), 1);
        assert_eq!(l.eventos[0].texto, "tres");
        assert!(Cliente::new(&d, "ui")
            .leer()
            .await
            .unwrap()
            .eventos
            .is_empty());
        // Una lectura sin confirmar (la interfaz murió a medias) se reentrega.
        b.publicar("aviso", "cuatro");
        let c = Cliente::new(&d, "ui");
        assert_eq!(c.leer().await.unwrap().eventos.len(), 1);
        assert_eq!(c.leer().await.unwrap().eventos.len(), 1);
        // Otro consumidor tiene su propio cursor.
        assert_eq!(
            Cliente::new(&d, "otro").leer().await.unwrap().eventos.len(),
            4
        );
        s.detener();
        std::fs::remove_dir_all(d).ok();
    }

    #[tokio::test]
    async fn la_interfaz_se_reconecta_tras_reiniciar_el_servicio() {
        let d = dir();
        let ui = Cliente::new(&d, "ui");
        // Sin servicio: error claro, sin colgarse.
        assert_eq!(ui.leer().await.unwrap_err(), NO_ACTIVO);
        let b = Buzon::new();
        b.publicar("aviso", "a");
        let s = Servidor::iniciar(&d, b).await.unwrap();
        assert_eq!(ui.leer_y_confirmar().await.unwrap().eventos.len(), 1);
        s.detener();
        assert!(ui.leer().await.is_err());
        // El servicio vuelve (otro puerto y secreto): el mismo cliente reconecta.
        let b2 = Buzon::new();
        b2.publicar("aviso", "b");
        let s2 = Servidor::iniciar(&d, b2).await.unwrap();
        assert_eq!(ui.leer().await.unwrap().eventos[0].texto, "b");
        s2.detener();
        std::fs::remove_dir_all(d).ok();
    }

    #[tokio::test]
    async fn ack_no_pasa_de_lo_publicado() {
        let d = dir();
        let b = Buzon::new();
        let s = Servidor::iniciar(&d, b.clone()).await.unwrap();
        let ui = Cliente::new(&d, "ui");
        ui.confirmar(999).await.unwrap();
        b.publicar("aviso", "x");
        assert_eq!(ui.leer().await.unwrap().eventos.len(), 1);
        s.detener();
        std::fs::remove_dir_all(d).ok();
    }
}
