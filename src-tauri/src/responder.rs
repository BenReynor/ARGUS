//! Responder en Discord: para cada mensaje dirigido al bot, escribe «escribiendo…»,
//! lee el contexto del canal, lanza el agente (sin herramientas, con límite de
//! tiempo) y contesta como respuesta al mensaje original. Las respuestas se
//! encolan por canal y como mucho `MAX_AGENTES` agentes corren a la vez.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::{OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};
use twilight_model::channel::message::AllowedMentions;
use twilight_model::id::Id;

use crate::asistente::{clasificar_error, OrdenAgente};
use crate::chat::{debe_responder, orden_chat, prompt_chat, MensajeContexto, MensajeEntrante};
use crate::db::GuildSettings;
use crate::discord::{build_client, traducir_error, MAX_LONGITUD_MENSAJE};

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
}

impl Default for Coordinador {
    fn default() -> Self {
        Self::nuevo(MAX_AGENTES)
    }
}

impl Coordinador {
    pub fn nuevo(max_agentes: usize) -> Self {
        Self {
            canales: Mutex::new(HashMap::new()),
            agentes: Arc::new(Semaphore::new(max_agentes)),
            ultimas: Mutex::new(HashMap::new()),
        }
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

/// Un mensaje de servidor ya traducido a lo que necesita el conversador.
#[derive(Debug, Clone, Default)]
pub struct Entrada {
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
    std::fs::create_dir_all(&orden.directorio)
        .map_err(|e| format!("Could not prepare the AI agent folder: {e}"))?;
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
    let mut hijo = cmd
        .spawn()
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
        let ahora = ahora_s();
        let ultima = self.coord.ultima(e.guild_id, e.msg.autor_id);
        if !debe_responder(&e.msg, self.bot_id, ajustes, ultima, ahora) {
            return Resultado::Ignorado;
        }
        // Se anota al aceptar el mensaje para que una ráfaga no llene la cola.
        self.coord.anotar(e.guild_id, e.msg.autor_id, ahora);

        let _turno = self.coord.turno(e.msg.canal_id).await;
        let inicio = Instant::now();
        let resultado = self.responder(&e, ajustes).await;
        let ms = inicio.elapsed().as_millis();
        match &resultado {
            Ok(()) => self.log(format!(
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
            Ok(()) => Resultado::Respondido,
            Err(m) => Resultado::Error(m),
        }
    }

    async fn responder(&self, e: &Entrada, ajustes: &GuildSettings) -> Result<(), String> {
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
        let prompt = prompt_chat(
            &ajustes.chat_personalidad,
            &e.servidor,
            &e.canal,
            &previos,
            ajustes.chat_contexto as usize,
            &e.autor_nombre,
            &e.texto,
        );
        let orden = orden_chat(agente, &ruta, &ajustes.chat_modelo, &prompt)?;
        let respuesta = ejecutar_texto(&orden, self.limite).await?;
        self.enviar(e, &respuesta).await
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

    async fn enviar(&self, e: &Entrada, respuesta: &str) -> Result<(), String> {
        let client = build_client(&self.token, self.base_url.as_deref());
        let cid = Id::new_checked(e.msg.canal_id).ok_or("Invalid channel ID")?;
        let mid = Id::new_checked(e.mensaje_id).ok_or("Invalid message ID")?;
        for (i, trozo) in partir_mensaje(respuesta, MAX_LONGITUD_MENSAJE)
            .iter()
            .enumerate()
        {
            // Solo el primer trozo avisa al autor.
            let permitidas = menciones_permitidas(e.msg.autor_id, i == 0);
            client
                .create_message(cid)
                .reply(mid)
                .fail_if_not_exists(false)
                .allowed_mentions(Some(&permitidas))
                .content(trozo)
                .map_err(|err| format!("Invalid message: {err}"))?
                .await
                .map_err(traducir_error)?;
        }
        Ok(())
    }
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
}
