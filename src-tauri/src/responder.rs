//! Responder en Discord: para cada mensaje dirigido al bot, escribe «escribiendo…»,
//! lee el contexto del canal, lanza el agente (sin herramientas, con límite de
//! tiempo) y contesta como respuesta al mensaje original. Las respuestas se
//! encolan por canal y como mucho `MAX_AGENTES` agentes corren a la vez.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};
use twilight_model::channel::message::AllowedMentions;
use twilight_model::id::Id;

use crate::asistente::{clasificar_error, OrdenAgente};
use crate::chat::{
    confirmacion_de, debe_responder, orden_chat, prompt_chat, separar_orden, Confirmacion,
    MensajeContexto, MensajeEntrante,
};
use crate::db::GuildSettings;
use crate::discord::{build_client, traducir_error, MAX_LONGITUD_MENSAJE};
use crate::propuesta::{con_servidor, EstadoAccion, Propuesta, ResultadoAccion, ResumenPropuesta};

/// Agentes simultáneos en total.
pub const MAX_AGENTES: usize = 3;
/// Tiempo máximo de un agente.
pub const LIMITE_AGENTE: Duration = Duration::from_secs(60);

/// Colas por canal (una respuesta a la vez) y tope global de agentes.
pub struct Coordinador {
    canales: Mutex<HashMap<u64, Arc<tokio::sync::Mutex<()>>>>,
    agentes: Arc<Semaphore>,
    /// Última respuesta (segundos) por (servidor, usuario).
    ultimas: Mutex<HashMap<(u64, u64), u64>>,
    /// Servidores con un constructor de propuestas en marcha.
    constructores: Mutex<HashSet<u64>>,
    /// Propuestas esperando el «sí» de su autor, por (servidor, canal, autor).
    pendientes: Mutex<HashMap<(u64, u64, u64), Pendiente>>,
    /// Servidores donde se está aplicando una propuesta.
    aplicando: Mutex<HashSet<u64>>,
    /// Cuánto vive una propuesta pendiente.
    caducidad: Duration,
}

/// Tiempo que una propuesta espera el «sí».
pub const CADUCIDAD_PROPUESTA: Duration = Duration::from_secs(300);

/// Una propuesta publicada que espera confirmación.
pub struct Pendiente {
    propuesta: Propuesta,
    /// Mensaje del bot que la publicó.
    mensaje_id: u64,
    creada: Instant,
}

/// Qué hay pendiente para una clave (se retira al consultarlo).
pub enum EstadoPendiente {
    Ninguna,
    Caducada,
    Vigente(Pendiente),
}

/// Reserva de «aplicando» de un servidor: se libera al soltarla.
pub struct GuardiaAplicacion {
    coord: Arc<Coordinador>,
    guild: u64,
}

impl Drop for GuardiaAplicacion {
    fn drop(&mut self) {
        self.coord.aplicando.lock().unwrap().remove(&self.guild);
    }
}

/// Reserva del constructor de un servidor: se libera al soltarla.
pub struct GuardiaConstructor {
    coord: Arc<Coordinador>,
    guild: u64,
}

impl Drop for GuardiaConstructor {
    fn drop(&mut self) {
        self.coord.constructores.lock().unwrap().remove(&self.guild);
    }
}

impl Default for Coordinador {
    fn default() -> Self {
        Self::nuevo(MAX_AGENTES)
    }
}

impl Coordinador {
    pub fn nuevo(max_agentes: usize) -> Self {
        Self::con_caducidad(max_agentes, CADUCIDAD_PROPUESTA)
    }

    pub fn con_caducidad(max_agentes: usize, caducidad: Duration) -> Self {
        Self {
            canales: Mutex::new(HashMap::new()),
            agentes: Arc::new(Semaphore::new(max_agentes)),
            ultimas: Mutex::new(HashMap::new()),
            constructores: Mutex::new(HashSet::new()),
            pendientes: Mutex::new(HashMap::new()),
            aplicando: Mutex::new(HashSet::new()),
            caducidad,
        }
    }

    /// Guarda una propuesta pendiente (sustituye la anterior de la misma clave)
    /// y olvida las caducadas.
    pub fn guardar_pendiente(&self, clave: (u64, u64, u64), propuesta: Propuesta, mensaje_id: u64) {
        let mut p = self.pendientes.lock().unwrap();
        p.retain(|_, v| v.creada.elapsed() < self.caducidad);
        p.insert(
            clave,
            Pendiente {
                propuesta,
                mensaje_id,
                creada: Instant::now(),
            },
        );
    }

    /// Retira la propuesta pendiente de la clave.
    pub fn sacar_pendiente(&self, clave: (u64, u64, u64)) -> EstadoPendiente {
        match self.pendientes.lock().unwrap().remove(&clave) {
            None => EstadoPendiente::Ninguna,
            Some(p) if p.creada.elapsed() >= self.caducidad => EstadoPendiente::Caducada,
            Some(p) => EstadoPendiente::Vigente(p),
        }
    }

    /// ¿Hay en ese canal una propuesta vigente de otro autor publicada en `mensaje`?
    fn es_de_otro(&self, guild: u64, canal: u64, autor: u64, mensaje: Option<u64>) -> bool {
        let Some(mensaje) = mensaje else {
            return false;
        };
        self.pendientes.lock().unwrap().iter().any(|(k, v)| {
            k.0 == guild
                && k.1 == canal
                && k.2 != autor
                && v.mensaje_id == mensaje
                && v.creada.elapsed() < self.caducidad
        })
    }

    /// Marca el servidor como «aplicando»; `None` si ya se está aplicando algo.
    pub fn tomar_aplicacion(self: &Arc<Self>, guild: u64) -> Option<GuardiaAplicacion> {
        self.aplicando
            .lock()
            .unwrap()
            .insert(guild)
            .then(|| GuardiaAplicacion {
                coord: self.clone(),
                guild,
            })
    }

    pub fn esta_aplicando(&self, guild: u64) -> bool {
        self.aplicando.lock().unwrap().contains(&guild)
    }

    /// Reserva el constructor del servidor; `None` si ya hay uno en marcha.
    pub fn tomar_constructor(self: &Arc<Self>, guild: u64) -> Option<GuardiaConstructor> {
        self.constructores
            .lock()
            .unwrap()
            .insert(guild)
            .then(|| GuardiaConstructor {
                coord: self.clone(),
                guild,
            })
    }

    /// Un hueco del tope global de agentes.
    pub async fn permiso_agente(&self) -> OwnedSemaphorePermit {
        self.agentes
            .clone()
            .acquire_owned()
            .await
            .expect("el semáforo nunca se cierra")
    }

    /// Espera el turno del canal y después un hueco global. Se suelta al soltar
    /// el resultado. Los locks std se toman en bloques sin `.await`.
    pub async fn turno(&self, canal: u64) -> (OwnedMutexGuard<()>, OwnedSemaphorePermit) {
        let cola = {
            let mut c = self.canales.lock().unwrap();
            c.entry(canal).or_default().clone()
        };
        let guardia = cola.lock_owned().await;
        let permiso = self
            .agentes
            .clone()
            .acquire_owned()
            .await
            .expect("el semáforo nunca se cierra");
        (guardia, permiso)
    }

    fn ultima(&self, guild: u64, usuario: u64) -> Option<u64> {
        self.ultimas.lock().unwrap().get(&(guild, usuario)).copied()
    }

    fn anotar(&self, guild: u64, usuario: u64, ahora_s: u64) {
        self.ultimas
            .lock()
            .unwrap()
            .insert((guild, usuario), ahora_s);
    }
}

fn ahora_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Archivo adjunto de un mensaje.
#[derive(Debug, Clone, Default)]
pub struct Adjunto {
    pub nombre: String,
    pub url: String,
    /// Tamaño declarado por Discord (bytes).
    pub tamano: u64,
}

/// Tamaño máximo de un plan enviado como archivo.
pub const MAX_ARCHIVO: usize = 200 * 1024;
/// Dónde se explica la sintaxis del plan.
pub const ENLACE_MANUAL: &str =
    "https://github.com/BenReynor/ARGUS/blob/main/docs/PLAN_SERVIDOR.md";
/// Líneas de aviso del análisis que se publican como máximo.
const MAX_AVISOS: usize = 10;

fn es_archivo_de_plan(nombre: &str) -> bool {
    let n = nombre.to_lowercase();
    n.ends_with(".md") || n.ends_with(".txt")
}

/// Descarga `url` (http/https) con tope de tamaño y de tiempo.
pub async fn descargar_archivo(url: &str, max: usize) -> Result<Vec<u8>, String> {
    use hyper::body::HttpBody;
    let uri: hyper::Uri = url
        .parse()
        .map_err(|_| "Invalid file address".to_string())?;
    if !matches!(uri.scheme_str(), Some("http") | Some("https")) {
        return Err("Invalid file address".into());
    }
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .build();
    let cliente = hyper::Client::builder().build::<_, hyper::Body>(https);
    let leer = async {
        let resp = cliente
            .get(uri)
            .await
            .map_err(|e| format!("Download failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Download failed: HTTP {}", resp.status().as_u16()));
        }
        let mut cuerpo = resp.into_body();
        let mut datos = Vec::new();
        while let Some(trozo) = cuerpo.data().await {
            let trozo = trozo.map_err(|e| format!("Download failed: {e}"))?;
            datos.extend_from_slice(&trozo);
            if datos.len() > max {
                return Err("The file is too large".to_string());
            }
        }
        Ok(datos)
    };
    tokio::time::timeout(Duration::from_secs(30), leer)
        .await
        .map_err(|_| "Download failed: timed out".to_string())?
}

/// Un mensaje de servidor ya traducido a lo que necesita el conversador.
#[derive(Debug, Clone, Default)]
pub struct Entrada {
    pub adjuntos: Vec<Adjunto>,
    pub msg: MensajeEntrante,
    pub mensaje_id: u64,
    pub guild_id: u64,
    pub texto: String,
    pub autor_nombre: String,
    pub servidor: String,
    pub canal: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resultado {
    /// No había que responder (o el agente no está disponible, ya anotado).
    Ignorado,
    Respondido,
    /// El agente o Discord fallaron: no se respondió nada en Discord.
    Error(String),
}

type Registro = Arc<dyn Fn(String) + Send + Sync>;
type BuscarAgente = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type FuturoPlan = Pin<Box<dyn Future<Output = String> + Send>>;
type FuturoAplicar = Pin<Box<dyn Future<Output = Result<Vec<ResultadoAccion>, String>> + Send>>;

/// Tiempo máximo del constructor de propuestas (lee el servidor con varias herramientas).
pub const LIMITE_CONSTRUCTOR: Duration = Duration::from_secs(600);
/// Tamaño máximo (caracteres) del resumen y la lista de cambios en Discord.
pub const MAX_PROPUESTA: usize = 1800;

/// Lo que el constructor de propuestas necesita de la app.
#[derive(Clone)]
pub struct ContextoOrdenes {
    /// Ruta del binario `argus-mcp`.
    pub ruta_mcp: Arc<dyn Fn() -> Result<String, String> + Send + Sync>,
    /// Plan guardado del servidor (vacío si no hay).
    pub leer_plan: Arc<dyn Fn(u64) -> FuturoPlan + Send + Sync>,
    /// Contexto guardado del servidor (vacío si no hay).
    pub leer_contexto: Arc<dyn Fn(u64) -> FuturoPlan + Send + Sync>,
    /// Roles y canales conocidos del servidor, para validar.
    pub nombres: Arc<dyn Fn(u64) -> crate::propuesta::Nombres + Send + Sync>,
    /// Roles y canales del servidor, para compilar un plan.
    pub existentes:
        Arc<dyn Fn(u64) -> (Vec<crate::state::Role>, Vec<crate::state::Channel>) + Send + Sync>,
    /// Aplica la propuesta (ya con el servidor puesto): `ejecutar_propuesta`.
    pub aplicar: Arc<dyn Fn(Propuesta) -> FuturoAplicar + Send + Sync>,
    /// Idioma de la interfaz.
    pub idioma: Arc<dyn Fn() -> String + Send + Sync>,
}

/// Todo lo necesario para responder con un bot concreto.
#[derive(Clone)]
pub struct Conversador {
    pub token: String,
    pub base_url: Option<String>,
    pub bot_id: u64,
    pub coord: Arc<Coordinador>,
    pub limite: Duration,
    pub registro: Registro,
    pub buscar_agente: BuscarAgente,
    pub limite_constructor: Duration,
    pub ordenes: ContextoOrdenes,
}

/// Parte un texto en trozos de como mucho `max` caracteres, cortando en saltos
/// de línea o espacios cuando se puede.
pub fn partir_mensaje(texto: &str, max: usize) -> Vec<String> {
    let mut trozos = Vec::new();
    let mut resto: Vec<char> = texto.trim().chars().collect();
    while resto.len() > max {
        let ventana = &resto[..max];
        let corte = ventana
            .iter()
            .rposition(|c| *c == '\n')
            .or_else(|| ventana.iter().rposition(|c| c.is_whitespace()))
            .filter(|p| *p > max / 2)
            .unwrap_or(max);
        let trozo: String = resto[..corte].iter().collect();
        let trozo = trozo.trim().to_string();
        if !trozo.is_empty() {
            trozos.push(trozo);
        }
        resto.drain(..corte);
        while resto.first().is_some_and(|c| c.is_whitespace()) {
            resto.remove(0);
        }
    }
    let ultimo: String = resto.iter().collect();
    if !ultimo.trim().is_empty() {
        trozos.push(ultimo.trim().to_string());
    }
    trozos
}

/// Solo se puede mencionar al autor; nunca @everyone, @here ni roles.
pub fn menciones_permitidas(autor_id: u64, avisar_autor: bool) -> AllowedMentions {
    AllowedMentions {
        parse: Vec::new(),
        replied_user: avisar_autor,
        roles: Vec::new(),
        users: Id::new_checked(autor_id).into_iter().collect(),
    }
}

async fn leer_agente(
    out: Option<tokio::process::ChildStdout>,
    err: Option<tokio::process::ChildStderr>,
) -> (String, String) {
    use tokio::io::AsyncReadExt;
    let o = async {
        let mut s = String::new();
        if let Some(mut o) = out {
            let _ = o.read_to_string(&mut s).await;
        }
        s
    };
    let e = async {
        let mut s = String::new();
        if let Some(mut e) = err {
            let _ = e.read_to_string(&mut s).await;
        }
        s
    };
    tokio::join!(o, e)
}

/// Ejecuta la orden y devuelve su salida de texto. Al pasar `limite` mata el
/// proceso y su grupo.
pub async fn ejecutar_texto(orden: &OrdenAgente, limite: Duration) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    crate::asistente::preparar_directorio(orden)?;
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
    let mut hijo = crate::asistente::lanzar_agente(&mut cmd)
        .await
        .map_err(|e| format!("Could not start the AI agent: {e}"))?;
    let pid = hijo.id();
    if let Some(mut stdin) = hijo.stdin.take() {
        let texto = orden.stdin.clone();
        tokio::spawn(async move {
            let _ = stdin.write_all(texto.as_bytes()).await;
        });
    }
    let lectura = leer_agente(hijo.stdout.take(), hijo.stderr.take());
    let resultado = tokio::time::timeout(limite, async {
        let ((o, e), estado) = tokio::join!(lectura, hijo.wait());
        (o, e, estado)
    })
    .await;
    let (salida, errores, estado) = match resultado {
        Ok(r) => r,
        Err(_) => {
            if let Some(p) = pid {
                crate::asistente::matar_grupo(p);
            }
            let _ = hijo.start_kill();
            return Err("The AI agent took too long to reply".into());
        }
    };
    if !matches!(&estado, Ok(s) if s.success()) {
        let crudo = if errores.trim().is_empty() {
            &salida
        } else {
            &errores
        };
        return Err(if crudo.trim().is_empty() {
            "The AI agent stopped unexpectedly".into()
        } else {
            clasificar_error(crudo)
        });
    }
    if salida.trim().is_empty() {
        return Err("The AI agent returned an empty reply".into());
    }
    Ok(salida.trim().to_string())
}

impl Conversador {
    fn log(&self, texto: String) {
        (self.registro)(texto);
    }

    /// Decide, encola y responde. No devuelve error: lo que falle queda en Logs.
    pub async fn atender(&self, e: Entrada, ajustes: &GuildSettings) -> Resultado {
        if let Some(r) = self.confirmacion(&e).await {
            return r;
        }
        let ahora = ahora_s();
        let ultima = self.coord.ultima(e.guild_id, e.msg.autor_id);
        if !debe_responder(&e.msg, self.bot_id, ajustes, ultima, ahora) {
            return Resultado::Ignorado;
        }
        // Se anota al aceptar el mensaje para que una ráfaga no llene la cola.
        self.coord.anotar(e.guild_id, e.msg.autor_id, ahora);

        // Un archivo .md/.txt adjunto es un plan para construir, sin pasar por el chat.
        if let Some(adjunto) = e.adjuntos.iter().find(|a| es_archivo_de_plan(&a.nombre)) {
            self.desde_archivo(&e, ajustes, adjunto).await;
            return Resultado::Respondido;
        }

        let turno = self.coord.turno(e.msg.canal_id).await;
        let inicio = Instant::now();
        let resultado = self.responder(&e, ajustes).await;
        let ms = inicio.elapsed().as_millis();
        // El constructor puede tardar minutos: no retiene la cola del canal.
        drop(turno);
        match &resultado {
            Ok(_) => self.log(format!(
                "[CHAT] Replied in {} #{} to {} with {} in {} ms",
                e.servidor,
                e.canal,
                e.autor_nombre,
                modelo_visible(ajustes),
                ms
            )),
            Err(motivo) => self.log(format!(
                "[WARN] No reply in {} #{} to {}: {}",
                e.servidor, e.canal, e.autor_nombre, motivo
            )),
        }
        match resultado {
            Ok(orden) => {
                if let Some(orden) = orden {
                    self.preparar_orden(&e, ajustes, &orden).await;
                }
                Resultado::Respondido
            }
            Err(m) => Resultado::Error(m),
        }
    }

    /// Trata «sí»/«no» sobre una propuesta pendiente de este autor en este canal.
    /// `None` si el mensaje no tiene que ver (sigue el flujo normal).
    async fn confirmacion(&self, e: &Entrada) -> Option<Resultado> {
        if e.msg.autor_es_bot {
            return None;
        }
        let accion = confirmacion_de(&e.texto)?;
        let clave = (e.guild_id, e.msg.canal_id, e.msg.autor_id);
        let idioma = (self.ordenes.idioma)();
        match self.coord.sacar_pendiente(clave) {
            EstadoPendiente::Ninguna => {
                // El «sí» de otra persona a la propuesta de alguien no hace nada.
                self.coord
                    .es_de_otro(
                        e.guild_id,
                        e.msg.canal_id,
                        e.msg.autor_id,
                        e.msg.responde_a_mensaje,
                    )
                    .then_some(Resultado::Ignorado)
            }
            // Una propuesta caducada no hace nada.
            EstadoPendiente::Caducada => Some(Resultado::Ignorado),
            EstadoPendiente::Vigente(p) => match accion {
                Confirmacion::Descartar => {
                    self.log(format!(
                        "[ORDER] Proposal discarded by {} in {} #{}",
                        e.autor_nombre, e.servidor, e.canal
                    ));
                    let aviso = crate::i18n::tr(&idioma, "Proposal discarded");
                    self.avisar(e, &aviso).await;
                    Some(Resultado::Respondido)
                }
                Confirmacion::Aplicar => {
                    let Some(guardia) = self.coord.tomar_aplicacion(e.guild_id) else {
                        // Otra propuesta se está aplicando: esta sigue esperando.
                        self.coord
                            .guardar_pendiente(clave, p.propuesta, p.mensaje_id);
                        let aviso = crate::i18n::tr(&idioma, MOTIVO_APLICANDO);
                        self.avisar(e, &aviso).await;
                        return Some(Resultado::Respondido);
                    };
                    self.aplicar_propuesta(e, p.propuesta, &idioma).await;
                    drop(guardia);
                    Some(Resultado::Respondido)
                }
            },
        }
    }

    /// Avisa en el canal; un fallo solo queda en el log técnico.
    async fn avisar(&self, e: &Entrada, texto: &str) {
        if let Err(err) = self.enviar(e, texto).await {
            tracing::warn!("No se pudo avisar en el canal: {err}");
        }
    }

    /// Aplica la propuesta (el llamador ya reservó el servidor) y publica el resultado.
    async fn aplicar_propuesta(&self, e: &Entrada, propuesta: Propuesta, idioma: &str) {
        let propuesta = con_servidor(&propuesta, &e.guild_id.to_string());
        let total = propuesta.acciones.len();
        self.log(format!(
            "[ORDER] Applying the proposal of {} in {} #{}: {} actions",
            e.autor_nombre, e.servidor, e.canal, total
        ));
        let inicio = Instant::now();
        let texto = match (self.ordenes.aplicar)(propuesta).await {
            Ok(resultados) => {
                let r = ResumenPropuesta::de(total, resultados);
                self.log(format!(
                    "[ORDER] Applied the proposal of {} in {} #{}: {} done, {} with errors in {} ms",
                    e.autor_nombre,
                    e.servidor,
                    e.canal,
                    r.hechas,
                    r.fallidas + r.omitidas,
                    inicio.elapsed().as_millis()
                ));
                texto_resultado(&r, idioma)
            }
            Err(motivo) => {
                let motivo = crate::i18n::tr(idioma, &motivo);
                self.log(format!(
                    "[WARN] Could not apply the proposal in {} #{} for {}: {}",
                    e.servidor, e.canal, e.autor_nombre, motivo
                ));
                crate::i18n::tr(idioma, &format!("I could not apply the proposal: {motivo}"))
            }
        };
        self.avisar(e, &texto).await;
    }

    /// Responde al mensaje y devuelve la orden detectada, si la hay.
    async fn responder(
        &self,
        e: &Entrada,
        ajustes: &GuildSettings,
    ) -> Result<Option<String>, String> {
        let base = self.base_url.as_deref();
        let canal = e.msg.canal_id.to_string();
        let agente = ajustes.chat_agente.as_str();
        let ruta = (self.buscar_agente)(agente)
            .ok_or_else(|| format!("The AI agent «{agente}» is not installed"))?;
        // «escribiendo…»: un fallo aquí no impide responder.
        if let Some(cid) = Id::new_checked(e.msg.canal_id) {
            if let Ok(r) = build_client(&self.token, base)
                .create_typing_trigger(cid)
                .await
            {
                drop(r);
            }
        }
        let previos = self.leer_previos(&canal, e.mensaje_id).await;
        let autorizado = crate::db::ordenes_activas(ajustes, &e.msg.autor_id.to_string());
        let contexto = (self.ordenes.leer_contexto)(e.guild_id).await;
        let prompt = prompt_chat(
            &ajustes.chat_personalidad,
            &e.servidor,
            &contexto,
            &e.canal,
            &previos,
            ajustes.chat_contexto as usize,
            &e.autor_nombre,
            &e.texto,
            autorizado,
        );
        let orden = orden_chat(agente, &ruta, &ajustes.chat_modelo, &prompt)?;
        let respuesta = ejecutar_texto(&orden, self.limite).await?;
        // La línea `ORDEN:` nunca se publica; se prepara su propuesta aparte.
        let (visible, orden) = separar_orden(&respuesta, autorizado);
        self.enviar(e, &visible).await?;
        Ok(orden)
    }

    /// Prepara la propuesta de una orden y la publica como respuesta al mensaje.
    /// Todo lo que falle queda en Logs y se avisa en el canal con el motivo.
    async fn preparar_orden(&self, e: &Entrada, ajustes: &GuildSettings, orden: &str) {
        let idioma = (self.ordenes.idioma)();
        self.log(format!(
            "[ORDER] Order from {} in {} #{}: {}",
            e.autor_nombre, e.servidor, e.canal, orden
        ));
        // Mientras se aplica una propuesta, no se acepta otra orden en el servidor.
        if self.coord.esta_aplicando(e.guild_id) {
            self.fallo_orden(e, &idioma, MOTIVO_APLICANDO).await;
            return;
        }
        let Some(_guardia) = self.coord.tomar_constructor(e.guild_id) else {
            let motivo = "Another proposal is already being prepared in this server";
            self.fallo_orden(e, &idioma, motivo).await;
            return;
        };
        if let Err(err) = self
            .enviar(e, &crate::i18n::tr(&idioma, "Preparing the proposal…"))
            .await
        {
            self.log(format!(
                "[WARN] Could not prepare the proposal in {} #{} for {}: {}",
                e.servidor, e.canal, e.autor_nombre, err
            ));
            return;
        }
        let inicio = Instant::now();
        match self.construir(e, ajustes, orden, &idioma).await {
            Ok(ev) => {
                self.publicar_propuesta(e, ajustes, ev, "", &idioma, inicio)
                    .await
            }
            Err(motivo) => self.fallo_orden(e, &idioma, &motivo).await,
        }
    }

    /// Publica la propuesta (más `extra`, p. ej. avisos) y, según los ajustes,
    /// la deja esperando el «sí» o la aplica directamente.
    async fn publicar_propuesta(
        &self,
        e: &Entrada,
        ajustes: &GuildSettings,
        ev: crate::asistente::EventoPropuesta,
        extra: &str,
        idioma: &str,
        inicio: Instant,
    ) {
        let mut texto = texto_propuesta(&ev, false, idioma, MAX_PROPUESTA);
        texto.push_str(extra);
        if ajustes.chat_ordenes_confirmar {
            texto.push_str("\n\n");
            texto.push_str(&crate::i18n::tr(
                idioma,
                "Reply **yes** within the next 5 minutes to apply it",
            ));
        }
        match self.enviar(e, &texto).await {
            Ok(mensaje_id) => {
                self.log(format!(
                    "[ORDER] Proposal for {} in {} #{}: {} actions in {} ms",
                    e.autor_nombre,
                    e.servidor,
                    e.canal,
                    ev.propuesta.acciones.len(),
                    inicio.elapsed().as_millis()
                ));
                if ajustes.chat_ordenes_confirmar {
                    self.coord.guardar_pendiente(
                        (e.guild_id, e.msg.canal_id, e.msg.autor_id),
                        ev.propuesta,
                        mensaje_id,
                    );
                } else if let Some(guardia) = self.coord.tomar_aplicacion(e.guild_id) {
                    self.aplicar_propuesta(e, ev.propuesta, idioma).await;
                    drop(guardia);
                } else {
                    self.fallo_orden(e, idioma, MOTIVO_APLICANDO).await;
                }
            }
            Err(err) => self.log(format!(
                "[WARN] Could not prepare the proposal in {} #{} for {}: {}",
                e.servidor, e.canal, e.autor_nombre, err
            )),
        }
    }

    /// Avisa de que el archivo no se pudo usar (Logs y canal).
    async fn fallo_archivo(&self, e: &Entrada, idioma: &str, motivo: &str) {
        let motivo = crate::i18n::tr(idioma, motivo);
        self.log(format!(
            "[WARN] Could not build from a file in {} #{} for {}: {}",
            e.servidor, e.canal, e.autor_nombre, motivo
        ));
        self.avisar(e, &motivo).await;
    }

    /// Construye una propuesta desde un plan adjunto, sin IA salvo las
    /// descripciones `generar:`. Después sigue el flujo de confirmación normal.
    async fn desde_archivo(&self, e: &Entrada, ajustes: &GuildSettings, adjunto: &Adjunto) {
        let idioma = (self.ordenes.idioma)();
        if !crate::db::ordenes_activas(ajustes, &e.msg.autor_id.to_string()) {
            let motivo = "You are not allowed to give orders from Discord";
            self.fallo_archivo(e, &idioma, motivo).await;
            return;
        }
        if self.coord.esta_aplicando(e.guild_id) {
            self.fallo_archivo(e, &idioma, MOTIVO_APLICANDO).await;
            return;
        }
        let Some(_guardia) = self.coord.tomar_constructor(e.guild_id) else {
            let motivo = "Another proposal is already being prepared in this server";
            self.fallo_archivo(e, &idioma, motivo).await;
            return;
        };
        let inicio = Instant::now();
        self.log(format!(
            "[ORDER] File {} from {} in {} #{}",
            adjunto.nombre, e.autor_nombre, e.servidor, e.canal
        ));
        match self.compilar_archivo(e, ajustes, adjunto, &idioma).await {
            Ok((ev, extra)) => {
                self.publicar_propuesta(e, ajustes, ev, &extra, &idioma, inicio)
                    .await
            }
            Err(motivo) => self.fallo_archivo(e, &idioma, &motivo).await,
        }
    }

    /// Descarga, analiza y compila el archivo. Devuelve la propuesta validada y
    /// el texto de avisos para publicar tras ella.
    async fn compilar_archivo(
        &self,
        e: &Entrada,
        ajustes: &GuildSettings,
        adjunto: &Adjunto,
        idioma: &str,
    ) -> Result<(crate::asistente::EventoPropuesta, String), String> {
        if adjunto.tamano > MAX_ARCHIVO as u64 {
            return Err("The file is too large (the maximum is 200 KB)".into());
        }
        let datos = descargar_archivo(&adjunto.url, MAX_ARCHIVO)
            .await
            .map_err(|m| {
                if m == "The file is too large" {
                    "The file is too large (the maximum is 200 KB)".to_string()
                } else {
                    m
                }
            })?;
        let texto = String::from_utf8(datos).map_err(|_| "The file is not valid text")?;
        let analizado = crate::plan_estructurado::analizar(&texto);
        if !analizado.tiene_estructura() {
            return Err(format!(
                "The file has no structure to build from. Read the manual: {ENLACE_MANUAL}"
            ));
        }
        let (roles, canales) = (self.ordenes.existentes)(e.guild_id);
        let mut comp = crate::plan_compilar::compilar(
            &analizado,
            &crate::plan_compilar::Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        if !comp.pendientes.is_empty() {
            let agente = ajustes.ordenes_agente.as_str();
            match (self.buscar_agente)(agente) {
                Some(ruta) => {
                    let contexto = (self.ordenes.leer_contexto)(e.guild_id).await;
                    let _permiso = self.coord.permiso_agente().await;
                    crate::plan_descripciones::escribir_descripciones(
                        &mut comp,
                        &contexto,
                        idioma,
                        agente,
                        &ruta,
                        &ajustes.ordenes_modelo,
                        self.limite_constructor,
                    )
                    .await;
                }
                None => comp.avisos.push(
                    "The descriptions were not written: the AI agent is not installed".into(),
                ),
            }
        }
        let nombres = (self.ordenes.nombres)(e.guild_id);
        let mut propuesta = comp.propuesta;
        propuesta.resumen = crate::i18n::tr(idioma, &propuesta.resumen);
        let validacion = crate::propuesta::validar_traducida(&propuesta, &nombres, idioma);
        if let Some(err) = validacion.errores.first() {
            return Err(format!(
                "The plan is not valid: {}: {}",
                err.herramienta, err.mensaje
            ));
        }
        let mut lineas: Vec<String> = analizado
            .avisos
            .iter()
            .map(|a| {
                let motivo = crate::i18n::tr(idioma, &a.mensaje);
                crate::i18n::tr(idioma, &format!("Line {}: {}", a.linea, motivo))
            })
            .collect();
        lineas.extend(comp.avisos.iter().map(|a| crate::i18n::tr(idioma, a)));
        let mut extra = String::new();
        if !lineas.is_empty() {
            extra.push_str("\n\n");
            extra.push_str(&crate::i18n::tr(idioma, "Warnings:"));
            for l in lineas.iter().take(MAX_AVISOS) {
                let l: String = l.chars().take(200).collect();
                extra.push_str(&format!("\n⚠ {l}"));
            }
            if lineas.len() > MAX_AVISOS {
                extra.push('\n');
                extra.push_str(&crate::i18n::tr(
                    idioma,
                    &format!("…and {} more warnings", lineas.len() - MAX_AVISOS),
                ));
            }
        }
        Ok((
            crate::asistente::EventoPropuesta {
                propuesta,
                validacion,
            },
            extra,
        ))
    }

    /// Anota el fallo en Logs y avisa en el canal con el motivo traducido.
    async fn fallo_orden(&self, e: &Entrada, idioma: &str, motivo: &str) {
        // El motivo se traduce antes de meterlo en la frase (un hueco no se traduce).
        let motivo = crate::i18n::tr(idioma, motivo);
        self.log(format!(
            "[WARN] Could not prepare the proposal in {} #{} for {}: {}",
            e.servidor, e.canal, e.autor_nombre, motivo
        ));
        let aviso = crate::i18n::tr(
            idioma,
            &format!("I could not prepare the proposal: {motivo}"),
        );
        if let Err(err) = self.enviar(e, &aviso).await {
            tracing::warn!("No se pudo avisar del fallo de la orden: {err}");
        }
    }

    /// Lanza el constructor (modo Execute, solo lectura) y valida su propuesta.
    async fn construir(
        &self,
        e: &Entrada,
        ajustes: &GuildSettings,
        orden: &str,
        idioma: &str,
    ) -> Result<crate::asistente::EventoPropuesta, String> {
        use crate::asistente::{
            orden_agente, prompt_asistente, propuesta_de_respuesta, texto_del_constructor,
            ModoAsistente,
        };
        let agente = ajustes.ordenes_agente.as_str();
        let ruta = (self.buscar_agente)(agente)
            .ok_or_else(|| format!("The AI agent «{agente}» is not installed"))?;
        let mcp = (self.ordenes.ruta_mcp)()?;
        let plan = (self.ordenes.leer_plan)(e.guild_id).await;
        let contexto = (self.ordenes.leer_contexto)(e.guild_id).await;
        let gid = e.guild_id.to_string();
        let prompt = prompt_asistente(
            ModoAsistente::Execute,
            (&gid, &e.servidor),
            &plan,
            &contexto,
            idioma,
            orden,
        );
        let lanzar = orden_agente(
            agente,
            &ruta,
            Path::new(&mcp),
            &ajustes.ordenes_modelo,
            ModoAsistente::Execute,
            None,
            &prompt,
        )?;
        let _permiso = self.coord.permiso_agente().await;
        let texto = texto_del_constructor(&lanzar, agente, self.limite_constructor).await?;
        let nombres = (self.ordenes.nombres)(e.guild_id);
        propuesta_de_respuesta(&texto, |p| {
            crate::propuesta::validar_traducida(p, &nombres, idioma)
        })
    }

    /// Mensajes previos del canal, del más antiguo al más reciente, sin el que
    /// se responde. Si no se pueden leer, se responde sin contexto.
    async fn leer_previos(&self, canal: &str, mensaje_id: u64) -> Vec<MensajeContexto> {
        let id = mensaje_id.to_string();
        match crate::mensajes::fetch_channel_messages(&self.token, self.base_url.as_deref(), canal)
            .await
        {
            Ok(mut m) => {
                m.reverse();
                m.into_iter()
                    .filter(|m| m.id != id && !m.content.trim().is_empty())
                    .map(|m| MensajeContexto {
                        autor: m.author_name,
                        texto: m.content,
                    })
                    .collect()
            }
            Err(err) => {
                tracing::warn!("No se pudo leer el contexto del canal: {err}");
                Vec::new()
            }
        }
    }

    /// Publica la respuesta y devuelve el id del primer mensaje publicado (0 si
    /// Discord no lo devolvió legible).
    async fn enviar(&self, e: &Entrada, respuesta: &str) -> Result<u64, String> {
        let client = build_client(&self.token, self.base_url.as_deref());
        let cid = Id::new_checked(e.msg.canal_id).ok_or("Invalid channel ID")?;
        let mid = Id::new_checked(e.mensaje_id).ok_or("Invalid message ID")?;
        let mut primero = 0;
        for (i, trozo) in partir_mensaje(respuesta, MAX_LONGITUD_MENSAJE)
            .iter()
            .enumerate()
        {
            // Solo el primer trozo avisa al autor.
            let permitidas = menciones_permitidas(e.msg.autor_id, i == 0);
            let publicado = client
                .create_message(cid)
                .reply(mid)
                .fail_if_not_exists(false)
                .allowed_mentions(Some(&permitidas))
                .content(trozo)
                .map_err(|err| format!("Invalid message: {err}"))?
                .await
                .map_err(traducir_error)?;
            if i == 0 {
                primero = publicado
                    .model()
                    .await
                    .map(|m| m.id.get())
                    .unwrap_or_default();
            }
        }
        Ok(primero)
    }
}

/// Motivo cuando ya se está aplicando una propuesta en el servidor.
const MOTIVO_APLICANDO: &str = "A proposal is being applied in this server";
/// Errores que se listan al publicar el resultado.
const MAX_ERRORES: usize = 10;

/// Resultado de aplicar: «Hecho: N cambios, M con error» y los errores.
pub fn texto_resultado(r: &ResumenPropuesta, idioma: &str) -> String {
    let fallos = r.fallidas + r.omitidas;
    let mut texto = crate::i18n::tr(
        idioma,
        &format!("Done: {} changes, {} with errors", r.hechas, fallos),
    );
    let con_error: Vec<&ResultadoAccion> = r
        .resultados
        .iter()
        .filter(|x| x.estado != EstadoAccion::Ok)
        .collect();
    for x in con_error.iter().take(MAX_ERRORES) {
        let mensaje: String = x.mensaje.chars().take(200).collect();
        texto.push_str(&format!("\n• {}: {}", x.descripcion, mensaje));
    }
    if con_error.len() > MAX_ERRORES {
        texto.push('\n');
        texto.push_str(&crate::i18n::tr(
            idioma,
            &format!("…and {} more errors", con_error.len() - MAX_ERRORES),
        ));
    }
    if r.canceladas > 0 {
        texto.push('\n');
        texto.push_str(&crate::i18n::tr(
            idioma,
            &format!("{} actions were not attempted", r.canceladas),
        ));
    }
    texto
}

/// Texto que se publica: resumen y cambios agrupados (como mucho `max` caracteres;
/// el resto se cuenta en «…y N cambios más») y, si hace falta, el aviso de confirmar.
pub fn texto_propuesta(
    ev: &crate::asistente::EventoPropuesta,
    confirmar: bool,
    idioma: &str,
    max: usize,
) -> String {
    let mut texto: String = ev.propuesta.resumen.trim().chars().take(max / 3).collect();
    for (i, c) in ev.validacion.cambios.iter().enumerate() {
        let linea = format!("\n• {c}");
        if texto.chars().count() + linea.chars().count() > max {
            let resto = ev.validacion.cambios.len() - i;
            texto.push('\n');
            texto.push_str(&crate::i18n::tr(
                idioma,
                &format!("…and {resto} more changes"),
            ));
            break;
        }
        texto.push_str(&linea);
    }
    if confirmar {
        texto.push_str("\n\n");
        texto.push_str(&crate::i18n::tr(
            idioma,
            "Reply **yes** within the next 5 minutes to apply it",
        ));
    }
    texto
}

/// Modelo para el log: el elegido o, si está vacío, el del agente.
fn modelo_visible(a: &GuildSettings) -> String {
    if a.chat_modelo.trim().is_empty() {
        format!("{} (default)", a.chat_agente)
    } else {
        format!("{} {}", a.chat_agente, a.chat_modelo)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const BOT: u64 = 900;

    /// Agente falso: ignora los argumentos e imprime `cuerpo` (o ejecuta `script`).
    fn agente_falso(nombre: &str, script: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argus-test-{}-{nombre}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("agente.sh");
        std::fs::write(&ruta, format!("#!/bin/sh\ncat > /dev/null\n{script}\n")).unwrap();
        std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755)).unwrap();
        ruta
    }

    fn conversador(
        server: &MockServer,
        agente: PathBuf,
        limite: Duration,
    ) -> (Conversador, Arc<Mutex<Vec<String>>>) {
        let logs = Arc::new(Mutex::new(Vec::new()));
        let l = logs.clone();
        let c = Conversador {
            token: "token-falso".into(),
            base_url: Some(server.address().to_string()),
            bot_id: BOT,
            coord: Arc::new(Coordinador::default()),
            limite,
            registro: Arc::new(move |t| l.lock().unwrap().push(t)),
            buscar_agente: Arc::new(move |_| Some(agente.clone())),
            limite_constructor: Duration::from_secs(20),
            ordenes: ContextoOrdenes {
                ruta_mcp: Arc::new(|| Ok("/opt/argus-mcp".to_string())),
                leer_plan: Arc::new(|_| Box::pin(async { "PLAN-DEL-SERVIDOR".to_string() })),
                leer_contexto: Arc::new(|_| {
                    Box::pin(async { "CONTEXTO-DEL-SERVIDOR".to_string() })
                }),
                nombres: Arc::new(|_| crate::propuesta::Nombres::default()),
                existentes: Arc::new(|_| (Vec::new(), Vec::new())),
                aplicar: Arc::new(|_| Box::pin(async { Ok(Vec::new()) })),
                idioma: Arc::new(|| "en".to_string()),
            },
        };
        (c, logs)
    }

    fn ajustes() -> GuildSettings {
        GuildSettings {
            personality_enabled: true,
            chat_modelo: "mod/x".into(),
            ..GuildSettings::default()
        }
    }

    fn entrada(autor: u64, menciona: bool) -> Entrada {
        Entrada {
            msg: MensajeEntrante {
                autor_id: autor,
                canal_id: 10,
                menciones: if menciona { vec![BOT] } else { vec![] },
                ..Default::default()
            },
            adjuntos: Vec::new(),
            mensaje_id: 500,
            guild_id: 555,
            texto: "hola".into(),
            autor_nombre: "Ana".into(),
            servidor: "Mi Server".into(),
            canal: "general".into(),
        }
    }

    async fn mock_discord(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/10/typing"))
            .respond_with(ResponseTemplate::new(204))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/channels/10/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "500", "content": "hola", "author": {"id": "1", "username": "ana"}},
                {"id": "499", "content": "antes", "author": {"id": "2", "username": "luis"}}
            ])))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/10/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "600", "channel_id": "10", "content": "x", "timestamp":
                "2026-10-27T10:00:00.000000+00:00", "edited_timestamp": null, "tts": false,
                "mention_everyone": false, "mentions": [], "mention_roles": [],
                "attachments": [], "embeds": [], "pinned": false, "type": 0,
                "author": {"id": "900", "username": "bot", "discriminator": "0", "avatar": null}
            })))
            .mount(server)
            .await;
    }

    async fn envios(server: &MockServer) -> Vec<serde_json::Value> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method.as_str() == "POST" && r.url.path().ends_with("/messages"))
            .map(|r| serde_json::from_slice(&r.body).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn responde_a_una_mencion_como_respuesta_y_solo_menciona_al_autor() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, logs) = conversador(
            &server,
            agente_falso("ok", "echo 'Hola Ana @everyone'"),
            LIMITE_AGENTE,
        );
        let r = c.atender(entrada(1, true), &ajustes()).await;
        assert_eq!(r, Resultado::Respondido);

        let enviados = envios(&server).await;
        assert_eq!(enviados.len(), 1);
        let m = &enviados[0];
        assert_eq!(m["content"], "Hola Ana @everyone");
        assert_eq!(m["message_reference"]["message_id"], "500");
        let am = &m["allowed_mentions"];
        assert_eq!(am["parse"].as_array().map_or(0, |a| a.len()), 0);
        assert_eq!(am["users"], serde_json::json!(["1"]));
        assert_eq!(am["roles"].as_array().map_or(0, |a| a.len()), 0);
        assert_eq!(am["replied_user"], true);

        // «escribiendo…» y lectura del contexto.
        let reqs = server.received_requests().await.unwrap();
        assert!(reqs.iter().any(|r| r.url.path().ends_with("/typing")));
        assert!(reqs
            .iter()
            .any(|r| r.method.as_str() == "GET" && r.url.path().ends_with("/messages")));

        let logs = logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        for parte in ["Mi Server", "general", "Ana", "mod/x"] {
            assert!(logs[0].contains(parte), "falta {parte} en {}", logs[0]);
        }
        assert!(logs[0].contains(" ms"));
    }

    #[tokio::test]
    async fn no_responde_a_un_mensaje_normal_ni_a_un_bot() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, logs) = conversador(&server, agente_falso("no", "echo hola"), LIMITE_AGENTE);
        assert_eq!(
            c.atender(entrada(1, false), &ajustes()).await,
            Resultado::Ignorado
        );
        let mut de_bot = entrada(2, true);
        de_bot.msg.autor_es_bot = true;
        assert_eq!(c.atender(de_bot, &ajustes()).await, Resultado::Ignorado);
        assert!(server.received_requests().await.unwrap().is_empty());
        assert!(logs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn respeta_la_espera_por_usuario() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, _) = conversador(&server, agente_falso("espera", "echo hola"), LIMITE_AGENTE);
        let a = GuildSettings {
            chat_espera_s: 3600,
            ..ajustes()
        };
        assert_eq!(c.atender(entrada(1, true), &a).await, Resultado::Respondido);
        assert_eq!(c.atender(entrada(1, true), &a).await, Resultado::Ignorado);
        // Otro usuario no espera.
        assert_eq!(c.atender(entrada(3, true), &a).await, Resultado::Respondido);
        assert_eq!(envios(&server).await.len(), 2);
    }

    /// Agente que sirve de chat (responde con una orden) y de constructor (imprime
    /// `constructor` en stream-json). Guarda lo recibido junto al script.
    fn agente_con_orden(nombre: &str, constructor: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argus-test-{}-{nombre}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("agente.sh");
        let script = format!(
            r#"#!/bin/sh
case "$*" in
*stream-json*)
  cat > "$(dirname "$0")/stdin.txt"
  printf '%s' "$*" > "$(dirname "$0")/args.txt"
  {constructor}
  ;;
*)
  cat > /dev/null
  printf 'Voy a prepararlo.\nORDEN: crea un rol Mods\n'
  ;;
esac
"#
        );
        std::fs::write(&ruta, script).unwrap();
        std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o755)).unwrap();
        ruta
    }

    const PROPUESTA_OK: &str = r#"cat <<'EOF'
{"type":"assistant","message":{"content":[{"type":"text","text":"Listo\n```json\n{\"resumen\":\"Crear el rol Mods\",\"acciones\":[{\"herramienta\":\"crear_rol\",\"args\":{\"nombre\":\"Mods\"},\"descripcion\":\"Create Mods\"}]}\n```"}]}}
EOF"#;

    fn ajustes_ordenes(confirmar: bool) -> GuildSettings {
        GuildSettings {
            chat_ordenes: true,
            chat_ordenes_usuarios: vec!["1".into()],
            chat_ordenes_confirmar: confirmar,
            ..ajustes()
        }
    }

    fn textos(envios: &[serde_json::Value]) -> Vec<String> {
        envios
            .iter()
            .map(|b| b["content"].as_str().unwrap_or("").to_string())
            .collect()
    }

    #[tokio::test]
    async fn una_orden_publica_aviso_y_propuesta_con_confirmacion() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let agente = agente_con_orden("orden-ok", PROPUESTA_OK);
        let dir = agente.parent().unwrap().to_path_buf();
        let (c, logs) = conversador(&server, agente, LIMITE_AGENTE);
        let r = c.atender(entrada(1, true), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Respondido);
        let env = envios(&server).await;
        let t = textos(&env);
        assert_eq!(t.len(), 3, "{t:?}");
        assert_eq!(t[0], "Voy a prepararlo.");
        assert_eq!(t[1], "Preparing the proposal…");
        assert!(t[2].starts_with("Crear el rol Mods"), "{}", t[2]);
        assert!(t[2].contains("• Roles to create: 1"), "{}", t[2]);
        assert!(t[2].contains("Reply **yes** within the next 5 minutes to apply it"));
        assert!(!t.iter().any(|x| x.contains("ORDEN:")));
        // Todo se publica como respuesta al mensaje del usuario.
        for b in &env {
            assert_eq!(b["message_reference"]["message_id"], "500");
        }
        // El constructor recibe el plan, la orden y solo herramientas de lectura.
        let stdin = std::fs::read_to_string(dir.join("stdin.txt")).unwrap();
        assert!(stdin.contains("PLAN-DEL-SERVIDOR"), "{stdin}");
        assert!(stdin.contains("crea un rol Mods"));
        assert!(stdin.contains("## Execute mode"));
        assert!(stdin.contains("(id 555)"));
        let args = std::fs::read_to_string(dir.join("args.txt")).unwrap();
        assert!(args.contains("--model sonnet"), "{args}");
        assert!(args.contains("mcp__argus__crear_rol"), "{args}");
        assert!(args.contains("--disallowedTools"));
        let logs = logs.lock().unwrap();
        assert!(logs.iter().any(|l| l.starts_with("[ORDER] Order from Ana")));
        assert!(logs
            .iter()
            .any(|l| l.starts_with("[ORDER] Proposal for Ana")));
    }

    #[tokio::test]
    async fn sin_confirmacion_no_pide_el_si() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, _) = conversador(
            &server,
            agente_con_orden("orden-sin-conf", PROPUESTA_OK),
            LIMITE_AGENTE,
        );
        c.atender(entrada(1, true), &ajustes_ordenes(false)).await;
        let t = textos(&envios(&server).await);
        // Aviso, propuesta (sin pedir el «sí») y el resultado de aplicarla.
        assert_eq!(t.len(), 4, "{t:?}");
        assert!(!t[2].contains("Reply **yes**"));
    }

    #[tokio::test]
    async fn una_propuesta_invalida_avisa_con_el_motivo_y_queda_en_logs() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let invalida = r#"cat <<'EOF'
{"type":"assistant","message":{"content":[{"type":"text","text":"```json\n{\"resumen\":\"x\",\"acciones\":[{\"herramienta\":\"borrar_todo\",\"args\":{},\"descripcion\":\"d\"}]}\n```"}]}}
EOF"#;
        let (c, logs) = conversador(
            &server,
            agente_con_orden("orden-invalida", invalida),
            LIMITE_AGENTE,
        );
        c.atender(entrada(1, true), &ajustes_ordenes(true)).await;
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 3, "{t:?}");
        assert!(
            t[2].starts_with(
                "I could not prepare the proposal: The assistant's proposal is not valid"
            ),
            "{}",
            t[2]
        );
        assert!(logs
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("[WARN] Could not prepare the proposal")));
    }

    #[tokio::test]
    async fn un_agente_constructor_que_falla_avisa_con_el_motivo() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, _) = conversador(
            &server,
            agente_con_orden("orden-falla", "echo 'Please run /login' >&2; exit 1"),
            LIMITE_AGENTE,
        );
        c.atender(entrada(1, true), &ajustes_ordenes(true)).await;
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 3, "{t:?}");
        assert!(t[2].starts_with("I could not prepare the proposal: "));
        assert!(t[2].contains("not signed in"), "{}", t[2]);
    }

    #[tokio::test]
    async fn un_solo_constructor_a_la_vez_por_servidor() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let agente = agente_con_orden("orden-ocupado", PROPUESTA_OK);
        let dir = agente.parent().unwrap().to_path_buf();
        let (c, _) = conversador(&server, agente, LIMITE_AGENTE);
        let guardia = c.coord.tomar_constructor(555).expect("libre");
        assert!(c.coord.tomar_constructor(555).is_none());
        assert!(c.coord.tomar_constructor(556).is_some());
        c.atender(entrada(1, true), &ajustes_ordenes(true)).await;
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 2, "{t:?}");
        assert!(t[1].contains("Another proposal is already being prepared"));
        assert!(!dir.join("stdin.txt").exists());
        drop(guardia);
        assert!(c.coord.tomar_constructor(555).is_some());
    }

    type Aplicadas = Arc<Mutex<Vec<Propuesta>>>;

    /// Sustituye el aplicador por uno falso que anota lo recibido y devuelve un
    /// resultado correcto por acción.
    fn aplicador_falso(c: &mut Conversador) -> Aplicadas {
        let aplicadas: Aplicadas = Arc::new(Mutex::new(Vec::new()));
        let a = aplicadas.clone();
        c.ordenes.aplicar = Arc::new(move |p| {
            a.lock().unwrap().push(p.clone());
            Box::pin(async move {
                Ok(p.acciones
                    .iter()
                    .enumerate()
                    .map(|(indice, x)| ResultadoAccion {
                        indice,
                        herramienta: x.herramienta.clone(),
                        descripcion: x.descripcion.clone(),
                        estado: EstadoAccion::Ok,
                        mensaje: "ok".into(),
                    })
                    .collect())
            })
        });
        aplicadas
    }

    /// Mensaje de confirmación: responde a la propuesta publicada (id 600).
    fn confirmar(autor: u64, texto: &str) -> Entrada {
        let mut e = entrada(autor, false);
        e.mensaje_id = 700;
        e.texto = texto.into();
        e.msg.responde_a_mensaje = Some(600);
        e
    }

    /// Prepara una propuesta del autor 1 con confirmación y devuelve cuántos
    /// mensajes se publicaron hasta ahora.
    async fn con_propuesta_pendiente(c: &Conversador, server: &MockServer) -> usize {
        let r = c.atender(entrada(1, true), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Respondido);
        let n = envios(server).await.len();
        assert_eq!(n, 3);
        n
    }

    #[tokio::test]
    async fn el_si_del_autor_aplica_la_propuesta_una_sola_vez() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (mut c, logs) = conversador(
            &server,
            agente_con_orden("conf-aplica", PROPUESTA_OK),
            LIMITE_AGENTE,
        );
        let aplicadas = aplicador_falso(&mut c);
        let antes = con_propuesta_pendiente(&c, &server).await;
        assert!(aplicadas.lock().unwrap().is_empty());

        let r = c.atender(confirmar(1, "Sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Respondido);
        {
            let a = aplicadas.lock().unwrap();
            assert_eq!(a.len(), 1);
            assert_eq!(a[0].acciones[0].herramienta, "crear_rol");
            assert_eq!(a[0].acciones[0].args["guild_id"], "555");
        }
        let env = envios(&server).await;
        let t = textos(&env);
        assert_eq!(t.len(), antes + 1, "{t:?}");
        assert_eq!(t[antes], "Done: 1 changes, 0 with errors");
        assert_eq!(env[antes]["message_reference"]["message_id"], "700");

        // Ya aplicada: un segundo «sí» no hace nada.
        let r = c.atender(confirmar(1, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Ignorado);
        assert_eq!(aplicadas.lock().unwrap().len(), 1);
        assert_eq!(envios(&server).await.len(), antes + 1);
        assert!(logs
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("[ORDER] Applied the proposal of Ana")));
    }

    #[tokio::test]
    async fn el_si_de_otra_persona_no_hace_nada() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (mut c, _) = conversador(
            &server,
            agente_con_orden("conf-otro", PROPUESTA_OK),
            LIMITE_AGENTE,
        );
        let aplicadas = aplicador_falso(&mut c);
        let antes = con_propuesta_pendiente(&c, &server).await;

        // Sin mención y con ella (respuesta al bot): se ignora igual.
        let r = c.atender(confirmar(2, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Ignorado);
        let mut con_mencion = confirmar(2, "yes");
        con_mencion.msg.menciones = vec![BOT];
        con_mencion.msg.responde_a_autor = Some(BOT);
        let r = c.atender(con_mencion, &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Ignorado);
        assert!(aplicadas.lock().unwrap().is_empty());
        assert_eq!(envios(&server).await.len(), antes);

        // La propuesta sigue esperando a su autor.
        c.atender(confirmar(1, "ok"), &ajustes_ordenes(true)).await;
        assert_eq!(aplicadas.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn una_propuesta_caducada_no_se_aplica() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (mut c, _) = conversador(
            &server,
            agente_con_orden("conf-caduca", PROPUESTA_OK),
            LIMITE_AGENTE,
        );
        c.coord = Arc::new(Coordinador::con_caducidad(3, Duration::from_millis(300)));
        let aplicadas = aplicador_falso(&mut c);
        let antes = con_propuesta_pendiente(&c, &server).await;
        tokio::time::sleep(Duration::from_millis(400)).await;

        let r = c.atender(confirmar(1, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Ignorado);
        assert!(aplicadas.lock().unwrap().is_empty());
        assert_eq!(envios(&server).await.len(), antes);
        // Y ya no queda nada que confirmar.
        let r = c.atender(confirmar(1, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Ignorado);
        assert!(aplicadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancelar_descarta_la_propuesta() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (mut c, _) = conversador(
            &server,
            agente_con_orden("conf-cancela", PROPUESTA_OK),
            LIMITE_AGENTE,
        );
        let aplicadas = aplicador_falso(&mut c);
        let antes = con_propuesta_pendiente(&c, &server).await;

        let r = c
            .atender(confirmar(1, "cancelar"), &ajustes_ordenes(true))
            .await;
        assert_eq!(r, Resultado::Respondido);
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), antes + 1);
        assert_eq!(t[antes], "Proposal discarded");

        let r = c.atender(confirmar(1, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Ignorado);
        assert!(aplicadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn sin_confirmacion_se_aplica_directamente_tras_prepararla() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (mut c, _) = conversador(
            &server,
            agente_con_orden("conf-directa", PROPUESTA_OK),
            LIMITE_AGENTE,
        );
        let aplicadas = aplicador_falso(&mut c);
        c.atender(entrada(1, true), &ajustes_ordenes(false)).await;
        assert_eq!(aplicadas.lock().unwrap().len(), 1);
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 4, "{t:?}");
        assert!(!t[2].contains("Reply **yes**"));
        assert_eq!(t[3], "Done: 1 changes, 0 with errors");
        // Nada queda pendiente.
        let r = c.atender(confirmar(1, "sí"), &ajustes_ordenes(false)).await;
        assert_eq!(r, Resultado::Ignorado);
        assert_eq!(aplicadas.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn mientras_se_aplica_no_se_acepta_otra_orden_en_el_servidor() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let agente = agente_con_orden("conf-ocupado", PROPUESTA_OK);
        let dir = agente.parent().unwrap().to_path_buf();
        let (mut c, _) = conversador(&server, agente, LIMITE_AGENTE);
        let aplicadas = aplicador_falso(&mut c);
        let antes = con_propuesta_pendiente(&c, &server).await;
        let _ = std::fs::remove_file(dir.join("stdin.txt"));

        let guardia = c.coord.tomar_aplicacion(555).expect("libre");
        // Otra orden: se rechaza sin lanzar el constructor.
        let otra = entrada(3, true);
        let varios = GuildSettings {
            chat_ordenes_usuarios: vec!["1".into(), "3".into()],
            ..ajustes_ordenes(true)
        };
        c.atender(otra, &varios).await;
        // Un «sí» mientras tanto avisa y la propuesta sigue pendiente.
        c.atender(confirmar(1, "sí"), &varios).await;
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), antes + 3, "{t:?}");
        assert_eq!(t[antes], "Voy a prepararlo.");
        assert_eq!(
            t[antes + 1],
            "I could not prepare the proposal: A proposal is being applied in this server"
        );
        assert_eq!(t[antes + 2], "A proposal is being applied in this server");
        assert!(!dir.join("stdin.txt").exists());
        assert!(aplicadas.lock().unwrap().is_empty());

        drop(guardia);
        c.atender(confirmar(1, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(aplicadas.lock().unwrap().len(), 1);
    }

    #[test]
    fn el_resultado_cuenta_y_lista_los_errores() {
        let r = |i: usize, estado: EstadoAccion, m: &str| ResultadoAccion {
            indice: i,
            herramienta: "crear_rol".into(),
            descripcion: format!("Create {i}"),
            estado,
            mensaje: m.into(),
        };
        let resumen = ResumenPropuesta::de(
            3,
            vec![
                r(0, EstadoAccion::Ok, "hecho"),
                r(1, EstadoAccion::Error, "Missing permission"),
                r(2, EstadoAccion::Omitida, "No role named X"),
            ],
        );
        let t = texto_resultado(&resumen, "en");
        assert_eq!(
            t,
            "Done: 1 changes, 2 with errors\n• Create 1: Missing permission\n• Create 2: No role named X"
        );
        let es = texto_resultado(&resumen, "es");
        assert!(es.starts_with("Hecho: 1 cambios, 2 con error\n"), "{es}");
    }

    #[test]
    fn el_texto_de_la_propuesta_se_corta_con_y_n_cambios_mas() {
        use crate::asistente::EventoPropuesta;
        use crate::propuesta::{Propuesta, Validacion};
        let ev = EventoPropuesta {
            propuesta: Propuesta {
                resumen: "Resumen".into(),
                acciones: Vec::new(),
            },
            validacion: Validacion {
                cambios: (0..10).map(|i| format!("Cambio número {i:02}")).collect(),
                errores: Vec::new(),
            },
        };
        let t = texto_propuesta(&ev, false, "en", 80);
        assert!(t.chars().count() <= 80 + 40, "{t}");
        assert!(t.contains("• Cambio número 00"));
        assert!(t.ends_with("more changes"), "{t}");
        assert!(!t.contains("Cambio número 09"));
        let es = texto_propuesta(&ev, true, "es", 80);
        assert!(es.contains(" cambios más"), "{es}");
        assert!(es.ends_with("Responde **sí** en los próximos 5 minutos para aplicarla"));
        // Si cabe todo, no hay «más».
        let t = texto_propuesta(&ev, false, "en", 1800);
        assert!(!t.contains("more changes") && t.contains("Cambio número 09"));
    }

    #[tokio::test]
    async fn un_error_del_agente_no_responde_y_queda_en_logs() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, logs) = conversador(
            &server,
            agente_falso("sesion", "echo 'Please run /login' >&2; exit 1"),
            LIMITE_AGENTE,
        );
        let r = c.atender(entrada(1, true), &ajustes()).await;
        assert!(
            matches!(&r, Resultado::Error(m) if m.contains("not signed in")),
            "{r:?}"
        );
        assert!(envios(&server).await.is_empty());
        let logs = logs.lock().unwrap();
        assert!(logs[0].starts_with("[WARN]") && logs[0].contains("not signed in"));
    }

    #[tokio::test]
    async fn el_tiempo_agotado_no_responde_y_queda_en_logs() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, logs) = conversador(
            &server,
            agente_falso("lento", "sleep 30"),
            Duration::from_millis(300),
        );
        let r = c.atender(entrada(1, true), &ajustes()).await;
        assert_eq!(
            r,
            Resultado::Error("The AI agent took too long to reply".into())
        );
        assert!(envios(&server).await.is_empty());
        assert!(logs.lock().unwrap()[0].contains("took too long"));
    }

    #[tokio::test]
    async fn parte_las_respuestas_largas() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, _) = conversador(
            &server,
            agente_falso("largo", "head -c 4500 /dev/zero | tr '\\0' 'a'"),
            LIMITE_AGENTE,
        );
        assert_eq!(
            c.atender(entrada(1, true), &ajustes()).await,
            Resultado::Respondido
        );
        let enviados = envios(&server).await;
        assert_eq!(enviados.len(), 3);
        assert!(enviados
            .iter()
            .all(|m| m["content"].as_str().unwrap().chars().count() <= 2000));
        // Solo el primero avisa al autor.
        assert_eq!(enviados[0]["allowed_mentions"]["replied_user"], true);
        assert_ne!(enviados[1]["allowed_mentions"]["replied_user"], true);
    }

    #[test]
    fn partir_prefiere_saltos_de_linea_y_espacios() {
        let t = format!("{}\n{}", "a".repeat(1500), "b".repeat(1500));
        let p = partir_mensaje(&t, 2000);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], "a".repeat(1500));
        assert_eq!(p[1], "b".repeat(1500));
        assert_eq!(partir_mensaje("corto", 2000), vec!["corto"]);
        assert!(partir_mensaje("   ", 2000).is_empty());
        let emoji = "😀".repeat(2500);
        assert!(partir_mensaje(&emoji, 2000)
            .iter()
            .all(|t| t.chars().count() <= 2000));
    }

    #[test]
    fn las_menciones_permitidas_solo_incluyen_al_autor() {
        let m = menciones_permitidas(7, true);
        assert!(m.parse.is_empty() && m.roles.is_empty());
        assert_eq!(m.users.len(), 1);
        assert_eq!(m.users[0].get(), 7);
    }

    #[tokio::test]
    async fn una_respuesta_a_la_vez_por_canal_y_tres_agentes_en_total() {
        let coord = Arc::new(Coordinador::default());
        let activos = Arc::new(AtomicUsize::new(0));
        let maximo = Arc::new(AtomicUsize::new(0));
        let mut tareas = Vec::new();
        // Seis canales distintos: como mucho 3 a la vez.
        for canal in 0..6u64 {
            let (c, a, m) = (coord.clone(), activos.clone(), maximo.clone());
            tareas.push(tokio::spawn(async move {
                let _t = c.turno(canal).await;
                let n = a.fetch_add(1, Ordering::SeqCst) + 1;
                m.fetch_max(n, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(80)).await;
                a.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for t in tareas {
            t.await.unwrap();
        }
        assert_eq!(maximo.load(Ordering::SeqCst), MAX_AGENTES);

        // Mismo canal: nunca dos a la vez.
        let maximo = Arc::new(AtomicUsize::new(0));
        let mut tareas = Vec::new();
        for _ in 0..4 {
            let (c, a, m) = (coord.clone(), activos.clone(), maximo.clone());
            tareas.push(tokio::spawn(async move {
                let _t = c.turno(1).await;
                let n = a.fetch_add(1, Ordering::SeqCst) + 1;
                m.fetch_max(n, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                a.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for t in tareas {
            t.await.unwrap();
        }
        assert_eq!(maximo.load(Ordering::SeqCst), 1);
    }

    const PLAN_VALIDO: &str =
        "## Rol: Mods | color: #ff0000\n## Categoría: Staff\n- texto: general\n- texto: reglas\n";

    /// Sirve `cuerpo` en /files/plan.md y devuelve un mensaje con ese adjunto.
    async fn con_archivo(
        server: &MockServer,
        autor: u64,
        nombre: &str,
        cuerpo: Vec<u8>,
        tamano: u64,
    ) -> Entrada {
        // Una ruta por tamaño real, para poder servir varios archivos distintos.
        let ruta = format!("/files/{}/plan.md", cuerpo.len());
        Mock::given(method("GET"))
            .and(path(ruta.clone()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(cuerpo))
            .mount(server)
            .await;
        let mut e = entrada(autor, true);
        e.texto = "construye esto".into();
        e.adjuntos = vec![Adjunto {
            nombre: nombre.into(),
            url: format!("{}{ruta}", server.uri()),
            tamano,
        }];
        e
    }

    async fn descargas(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path().starts_with("/files/"))
            .count()
    }

    #[tokio::test]
    async fn un_archivo_valido_se_compila_sin_ia_y_el_si_lo_aplica() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        // El agente fallaría si se lanzara: no debe usarse.
        let (mut c, logs) = conversador(
            &server,
            agente_falso("archivo-ok", "echo no-debe-usarse >&2; exit 1"),
            LIMITE_AGENTE,
        );
        let aplicadas = aplicador_falso(&mut c);
        let e = con_archivo(
            &server,
            1,
            "Plan.MD",
            PLAN_VALIDO.as_bytes().to_vec(),
            PLAN_VALIDO.len() as u64,
        )
        .await;
        let r = c.atender(e, &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Respondido);
        let env = envios(&server).await;
        let t = textos(&env);
        assert_eq!(t.len(), 1, "{t:?}");
        assert!(t[0].contains("• Roles to create: 1"), "{}", t[0]);
        assert!(t[0].contains("Reply **yes** within the next 5 minutes"));
        assert!(!t[0].contains("Warnings:"), "{}", t[0]);
        assert_eq!(env[0]["message_reference"]["message_id"], "500");
        assert!(aplicadas.lock().unwrap().is_empty());
        assert!(logs
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("[ORDER] File Plan.MD from Ana")));

        // El «sí» del mismo autor la aplica.
        let r = c.atender(confirmar(1, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Respondido);
        {
            let a = aplicadas.lock().unwrap();
            assert_eq!(a.len(), 1);
            assert_eq!(a[0].acciones[0].herramienta, "crear_rol");
            assert_eq!(a[0].acciones[0].args["guild_id"], "555");
        }
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 2);
        assert!(t[1].starts_with("Done: "), "{}", t[1]);
    }

    #[tokio::test]
    async fn un_archivo_con_avisos_los_publica_con_su_linea() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, _) = conversador(&server, agente_falso("archivo-av", "exit 1"), LIMITE_AGENTE);
        let plan = "## Categoría: Staff | rareza: alta\n- texto: general\n";
        let e = con_archivo(&server, 1, "plan.txt", plan.as_bytes().to_vec(), 50).await;
        c.atender(e, &ajustes_ordenes(true)).await;
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 1, "{t:?}");
        assert!(t[0].contains("Warnings:\n⚠ Line 1:"), "{}", t[0]);
        assert!(t[0].contains("Reply **yes**"));
    }

    #[tokio::test]
    async fn un_archivo_sin_estructura_se_rechaza_con_el_enlace_al_manual() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, _) = conversador(
            &server,
            agente_falso("archivo-sin", "exit 1"),
            LIMITE_AGENTE,
        );
        let libre = b"Quiero un servidor bonito con muchos canales.".to_vec();
        let e = con_archivo(&server, 1, "idea.md", libre, 45).await;
        c.atender(e, &ajustes_ordenes(true)).await;
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 1, "{t:?}");
        assert!(t[0].contains("no structure"), "{}", t[0]);
        assert!(t[0].contains(ENLACE_MANUAL), "{}", t[0]);
        // No queda nada pendiente.
        let r = c.atender(confirmar(1, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Ignorado);
    }

    #[tokio::test]
    async fn un_archivo_demasiado_grande_no_se_descarga_o_se_corta() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (c, _) = conversador(&server, agente_falso("archivo-gr", "exit 1"), LIMITE_AGENTE);
        let a = GuildSettings {
            chat_espera_s: 0,
            ..ajustes_ordenes(true)
        };
        // Tamaño declarado por Discord.
        let e = con_archivo(&server, 1, "plan.md", PLAN_VALIDO.into(), 300 * 1024).await;
        c.atender(e, &a).await;
        assert_eq!(descargas(&server).await, 0);
        // Declara poco pero el contenido real es grande.
        let grande = vec![b'a'; MAX_ARCHIVO + 1];
        let e = con_archivo(&server, 1, "plan.md", grande, 10).await;
        c.atender(e, &a).await;
        assert_eq!(descargas(&server).await, 1);
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 2, "{t:?}");
        for x in &t {
            assert!(x.contains("too large (the maximum is 200 KB)"), "{x}");
        }
    }

    #[tokio::test]
    async fn un_autor_sin_ordenes_no_puede_construir_desde_un_archivo() {
        let server = MockServer::start().await;
        mock_discord(&server).await;
        let (mut c, _) = conversador(&server, agente_falso("archivo-no", "exit 1"), LIMITE_AGENTE);
        let aplicadas = aplicador_falso(&mut c);
        let e = con_archivo(&server, 2, "plan.md", PLAN_VALIDO.into(), 90).await;
        // Autor fuera de la lista, y órdenes desactivadas para el autor 1.
        c.atender(e, &ajustes_ordenes(true)).await;
        let e = con_archivo(&server, 1, "plan.md", PLAN_VALIDO.into(), 90).await;
        c.atender(e, &ajustes()).await;
        assert_eq!(descargas(&server).await, 0);
        let t = textos(&envios(&server).await);
        assert_eq!(t.len(), 2, "{t:?}");
        for x in &t {
            assert!(x.contains("not allowed to give orders"), "{x}");
        }
        let r = c.atender(confirmar(2, "sí"), &ajustes_ordenes(true)).await;
        assert_eq!(r, Resultado::Ignorado);
        assert!(aplicadas.lock().unwrap().is_empty());
    }
}
