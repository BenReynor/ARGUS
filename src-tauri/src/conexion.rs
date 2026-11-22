//! Una sola conexión al Gateway por bot.
//!
//! El propietario de la conexión de un bot es quien tiene el lock de archivo del
//! sistema `gateway-<bot>.lock` (en la carpeta de datos). El lock es del sistema
//! operativo: si el proceso muere, se libera solo. Tanto el servicio sin ventana
//! como la interfaz lo toman antes de abrir el Gateway (`Conexion::iniciar`).
//! Este módulo solo usa std y tokio (lo comparte `state.rs`).

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Espera máxima de una parada ordenada antes de abortar la tarea.
pub const ESPERA_PARADA: Duration = Duration::from_secs(5);

const OCUPADO: &str =
    "This bot already has a Gateway connection in another ARGUS process (service or interface)";

/// Lock de archivo del propietario de la conexión de un bot.
#[derive(Debug)]
pub struct BloqueoBot {
    _archivo: File,
    ruta: PathBuf,
}

impl BloqueoBot {
    /// Intenta tomar el lock del bot sin esperar. Falla si otro proceso (o este
    /// mismo, con otra conexión) ya es el propietario.
    pub fn tomar(carpeta: &Path, bot_id: &str) -> Result<Self, String> {
        let id: String = bot_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        if id.is_empty() {
            return Err("No bot selected".into());
        }
        std::fs::create_dir_all(carpeta).map_err(|e| e.to_string())?;
        let ruta = carpeta.join(format!("gateway-{id}.lock"));
        let archivo = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&ruta)
            .map_err(|e| e.to_string())?;
        match archivo.try_lock() {
            Ok(()) => Ok(Self {
                _archivo: archivo,
                ruta,
            }),
            Err(TryLockError::WouldBlock) => Err(OCUPADO.into()),
            Err(TryLockError::Error(e)) => Err(e.to_string()),
        }
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }
}

/// Conexión de un bot al Gateway: tarea + lock de propietario. El lock se suelta
/// al descartarse (tras detener la tarea).
pub struct Conexion {
    parar: Arc<Notify>,
    tarea: JoinHandle<()>,
    _bloqueo: BloqueoBot,
}

impl Conexion {
    /// Toma el lock del bot y, solo entonces, llama a `lanzar` (que abre el Gateway).
    /// `lanzar` recibe el aviso de parada ordenada.
    pub fn iniciar(
        carpeta: &Path,
        bot_id: &str,
        lanzar: impl FnOnce(Arc<Notify>) -> JoinHandle<()>,
    ) -> Result<Self, String> {
        let bloqueo = BloqueoBot::tomar(carpeta, bot_id)?;
        let parar = Arc::new(Notify::new());
        let tarea = lanzar(parar.clone());
        Ok(Self {
            parar,
            tarea,
            _bloqueo: bloqueo,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.tarea.is_finished()
    }

    /// Parada ordenada: avisa a la tarea, espera `espera` y, si no termina, la
    /// aborta. Después suelta el lock.
    pub async fn detener(mut self, espera: Duration) {
        self.parar.notify_one();
        if tokio::time::timeout(espera, &mut self.tarea).await.is_err() {
            self.tarea.abort();
            let _ = (&mut self.tarea).await;
        }
    }
}

/// Envuelve la tarea de un Gateway para que también termine con la parada ordenada.
pub fn con_parada(mut tarea: JoinHandle<()>, parar: Arc<Notify>) -> JoinHandle<()> {
    tokio::spawn(async move {
        tokio::select! {
            _ = &mut tarea => {}
            _ = parar.notified() => {
                tarea.abort();
                let _ = tarea.await;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    fn carpeta() -> PathBuf {
        let p = std::env::temp_dir().join(format!("argus-locks-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Gateway falso: cuenta aperturas y registra el cierre ordenado.
    #[derive(Clone, Default)]
    struct GatewayFalso {
        aperturas: Arc<AtomicU32>,
        cerrado: Arc<AtomicBool>,
    }

    impl GatewayFalso {
        fn lanzar(&self, parar: Arc<Notify>) -> JoinHandle<()> {
            let (a, c) = (self.aperturas.clone(), self.cerrado.clone());
            a.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                parar.notified().await;
                c.store(true, Ordering::SeqCst);
            })
        }
    }

    #[tokio::test]
    async fn doble_arranque_del_mismo_bot_abre_una_sola_conexion() {
        let dir = carpeta();
        let gw = GatewayFalso::default();
        let servicio = Conexion::iniciar(&dir, "111", |p| gw.lanzar(p)).unwrap();
        // La interfaz intenta conectar el mismo bot: se rechaza sin abrir Gateway.
        let err = Conexion::iniciar(&dir, "111", |p| gw.lanzar(p))
            .err()
            .expect("debe rechazarse");
        assert!(err.contains("already has a Gateway connection"));
        assert_eq!(gw.aperturas.load(Ordering::SeqCst), 1);
        // Otro bot no se ve afectado.
        let otro = Conexion::iniciar(&dir, "222", |p| gw.lanzar(p)).unwrap();
        assert_eq!(gw.aperturas.load(Ordering::SeqCst), 2);
        servicio.detener(ESPERA_PARADA).await;
        otro.detener(ESPERA_PARADA).await;
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn parada_ordenada_cierra_el_gateway_y_libera_el_lock() {
        let dir = carpeta();
        let gw = GatewayFalso::default();
        let c = Conexion::iniciar(&dir, "111", |p| gw.lanzar(p)).unwrap();
        assert!(!c.is_finished());
        c.detener(ESPERA_PARADA).await;
        assert!(gw.cerrado.load(Ordering::SeqCst));
        // Tras la parada se puede reconectar.
        let c = Conexion::iniciar(&dir, "111", |p| gw.lanzar(p)).unwrap();
        assert_eq!(gw.aperturas.load(Ordering::SeqCst), 2);
        c.detener(ESPERA_PARADA).await;
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test(start_paused = true)]
    async fn gateway_que_no_responde_se_aborta_y_libera_el_lock() {
        let dir = carpeta();
        let c =
            Conexion::iniciar(&dir, "111", |_| tokio::spawn(std::future::pending::<()>())).unwrap();
        c.detener(Duration::from_millis(100)).await;
        assert!(BloqueoBot::tomar(&dir, "111").is_ok());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn el_lock_es_del_sistema_y_se_suelta_al_descartar() {
        let dir = carpeta();
        let a = BloqueoBot::tomar(&dir, "111").unwrap();
        assert!(a.ruta().ends_with("gateway-111.lock"));
        assert!(BloqueoBot::tomar(&dir, "111").is_err());
        drop(a);
        assert!(BloqueoBot::tomar(&dir, "111").is_ok());
        assert!(BloqueoBot::tomar(&dir, "").is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn con_parada_termina_la_tarea_interna() {
        let parar = Arc::new(Notify::new());
        let interna = tokio::spawn(std::future::pending::<()>());
        let envuelta = con_parada(interna, parar.clone());
        parar.notify_one();
        tokio::time::timeout(Duration::from_secs(2), envuelta)
            .await
            .unwrap()
            .unwrap();
    }
}
