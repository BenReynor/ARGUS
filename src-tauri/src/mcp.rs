//! Servidor MCP de ARGUS (herramientas que usa el binario `argus-mcp`).
//!
//! Lee la misma base de datos que la app, así que funciona con la app cerrada.
//! El token del bot nunca sale de aquí: ni en respuestas, ni en logs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{Implementation, ServerCapabilities, ServerConfig},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::commands::{self, sincronizar_guilds, token_del_bot_activo};
use crate::discord::{DatosCanal, DatosRol};
use crate::i18n;
use crate::propuesta::{
    orden_de_ejecucion, EstadoAccion, Nombres, Propuesta, ResultadoAccion, ResumenPropuesta,
};
use crate::state::{AppState, Channel, Guild, Member, Role};

/// Respuesta de cada herramienta cuando no hay ningún bot activo.
pub const SIN_BOT: &str = "No active bot in ARGUS: add or choose one in the app";

/// Respuesta cuando una herramienta no recibe `guild_id` y no hay servidor activo.
pub const SIN_SERVIDOR: &str =
    "No active server in ARGUS: pass `guild_id` or choose one with `elegir_servidor`";

/// Mensajes que devuelve `leer_mensajes` como máximo.
/// Máximo de `leer_mensajes` por llamada (más de 50 se lee paginando).
const MENSAJES_MAX: usize = 500;
/// Lo que devuelve `leer_mensajes` si no se indica `limite`.
const MENSAJES_DEFECTO: usize = 50;
/// Mensajes revisados por canal en `buscar_mensajes` (por defecto y máximo).
const BUSQUEDA_DEFECTO: usize = 100;
/// «Todos» en la búsqueda: el historial completo, hasta este tope por canal.
pub const BUSQUEDA_MAX: usize = 5000;
/// Caracteres máximos del fragmento que devuelve `buscar_mensajes`.
const FRAGMENTO_MAX: usize = 160;
/// Miembros por defecto y máximo de `listar_miembros`.
const MIEMBROS_DEFECTO: usize = 25;
const MIEMBROS_MAX: usize = 100;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PlantillaParams {
    /// Identifier returned by listar_plantillas.
    pub id: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ServidorParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
}

/// Explicit privacy expectation for one member and channel, supplied by the caller.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ExpectativaPrivacidad {
    pub channel_id: String,
    pub member_id: String,
    /// Whether this member is expected to be able to view the channel.
    pub can_view: bool,
    /// Source of the expectation: user, plan, or snapshot.
    pub source: FuenteExpectativa,
    /// Identifies the statement, plan passage, or snapshot used as evidence.
    pub evidence: String,
}

#[derive(Debug, Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(crate = "rmcp::schemars")]
pub enum FuenteExpectativa {
    User,
    Plan,
    Snapshot,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InformePermisosParams {
    pub guild_id: Option<String>,
    /// Only explicit expectations; channel names do not imply privacy.
    pub expectations: Option<Vec<ExpectativaPrivacidad>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ListarMiembrosParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Texto a buscar en el usuario, el apodo o el ID (sin distinguir mayúsculas).
    pub buscar: Option<String>,
    /// Máximo de miembros a devolver (por defecto 25, máximo 100).
    pub limite: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InfoMiembroParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID (snowflake) del usuario.
    pub user_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct LeerMensajesParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// Cuántos de los últimos mensajes devolver (por defecto 50, máximo 500).
    pub limite: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BuscarMensajesParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Texto a buscar en el contenido (sin distinguir mayúsculas ni tildes).
    pub texto: String,
    /// IDs o nombres de canales donde buscar. Si se omite, todos los de texto.
    pub canales: Option<Vec<String>>,
    /// Solo mensajes de este autor (ID o parte del nombre).
    pub autor: Option<String>,
    /// Mensajes recientes que se revisan por canal (por defecto 100, máximo 5000).
    pub limite_por_canal: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ElegirServidorParams {
    /// ID (snowflake) del servidor que pasa a ser el servidor activo. Debe ser
    /// uno de los que devuelve `listar_servidores`.
    pub guild_id: String,
}

/// Permisos como texto de bits (`"8192"`) o como lista de nombres legibles
/// (`["ManageMessages","ViewChannel"]` o `["Gestionar mensajes"]`).
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(untagged)]
pub enum PermisosParam {
    Bits(String),
    Nombres(Vec<String>),
}

impl PermisosParam {
    /// Texto de bits que espera Discord.
    pub fn a_bits(&self) -> Result<String, String> {
        match self {
            Self::Bits(t) => t
                .trim()
                .parse::<u64>()
                .map(|b| b.to_string())
                .map_err(|_| "Permissions must be a bits number or a list of names".to_string()),
            Self::Nombres(n) => crate::discord::bits_desde_nombres(n),
        }
    }
}

fn bits_opcionales(p: &Option<PermisosParam>) -> Result<Option<String>, String> {
    p.as_ref().map(PermisosParam::a_bits).transpose()
}

/// Color de un rol: número RGB (`5793266`) o texto hexadecimal (`"#5865F2"`),
/// el mismo formato que devuelve `listar_roles`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(untagged)]
pub enum ColorParam {
    Numero(u32),
    Hex(String),
}

impl ColorParam {
    pub fn a_u32(&self) -> Result<u32, String> {
        let valor = match self {
            ColorParam::Numero(n) => *n,
            ColorParam::Hex(t) => {
                let hex = t.trim().trim_start_matches('#');
                u32::from_str_radix(hex, 16)
                    .map_err(|_| format!("Invalid color: «{t}» (use #RRGGBB or an RGB number)"))?
            }
        };
        if valor > 0xFF_FFFF {
            return Err(format!("Color out of range: {valor} (maximum #FFFFFF)"));
        }
        Ok(valor)
    }
}

fn color_opcional(c: &Option<ColorParam>) -> Result<Option<u32>, String> {
    c.as_ref().map(ColorParam::a_u32).transpose()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EnviarMensajeParams {
    /// ID del canal de texto.
    pub channel_id: String,
    /// Texto del mensaje (hasta 2000 caracteres). Opcional si hay `embed`.
    pub contenido: Option<String>,
    /// Embed opcional (título, descripción, color hex, pie, campos, imagen).
    pub embed: Option<crate::discord::EmbedMensaje>,
    /// ID de un mensaje de ese canal al que se responde (opcional).
    pub responder_a: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarMensajeParams {
    /// ID del canal del mensaje.
    pub channel_id: String,
    /// ID del mensaje del bot que se edita.
    pub message_id: String,
    /// Texto nuevo (hasta 2000 caracteres); reemplaza al anterior. Opcional si hay `embed`.
    pub contenido: Option<String>,
    /// Embed nuevo opcional (título, descripción, color hex, pie, campos, imagen).
    pub embed: Option<crate::discord::EmbedMensaje>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ReaccionParams {
    /// ID del servidor (opcional; con él se comprueba el permiso antes).
    pub guild_id: Option<String>,
    /// ID del canal del mensaje.
    pub channel_id: String,
    /// ID del mensaje.
    pub message_id: String,
    /// Emoji unicode (👍) o personalizado `nombre:id`.
    pub emoji: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RespuestaEncuestaParams {
    /// Texto de la respuesta (≤ 55 caracteres).
    pub texto: String,
    /// Emoji opcional: unicode (🍕) o personalizado `nombre:id`.
    pub emoji: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearEncuestaParams {
    /// ID del servidor (opcional; con él se comprueba el permiso antes).
    pub guild_id: Option<String>,
    /// ID del canal donde se publica.
    pub channel_id: String,
    /// Pregunta (≤ 300 caracteres).
    pub pregunta: String,
    /// De 2 a 10 respuestas.
    pub respuestas: Vec<RespuestaEncuestaParams>,
    /// Duración en horas, de 1 a 768 (por defecto 24).
    pub duracion_horas: Option<u32>,
    /// Permite votar varias respuestas (por defecto no).
    pub voto_multiple: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EncuestaMensajeParams {
    /// ID del canal del mensaje con la encuesta.
    pub channel_id: String,
    /// ID del mensaje con la encuesta.
    pub message_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearPostForoParams {
    /// ID del canal de foro.
    pub channel_id: String,
    /// Título del post (1–100 caracteres).
    pub titulo: String,
    /// Texto del primer mensaje del post (hasta 2000 caracteres).
    pub contenido: String,
    /// Etiquetas del foro (nombre o id, hasta 5; deben existir).
    #[serde(default)]
    pub etiquetas: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EtiquetasForoParams {
    /// ID del canal de foro.
    pub channel_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CambioEtiquetaParams {
    /// Nombre (o id) de la etiqueta: si existe se modifica, si no se crea (≤ 20).
    pub nombre: String,
    /// Nuevo nombre para renombrarla (≤ 20 caracteres).
    pub nuevo_nombre: Option<String>,
    /// Emoji opcional: unicode o `nombre:id`.
    pub emoji: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarEtiquetasForoParams {
    /// ID del canal de foro.
    pub channel_id: String,
    /// Etiquetas a crear o renombrar (el foro admite como máximo 20).
    pub etiquetas: Vec<CambioEtiquetaParams>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EtiquetarPostParams {
    /// ID del post (el hilo del foro).
    pub post_id: String,
    /// Etiquetas existentes del foro (nombre o id, hasta 5); sustituyen las actuales.
    pub etiquetas: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EnviarDmParams {
    /// ID del usuario que recibe el mensaje directo.
    pub user_id: String,
    /// Texto del mensaje (hasta 2000 caracteres).
    pub contenido: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AvisarParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario o su nombre (usuario o apodo) en el servidor.
    pub user_id: String,
    /// Motivo del aviso.
    pub motivo: String,
    /// ID del mensaje que motiva el aviso (opcional).
    pub mensaje_id: Option<String>,
    /// Si es true (por defecto) se envía un DM al usuario con el motivo y sus avisos.
    pub dm: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ListarAvisosParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID o nombre de un usuario para ver solo sus avisos (opcional).
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct MensajeParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// ID del mensaje.
    pub message_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BorrarMensajesParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// IDs de los mensajes: entre 2 y 100, todos de menos de 14 días.
    pub message_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "lowercase")]
pub enum TipoObjetivoCanal {
    Rol,
    Miembro,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PermisosCrearCanal {
    pub objetivo: String,
    pub tipo: TipoObjetivoCanal,
    #[serde(default)]
    pub permitir: Vec<String>,
    #[serde(default)]
    pub denegar: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearCanalParams {
    /// Permisos opcionales: [{objetivo: nombre o id, tipo: "rol"|"miembro", permitir: ["ViewChannel"], denegar: ["SendMessages"]}]. @everyone usa el id del servidor; admite roles creados antes en la propuesta. Se unen con los de la categoría; para el mismo objetivo gana el canal.
    pub permisos: Option<Vec<PermisosCrearCanal>>,
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Nombre del canal (1–100 caracteres).
    pub nombre: String,
    /// «texto» (por defecto), «voz», «categoria», «anuncios» o «foro».
    pub tipo: Option<String>,
    /// ID de la categoría donde se crea.
    pub categoria_id: Option<String>,
    /// Tema del canal.
    pub tema: Option<String>,
    /// Marcar el canal como NSFW.
    pub nsfw: Option<bool>,
    /// Modo lento en segundos (0–21600).
    pub slowmode_segundos: Option<u16>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarCanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// Nombre nuevo.
    pub nombre: Option<String>,
    /// ID de la categoría nueva; texto vacío para sacarlo de su categoría.
    pub categoria_id: Option<String>,
    /// Tema nuevo.
    pub tema: Option<String>,
    /// Marcar o desmarcar NSFW.
    pub nsfw: Option<bool>,
    /// Modo lento en segundos (0–21600).
    pub slowmode_segundos: Option<u16>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct Posicion {
    /// ID del canal o del rol.
    pub id: String,
    /// Posición nueva.
    pub posicion: u64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct OrdenarParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Pares (id, posición) a aplicar.
    pub posiciones: Vec<Posicion>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearRolParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Nombre del rol (1–100 caracteres).
    pub nombre: String,
    /// Color: "#RRGGBB" o número RGB (p. ej. 255 = azul).
    pub color: Option<ColorParam>,
    /// Mostrar a los miembros del rol por separado.
    pub separado: Option<bool>,
    /// Permitir que cualquiera mencione el rol.
    pub mencionable: Option<bool>,
    /// Permisos: texto de bits o lista de nombres (p. ej. ["ManageMessages","ViewChannel"]).
    pub permisos: Option<PermisosParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarRolParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del rol.
    pub role_id: String,
    /// Nombre nuevo.
    pub nombre: Option<String>,
    /// Color nuevo: "#RRGGBB" o número RGB.
    pub color: Option<ColorParam>,
    /// Mostrar por separado.
    pub separado: Option<bool>,
    /// Mencionable.
    pub mencionable: Option<bool>,
    /// Permisos nuevos (sustituyen a los actuales): bits o lista de nombres.
    pub permisos: Option<PermisosParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RolParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del rol.
    pub role_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RolMiembroParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario.
    pub user_id: String,
    /// ID del rol.
    pub role_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PermisoCanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// ID del rol o del miembro al que se aplica.
    pub target_id: String,
    /// «rol» o «miembro».
    pub tipo: String,
    /// Permisos que se conceden (bits o lista de nombres).
    pub permitir: Option<PermisosParam>,
    /// Permisos que se deniegan (bits o lista de nombres).
    pub denegar: Option<PermisosParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct QuitarPermisoCanalParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal.
    pub channel_id: String,
    /// ID del rol o del miembro.
    pub target_id: String,
    /// «rol» o «miembro».
    pub tipo: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ExpulsarParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario.
    pub user_id: String,
    /// Motivo; queda en el registro de auditoría.
    pub motivo: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BanearParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario (puede no estar en el servidor).
    pub user_id: String,
    /// Motivo; queda en el registro de auditoría.
    pub motivo: Option<String>,
    /// Días de mensajes a borrar, de 0 a 7 (por defecto 0).
    pub dias_mensajes: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SilenciarParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario.
    pub user_id: String,
    /// Duración en minutos (máximo 40320 = 28 días); 0 quita el silencio.
    pub minutos: u32,
    /// Motivo; queda en el registro de auditoría.
    pub motivo: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CambiarApodoParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del usuario.
    pub user_id: String,
    /// Apodo nuevo (hasta 32 caracteres); vacío u omitido lo borra.
    pub apodo: Option<String>,
    /// Motivo; queda en el registro de auditoría.
    pub motivo: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarServidorParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Nombre nuevo (2–100 caracteres).
    pub nombre: Option<String>,
    /// Descripción nueva (hasta 120 caracteres); texto vacío la borra.
    pub descripcion: Option<String>,
    /// Nivel de verificación (0–4).
    pub nivel_verificacion: Option<u8>,
    /// Filtro de contenido explícito (0–2).
    pub filtro_contenido_explicito: Option<u8>,
    /// Notificaciones por defecto (0 = todos los mensajes, 1 = solo menciones).
    pub notificaciones_por_defecto: Option<u8>,
    /// ID del canal de sistema; texto vacío lo quita.
    pub canal_del_sistema_id: Option<String>,
    /// ID del canal AFK; texto vacío lo quita.
    pub canal_afk_id: Option<String>,
    /// Inactividad AFK en segundos (60, 300, 900, 1800 o 3600).
    pub afk_segundos: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearInvitacionParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal al que lleva la invitación.
    pub channel_id: String,
    /// Caducidad en segundos (0 = no caduca; máximo 604800; por defecto 86400).
    pub caducidad_segundos: Option<u32>,
    /// Usos máximos (0 = ilimitados; máximo 100; por defecto 0).
    pub usos_maximos: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BorrarInvitacionParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Código de la invitación (la parte final del enlace).
    pub codigo: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearWebhookParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del canal donde publicará el webhook.
    pub channel_id: String,
    /// Nombre del webhook (1–80 caracteres).
    pub nombre: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BorrarWebhookParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del webhook.
    pub webhook_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CrearEventoParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// Nombre del evento (≤ 100 caracteres).
    pub nombre: String,
    /// Descripción (≤ 1000 caracteres).
    pub descripcion: Option<String>,
    /// Inicio en ISO 8601 con zona (2030-05-01T20:00:00+02:00); debe ser futuro.
    pub inicio: String,
    /// Fin en ISO 8601 con zona; posterior al inicio. Obligatorio con lugar externo.
    pub fin: Option<String>,
    /// Lugar externo (≤ 100 caracteres). Excluyente con `canal_id`.
    pub lugar: Option<String>,
    /// Canal de voz o escenario donde se celebra. Excluyente con `lugar`.
    pub canal_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EditarEventoParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del evento.
    pub evento_id: String,
    pub nombre: Option<String>,
    pub descripcion: Option<String>,
    /// Nuevo inicio (ISO 8601 con zona, futuro).
    pub inicio: Option<String>,
    /// Nuevo fin (ISO 8601 con zona, posterior al inicio).
    pub fin: Option<String>,
    /// Nuevo lugar externo (convierte el evento en externo).
    pub lugar: Option<String>,
    /// Nuevo canal de voz o escenario.
    pub canal_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CancelarEventoParams {
    /// ID del servidor. Si se omite se usa el servidor activo.
    pub guild_id: Option<String>,
    /// ID del evento.
    pub evento_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RestoreParams {
    /// Omit snapshot_id to list saved snapshots. Otherwise generate a read-only proposal.
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub undo: bool,
    #[serde(default)]
    pub delete_extra: bool,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SnapshotParams {
    pub snapshot_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ApplyRestoreParams {
    /// Identifier of the exact reviewed proposal, generated by info_restauracion.
    pub id: String,
    /// Server name required when deleting extras. Never infer user confirmation.
    #[serde(default)]
    pub confirmation: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct GuardarPlantillaPlanParams {
    /// Markdown plan of the server. It must describe at least one role or category.
    pub plan: String,
    /// Name of the new template.
    pub nombre: String,
    #[serde(default)]
    pub descripcion: String,
    /// Comma-separated tags.
    #[serde(default)]
    pub etiquetas: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct GuardarPlantillaSnapshotParams {
    /// Identifier returned by listar_snapshots (active bot and server only).
    pub snapshot_id: String,
    /// Name of the new template.
    pub nombre: String,
    #[serde(default)]
    pub descripcion: String,
    /// Comma-separated tags.
    #[serde(default)]
    pub etiquetas: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ProponerCorreccionParams {
    pub guild_id: Option<String>,
    /// A finding exactly as returned by informe_permisos.
    pub finding: serde_json::Value,
}

#[derive(Clone)]
pub struct ArgusMcp {
    pool: SqlitePool,
    state: Arc<AppState>,
    /// URL base de Discord; solo los tests la cambian (Discord falso).
    base_url: Option<String>,
    contexto_fijado: bool,
    /// Capa de conectores (solo lectura); None si no hay conectores habilitados.
    conectores: Option<Arc<crate::agregador::Aggregator>>,
}

impl ArgusMcp {
    /// Activa la capa de conectores: solo herramientas de lectura del catálogo.
    pub fn con_conectores(mut self, capa: Arc<crate::agregador::Aggregator>) -> Self {
        self.conectores = Some(capa);
        self
    }

    /// Herramientas `conector__herramienta` expuestas (misma política para todos los agentes).
    pub async fn herramientas_conectores(&self) -> Vec<String> {
        match &self.conectores {
            Some(c) => c.list_tools().await,
            None => Vec::new(),
        }
    }

    /// Builds (without applying) the proposal that creates a saved template on
    /// the active server. The user reviews and approves it in ARGUS.
    pub async fn proponer_plantilla_json(&self, id: &str) -> Result<String, String> {
        let plantilla = crate::db::get_template(&self.pool, id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Template not found")?;
        let documento = crate::template_transfer::parse(&plantilla.data)?;
        let propuesta = crate::template_aplicar::propuesta_de_documento(&documento);
        let validacion = crate::propuesta::validar_en_estado(&self.state, &propuesta);
        Ok(json!({
            "propuesta": propuesta,
            "validacion": validacion,
            "nota": "Proposal only: nothing was changed. Return it in Execute mode for the user to approve.",
        })
        .to_string())
    }

    /// Prepares the correction of an audit finding as a proposal (never applied here).
    pub async fn proponer_correccion_json(
        &self,
        guild_id: Option<&str>,
        hallazgo: &serde_json::Value,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let r = self.preparar_correccion_json(&gid, hallazgo).await?;
        Ok(r.to_string())
    }

    /// Local write (template library): only runs after the user approves a proposal.
    async fn guardar_plantilla_plan_json(
        &self,
        p: GuardarPlantillaPlanParams,
    ) -> Result<String, String> {
        let (d, omitidos) = crate::template_guardar::desde_plan(&p.plan, p.nombre, p.descripcion)?;
        let id = crate::template_guardar::guardar(&self.pool, d, &p.etiquetas).await?;
        Ok(json!({"template_id": id, "omitidos": omitidos}).to_string())
    }

    async fn guardar_plantilla_snapshot_json(
        &self,
        p: GuardarPlantillaSnapshotParams,
    ) -> Result<String, String> {
        let guild = self.servidor_de(None).await?;
        let bot = self.state.get_active_bot().ok_or(SIN_BOT)?;
        let snapshot = crate::snapshots::load(&self.pool, &bot.id, &guild, &p.snapshot_id)
            .await
            .map_err(|_| "Snapshot not found".to_string())?;
        let (d, omitidos) =
            crate::template_guardar::desde_snapshot(&snapshot, p.nombre, p.descripcion)?;
        let id = crate::template_guardar::guardar(&self.pool, d, &p.etiquetas).await?;
        Ok(json!({"template_id": id, "omitidos": omitidos}).to_string())
    }

    /// Tool list the MCP exposes: the fixed ARGUS tools plus, only when there
    /// are enabled connectors, their read tools (`connector__tool`).
    pub async fn herramientas_publicas(&self) -> Vec<rmcp::model::Tool> {
        let mut tools = Self::tool_router().list_all();
        for nombre in self.herramientas_conectores().await {
            let esquema: rmcp::model::JsonObject =
                serde_json::from_value(json!({"type": "object", "additionalProperties": true}))
                    .unwrap_or_default();
            tools.push(rmcp::model::Tool::new(
                nombre.clone(),
                format!(
                    "Read-only tool «{nombre}» of an enabled connector. It never changes anything."
                ),
                std::sync::Arc::new(esquema),
            ));
        }
        tools
    }

    pub async fn llamar_conector(
        &self,
        nombre: &str,
        args: serde_json::Value,
        cancelar: tokio::sync::watch::Receiver<bool>,
    ) -> Result<serde_json::Value, String> {
        let capa = self
            .conectores
            .as_ref()
            .ok_or_else(|| "Unknown connector tool".to_string())?;
        capa.call(nombre, args, cancelar)
            .await
            .map_err(|e| e.to_string())
    }

    /// Abre el servidor sobre una base ya abierta y carga el bot activo. Si hay
    /// un bot activo comprueba que su token se pueda descifrar (sin mostrarlo).
    pub async fn nuevo(pool: SqlitePool, base_url: Option<String>) -> Result<Self, String> {
        let servidor = Self {
            pool,
            state: Arc::new(AppState::new()),
            base_url,
            contexto_fijado: false,
            conectores: None,
        };
        servidor.recargar().await?;
        if servidor.state.get_active_bot().is_some() {
            token_del_bot_activo(&servidor.state)?;
        }
        Ok(servidor)
    }

    /// Ejecuta usando una copia del contexto capturada por el llamador.
    pub fn con_contexto(
        pool: SqlitePool,
        base_url: Option<String>,
        config: crate::state::Config,
    ) -> Self {
        let state = AppState::new();
        *state.config.lock().unwrap() = config;
        Self {
            pool,
            state: Arc::new(state),
            base_url,
            contexto_fijado: true,
            conectores: None,
        }
    }

    /// Relee la configuración de la base: la app puede haber cambiado de bot o
    /// de servidor mientras el MCP seguía abierto.
    async fn recargar(&self) -> Result<(), String> {
        self.state
            .load_config_from_pool(&self.pool)
            .await
            .map_err(|e| format!("Could not read the ARGUS configuration: {e}"))
    }

    /// Recarga y exige un bot activo.
    async fn preparar(&self) -> Result<(), String> {
        if !self.contexto_fijado {
            self.recargar().await?;
        }
        if self.state.get_active_bot().is_none() {
            return Err(SIN_BOT.to_string());
        }
        Ok(())
    }

    /// Quita el token de un mensaje de error por si Discord lo repitiera.
    fn limpiar(&self, mensaje: String) -> String {
        match token_del_bot_activo(&self.state) {
            Ok(token) if !token.is_empty() => mensaje.replace(&token, "***"),
            _ => mensaje,
        }
    }

    /// Servidores del bot activo: los pide a Discord y, si falla, usa los
    /// guardados en la base.
    async fn servidores(&self) -> Result<Vec<Guild>, String> {
        match sincronizar_guilds(&self.pool, &self.state, self.base_url.as_deref()).await {
            Ok(guilds) => Ok(guilds),
            Err(e) => {
                let cache = crate::db::list_guilds_cached(&self.pool)
                    .await
                    .unwrap_or_default();
                if cache.is_empty() {
                    Err(e)
                } else {
                    Ok(cache)
                }
            }
        }
    }

    pub async fn listar_snapshots_json(&self) -> Result<String, String> {
        self.info_restauracion_json(&RestoreParams {
            snapshot_id: None,
            undo: false,
            delete_extra: false,
        })
        .await
    }

    pub async fn crear_snapshot_json(&self) -> Result<String, String> {
        let guild = self.servidor_de(None).await?;
        let bot = self.state.get_active_bot().ok_or(SIN_BOT)?;
        let token = token_del_bot_activo(&self.state)?;
        let saved = crate::snapshots::capture(&token, self.url(), &guild)
            .await
            .map_err(|e| format!("{e:#}"))?;
        let id = crate::snapshots::store(&self.pool, &bot.id, &guild, &saved, false)
            .await
            .map_err(|e| format!("{e:#}"))?;
        Ok(json!({"snapshot_id":id}).to_string())
    }

    pub async fn comparar_snapshot_json(&self, snapshot: &str) -> Result<String, String> {
        let guild = self.servidor_de(None).await?;
        let bot = self.state.get_active_bot().ok_or(SIN_BOT)?;
        let token = token_del_bot_activo(&self.state)?;
        let comparison = crate::snapshots::compare::compare_saved(
            &self.pool,
            &bot.id,
            &guild,
            snapshot,
            &token,
            self.url(),
        )
        .await
        .map_err(|e| format!("{e:#}"))?;
        serde_json::to_string(&comparison).map_err(|e| e.to_string())
    }

    pub async fn info_restauracion_json(&self, p: &RestoreParams) -> Result<String, String> {
        let guild = self.servidor_de(None).await?;
        let bot = self.state.get_active_bot().ok_or(SIN_BOT)?;
        let token = token_del_bot_activo(&self.state)?;
        let mut value = if let Some(snapshot) = &p.snapshot_id {
            serde_json::to_value(
                crate::snapshots::restore::preview(
                    &self.pool,
                    &bot.id,
                    &guild,
                    snapshot,
                    &token,
                    self.url(),
                    p.undo,
                    p.delete_extra,
                )
                .await
                .map_err(|e| format!("{e:#}"))?,
            )
            .map_err(|e| e.to_string())?
        } else {
            crate::snapshots::listing(&self.pool, &bot.id, &guild)
                .await
                .map_err(|e| format!("{e:#}"))?
        };
        if p.snapshot_id.is_some() {
            value["requires_confirmation"] = json!(true);
            value["propuesta"] = json!({"resumen":"Restore snapshot", "acciones":[{
                "herramienta":"aplicar_restauracion",
                "args":{"id":value["id"], "guild_id":guild},
                "descripcion":"Apply the reviewed restore proposal"
            }]});
        }
        Ok(value.to_string())
    }

    async fn aplicar_restauracion_con(
        &self,
        p: &ApplyRestoreParams,
        cancelar: &AtomicBool,
    ) -> Result<ResumenPropuesta, String> {
        self.preparar().await?;
        crate::snapshots::restore::load_reviewed(&self.pool, &self.state, &p.id)
            .await
            .map_err(|e| format!("{e:#}"))?;
        let summary = crate::snapshots::restore::apply_cancelable(
            &self.state,
            &p.id,
            &p.confirmation,
            self.url(),
            cancelar,
            |_| {},
        )
        .await
        .map_err(|e| format!("{e:#}"))?;
        if summary.fallidas > 0 || summary.canceladas > 0 {
            return Err(serde_json::to_string(&summary).map_err(|e| e.to_string())?);
        }
        Ok(summary)
    }

    pub async fn estado_json(&self) -> Result<String, String> {
        self.preparar().await?;
        let bot = self.state.get_active_bot().ok_or(SIN_BOT)?;
        let activo = self.state.get_active_guild();
        let servidor = match activo {
            Some(id) => {
                let nombre = crate::db::list_guilds_cached(&self.pool)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .find(|g| g.id == id)
                    .map(|g| g.name);
                json!({ "id": id, "nombre": nombre })
            }
            None => serde_json::Value::Null,
        };
        let registrados = self.state.config.lock().unwrap().bots.len();
        Ok(json!({
            "bot_activo": { "id": bot.id, "nombre": bot.name, "avatar": bot.avatar },
            "servidor_activo": servidor,
            "bots_registrados": registrados,
        })
        .to_string())
    }

    pub async fn listar_servidores_json(&self) -> Result<String, String> {
        self.preparar().await?;
        let activo = self.state.get_active_guild();
        let guilds = self.servidores().await.map_err(|e| self.limpiar(e))?;
        let lista: Vec<_> = guilds
            .iter()
            .map(|g| json!({ "id": g.id, "nombre": g.name, "activo": activo.as_deref() == Some(&g.id) }))
            .collect();
        Ok(json!({ "servidores": lista }).to_string())
    }

    pub async fn elegir_servidor_json(&self, guild_id: &str) -> Result<String, String> {
        self.preparar().await?;
        let guild_id = guild_id.trim();
        let guilds = self.servidores().await.map_err(|e| self.limpiar(e))?;
        let guild = guilds.iter().find(|g| g.id == guild_id).ok_or_else(|| {
            format!("The active bot is not in server {guild_id}: use `listar_servidores`")
        })?;
        self.state.set_active_guild(&guild.id);
        self.state
            .save_config_to_pool(&self.pool)
            .await
            .map_err(|e| format!("Could not save the active server: {e}"))?;
        Ok(json!({ "servidor_activo": { "id": guild.id, "nombre": guild.name } }).to_string())
    }

    /// Recarga, exige bot activo y resuelve el servidor: el indicado o el activo.
    async fn servidor_de(&self, guild_id: Option<&str>) -> Result<String, String> {
        self.preparar().await?;
        match guild_id.map(str::trim).filter(|g| !g.is_empty()) {
            Some(g) => Ok(g.to_string()),
            None => self
                .state
                .get_active_guild()
                .ok_or_else(|| SIN_SERVIDOR.to_string()),
        }
    }

    fn url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// Roles del servidor para poner nombre a los IDs (vacío si no se pueden leer).
    async fn roles_con_nombre(&self, gid: &str) -> Vec<Role> {
        commands::leer_roles(&self.state, gid, self.url())
            .await
            .unwrap_or_default()
    }

    pub async fn mapa_servidor_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let guild = commands::informacion_del_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let roles = commands::leer_roles(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        // Sin el intent de miembros Discord rechaza la lista: el mapa sigue siendo útil.
        let (miembros, aviso) = match commands::leer_miembros(&self.state, &gid, self.url()).await {
            Ok(m) => (Some(m), None),
            Err(e) => (None, Some(self.tr(&self.limpiar(e)))),
        };
        let visibles: Vec<&Channel> = canales
            .iter()
            .filter(|c| !matches!(c.channel_type, 10..=12))
            .collect();
        let hijos = |padre: Option<&str>| {
            let mut v: Vec<&&Channel> = visibles
                .iter()
                .filter(|c| c.channel_type != 4 && c.parent_id.as_deref() == padre)
                .collect();
            v.sort_by_key(|c| c.position);
            v.into_iter().map(|c| canal_json(c)).collect::<Vec<_>>()
        };
        let mut categorias: Vec<&&Channel> =
            visibles.iter().filter(|c| c.channel_type == 4).collect();
        categorias.sort_by_key(|c| c.position);
        let categorias: Vec<_> = categorias
            .iter()
            .map(|c| json!({ "id": c.id, "nombre": c.name, "canales": hijos(Some(&c.id)) }))
            .collect();
        let mut roles_ord: Vec<&Role> = roles.iter().collect();
        roles_ord.sort_by_key(|r| std::cmp::Reverse(r.position));
        let roles_json: Vec<_> = roles_ord
            .iter()
            .map(|r| {
                let mut v = json!({ "id": r.id, "nombre": r.name, "posicion": r.position });
                if let Some(m) = &miembros {
                    v["miembros"] = json!(m.iter().filter(|x| x.roles.contains(&r.id)).count());
                }
                v
            })
            .collect();
        let mut salida = json!({
            "servidor": { "id": guild.id, "nombre": guild.name, "miembros": guild.member_count },
            "categorias": categorias,
            "sin_categoria": hijos(None),
            "roles": roles_json,
        });
        if let Some(m) = &miembros {
            salida["miembros_cargados"] = json!(m.len());
        }
        if let Some(a) = aviso {
            salida["aviso_miembros"] = json!(a);
        }
        Ok(salida.to_string())
    }

    pub async fn listar_canales_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        // Nombres de los roles para los permisos (el de @everyone es el del servidor).
        let roles = commands::leer_roles(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let nombre_de = |id: &str, tipo: u8| -> Option<String> {
            if tipo != 0 {
                return None;
            }
            if id == gid {
                return Some("@everyone".into());
            }
            roles.iter().find(|r| r.id == id).map(|r| r.name.clone())
        };
        let nombres =
            |bits: &str| crate::discord::nombres_api_de_permisos(bits.parse::<u64>().unwrap_or(0));
        let lista: Vec<_> = canales
            .iter()
            .map(|c| {
                let categoria = c
                    .parent_id
                    .as_ref()
                    .and_then(|p| canales.iter().find(|x| &x.id == p))
                    .map(|p| p.name.clone());
                let mut v = canal_json(c);
                v["categoria_id"] = json!(c.parent_id);
                v["categoria"] = json!(categoria);
                if let Some(t) = c.topic.as_deref().filter(|t| !t.is_empty()) {
                    v["tema"] = json!(t);
                }
                if let Some(o) = c.overwrites.as_ref().filter(|o| !o.is_empty()) {
                    v["overwrites"] = json!(o
                        .iter()
                        .map(|o| json!({
                            "id": o.id,
                            "nombre": nombre_de(&o.id, o.type_),
                            "tipo": if o.type_ == 0 { "rol" } else { "miembro" },
                            // Nombres de la API, los mismos que acepta `permiso_canal`.
                            "permitir": nombres(&o.allow),
                            "denegar": nombres(&o.deny),
                        }))
                        .collect::<Vec<_>>());
                }
                v
            })
            .collect();
        Ok(json!({ "canales": lista }).to_string())
    }

    pub async fn listar_roles_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let mut roles = commands::leer_roles(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        roles.sort_by_key(|r| std::cmp::Reverse(r.position));
        let lista: Vec<_> = roles
            .iter()
            .map(|r| {
                let bits = r.permissions.parse::<u64>().unwrap_or(0);
                json!({
                    "id": r.id,
                    "nombre": r.name,
                    "posicion": r.position,
                    "color": format!("#{:06x}", r.color),
                    "separado": r.hoist,
                    "mencionable": r.mentionable,
                    "permisos_bits": r.permissions,
                    "permisos": crate::discord::nombres_de_permisos(bits),
                })
            })
            .collect();
        Ok(json!({ "roles": lista }).to_string())
    }

    pub async fn listar_miembros_json(
        &self,
        guild_id: Option<&str>,
        buscar: Option<&str>,
        limite: Option<u32>,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let limite = (limite.unwrap_or(MIEMBROS_DEFECTO as u32) as usize).clamp(1, MIEMBROS_MAX);
        let miembros = commands::leer_miembros(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let roles = self.roles_con_nombre(&gid).await;
        let buscar = buscar
            .map(|b| b.trim().to_lowercase())
            .filter(|b| !b.is_empty());
        let coinciden: Vec<&Member> = miembros
            .iter()
            .filter(|m| match &buscar {
                None => true,
                Some(b) => {
                    m.username.to_lowercase().contains(b)
                        || m.user_id == *b
                        || m.nickname
                            .as_deref()
                            .is_some_and(|n| n.to_lowercase().contains(b))
                }
            })
            .collect();
        let lista: Vec<_> = coinciden
            .iter()
            .take(limite)
            .map(|m| miembro_json(m, &roles))
            .collect();
        Ok(json!({
            "total_servidor": miembros.len(),
            "coincidencias": coinciden.len(),
            "devueltos": lista.len(),
            "miembros": lista,
        })
        .to_string())
    }

    pub async fn info_miembro_json(
        &self,
        guild_id: Option<&str>,
        user_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let miembro = commands::leer_miembro(&self.state, &gid, user_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let roles = self.roles_con_nombre(&gid).await;
        let mut v = miembro_json(&miembro, &roles);
        v["unido"] = json!(miembro.joined_at);
        Ok(v.to_string())
    }

    pub async fn permisos_bot_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let bot = self.state.get_active_bot().ok_or(SIN_BOT)?;
        let permisos = commands::permisos_del_bot(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(
            json!({ "bot": { "id": bot.id, "nombre": bot.name }, "permisos": permisos })
                .to_string(),
        )
    }

    pub async fn info_servidor_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let g = commands::informacion_del_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({
            "id": g.id,
            "nombre": g.name,
            "icono": g.icon,
            "miembros_aprox": g.member_count,
        })
        .to_string())
    }

    pub async fn ajustes_servidor_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let a = commands::leer_ajustes_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .unwrap_or_default();
        let canal = |id: &Option<String>| {
            id.as_ref().map(|id| {
                json!({
                    "id": id,
                    "nombre": canales.iter().find(|c| &c.id == id).map(|c| c.name.clone()),
                })
            })
        };
        Ok(json!({
            "nombre": a.name,
            "descripcion": a.description,
            "nivel_verificacion": a.verification_level,
            "filtro_contenido_explicito": a.explicit_content_filter,
            "notificaciones_por_defecto": a.default_message_notifications,
            "canal_del_sistema": canal(&a.system_channel_id),
            "canal_afk": canal(&a.afk_channel_id),
            "afk_segundos": a.afk_timeout,
        })
        .to_string())
    }

    pub async fn registro_auditoria_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let entradas = commands::leer_auditoria(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "entradas": entradas }).to_string())
    }

    /// Point-in-time, read-only permission audit. Missing inputs remain explicitly unknown.
    /// Prepara la corrección de un hallazgo de la auditoría como propuesta del
    /// motor (sin aplicarla). Rehace la auditoría para trabajar con datos
    /// frescos y se niega si el bot no podría aplicarla (permiso o jerarquía):
    /// nunca se ofrece una acción que supere al bot.
    pub async fn preparar_correccion_json(
        &self,
        guild_id: &str,
        hallazgo: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let informe: serde_json::Value =
            serde_json::from_str(&self.informe_permisos_json(Some(guild_id)).await?)
                .map_err(|e| e.to_string())?;
        if informe["incomplete"] == true {
            return Err("The audit is incomplete; a correction cannot be prepared".into());
        }
        let mismo = |f: &&serde_json::Value| {
            [
                "tipo",
                "role_id",
                "channel_id",
                "target_id",
                "permission_bit",
            ]
            .iter()
            .all(|k| f[*k] == hallazgo[*k])
        };
        let actual = informe["findings"]
            .as_array()
            .and_then(|f| f.iter().find(mismo))
            .ok_or("The finding is no longer present; run the audit again")?;
        let bit = actual["permission_bit"]
            .as_u64()
            .filter(|b| *b < 53)
            .ok_or("Invalid finding")?;
        let bits = |v: &serde_json::Value| v.as_str().and_then(|t| t.parse::<u64>().ok());
        let token = commands::token_del_bot_activo(&self.state)?;
        commands::exigir_permiso(&self.state, guild_id, "Gestionar roles", self.url()).await?;
        let (accion, antes, despues) = match actual["tipo"].as_str() {
            Some("role_grant") => {
                let role_id = actual["role_id"].as_str().ok_or("Invalid finding")?;
                if role_id == guild_id {
                    // @everyone: no hay jerarquía, pero el bot debe poder gestionar roles.
                } else {
                    commands::exigir_jerarquia(
                        &self.state,
                        guild_id,
                        &[(role_id, None)],
                        self.url(),
                    )
                    .await?;
                }
                let antes = bits(&actual["evidence_bits"]).ok_or("Invalid finding")?;
                let despues = antes & !(1u64 << bit);
                (
                    crate::propuesta::Accion {
                        herramienta: "editar_rol".into(),
                        args: json!({"guild_id": guild_id, "role_id": role_id, "permisos": despues.to_string()}),
                        descripcion: format!(
                            "Remove {} from role «{}»",
                            actual["permission"].as_str().unwrap_or_default(),
                            actual["role_name"].as_str().unwrap_or(role_id)
                        ),
                    },
                    antes,
                    despues,
                )
            }
            Some("channel_overwrite_grant") => {
                let channel_id = actual["channel_id"].as_str().ok_or("Invalid finding")?;
                let target_id = actual["target_id"].as_str().ok_or("Invalid finding")?;
                let tipo = match actual["target_type"].as_u64() {
                    Some(0) => "rol",
                    Some(1) => "miembro",
                    _ => return Err("Invalid finding".into()),
                };
                if tipo == "rol" && target_id != guild_id {
                    commands::exigir_jerarquia(
                        &self.state,
                        guild_id,
                        &[(target_id, None)],
                        self.url(),
                    )
                    .await?;
                }
                let canal = crate::discord::read_channel(&token, self.url(), channel_id).await?;
                let ow = canal
                    .overwrites
                    .iter()
                    .flatten()
                    .find(|o| o.id == target_id)
                    .ok_or("The channel overwrite no longer exists; run the audit again")?;
                let antes = ow.allow.parse::<u64>().map_err(|_| "Invalid overwrite")?;
                let despues = antes & !(1u64 << bit);
                (
                    crate::propuesta::Accion {
                        herramienta: "permiso_canal".into(),
                        args: json!({
                            "guild_id": guild_id, "channel_id": channel_id, "target_id": target_id,
                            "tipo": tipo, "permitir": despues.to_string(), "denegar": ow.deny,
                        }),
                        descripcion: format!(
                            "Remove {} from a channel overwrite",
                            actual["permission"].as_str().unwrap_or_default()
                        ),
                    },
                    antes,
                    despues,
                )
            }
            _ => return Err("This finding has no automatic correction".into()),
        };
        let propuesta = crate::propuesta::Propuesta {
            resumen: accion.descripcion.clone(),
            acciones: vec![accion],
        };
        let validacion = crate::propuesta::validar_en_estado(&self.state, &propuesta);
        Ok(json!({
            "hallazgo": actual,
            "propuesta": propuesta,
            "validacion": validacion,
            "antes": crate::discord::nombres_api_de_permisos(antes),
            "despues": crate::discord::nombres_api_de_permisos(despues),
        }))
    }

    pub async fn informe_permisos_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        self.informe_permisos_con_expectativas_json(guild_id, &[])
            .await
    }

    pub async fn informe_permisos_con_expectativas_json(
        &self,
        guild_id: Option<&str>,
        expectations: &[ExpectativaPrivacidad],
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let mut missing = Vec::new();
        let roles = commands::leer_roles(&self.state, &gid, self.url())
            .await
            .ok();
        let channels = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .ok();
        let members = commands::leer_miembros(&self.state, &gid, self.url())
            .await
            .ok();
        let owner = match commands::token_del_bot_activo(&self.state) {
            Ok(token) => crate::discord::fetch_guild_owner_id(&token, self.url(), &gid)
                .await
                .ok()
                .flatten(),
            Err(_) => None,
        };
        if roles.is_none() {
            missing.push("roles");
        }
        if channels.is_none() {
            missing.push("channels");
        }
        if members.is_none() {
            missing.push("members");
        }
        if owner.is_none() {
            missing.push("owner");
        }
        let roles = roles.unwrap_or_default();
        let channels = channels.unwrap_or_default();
        let members = members.unwrap_or_default();
        if !roles.iter().any(|r| r.id == gid)
            || roles.iter().any(|r| r.permissions.parse::<u64>().is_err())
        {
            missing.push("role permissions");
        }
        if channels.iter().any(|c| {
            c.overwrites.is_none()
                || c.overwrites.iter().flatten().any(|o| {
                    o.allow.parse::<u64>().is_err() || o.deny.parse::<u64>().is_err() || o.type_ > 1
                })
        }) {
            missing.push("channel overwrites");
        }
        if members
            .iter()
            .any(|m| m.roles.iter().any(|id| !roles.iter().any(|r| &r.id == id)))
        {
            missing.push("member roles");
        }
        let checked_at = chrono::Utc::now().to_rfc3339();
        let sensibles = [
            (3, "Administrator"),
            (13, "ManageMessages"),
            (4, "ManageChannels"),
            (28, "ManageRoles"),
            (29, "ManageWebhooks"),
            (1, "KickMembers"),
            (2, "BanMembers"),
            (17, "MentionEveryone"),
        ];
        let mut findings = Vec::new();
        for role in &roles {
            let bits = role.permissions.parse::<u64>().unwrap_or(0);
            for (bit, permission) in sensibles {
                if bits & (1 << bit) != 0 {
                    findings.push(json!({"tipo":"role_grant","role_id":role.id,"role_name":role.name,"permission":permission,"permission_bit":bit,"evidence_bits":bits.to_string()}));
                }
            }
        }
        let mut overwrites = Vec::new();
        for channel in &channels {
            for o in channel.overwrites.iter().flatten() {
                overwrites.push(json!({"channel_id":channel.id,"channel_name":channel.name,"target_id":o.id,"target_type":o.type_,"allow":o.allow,"deny":o.deny}));
                let allow = o.allow.parse::<u64>().unwrap_or(0);
                for (bit, permission) in sensibles {
                    if allow & (1 << bit) != 0 {
                        findings.push(json!({"tipo":"channel_overwrite_grant","channel_id":channel.id,"target_id":o.id,"target_type":o.type_,"permission":permission,"permission_bit":bit,"evidence_bits":o.allow}));
                    }
                }
            }
        }
        let inputs_ok = missing.is_empty();
        let mut access = Vec::new();
        if inputs_ok {
            for channel in &channels {
                for member in &members {
                    let bits =
                        audit_channel_permissions(&gid, owner.as_deref(), member, &roles, channel);
                    let view = bits & (1 << 10) != 0;
                    let sensitive: Vec<_> = sensibles
                        .iter()
                        .filter_map(|(bit, name)| (bits & (1 << bit) != 0).then_some(*name))
                        .collect();
                    access.push(json!({"channel_id":channel.id,"channel_name":channel.name,"parent_id":channel.parent_id,"member_id":member.user_id,"member_name":member.username,"view":view,"write":view && bits & (1 << 11) != 0,"manage":view && bits & ((1 << 4)|(1 << 28)|(1 << 29)|(1 << 13)) != 0,"permissions_bits":bits.to_string(),"sensitive_permissions":sensitive}));
                }
            }
        }
        for expected in expectations {
            if expected.evidence.trim().is_empty() {
                continue;
            }
            if let Some(observed) = access.iter().find(|a| {
                a["channel_id"] == expected.channel_id && a["member_id"] == expected.member_id
            }) {
                let actual = observed["view"].as_bool().unwrap_or(false);
                if actual != expected.can_view {
                    findings.push(json!({"tipo":"privacy_discrepancy", "channel_id":expected.channel_id, "member_id":expected.member_id, "expected_can_view":expected.can_view, "actual_can_view":actual, "expectation_source":expected.source, "expectation_evidence":expected.evidence, "permissions_bits":observed["permissions_bits"], "checked_at":checked_at}));
                }
            }
        }
        for finding in &mut findings {
            finding["checked_at"] = json!(checked_at);
        }
        let bot = self.state.get_active_bot();
        let bot_member = bot
            .as_ref()
            .and_then(|b| members.iter().find(|m| m.user_id == b.id));
        let bot_missing = if inputs_ok {
            bot_member.map(|m| {
                let bits = crate::discord::calcular_permisos(
                    &gid,
                    owner.as_deref(),
                    &m.user_id,
                    &roles,
                    &m.roles,
                );
                [(4, "ManageChannels"), (28, "ManageRoles")]
                    .iter()
                    .filter_map(|(bit, name)| (bits & (1 << bit) == 0).then_some(*name))
                    .collect::<Vec<_>>()
            })
        } else {
            None
        };
        let bot_channel_missing = if inputs_ok {
            bot_member.map(|member| {
                channels
                    .iter()
                    .map(|channel| {
                        let bits = audit_channel_permissions(
                            &gid,
                            owner.as_deref(),
                            member,
                            &roles,
                            channel,
                        );
                        let absent: Vec<_> = [
                            (10, "ViewChannel"),
                            (4, "ManageChannels"),
                            (28, "ManageRoles"),
                        ]
                        .iter()
                        .filter_map(|(bit, name)| (bits & (1 << bit) == 0).then_some(*name))
                        .collect();
                        json!({"channel_id": channel.id, "missing_permissions": absent})
                    })
                    .collect::<Vec<_>>()
            })
        } else {
            None
        };
        let role_hierarchy = if inputs_ok {
            bot_member.map(|member| {
            let top = roles.iter().filter(|r| member.roles.contains(&r.id)).map(commands::rango_rol).max();
            roles.iter().filter(|r| r.id != gid).map(|r| json!({"role_id":r.id, "role_name":r.name, "below_bot":top.is_some_and(|rank| commands::rango_rol(r) < rank)})).collect::<Vec<_>>()
        })
        } else {
            None
        };
        if bot_missing.is_none() {
            missing.push("bot permissions");
        }
        Ok(json!({"guild_id":gid,"checked_at":checked_at,"status":"partial","incomplete":!missing.is_empty(),"missing_data":missing,"findings":findings,"channel_overwrites":overwrites,"channel_access":access,"bot_missing_build_permissions":bot_missing,"bot_channel_missing_permissions":bot_channel_missing,"bot_role_hierarchy":role_hierarchy,
            "limits":["Data is a point-in-time observation; threads and expectations not supplied are not assessed, and absence of findings does not establish security."],
            "correction_workflow":"Explain evidence and prepare a proposal; the user must review and Apply it in ARGUS, including any destructive confirmations."}).to_string())
    }

    pub async fn leer_mensajes_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
        limite: Option<u32>,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let limite = (limite.unwrap_or(MENSAJES_DEFECTO as u32) as usize).clamp(1, MENSAJES_MAX);
        // Hasta 50 basta una petición; más, se pagina hacia atrás.
        let mensajes = if limite <= MENSAJES_DEFECTO {
            commands::leer_mensajes(&self.state, &gid, channel_id.trim(), self.url()).await
        } else {
            commands::leer_historial(&self.state, &gid, channel_id.trim(), limite, self.url()).await
        }
        .map_err(|e| self.limpiar(e))?;
        // Discord los entrega del más nuevo al más antiguo.
        let lista: Vec<_> = mensajes.into_iter().take(limite).collect();
        Ok(json!({ "canal_id": channel_id.trim(), "mensajes": lista }).to_string())
    }

    /// Los bots no pueden usar la búsqueda de Discord: se lee el historial
    /// reciente de cada canal y se filtra en local.
    pub async fn buscar_mensajes_json(
        &self,
        guild_id: Option<&str>,
        texto: &str,
        canales: Option<&[String]>,
        autor: Option<&str>,
        limite_por_canal: Option<u32>,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let aguja = plegar(texto);
        if aguja.is_empty() {
            return Err("Enter the text to search for".to_string());
        }
        let limite =
            (limite_por_canal.unwrap_or(BUSQUEDA_DEFECTO as u32) as usize).clamp(1, BUSQUEDA_MAX);
        let autor = autor.map(plegar).filter(|a| !a.is_empty());
        let todos = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let de_texto: Vec<&Channel> = todos
            .iter()
            .filter(|c| matches!(c.channel_type, 0 | 5))
            .collect();
        let elegidos: Vec<&Channel> = match canales.filter(|c| !c.is_empty()) {
            None => de_texto,
            Some(pedidos) => {
                let mut v: Vec<&Channel> = Vec::new();
                for p in pedidos {
                    let limpio = p.trim().trim_start_matches('#');
                    let nombre = plegar(limpio);
                    let c = de_texto
                        .iter()
                        .find(|c| c.id == limpio || plegar(&c.name) == nombre)
                        .ok_or_else(|| format!("Text channel not found: «{}»", p.trim()))?;
                    if !v.iter().any(|x| x.id == c.id) {
                        v.push(c);
                    }
                }
                v
            }
        };
        let mut coincidencias = Vec::new();
        let mut revisados = Vec::new();
        for c in elegidos {
            match commands::leer_historial(&self.state, &gid, &c.id, limite, self.url()).await {
                Ok(mensajes) => {
                    revisados.push(json!({
                        "canal_id": c.id, "canal": c.name, "revisados": mensajes.len(),
                    }));
                    for m in mensajes {
                        let del_autor = autor.as_ref().is_none_or(|a| {
                            m.author_id == *a || plegar(&m.author_name).contains(a.as_str())
                        });
                        if !del_autor {
                            continue;
                        }
                        let Some(fragmento) = fragmento_de(&m.content, &aguja) else {
                            continue;
                        };
                        coincidencias.push(json!({
                            "canal_id": c.id,
                            "canal": c.name,
                            "autor": { "id": m.author_id, "nombre": m.author_name },
                            "mensaje_id": m.id,
                            "enlace": format!(
                                "https://discord.com/channels/{gid}/{}/{}", c.id, m.id
                            ),
                            "fecha": m.timestamp,
                            "fragmento": fragmento,
                        }));
                    }
                }
                Err(e) => revisados.push(json!({
                    "canal_id": c.id, "canal": c.name, "revisados": 0,
                    "error": self.limpiar(e),
                })),
            }
        }
        Ok(json!({
            "total": coincidencias.len(),
            "coincidencias": coincidencias,
            "revisados_por_canal": revisados,
        })
        .to_string())
    }

    pub async fn listar_baneos_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let bans = commands::listar_bans(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "baneos": bans }).to_string())
    }

    pub async fn listar_invitaciones_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let invitaciones = commands::leer_invitaciones(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "invitaciones": invitaciones }).to_string())
    }

    /// Los webhooks nunca llevan su URL (contiene el token): solo se da al crearlos.
    pub async fn listar_webhooks_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let webhooks = commands::leer_webhooks(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .unwrap_or_default();
        let lista: Vec<_> = webhooks
            .iter()
            .map(|w| {
                let canal = w
                    .channel_id
                    .as_ref()
                    .and_then(|id| canales.iter().find(|c| &c.id == id))
                    .map(|c| c.name.clone());
                json!({ "id": w.id, "nombre": w.name, "canal_id": w.channel_id, "canal": canal })
            })
            .collect();
        Ok(json!({ "webhooks": lista }).to_string())
    }

    // ------------------------------------------- escritura: mensajes, canales, roles

    fn mensaje(&self, texto: String) -> Result<String, String> {
        Ok(json!({ "mensaje": self.tr(&texto) }).to_string())
    }

    /// Idioma guardado en la config de la base (inglés por defecto).
    fn idioma(&self) -> String {
        i18n::idioma(&self.state)
    }

    fn tr(&self, texto: &str) -> String {
        i18n::tr(&self.idioma(), texto)
    }

    /// Salida de una herramienta: el error sale en el idioma de la config.
    fn salida(&self, r: Result<String, String>) -> Result<String, String> {
        r.map_err(|e| self.tr(&e))
    }

    pub async fn enviar_mensaje_json(
        &self,
        channel_id: &str,
        contenido: &str,
        responder_a: Option<&str>,
    ) -> Result<String, String> {
        self.enviar_mensaje_embed_json(channel_id, Some(contenido), None, responder_a)
            .await
    }

    pub async fn enviar_mensaje_embed_json(
        &self,
        channel_id: &str,
        contenido: Option<&str>,
        embed: Option<&crate::discord::EmbedMensaje>,
        responder_a: Option<&str>,
    ) -> Result<String, String> {
        self.preparar().await?;
        let reply = responder_a.map(str::trim).filter(|m| !m.is_empty());
        let texto = commands::enviar_a_canal_embed(
            &self.state,
            channel_id.trim(),
            contenido,
            embed,
            reply,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn editar_mensaje_json(
        &self,
        channel_id: &str,
        message_id: &str,
        contenido: &str,
    ) -> Result<String, String> {
        self.editar_mensaje_embed_json(channel_id, message_id, Some(contenido), None)
            .await
    }

    pub async fn editar_mensaje_embed_json(
        &self,
        channel_id: &str,
        message_id: &str,
        contenido: Option<&str>,
        embed: Option<&crate::discord::EmbedMensaje>,
    ) -> Result<String, String> {
        self.preparar().await?;
        let texto = commands::editar_mensaje_con_embed(
            &self.state,
            channel_id.trim(),
            message_id.trim(),
            contenido,
            embed,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// `reaccionar` (anadir) y `quitar_reaccion`. El servidor es opcional: si
    /// se conoce se comprueba antes el permiso «Añadir reacciones».
    pub async fn reaccion_json(&self, p: &ReaccionParams, anadir: bool) -> Result<String, String> {
        self.preparar().await?;
        let gid = p
            .guild_id
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_string)
            .or_else(|| self.state.get_active_guild());
        let texto = commands::reaccionar_a_mensaje(
            &self.state,
            gid.as_deref(),
            p.channel_id.trim(),
            p.message_id.trim(),
            &p.emoji,
            anadir,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// `crear_encuesta`. Con servidor conocido exige «Crear encuestas».
    pub async fn crear_encuesta_json(&self, p: &CrearEncuestaParams) -> Result<String, String> {
        self.preparar().await?;
        let gid = p
            .guild_id
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_string)
            .or_else(|| self.state.get_active_guild());
        let respuestas: Vec<crate::encuestas::RespuestaEncuesta> = p
            .respuestas
            .iter()
            .map(|r| crate::encuestas::RespuestaEncuesta {
                texto: r.texto.clone(),
                emoji: r.emoji.clone(),
            })
            .collect();
        let texto = commands::crear_encuesta_en_canal(
            &self.state,
            gid.as_deref(),
            p.channel_id.trim(),
            &p.pregunta,
            &respuestas,
            p.duracion_horas.unwrap_or(24),
            p.voto_multiple.unwrap_or(false),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// `resultados_encuesta` (lectura).
    pub async fn resultados_encuesta_json(
        &self,
        p: &EncuestaMensajeParams,
    ) -> Result<String, String> {
        self.preparar().await?;
        let r = commands::resultados_de_encuesta(
            &self.state,
            p.channel_id.trim(),
            p.message_id.trim(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        Ok(r.to_string())
    }

    pub async fn cerrar_encuesta_json(&self, p: &EncuestaMensajeParams) -> Result<String, String> {
        self.preparar().await?;
        let texto = commands::cerrar_encuesta_de_canal(
            &self.state,
            p.channel_id.trim(),
            p.message_id.trim(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn crear_post_foro_json(
        &self,
        channel_id: &str,
        titulo: &str,
        contenido: &str,
        etiquetas: &[String],
    ) -> Result<String, String> {
        self.preparar().await?;
        let texto = commands::crear_post_en_foro(
            &self.state,
            channel_id.trim(),
            titulo,
            contenido,
            etiquetas,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// `etiquetas_foro` (lectura).
    pub async fn etiquetas_foro_json(&self, channel_id: &str) -> Result<String, String> {
        self.preparar().await?;
        let tags = commands::etiquetas_del_foro(&self.state, channel_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "etiquetas": tags }).to_string())
    }

    pub async fn editar_etiquetas_foro_json(
        &self,
        p: &EditarEtiquetasForoParams,
    ) -> Result<String, String> {
        self.preparar().await?;
        let cambios: Vec<crate::discord::CambioEtiqueta> = p
            .etiquetas
            .iter()
            .map(|c| crate::discord::CambioEtiqueta {
                nombre: c.nombre.clone(),
                nuevo_nombre: c.nuevo_nombre.clone(),
                emoji: c.emoji.clone(),
            })
            .collect();
        let texto = commands::editar_etiquetas_del_foro(
            &self.state,
            p.channel_id.trim(),
            &cambios,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn etiquetar_post_json(&self, p: &EtiquetarPostParams) -> Result<String, String> {
        self.preparar().await?;
        let texto = commands::etiquetar_post_de_foro(
            &self.state,
            p.post_id.trim(),
            &p.etiquetas,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn enviar_dm_json(&self, user_id: &str, contenido: &str) -> Result<String, String> {
        self.preparar().await?;
        let texto = commands::enviar_dm(&self.state, user_id.trim(), contenido, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// Resuelve un usuario dado por ID o por nombre (usuario o apodo, sin
    /// distinguir mayúsculas) a su ID.
    async fn resolver_usuario(&self, gid: &str, texto: &str) -> Result<String, String> {
        let texto = texto.trim();
        if texto.is_empty() {
            return Err("The user is empty".to_string());
        }
        if texto.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(texto.to_string());
        }
        let miembros = commands::leer_miembros(&self.state, gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let b = texto.trim_start_matches('@').to_lowercase();
        let coincide = |m: &Member| {
            m.username.to_lowercase() == b
                || m.nickname.as_deref().is_some_and(|n| n.to_lowercase() == b)
        };
        let halladas: Vec<&Member> = miembros.iter().filter(|m| coincide(m)).collect();
        match halladas.as_slice() {
            [m] => Ok(m.user_id.clone()),
            [] => Err(format!("No member named «{texto}» in the server")),
            _ => Err(format!(
                "More than one member is named «{texto}»: use the user ID"
            )),
        }
    }

    pub async fn avisar_json(&self, p: &AvisarParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let motivo = p.motivo.trim();
        if motivo.is_empty() {
            return Err("The warning needs a reason".to_string());
        }
        let user = self.resolver_usuario(&gid, &p.user_id).await?;
        let mensaje_id = p
            .mensaje_id
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty());
        let autor = self
            .state
            .get_active_bot()
            .map(|b| b.id)
            .unwrap_or_default();
        let (aviso, total) =
            crate::db::guardar_aviso(&self.pool, &gid, &user, motivo, mensaje_id, &autor)
                .await
                .map_err(|e| format!("Could not save the warning: {e}"))?;
        let dm = if p.dm.unwrap_or(true) {
            let texto = format!(
                "{}\n{}",
                self.tr(&format!("You have received a warning: {motivo}")),
                self.tr(&format!("Warnings so far: {total}"))
            );
            match commands::enviar_dm(&self.state, &user, &texto, self.url()).await {
                Ok(_) => json!({ "enviado": true }),
                Err(e) => json!({
                    "enviado": false,
                    "error": self.tr(&self.limpiar(e)),
                    "nota": self.tr("The warning was saved, but the direct message could not be sent"),
                }),
            }
        } else {
            json!({ "enviado": false, "omitido": true })
        };
        Ok(json!({
            "mensaje": self.tr(&format!("Warning {total} saved for user {user}")),
            "aviso": aviso,
            "total_avisos": total,
            "dm": dm,
        })
        .to_string())
    }

    pub async fn listar_avisos_json(
        &self,
        guild_id: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let user = match user_id.map(str::trim).filter(|u| !u.is_empty()) {
            Some(u) => Some(self.resolver_usuario(&gid, u).await?),
            None => None,
        };
        let err = |e: anyhow::Error| format!("Could not read the warnings: {e}");
        let recuento = crate::db::recuento_avisos(&self.pool, &gid, user.as_deref())
            .await
            .map_err(err)?;
        let ultimos = crate::db::ultimos_avisos(&self.pool, &gid, user.as_deref(), 20)
            .await
            .map_err(err)?;
        let total: i64 = recuento.iter().map(|(_, n)| n).sum();
        let por_usuario: Vec<_> = recuento
            .iter()
            .map(|(u, n)| json!({ "user_id": u, "avisos": n }))
            .collect();
        Ok(json!({
            "total": total,
            "por_usuario": por_usuario,
            "ultimos": ultimos,
        })
        .to_string())
    }

    pub async fn borrar_mensaje_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
        message_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_mensaje(
            &self.state,
            &gid,
            channel_id.trim(),
            message_id.trim(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn borrar_mensajes_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
        message_ids: &[String],
    ) -> Result<String, String> {
        // Discord exige de 2 a 100 mensajes para el borrado masivo: con uno
        // solo se borra ese mensaje en vez de fallar.
        if let [unico] = message_ids {
            return self.borrar_mensaje_json(guild_id, channel_id, unico).await;
        }
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_mensajes_en_masa(
            &self.state,
            &gid,
            channel_id.trim(),
            message_ids,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn fijar_mensaje_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
        message_id: &str,
        fijar: bool,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::fijar_mensaje(
            &self.state,
            &gid,
            channel_id.trim(),
            message_id.trim(),
            fijar,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn crear_canal_json(&self, p: &CrearCanalParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let tipo = codigo_tipo_canal(p.tipo.as_deref())?;
        let mut overwrites = Vec::new();
        if let Some(permisos) = &p.permisos {
            let roles = commands::leer_roles(&self.state, &gid, self.url()).await?;
            let mut nombres = Nombres::desde(&roles, &[]);
            if permisos
                .iter()
                .any(|p| matches!(p.tipo, TipoObjetivoCanal::Miembro))
            {
                let miembros = commands::leer_miembros(&self.state, &gid, self.url()).await?;
                nombres.registrar_miembros(&miembros);
            }
            for p in permisos {
                let tipo = match p.tipo {
                    TipoObjetivoCanal::Rol => "rol",
                    TipoObjetivoCanal::Miembro => "miembro",
                };
                let id = if tipo == "rol" && p.objetivo.trim() == "@everyone" {
                    gid.clone()
                } else {
                    nombres.resolver_objetivo(&p.objetivo, tipo)?
                };
                overwrites.push(
                    serde_json::from_value(json!({
                        "id": id, "type": if tipo == "rol" { 0 } else { 1 },
                        "allow": crate::discord::bits_desde_nombres(&p.permitir)?,
                        "deny": crate::discord::bits_desde_nombres(&p.denegar)?
                    }))
                    .map_err(|e| format!("Invalid permission overwrite: {e}"))?,
                );
            }
        }
        let datos = DatosCanal {
            permission_overwrites: Some(overwrites),
            name: Some(p.nombre.clone()),
            channel_type: Some(tipo),
            parent_id: p.categoria_id.clone(),
            topic: p.tema.clone(),
            nsfw: p.nsfw,
            rate_limit_per_user: p.slowmode_segundos,
        };
        let canal = commands::crear_canal(&self.state, &gid, &datos, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "canal": canal_json(&canal) }).to_string())
    }

    pub async fn editar_canal_json(&self, p: &EditarCanalParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let datos = DatosCanal {
            permission_overwrites: None,
            name: p.nombre.clone(),
            channel_type: None,
            parent_id: p.categoria_id.clone(),
            topic: p.tema.clone(),
            nsfw: p.nsfw,
            rate_limit_per_user: p.slowmode_segundos,
        };
        let canal =
            commands::editar_canal(&self.state, &gid, p.channel_id.trim(), &datos, self.url())
                .await
                .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "canal": canal_json(&canal) }).to_string())
    }

    pub async fn borrar_canal_json(
        &self,
        guild_id: Option<&str>,
        channel_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_canal(&self.state, &gid, channel_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn ordenar_canales_json(
        &self,
        guild_id: Option<&str>,
        posiciones: &[Posicion],
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let pares: Vec<(String, u64)> = posiciones
            .iter()
            .map(|p| (p.id.trim().to_string(), p.posicion))
            .collect();
        let texto = commands::reordenar_canales(&self.state, &gid, &pares, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn crear_rol_json(&self, p: &CrearRolParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let datos = DatosRol {
            name: Some(p.nombre.clone()),
            color: color_opcional(&p.color)?,
            hoist: p.separado,
            mentionable: p.mencionable,
            permissions: bits_opcionales(&p.permisos)?,
        };
        let rol = commands::crear_rol(&self.state, &gid, &datos, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "rol": rol_json(&rol) }).to_string())
    }

    pub async fn editar_rol_json(&self, p: &EditarRolParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let datos = DatosRol {
            name: p.nombre.clone(),
            color: color_opcional(&p.color)?,
            hoist: p.separado,
            mentionable: p.mencionable,
            permissions: bits_opcionales(&p.permisos)?,
        };
        let rol = commands::editar_rol(&self.state, &gid, p.role_id.trim(), &datos, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "rol": rol_json(&rol) }).to_string())
    }

    pub async fn borrar_rol_json(
        &self,
        guild_id: Option<&str>,
        role_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_rol(&self.state, &gid, role_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn ordenar_roles_json(
        &self,
        guild_id: Option<&str>,
        posiciones: &[Posicion],
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let pares: Vec<(String, u64)> = posiciones
            .iter()
            .map(|p| (p.id.trim().to_string(), p.posicion))
            .collect();
        let texto = commands::reordenar_roles(&self.state, &gid, &pares, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn cambiar_rol_json(
        &self,
        guild_id: Option<&str>,
        user_id: &str,
        role_id: &str,
        anadir: bool,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::cambiar_rol(
            &self.state,
            &gid,
            user_id.trim(),
            role_id.trim(),
            anadir,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn permiso_canal_json(&self, p: &PermisoCanalParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let tipo = tipo_de_overwrite(&p.tipo)?;
        let permitir = bits_opcionales(&p.permitir)?.unwrap_or_else(|| "0".to_string());
        let denegar = bits_opcionales(&p.denegar)?.unwrap_or_else(|| "0".to_string());
        let texto = commands::fijar_permiso_canal(
            &self.state,
            &gid,
            p.channel_id.trim(),
            p.target_id.trim(),
            tipo,
            &permitir,
            &denegar,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn quitar_permiso_canal_json(
        &self,
        p: &QuitarPermisoCanalParams,
    ) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let tipo = tipo_de_overwrite(&p.tipo)?;
        let texto = commands::quitar_permiso_canal(
            &self.state,
            &gid,
            p.channel_id.trim(),
            p.target_id.trim(),
            tipo,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    // ------------------------------- escritura: moderación, servidor, invitaciones, webhooks

    pub async fn expulsar_json(&self, p: &ExpulsarParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let texto = commands::expulsar_miembro(
            &self.state,
            &gid,
            p.user_id.trim(),
            p.motivo.as_deref(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn banear_json(&self, p: &BanearParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        // Fuera de 0–7 (incluso si no cabe en u8) se rechaza con el mismo mensaje de la app.
        let dias = u8::try_from(p.dias_mensajes.unwrap_or(0)).unwrap_or(u8::MAX);
        let texto = commands::banear_miembro(
            &self.state,
            &gid,
            p.user_id.trim(),
            p.motivo.as_deref(),
            dias,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn desbanear_json(
        &self,
        guild_id: Option<&str>,
        user_id: &str,
        motivo: Option<&str>,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::quitar_ban(&self.state, &gid, user_id.trim(), motivo, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn silenciar_json(&self, p: &SilenciarParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let texto = commands::silenciar_miembro(
            &self.state,
            &gid,
            p.user_id.trim(),
            p.minutos,
            p.motivo.as_deref(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn cambiar_apodo_json(&self, p: &CambiarApodoParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let texto = commands::cambiar_apodo(
            &self.state,
            &gid,
            p.user_id.trim(),
            p.apodo.as_deref(),
            p.motivo.as_deref(),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// Lee los ajustes actuales y cambia solo los campos indicados; el guardado
    /// es el de la app (`PATCH /guilds/{id}` con todos los campos).
    pub async fn editar_servidor_json(&self, p: &EditarServidorParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let mut a = commands::leer_ajustes_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        if let Some(n) = &p.nombre {
            a.name = n.clone();
        }
        if let Some(d) = &p.descripcion {
            a.description = Some(d.clone());
        }
        if let Some(n) = p.nivel_verificacion {
            a.verification_level = n;
        }
        if let Some(n) = p.filtro_contenido_explicito {
            a.explicit_content_filter = n;
        }
        if let Some(n) = p.notificaciones_por_defecto {
            a.default_message_notifications = n;
        }
        if let Some(c) = &p.canal_del_sistema_id {
            a.system_channel_id = Some(c.clone());
        }
        if let Some(c) = &p.canal_afk_id {
            a.afk_channel_id = Some(c.clone());
        }
        if let Some(s) = p.afk_segundos {
            a.afk_timeout = s;
        }
        let nuevo = commands::guardar_ajustes_servidor(&self.state, &gid, &a, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({
            "servidor": {
                "nombre": nuevo.name,
                "descripcion": nuevo.description,
                "nivel_verificacion": nuevo.verification_level,
                "filtro_contenido_explicito": nuevo.explicit_content_filter,
                "notificaciones_por_defecto": nuevo.default_message_notifications,
                "canal_del_sistema_id": nuevo.system_channel_id,
                "canal_afk_id": nuevo.afk_channel_id,
                "afk_segundos": nuevo.afk_timeout,
            }
        })
        .to_string())
    }

    pub async fn crear_invitacion_json(&self, p: &CrearInvitacionParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let invitacion = commands::crear_invitacion(
            &self.state,
            &gid,
            p.channel_id.trim(),
            p.caducidad_segundos.unwrap_or(86_400),
            p.usos_maximos.unwrap_or(0),
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "invitacion": invitacion }).to_string())
    }

    pub async fn borrar_invitacion_json(
        &self,
        guild_id: Option<&str>,
        codigo: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_invitacion(&self.state, &gid, codigo.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// Única respuesta que incluye la URL completa del webhook (lleva su token).
    pub async fn crear_webhook_json(&self, p: &CrearWebhookParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let w = commands::crear_webhook(
            &self.state,
            &gid,
            p.channel_id.trim(),
            &p.nombre,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        Ok(json!({
            "webhook": { "id": w.id, "nombre": w.name, "canal_id": w.channel_id, "url": w.url }
        })
        .to_string())
    }

    pub async fn borrar_webhook_json(
        &self,
        guild_id: Option<&str>,
        webhook_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto = commands::borrar_webhook(&self.state, &gid, webhook_id.trim(), self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    /// `listar_eventos` (lectura).
    pub async fn listar_eventos_json(&self, guild_id: Option<&str>) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let eventos = commands::listar_eventos_del_servidor(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        Ok(json!({ "eventos": eventos }).to_string())
    }

    pub async fn crear_evento_json(&self, p: &CrearEventoParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let datos = crate::eventos::DatosEvento {
            nombre: Some(p.nombre.clone()),
            descripcion: p.descripcion.clone(),
            inicio: Some(p.inicio.clone()),
            fin: p.fin.clone(),
            lugar: p.lugar.clone(),
            canal_id: p.canal_id.clone(),
        };
        let texto = commands::crear_evento_del_servidor(&self.state, &gid, &datos, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn editar_evento_json(&self, p: &EditarEventoParams) -> Result<String, String> {
        let gid = self.servidor_de(p.guild_id.as_deref()).await?;
        let datos = crate::eventos::DatosEvento {
            nombre: p.nombre.clone(),
            descripcion: p.descripcion.clone(),
            inicio: p.inicio.clone(),
            fin: p.fin.clone(),
            lugar: p.lugar.clone(),
            canal_id: p.canal_id.clone(),
        };
        let texto = commands::editar_evento_del_servidor(
            &self.state,
            &gid,
            p.evento_id.trim(),
            &datos,
            self.url(),
        )
        .await
        .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }

    pub async fn cancelar_evento_json(
        &self,
        guild_id: Option<&str>,
        evento_id: &str,
    ) -> Result<String, String> {
        let gid = self.servidor_de(guild_id).await?;
        let texto =
            commands::cancelar_evento_del_servidor(&self.state, &gid, evento_id.trim(), self.url())
                .await
                .map_err(|e| self.limpiar(e))?;
        self.mensaje(texto)
    }
}

/// Minúsculas y sin tildes para comparar textos sueltos («categoría» = «categoria»).
fn normalizar(t: &str) -> String {
    t.trim()
        .to_lowercase()
        .replace('í', "i")
        .replace('á', "a")
        .replace('é', "e")
}

/// Minúsculas y sin tildes, para comparar textos.
fn plegar(t: &str) -> String {
    t.trim().chars().map(plegar_char).collect()
}

/// Un carácter en minúscula y sin tilde (siempre devuelve un solo carácter, así
/// los índices del texto plegado coinciden con los del original).
fn plegar_char(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        c => c,
    }
}

/// Trozo del contenido alrededor de la primera coincidencia (None si no hay).
fn fragmento_de(contenido: &str, aguja_plegada: &str) -> Option<String> {
    let chars: Vec<char> = contenido.chars().collect();
    let pleg: Vec<char> = chars.iter().map(|c| plegar_char(*c)).collect();
    let aguja: Vec<char> = aguja_plegada.chars().collect();
    if aguja.is_empty() || aguja.len() > pleg.len() {
        return None;
    }
    let pos = pleg
        .windows(aguja.len())
        .position(|w| w == aguja.as_slice())?;
    let ini = pos.saturating_sub(FRAGMENTO_MAX / 4);
    let fin = (ini + FRAGMENTO_MAX).min(chars.len());
    let mut s: String = chars[ini..fin].iter().collect();
    if ini > 0 {
        s.insert(0, '…');
    }
    if fin < chars.len() {
        s.push('…');
    }
    Some(s)
}

/// 0 = rol, 1 = miembro (igual que `Overwrite::type_`).
/// Discord applies @everyone, combined role overwrites, then the member overwrite.
fn audit_channel_permissions(
    gid: &str,
    owner: Option<&str>,
    member: &crate::state::Member,
    roles: &[crate::state::Role],
    channel: &crate::state::Channel,
) -> u64 {
    let mut bits =
        crate::discord::calcular_permisos(gid, owner, &member.user_id, roles, &member.roles);
    if owner == Some(member.user_id.as_str()) || bits & (1 << 3) != 0 {
        return bits;
    }
    let overwrites = channel.overwrites.as_deref().unwrap_or_default();
    let apply = |bits: u64, allow: u64, deny: u64| (bits & !deny) | allow;
    for o in overwrites.iter().filter(|o| o.type_ == 0 && o.id == gid) {
        bits = apply(
            bits,
            o.allow.parse().unwrap_or(0),
            o.deny.parse().unwrap_or(0),
        );
    }
    let (mut allow, mut deny) = (0, 0);
    for o in overwrites
        .iter()
        .filter(|o| o.type_ == 0 && o.id != gid && member.roles.contains(&o.id))
    {
        allow |= o.allow.parse::<u64>().unwrap_or(0);
        deny |= o.deny.parse::<u64>().unwrap_or(0);
    }
    bits = apply(bits, allow, deny);
    for o in overwrites
        .iter()
        .filter(|o| o.type_ == 1 && o.id == member.user_id)
    {
        bits = apply(
            bits,
            o.allow.parse().unwrap_or(0),
            o.deny.parse().unwrap_or(0),
        );
    }
    bits
}

fn tipo_de_overwrite(t: &str) -> Result<u8, String> {
    match normalizar(t).as_str() {
        "rol" | "role" | "0" => Ok(0),
        "miembro" | "member" | "usuario" | "1" => Ok(1),
        _ => Err("The type must be «rol» or «miembro»".to_string()),
    }
}

fn rol_json(r: &Role) -> serde_json::Value {
    let bits = r.permissions.parse::<u64>().unwrap_or(0);
    json!({
        "id": r.id,
        "nombre": r.name,
        "posicion": r.position,
        "color": format!("#{:06x}", r.color),
        "separado": r.hoist,
        "mencionable": r.mentionable,
        "permisos_bits": r.permissions,
        "permisos": crate::discord::nombres_de_permisos(bits),
    })
}

/// Discord's channel type code for a requested type («texto» by default).
/// Shared by creation and by its verification, so both read the type the same way.
fn codigo_tipo_canal(tipo: Option<&str>) -> Result<u8, String> {
    match tipo.map(normalizar) {
        None => Ok(0),
        Some(t) => match t.as_str() {
            "texto" | "text" => Ok(0),
            "voz" | "voice" => Ok(2),
            "categoria" | "category" => Ok(4),
            "anuncios" | "announcement" => Ok(5),
            "foro" | "forum" => Ok(15),
            _ => Err(
                "Unsupported channel type: use texto, voz, categoria, anuncios or foro".to_string(),
            ),
        },
    }
}

fn tipo_de_canal(t: u8) -> &'static str {
    match t {
        0 => "texto",
        2 => "voz",
        4 => "categoría",
        5 => "anuncios",
        10..=12 => "hilo",
        13 => "escenario",
        15 => "foro",
        16 => "multimedia",
        _ => "otro",
    }
}

fn canal_json(c: &Channel) -> serde_json::Value {
    json!({ "id": c.id, "nombre": c.name, "tipo": tipo_de_canal(c.channel_type), "posicion": c.position, "categoria_id": c.parent_id, "tema": c.topic })
}

fn miembro_json(m: &Member, roles: &[Role]) -> serde_json::Value {
    let con_nombre: Vec<_> = m
        .roles
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "nombre": roles.iter().find(|r| &r.id == id).map(|r| r.name.clone()),
            })
        })
        .collect();
    json!({
        "id": m.user_id,
        "usuario": m.username,
        "apodo": m.nickname,
        "bot": m.bot,
        "roles": con_nombre,
    })
}

/// Herramientas que solo leen (no cambian nada en Discord ni en la BD). Son las
/// únicas que se le dan a un agente de IA; el resto va por propuesta aprobada.
pub const HERRAMIENTAS_LECTURA: &[&str] = &[
    "listar_plantillas",
    "consultar_plantilla",
    "proponer_plantilla",
    "proponer_correccion",
    "informe_permisos",
    "info_restauracion",
    "listar_snapshots",
    "comparar_snapshot",
    "estado",
    "listar_servidores",
    "mapa_servidor",
    "listar_canales",
    "listar_roles",
    "listar_miembros",
    "info_miembro",
    "permisos_bot",
    "info_servidor",
    "ajustes_servidor",
    "registro_auditoria",
    "leer_mensajes",
    "buscar_mensajes",
    "listar_baneos",
    "listar_invitaciones",
    "listar_webhooks",
    "etiquetas_foro",
    "listar_eventos",
    "resultados_encuesta",
    "listar_avisos",
];

/// Herramientas que modifican algo (Discord o el servidor activo guardado).
pub const HERRAMIENTAS_ESCRITURA: &[&str] = &[
    "guardar_plantilla_desde_plan",
    "guardar_plantilla_desde_snapshot",
    "crear_snapshot",
    "aplicar_restauracion",
    "elegir_servidor",
    "enviar_mensaje",
    "editar_mensaje",
    "crear_post_foro",
    "editar_etiquetas_foro",
    "etiquetar_post",
    "crear_evento",
    "editar_evento",
    "cancelar_evento",
    "crear_encuesta",
    "cerrar_encuesta",
    "reaccionar",
    "quitar_reaccion",
    "enviar_dm",
    "avisar",
    "borrar_mensaje",
    "borrar_mensajes",
    "fijar_mensaje",
    "desfijar_mensaje",
    "crear_canal",
    "editar_canal",
    "borrar_canal",
    "ordenar_canales",
    "crear_rol",
    "editar_rol",
    "borrar_rol",
    "ordenar_roles",
    "dar_rol",
    "quitar_rol",
    "permiso_canal",
    "quitar_permiso_canal",
    "expulsar",
    "banear",
    "desbanear",
    "silenciar",
    "cambiar_apodo",
    "editar_servidor",
    "crear_invitacion",
    "borrar_invitacion",
    "crear_webhook",
    "borrar_webhook",
];

fn args_de<T: serde::de::DeserializeOwned>(
    nombre: &str,
    args: serde_json::Value,
) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("Invalid arguments for «{nombre}»: {e}"))
}

/// Rechaza lo que no sea una herramienta de escritura (desconocida o de lectura).
fn comprobar_escritura(nombre: &str) -> Result<(), String> {
    if HERRAMIENTAS_ESCRITURA.contains(&nombre) {
        return Ok(());
    }
    Err(if HERRAMIENTAS_LECTURA.contains(&nombre) {
        format!("«{nombre}» is a read-only tool, not an action")
    } else {
        format!("Unknown action: «{nombre}»")
    })
}

/// Comprueba, sin ejecutar nada, que `nombre` es una herramienta de escritura y
/// que `args` se deserializa en sus parámetros.
pub fn validar_accion(nombre: &str, args: &serde_json::Value) -> Result<(), String> {
    comprobar_escritura(nombre)?;
    fn ok<T: serde::de::DeserializeOwned>(n: &str, a: &serde_json::Value) -> Result<(), String> {
        args_de::<T>(n, a.clone()).map(|_| ())
    }
    match nombre {
        "guardar_plantilla_desde_plan" => ok::<GuardarPlantillaPlanParams>(nombre, args),
        "guardar_plantilla_desde_snapshot" => ok::<GuardarPlantillaSnapshotParams>(nombre, args),
        "crear_snapshot" => ok::<ServidorParams>(nombre, args),
        "aplicar_restauracion" => ok::<ApplyRestoreParams>(nombre, args),
        "elegir_servidor" => ok::<ElegirServidorParams>(nombre, args),
        "enviar_mensaje" => ok::<EnviarMensajeParams>(nombre, args),
        "crear_post_foro" => ok::<CrearPostForoParams>(nombre, args),
        "editar_etiquetas_foro" => ok::<EditarEtiquetasForoParams>(nombre, args),
        "etiquetar_post" => ok::<EtiquetarPostParams>(nombre, args),
        "crear_encuesta" => ok::<CrearEncuestaParams>(nombre, args),
        "cerrar_encuesta" => ok::<EncuestaMensajeParams>(nombre, args),
        "crear_evento" => ok::<CrearEventoParams>(nombre, args),
        "editar_evento" => ok::<EditarEventoParams>(nombre, args),
        "cancelar_evento" => ok::<CancelarEventoParams>(nombre, args),
        "editar_mensaje" => ok::<EditarMensajeParams>(nombre, args),
        "reaccionar" | "quitar_reaccion" => ok::<ReaccionParams>(nombre, args),
        "enviar_dm" => ok::<EnviarDmParams>(nombre, args),
        "avisar" => ok::<AvisarParams>(nombre, args),
        "borrar_mensaje" | "fijar_mensaje" | "desfijar_mensaje" => {
            ok::<MensajeParams>(nombre, args)
        }
        "borrar_mensajes" => ok::<BorrarMensajesParams>(nombre, args),
        "crear_canal" => {
            let p = args_de::<CrearCanalParams>(nombre, args.clone())?;
            for permiso in p.permisos.iter().flatten() {
                crate::discord::bits_desde_nombres(&permiso.permitir)?;
                crate::discord::bits_desde_nombres(&permiso.denegar)?;
            }
            Ok(())
        }
        "editar_canal" => ok::<EditarCanalParams>(nombre, args),
        "borrar_canal" => ok::<CanalParams>(nombre, args),
        "ordenar_canales" | "ordenar_roles" => ok::<OrdenarParams>(nombre, args),
        "crear_rol" => ok::<CrearRolParams>(nombre, args),
        "editar_rol" => ok::<EditarRolParams>(nombre, args),
        "borrar_rol" => ok::<RolParams>(nombre, args),
        "dar_rol" | "quitar_rol" => ok::<RolMiembroParams>(nombre, args),
        "permiso_canal" => ok::<PermisoCanalParams>(nombre, args),
        "quitar_permiso_canal" => ok::<QuitarPermisoCanalParams>(nombre, args),
        "expulsar" | "desbanear" => ok::<ExpulsarParams>(nombre, args),
        "banear" => ok::<BanearParams>(nombre, args),
        "silenciar" => ok::<SilenciarParams>(nombre, args),
        "cambiar_apodo" => ok::<CambiarApodoParams>(nombre, args),
        "editar_servidor" => ok::<EditarServidorParams>(nombre, args),
        "crear_invitacion" => ok::<CrearInvitacionParams>(nombre, args),
        "borrar_invitacion" => ok::<BorrarInvitacionParams>(nombre, args),
        "crear_webhook" => ok::<CrearWebhookParams>(nombre, args),
        "borrar_webhook" => ok::<BorrarWebhookParams>(nombre, args),
        _ => Err(format!("Unknown action: «{nombre}»")),
    }
}

/// Esquema JSON de los argumentos de una herramienta de escritura (el mismo tipo
/// que valida `validar_args`), o `None` si no es de escritura.
pub fn esquema_args(nombre: &str) -> Option<serde_json::Value> {
    fn de<T: schemars::JsonSchema>() -> Option<serde_json::Value> {
        serde_json::to_value(schemars::schema_for!(T)).ok()
    }
    match nombre {
        "guardar_plantilla_desde_plan" => de::<GuardarPlantillaPlanParams>(),
        "guardar_plantilla_desde_snapshot" => de::<GuardarPlantillaSnapshotParams>(),
        "crear_snapshot" => de::<ServidorParams>(),
        "aplicar_restauracion" => de::<ApplyRestoreParams>(),
        "elegir_servidor" => de::<ElegirServidorParams>(),
        "enviar_mensaje" => de::<EnviarMensajeParams>(),
        "crear_post_foro" => de::<CrearPostForoParams>(),
        "editar_etiquetas_foro" => de::<EditarEtiquetasForoParams>(),
        "etiquetar_post" => de::<EtiquetarPostParams>(),
        "crear_encuesta" => de::<CrearEncuestaParams>(),
        "cerrar_encuesta" => de::<EncuestaMensajeParams>(),
        "crear_evento" => de::<CrearEventoParams>(),
        "editar_evento" => de::<EditarEventoParams>(),
        "cancelar_evento" => de::<CancelarEventoParams>(),
        "editar_mensaje" => de::<EditarMensajeParams>(),
        "reaccionar" | "quitar_reaccion" => de::<ReaccionParams>(),
        "enviar_dm" => de::<EnviarDmParams>(),
        "avisar" => de::<AvisarParams>(),
        "borrar_mensaje" | "fijar_mensaje" | "desfijar_mensaje" => de::<MensajeParams>(),
        "borrar_mensajes" => de::<BorrarMensajesParams>(),
        "crear_canal" => de::<CrearCanalParams>(),
        "editar_canal" => de::<EditarCanalParams>(),
        "borrar_canal" => de::<CanalParams>(),
        "ordenar_canales" | "ordenar_roles" => de::<OrdenarParams>(),
        "crear_rol" => de::<CrearRolParams>(),
        "editar_rol" => de::<EditarRolParams>(),
        "borrar_rol" => de::<RolParams>(),
        "dar_rol" | "quitar_rol" => de::<RolMiembroParams>(),
        "permiso_canal" => de::<PermisoCanalParams>(),
        "quitar_permiso_canal" => de::<QuitarPermisoCanalParams>(),
        "expulsar" | "desbanear" => de::<ExpulsarParams>(),
        "banear" => de::<BanearParams>(),
        "silenciar" => de::<SilenciarParams>(),
        "cambiar_apodo" => de::<CambiarApodoParams>(),
        "editar_servidor" => de::<EditarServidorParams>(),
        "crear_invitacion" => de::<CrearInvitacionParams>(),
        "borrar_invitacion" => de::<BorrarInvitacionParams>(),
        "crear_webhook" => de::<CrearWebhookParams>(),
        "borrar_webhook" => de::<BorrarWebhookParams>(),
        _ => None,
    }
}

/// Una línea por herramienta de escritura con sus argumentos (nombre, tipo,
/// obligatorio u opcional y su descripción), sacada del esquema de sus params:
/// así el asistente no adivina argumentos (antes solo se documentaban cuatro).
/// `guild_id` se omite: siempre se usa el servidor activo.
pub fn firmas_escritura() -> String {
    firmas_de(HERRAMIENTAS_ESCRITURA)
}

/// Como `firmas_escritura` pero solo para las herramientas indicadas.
pub fn firmas_de(herramientas: &[&str]) -> String {
    fn tipo(v: &serde_json::Value) -> String {
        match v.get("type") {
            Some(serde_json::Value::String(t)) if t == "array" => {
                let item = v.get("items").map(tipo).unwrap_or_else(|| "any".into());
                format!("list of {item}")
            }
            Some(serde_json::Value::String(t)) => t.clone(),
            Some(serde_json::Value::Array(ts)) => ts
                .iter()
                .filter_map(|t| t.as_str())
                .filter(|t| *t != "null")
                .collect::<Vec<_>>()
                .join("|"),
            _ => "any".into(),
        }
    }
    let mut lineas = Vec::new();
    for h in HERRAMIENTAS_ESCRITURA {
        if *h == "elegir_servidor" || !herramientas.contains(h) {
            continue;
        }
        let Some(esquema) = esquema_args(h) else {
            continue;
        };
        let requeridos: Vec<&str> = esquema["required"]
            .as_array()
            .map(|r| r.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        let mut args = Vec::new();
        if let Some(props) = esquema["properties"].as_object() {
            for (campo, def) in props {
                if campo == "guild_id" {
                    continue;
                }
                let oblig = if requeridos.contains(&campo.as_str()) {
                    "required"
                } else {
                    "optional"
                };
                let mut texto = format!("{campo} ({}, {oblig})", tipo(def));
                if let Some(d) = def.get("description").and_then(|d| d.as_str()) {
                    let d: String = d.split_whitespace().collect::<Vec<_>>().join(" ");
                    texto.push_str(&format!(": {d}"));
                }
                args.push(texto);
            }
        }
        lineas.push(format!(
            "- `{h}`: {}",
            if args.is_empty() {
                "no arguments".into()
            } else {
                args.join("; ")
            }
        ));
    }
    lineas.join("\n")
}

impl ArgusMcp {
    /// Ejecuta una herramienta de escritura por nombre con la misma lógica que
    /// el MCP (permisos, jerarquía y mensajes). Rechaza nombres desconocidos,
    /// herramientas de lectura y argumentos inválidos.
    pub async fn ejecutar_accion(
        &self,
        nombre: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        let r = self.despachar_accion(nombre, args).await;
        self.salida(r)
    }

    /// Ejecuta una propuesta en orden seguro. Los nombres de roles, categorías y
    /// canales se resuelven a IDs con lo que ya existe o se ha creado antes. Una
    /// acción que falla no detiene las demás; una cuya referencia no se resuelve
    /// se marca como omitida.
    pub async fn ejecutar_propuesta(
        &self,
        propuesta: &Propuesta,
    ) -> Result<Vec<ResultadoAccion>, String> {
        let cancelar = AtomicBool::new(false);
        Ok(self
            .ejecutar_propuesta_con(propuesta, &cancelar, &mut |_, _| {})
            .await?
            .resultados)
    }

    /// Como `ejecutar_propuesta`, avisando de cada acción en cuanto termina
    /// (`avance(total, resultado)`) y parando antes de la siguiente si `cancelar`
    /// se activa.
    pub async fn ejecutar_propuesta_con(
        &self,
        propuesta: &Propuesta,
        cancelar: &AtomicBool,
        avance: &mut (dyn FnMut(usize, &ResultadoAccion) + Send),
    ) -> Result<ResumenPropuesta, String> {
        self.ejecutar_propuesta_reanudable(propuesta, cancelar, avance, &[])
            .await
    }

    /// Reanuda la misma propuesta conservando únicamente las acciones exitosas.
    pub async fn ejecutar_propuesta_reanudable(
        &self,
        propuesta: &Propuesta,
        cancelar: &AtomicBool,
        avance: &mut (dyn FnMut(usize, &ResultadoAccion) + Send),
        anteriores: &[ResultadoAccion],
    ) -> Result<ResumenPropuesta, String> {
        // El servidor de la propuesta (la app pone su `guild_id` en cada
        // acción); si no lo trae, el servidor activo del MCP.
        let de_la_propuesta = propuesta
            .acciones
            .iter()
            .find_map(|a| a.args.get("guild_id").and_then(serde_json::Value::as_str));
        self.preparar().await?;
        let mut config = { self.state.config.lock().unwrap().clone() };
        let gid = de_la_propuesta
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_owned)
            .or_else(|| config.active_guild_id.clone())
            .ok_or_else(|| SIN_SERVIDOR.to_string())?;
        if self.contexto_fijado && config.active_guild_id.as_deref() != Some(gid.as_str()) {
            return Err("The proposal does not match the selected server".to_string());
        }
        if propuesta.acciones.iter().any(|a| {
            a.args
                .get("guild_id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|g| g != gid)
        }) {
            return Err("All actions must target the same server".to_string());
        }
        if propuesta
            .acciones
            .iter()
            .any(|a| a.herramienta == "elegir_servidor")
        {
            return Err("A proposal cannot change its server".to_string());
        }
        config.active_guild_id = Some(gid.clone());
        let estado_fijado = AppState::new();
        *estado_fijado.config.lock().unwrap() = config;
        let bot = estado_fijado
            .get_active_bot()
            .ok_or_else(|| SIN_BOT.to_string())?;
        let id = crate::operaciones::iniciar(&self.pool, &bot.id, &gid, propuesta)
            .await
            .map_err(|e| e.to_string())?;
        let fijado = Self {
            pool: self.pool.clone(),
            state: Arc::new(estado_fijado),
            base_url: self.base_url.clone(),
            contexto_fijado: true,
            conectores: None,
        };
        let resultado = fijado
            .ejecutar_propuesta_registrada(propuesta, cancelar, avance, anteriores, &gid, &id)
            .await;
        let preparacion = resultado
            .as_ref()
            .err()
            .map(|e| crate::operaciones::motivo_seguro(e));
        let inciertas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_steps WHERE operation_id=? AND status = 'unverified' AND reason LIKE '%timed out%'")
            .bind(&id).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        let (estado, razon) = match &resultado {
            Err(_) => (crate::operaciones::FAILED, preparacion.as_deref().unwrap()),
            Ok(r) if inciertas > 0 && r.hechas == 0 => (
                crate::operaciones::UNVERIFIED,
                "The outcome of a timed-out creation is unknown",
            ),
            Ok(r) if r.hechas > 0 && (r.fallidas > 0 || r.omitidas > 0 || r.canceladas > 0) => (
                crate::operaciones::PARTIAL,
                "Some actions were accepted; others were not completed",
            ),
            Ok(r) if r.fallidas > 0 => (
                crate::operaciones::FAILED,
                "No actions were accepted; at least one failed",
            ),
            Ok(r) if r.canceladas > 0 => (
                "cancelled",
                "Operation stopped before all steps were attempted",
            ),
            Ok(r) if r.omitidas > 0 => ("skipped", "No actions could be attempted"),
            Ok(_) => {
                let pendientes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_steps WHERE operation_id=? AND status != 'correct'")
                    .bind(&id).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
                if pendientes == 0 {
                    (crate::operaciones::CORRECT, "All results verified")
                } else {
                    (
                        crate::operaciones::UNVERIFIED,
                        "Some results have not been verified",
                    )
                }
            }
        };
        crate::operaciones::terminar(&self.pool, &id, estado, razon)
            .await
            .map_err(|e| e.to_string())?;
        resultado.map(|mut resumen| {
            resumen.operation_id = Some(id);
            resumen.operation_status = Some(estado.to_string());
            resumen
        })
    }

    async fn ejecutar_propuesta_registrada(
        &self,
        propuesta: &Propuesta,
        cancelar: &AtomicBool,
        avance: &mut (dyn FnMut(usize, &ResultadoAccion) + Send),
        anteriores: &[ResultadoAccion],
        gid: &str,
        id: &str,
    ) -> Result<ResumenPropuesta, String> {
        let roles = commands::leer_roles(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let canales = commands::leer_canales(&self.state, &gid, self.url())
            .await
            .map_err(|e| self.limpiar(e))?;
        let mut nombres = Nombres::desde(&roles, &canales);
        nombres.olvidar_nombres_creados(propuesta);
        if propuesta.acciones.iter().any(|a| {
            a.herramienta == "crear_canal"
                && a.args["permisos"]
                    .as_array()
                    .is_some_and(|p| p.iter().any(|p| p["tipo"] == "miembro"))
        }) {
            let miembros = commands::leer_miembros(&self.state, &gid, self.url()).await?;
            nombres.registrar_miembros(&miembros);
        }
        let total = propuesta.acciones.len();
        let mut resultados = Vec::new();
        // `indice` es la posición original de la acción: la app empareja así
        // cada resultado con su fila aunque el orden de ejecución sea otro.
        for indice in orden_de_ejecucion(propuesta) {
            let a = &propuesta.acciones[indice];
            if cancelar.load(Ordering::SeqCst) {
                break;
            }
            crate::operaciones::paso(&self.pool, id, indice, "in_progress", None)
                .await
                .map_err(|e| e.to_string())?;
            let mut argumentos_resueltos = None;
            let previo = anteriores.iter().find(|r| {
                r.indice == indice && r.herramienta == a.herramienta && r.estado == EstadoAccion::Ok
            });
            let (estado, mensaje) = match nombres.resolver_args(&a.herramienta, &a.args) {
                Err(e) => (EstadoAccion::Omitida, e),
                Ok(args) => {
                    argumentos_resueltos = Some(args.clone());
                    match if let Some(previo) = previo {
                        Ok(previo.mensaje.clone())
                    } else if a.herramienta == "aplicar_restauracion" {
                        match args_de::<ApplyRestoreParams>(&a.herramienta, args.clone()) {
                            Ok(p) => self
                                .aplicar_restauracion_con(&p, cancelar)
                                .await
                                .and_then(|s| serde_json::to_string(&s).map_err(|e| e.to_string())),
                            Err(e) => Err(e),
                        }
                    } else {
                        self.ejecutar_accion(&a.herramienta, args.clone()).await
                    } {
                        Ok(salida) => {
                            nombres.registrar_creado(&a.herramienta, &args, &salida);
                            (EstadoAccion::Ok, salida)
                        }
                        Err(e) => (EstadoAccion::Error, e),
                    }
                }
            };
            let r = ResultadoAccion {
                indice,
                herramienta: a.herramienta.clone(),
                descripcion: a.descripcion.clone(),
                estado,
                mensaje,
            };
            let motivo = crate::operaciones::motivo_seguro(&r.mensaje);
            let (status, reason) = match r.estado {
                EstadoAccion::Ok => (
                    crate::operaciones::UNVERIFIED,
                    "Discord accepted the action; the result has not been verified",
                ),
                EstadoAccion::Error
                    if a.herramienta == "crear_canal"
                        && r.mensaje.contains("create request timed out") =>
                {
                    (crate::operaciones::UNVERIFIED, motivo.as_str())
                }
                EstadoAccion::Error => (crate::operaciones::FAILED, motivo.as_str()),
                EstadoAccion::Omitida => ("skipped", "A required reference could not be resolved"),
            };
            crate::operaciones::paso(&self.pool, id, indice, status, Some(reason))
                .await
                .map_err(|e| e.to_string())?;
            if r.estado == EstadoAccion::Ok
                && matches!(a.herramienta.as_str(), "crear_canal" | "editar_canal")
            {
                crate::operaciones::iniciar_verificacion(&self.pool, id, indice)
                    .await
                    .map_err(|e| e.to_string())?;
                let verification = async {
                    let output: serde_json::Value = serde_json::from_str(&r.mensaje)
                        .map_err(|_| "Discord returned an unreadable channel".to_string())?;
                    let expected = &output["canal"];
                    let channel_id = expected["id"]
                        .as_str()
                        .ok_or("Discord did not return a channel ID")?;
                    let token = commands::token_del_bot_activo(&self.state)?;
                    let actual =
                        crate::discord::read_channel(&token, self.url(), channel_id).await?;
                    let actual = canal_json(&actual);
                    if let Some(args) = &argumentos_resueltos {
                        for (argument, field) in [
                            ("nombre", "nombre"),
                            ("categoria_id", "categoria_id"),
                            ("tema", "tema"),
                        ] {
                            if let Some(value) = args.get(argument).filter(|v| !v.is_null()) {
                                let desired =
                                    if argument == "categoria_id" && value.as_str() == Some("") {
                                        &serde_json::Value::Null
                                    } else {
                                        value
                                    };
                                if &actual[field] != desired {
                                    return Err(format!(
                                        "Discord does not reflect the requested field: {field}"
                                    ));
                                }
                            }
                        }
                    }
                    if a.herramienta == "crear_canal" {
                        // The created channel must be of the requested type, not
                        // only match what Discord said it accepted.
                        let pedido = argumentos_resueltos
                            .as_ref()
                            .and_then(|args| args.get("tipo"))
                            .and_then(|v| v.as_str());
                        let codigo = codigo_tipo_canal(pedido)?;
                        if actual["tipo"] != tipo_de_canal(codigo) {
                            return Err(
                                "Discord does not reflect the requested field: tipo".to_string()
                            );
                        }
                    }
                    for field in ["id", "nombre", "tipo", "categoria_id", "tema", "posicion"] {
                        if actual[field] != expected[field] {
                            return Err(format!(
                                "Discord does not reflect the channel field: {field}"
                            ));
                        }
                    }
                    Ok::<(), String>(())
                }
                .await;
                match verification {
                    Ok(()) => crate::operaciones::confirmar_verificacion(&self.pool, id, indice)
                        .await
                        .map_err(|e| e.to_string())?,
                    Err(e) => crate::operaciones::paso(
                        &self.pool,
                        id,
                        indice,
                        crate::operaciones::UNVERIFIED,
                        Some(&self.limpiar(e)),
                    )
                    .await
                    .map_err(|e| e.to_string())?,
                }
            }
            if r.estado == EstadoAccion::Ok
                && matches!(
                    a.herramienta.as_str(),
                    "crear_rol"
                        | "editar_rol"
                        | "ordenar_roles"
                        | "permiso_canal"
                        | "quitar_permiso_canal"
                )
            {
                crate::operaciones::iniciar_verificacion(&self.pool, id, indice)
                    .await
                    .map_err(|e| e.to_string())?;
                let verification = async {
                    let args = argumentos_resueltos
                        .as_ref()
                        .ok_or("Missing action arguments")?;
                    match a.herramienta.as_str() {
                        "crear_rol" | "editar_rol" => {
                            let output: serde_json::Value = serde_json::from_str(&r.mensaje)
                                .map_err(|_| "Discord returned an unreadable role")?;
                            let expected = &output["rol"];
                            let role_id = expected["id"]
                                .as_str()
                                .ok_or("Discord did not return a role ID")?;
                            let roles = commands::leer_roles(&self.state, gid, self.url())
                                .await
                                .map_err(|e| self.limpiar(e))?;
                            let role = roles
                                .iter()
                                .find(|role| role.id == role_id)
                                .ok_or("Discord did not return the role")?;
                            let actual = rol_json(role);
                            for field in ["nombre", "color", "permisos_bits", "posicion"] {
                                if actual[field] != expected[field] {
                                    return Err(format!(
                                        "Discord does not reflect the role field: {field}"
                                    ));
                                }
                            }
                            if let Some(name) = args.get("nombre").and_then(|v| v.as_str()) {
                                if role.name != name {
                                    return Err(
                                        "Discord does not reflect the requested role name".into()
                                    );
                                }
                            }
                            if let Some(value) = args.get("color").filter(|v| !v.is_null()) {
                                let color: ColorParam = serde_json::from_value(value.clone())
                                    .map_err(|e| e.to_string())?;
                                if role.color != color.a_u32()? {
                                    return Err(
                                        "Discord does not reflect the requested role color".into(),
                                    );
                                }
                            }
                            if let Some(value) = args.get("permisos").filter(|v| !v.is_null()) {
                                let permissions: PermisosParam =
                                    serde_json::from_value(value.clone())
                                        .map_err(|e| e.to_string())?;
                                if role.permissions != permissions.a_bits()? {
                                    return Err(
                                        "Discord does not reflect the requested role permissions"
                                            .into(),
                                    );
                                }
                            }
                        }
                        "ordenar_roles" => {
                            let p: OrdenarParams = args_de(&a.herramienta, args.clone())?;
                            let roles = commands::leer_roles(&self.state, gid, self.url())
                                .await
                                .map_err(|e| self.limpiar(e))?;
                            for position in p.posiciones {
                                if !roles.iter().any(|role| {
                                    role.id == position.id
                                        && role.position >= 0
                                        && role.position as u64 == position.posicion
                                }) {
                                    return Err(
                                        "Discord does not reflect the requested role position"
                                            .into(),
                                    );
                                }
                            }
                        }
                        _ => {
                            let channel_id =
                                args["channel_id"].as_str().ok_or("Missing channel ID")?;
                            let target_id =
                                args["target_id"].as_str().ok_or("Missing target ID")?;
                            let kind = tipo_de_overwrite(
                                args["tipo"].as_str().ok_or("Missing overwrite type")?,
                            )?;
                            let token = commands::token_del_bot_activo(&self.state)?;
                            let channel =
                                crate::discord::read_channel(&token, self.url(), channel_id)
                                    .await?;
                            let overwrites = channel
                                .overwrites
                                .as_ref()
                                .ok_or("Discord did not return channel overwrites")?;
                            let actual = overwrites
                                .iter()
                                .find(|o| o.id == target_id && o.type_ == kind);
                            if a.herramienta == "quitar_permiso_canal" {
                                if actual.is_some() {
                                    return Err(
                                        "Discord still reports the channel overwrite".into()
                                    );
                                }
                            } else {
                                let p: PermisoCanalParams = args_de(&a.herramienta, args.clone())?;
                                let allow =
                                    bits_opcionales(&p.permitir)?.unwrap_or_else(|| "0".into());
                                let deny =
                                    bits_opcionales(&p.denegar)?.unwrap_or_else(|| "0".into());
                                if !actual.is_some_and(|o| o.allow == allow && o.deny == deny) {
                                    return Err(
                                        "Discord does not reflect the requested channel overwrite"
                                            .into(),
                                    );
                                }
                            }
                        }
                    }
                    Ok::<(), String>(())
                }
                .await;
                match verification {
                    Ok(()) => crate::operaciones::confirmar_verificacion(&self.pool, id, indice)
                        .await
                        .map_err(|e| e.to_string())?,
                    Err(e) => crate::operaciones::paso(
                        &self.pool,
                        id,
                        indice,
                        "unverified",
                        Some(&self.limpiar(e)),
                    )
                    .await
                    .map_err(|e| e.to_string())?,
                }
            }
            if previo.is_none() {
                avance(total, &r);
            }
            resultados.push(r);
        }
        Ok(ResumenPropuesta::de(total, resultados))
    }

    async fn despachar_accion(
        &self,
        nombre: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        comprobar_escritura(nombre)?;
        match nombre {
            "guardar_plantilla_desde_plan" => {
                self.guardar_plantilla_plan_json(args_de(nombre, args)?)
                    .await
            }
            "guardar_plantilla_desde_snapshot" => {
                self.guardar_plantilla_snapshot_json(args_de(nombre, args)?)
                    .await
            }
            "crear_snapshot" => self.crear_snapshot_json().await,
            "aplicar_restauracion" => {
                let p: ApplyRestoreParams = args_de(nombre, args)?;
                let result = self
                    .aplicar_restauracion_con(&p, &AtomicBool::new(false))
                    .await?;
                serde_json::to_string(&result).map_err(|e| e.to_string())
            }
            "elegir_servidor" => {
                let p: ElegirServidorParams = args_de(nombre, args)?;
                self.elegir_servidor_json(&p.guild_id).await
            }
            "editar_mensaje" => {
                let p: EditarMensajeParams = args_de(nombre, args)?;
                self.editar_mensaje_embed_json(
                    &p.channel_id,
                    &p.message_id,
                    p.contenido.as_deref(),
                    p.embed.as_ref(),
                )
                .await
            }
            "reaccionar" | "quitar_reaccion" => {
                let p: ReaccionParams = args_de(nombre, args)?;
                self.reaccion_json(&p, nombre == "reaccionar").await
            }
            "crear_post_foro" => {
                let p: CrearPostForoParams = args_de(nombre, args)?;
                self.crear_post_foro_json(
                    &p.channel_id,
                    &p.titulo,
                    &p.contenido,
                    p.etiquetas.as_deref().unwrap_or(&[]),
                )
                .await
            }
            "editar_etiquetas_foro" => {
                self.editar_etiquetas_foro_json(&args_de(nombre, args)?)
                    .await
            }
            "etiquetar_post" => self.etiquetar_post_json(&args_de(nombre, args)?).await,
            "crear_evento" => self.crear_evento_json(&args_de(nombre, args)?).await,
            "crear_encuesta" => self.crear_encuesta_json(&args_de(nombre, args)?).await,
            "cerrar_encuesta" => self.cerrar_encuesta_json(&args_de(nombre, args)?).await,
            "editar_evento" => self.editar_evento_json(&args_de(nombre, args)?).await,
            "cancelar_evento" => {
                let p: CancelarEventoParams = args_de(nombre, args)?;
                self.cancelar_evento_json(p.guild_id.as_deref(), &p.evento_id)
                    .await
            }
            "enviar_mensaje" => {
                let p: EnviarMensajeParams = args_de(nombre, args)?;
                self.enviar_mensaje_embed_json(
                    &p.channel_id,
                    p.contenido.as_deref(),
                    p.embed.as_ref(),
                    p.responder_a.as_deref(),
                )
                .await
            }
            "enviar_dm" => {
                let p: EnviarDmParams = args_de(nombre, args)?;
                self.enviar_dm_json(&p.user_id, &p.contenido).await
            }
            "avisar" => self.avisar_json(&args_de(nombre, args)?).await,
            "borrar_mensaje" => {
                let p: MensajeParams = args_de(nombre, args)?;
                self.borrar_mensaje_json(p.guild_id.as_deref(), &p.channel_id, &p.message_id)
                    .await
            }
            "borrar_mensajes" => {
                let p: BorrarMensajesParams = args_de(nombre, args)?;
                self.borrar_mensajes_json(p.guild_id.as_deref(), &p.channel_id, &p.message_ids)
                    .await
            }
            "fijar_mensaje" | "desfijar_mensaje" => {
                let p: MensajeParams = args_de(nombre, args)?;
                self.fijar_mensaje_json(
                    p.guild_id.as_deref(),
                    &p.channel_id,
                    &p.message_id,
                    nombre == "fijar_mensaje",
                )
                .await
            }
            "crear_canal" => self.crear_canal_json(&args_de(nombre, args)?).await,
            "editar_canal" => self.editar_canal_json(&args_de(nombre, args)?).await,
            "borrar_canal" => {
                let p: CanalParams = args_de(nombre, args)?;
                self.borrar_canal_json(p.guild_id.as_deref(), &p.channel_id)
                    .await
            }
            "ordenar_canales" => {
                let p: OrdenarParams = args_de(nombre, args)?;
                self.ordenar_canales_json(p.guild_id.as_deref(), &p.posiciones)
                    .await
            }
            "crear_rol" => self.crear_rol_json(&args_de(nombre, args)?).await,
            "editar_rol" => self.editar_rol_json(&args_de(nombre, args)?).await,
            "borrar_rol" => {
                let p: RolParams = args_de(nombre, args)?;
                self.borrar_rol_json(p.guild_id.as_deref(), &p.role_id)
                    .await
            }
            "ordenar_roles" => {
                let p: OrdenarParams = args_de(nombre, args)?;
                self.ordenar_roles_json(p.guild_id.as_deref(), &p.posiciones)
                    .await
            }
            "dar_rol" | "quitar_rol" => {
                let p: RolMiembroParams = args_de(nombre, args)?;
                self.cambiar_rol_json(
                    p.guild_id.as_deref(),
                    &p.user_id,
                    &p.role_id,
                    nombre == "dar_rol",
                )
                .await
            }
            "permiso_canal" => self.permiso_canal_json(&args_de(nombre, args)?).await,
            "quitar_permiso_canal" => {
                let p: QuitarPermisoCanalParams = args_de(nombre, args)?;
                self.quitar_permiso_canal_json(&p).await
            }
            "expulsar" => self.expulsar_json(&args_de(nombre, args)?).await,
            "banear" => self.banear_json(&args_de(nombre, args)?).await,
            "desbanear" => {
                let p: ExpulsarParams = args_de(nombre, args)?;
                self.desbanear_json(p.guild_id.as_deref(), &p.user_id, p.motivo.as_deref())
                    .await
            }
            "silenciar" => self.silenciar_json(&args_de(nombre, args)?).await,
            "cambiar_apodo" => self.cambiar_apodo_json(&args_de(nombre, args)?).await,
            "editar_servidor" => self.editar_servidor_json(&args_de(nombre, args)?).await,
            "crear_invitacion" => self.crear_invitacion_json(&args_de(nombre, args)?).await,
            "borrar_invitacion" => {
                let p: BorrarInvitacionParams = args_de(nombre, args)?;
                self.borrar_invitacion_json(p.guild_id.as_deref(), &p.codigo)
                    .await
            }
            "crear_webhook" => self.crear_webhook_json(&args_de(nombre, args)?).await,
            "borrar_webhook" => {
                let p: BorrarWebhookParams = args_de(nombre, args)?;
                self.borrar_webhook_json(p.guild_id.as_deref(), &p.webhook_id)
                    .await
            }
            _ => Err(format!("Unknown action: «{nombre}»")),
        }
    }
}

#[tool_router]
impl ArgusMcp {
    #[tool(
        name = "listar_plantillas",
        description = "Lists the user's local template library, without modifying it."
    )]
    async fn listar_plantillas(&self) -> Result<String, String> {
        let items = crate::db::list_templates(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(json!(items.into_iter().map(|t| json!({"id": t.id, "name": t.name, "description": t.description, "tags": t.tags})).collect::<Vec<_>>()).to_string())
    }

    #[tool(
        name = "consultar_plantilla",
        description = "Reads one template by its library ID. Never applies or modifies it."
    )]
    async fn consultar_plantilla(
        &self,
        Parameters(p): Parameters<PlantillaParams>,
    ) -> Result<String, String> {
        let item = crate::db::get_template(&self.pool, &p.id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Template not found")?;
        serde_json::to_string(&item).map_err(|e| e.to_string())
    }

    #[tool(
        name = "proponer_plantilla",
        description = "Prepares, without applying it, the proposal that would create a saved template (see listar_plantillas) on the active server, with its validation. Nothing is changed: return the proposal in Execute mode so the user can approve it."
    )]
    async fn proponer_plantilla(
        &self,
        Parameters(p): Parameters<PlantillaParams>,
    ) -> Result<String, String> {
        self.salida(self.proponer_plantilla_json(&p.id).await)
    }

    #[tool(
        name = "guardar_plantilla_desde_plan",
        description = "Saves a Markdown server plan as a template in the local library. It is a write: it only runs from an approved proposal, never from a direct call. Return a guardar_plantilla_desde_plan action and wait for the user to approve it."
    )]
    async fn guardar_plantilla_desde_plan(
        &self,
        Parameters(p): Parameters<GuardarPlantillaPlanParams>,
    ) -> Result<String, String> {
        let _ = p;
        self.salida(Err(
            "Saving a template requires approval in ARGUS".to_string()
        ))
    }

    #[tool(
        name = "guardar_plantilla_desde_snapshot",
        description = "Saves a snapshot of the active server (see listar_snapshots) as a template in the local library, without tokens or source IDs. It is a write: it only runs from an approved proposal, never from a direct call. Return a guardar_plantilla_desde_snapshot action and wait for the user to approve it."
    )]
    async fn guardar_plantilla_desde_snapshot(
        &self,
        Parameters(p): Parameters<GuardarPlantillaSnapshotParams>,
    ) -> Result<String, String> {
        let _ = p;
        self.salida(Err(
            "Saving a template requires approval in ARGUS".to_string()
        ))
    }

    #[tool(
        name = "proponer_correccion",
        description = "Prepares, without applying it, the correction proposal for one finding of informe_permisos (pass the finding unchanged). It refuses when the audit is incomplete, the finding is gone or the bot could not apply it. Nothing is changed: return the proposal in Execute mode so the user can approve it."
    )]
    async fn proponer_correccion(
        &self,
        Parameters(p): Parameters<ProponerCorreccionParams>,
    ) -> Result<String, String> {
        let r = self
            .proponer_correccion_json(p.guild_id.as_deref(), &p.finding)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "listar_snapshots",
        description = "Lists local snapshots for the active bot and server. Does not modify Discord."
    )]
    async fn listar_snapshots(&self) -> Result<String, String> {
        self.salida(self.listar_snapshots_json().await)
    }

    #[tool(
        name = "crear_snapshot",
        description = "Captures a local snapshot of the active server using only Discord reads. Never changes the server or applies a restore."
    )]
    async fn crear_snapshot(&self) -> Result<String, String> {
        self.salida(self.crear_snapshot_json().await)
    }

    #[tool(
        name = "comparar_snapshot",
        description = "Compares a saved snapshot with the active server without applying changes."
    )]
    async fn comparar_snapshot(
        &self,
        Parameters(p): Parameters<SnapshotParams>,
    ) -> Result<String, String> {
        self.salida(self.comparar_snapshot_json(&p.snapshot_id).await)
    }

    #[tool(
        name = "info_restauracion",
        description = "Lists snapshots or generates a restore proposal without applying it. Reports conflicts, unrecoverable items, exact steps and duration. Review all steps before applying."
    )]
    async fn info_restauracion(
        &self,
        Parameters(p): Parameters<RestoreParams>,
    ) -> Result<String, String> {
        let r = self.info_restauracion_json(&p).await;
        self.salida(r)
    }
    #[tool(
        name = "aplicar_restauracion",
        description = "Restore execution is available only through ARGUS proposal approval, never through a direct MCP call. Return an aplicar_restauracion action in the proposal and wait for user confirmation."
    )]
    async fn aplicar_restauracion(
        &self,
        Parameters(p): Parameters<ApplyRestoreParams>,
    ) -> Result<String, String> {
        let _ = p;
        self.salida(Err("Restore requires confirmation in ARGUS".to_string()))
    }

    #[tool(
        name = "estado",
        description = "Shows the active ARGUS bot (with its official Discord name) and the active server, which the other tools use by default."
    )]
    async fn estado(&self) -> Result<String, String> {
        let r = self.estado_json().await;
        self.salida(r)
    }

    #[tool(
        name = "listar_servidores",
        description = "Lists the Discord servers the active bot is in (id and name) and marks which one is the active server."
    )]
    async fn listar_servidores(&self) -> Result<String, String> {
        let r = self.listar_servidores_json().await;
        self.salida(r)
    }

    #[tool(
        name = "elegir_servidor",
        description = "Changes the active server (saved in the ARGUS database, same as choosing it in the app). It must be one of the active bot's servers."
    )]
    async fn elegir_servidor(
        &self,
        Parameters(p): Parameters<ElegirServidorParams>,
    ) -> Result<String, String> {
        let r = self.elegir_servidor_json(&p.guild_id).await;
        self.salida(r)
    }

    #[tool(
        name = "mapa_servidor",
        description = "Server map: categories with their channels, channels without a category and roles (with member counts). Read-only. `guild_id` is optional (defaults to the active server)."
    )]
    async fn mapa_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.mapa_servidor_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_canales",
        description = "Lists the server's channels and categories (id, name, type, category, position and specific permissions). Read-only. `guild_id` is optional."
    )]
    async fn listar_canales(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_canales_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_roles",
        description = "Lists the server's roles, highest to lowest, with color and readable permissions. Read-only. `guild_id` is optional."
    )]
    async fn listar_roles(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_roles_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_miembros",
        description = "Lists server members with their roles. Supports `buscar` (user, nickname or ID) and `limite` (default 25, maximum 100). Read-only. `guild_id` is optional."
    )]
    async fn listar_miembros(
        &self,
        Parameters(p): Parameters<ListarMiembrosParams>,
    ) -> Result<String, String> {
        let r = self
            .listar_miembros_json(p.guild_id.as_deref(), p.buscar.as_deref(), p.limite)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "info_miembro",
        description = "Details of one member (user, nickname, roles with names, join date). Read-only. `guild_id` is optional."
    )]
    async fn info_miembro(
        &self,
        Parameters(p): Parameters<InfoMiembroParams>,
    ) -> Result<String, String> {
        let r = self
            .info_miembro_json(p.guild_id.as_deref(), &p.user_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "permisos_bot",
        description = "Effective permissions of the active bot on the server, with their official names. Read-only. `guild_id` is optional."
    )]
    async fn permisos_bot(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.permisos_bot_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "info_servidor",
        description = "Name, icon and approximate member count of the server. Read-only. `guild_id` is optional."
    )]
    async fn info_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.info_servidor_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "ajustes_servidor",
        description = "General server settings (description, verification, content filter, notifications, system and AFK channels). Requires «Manage Server». Read-only. `guild_id` is optional."
    )]
    async fn ajustes_servidor(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.ajustes_servidor_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "informe_permisos",
        description = "Read a partial structured permissions report, evidence and limitations. Never concludes the server is safe. Corrections require a proposal and user Apply."
    )]
    async fn informe_permisos(
        &self,
        Parameters(p): Parameters<InformePermisosParams>,
    ) -> Result<String, String> {
        let r = self
            .informe_permisos_con_expectativas_json(
                p.guild_id.as_deref(),
                p.expectations.as_deref().unwrap_or(&[]),
            )
            .await;
        self.salida(r)
    }

    #[tool(
        name = "registro_auditoria",
        description = "Latest 50 audit log entries as readable text. Requires «View Audit Log». Read-only. `guild_id` is optional."
    )]
    async fn registro_auditoria(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.registro_auditoria_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "leer_mensajes",
        description = "Reads the latest messages of a channel (`limite` from 1 to 500, default 50, newest first). Requires «Read Message History». Read-only. `guild_id` is optional."
    )]
    async fn leer_mensajes(
        &self,
        Parameters(p): Parameters<LeerMensajesParams>,
    ) -> Result<String, String> {
        let r = self
            .leer_mensajes_json(p.guild_id.as_deref(), &p.channel_id, p.limite)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "buscar_mensajes",
        description = "Searches messages by text (ignoring case and accents). Bots cannot use Discord search, so it reads the recent history of each channel (`limite_por_canal`, default 100, max 5000 = the whole history) and filters locally. `canales` (IDs or names) defaults to all text channels; `autor` (ID or name) is optional. Returns matches (channel, author, message id, link, date, snippet) and how many messages were checked per channel. Requires «Read Message History». Read-only. `guild_id` is optional."
    )]
    async fn buscar_mensajes(
        &self,
        Parameters(p): Parameters<BuscarMensajesParams>,
    ) -> Result<String, String> {
        let r = self
            .buscar_mensajes_json(
                p.guild_id.as_deref(),
                &p.texto,
                p.canales.as_deref(),
                p.autor.as_deref(),
                p.limite_por_canal,
            )
            .await;
        self.salida(r)
    }

    #[tool(
        name = "listar_baneos",
        description = "Lists banned users with their reason. Requires «Ban Members». Read-only. `guild_id` is optional."
    )]
    async fn listar_baneos(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_baneos_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_invitaciones",
        description = "Lists the server's invites (code, link, channel, uses). Requires «Manage Server». Read-only. `guild_id` is optional."
    )]
    async fn listar_invitaciones(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_invitaciones_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_webhooks",
        description = "Lists the server's webhooks (id, name, channel) without token or URL: the full URL is only given when creating one. Requires «Manage Webhooks». Read-only. `guild_id` is optional."
    )]
    async fn listar_webhooks(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_webhooks_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "enviar_mensaje",
        description = "Sends a text message (up to 2000 characters) and/or an optional embed (titulo, descripcion, color hex, pie, campos nombre/valor/en_linea, imagen_url) to a channel as the active bot; at least one of contenido or embed is required; with responder_a it replies to that message. Requires permission to send messages in that channel."
    )]
    async fn enviar_mensaje(
        &self,
        Parameters(p): Parameters<EnviarMensajeParams>,
    ) -> Result<String, String> {
        let r = self
            .enviar_mensaje_embed_json(
                &p.channel_id,
                p.contenido.as_deref(),
                p.embed.as_ref(),
                p.responder_a.as_deref(),
            )
            .await;
        self.salida(r)
    }

    #[tool(
        name = "editar_mensaje",
        description = "Edits the text (up to 2000 characters) and/or the embed of a message previously sent by the active bot; at least one of contenido or embed is required; Discord does not allow editing other users' messages."
    )]
    async fn editar_mensaje(
        &self,
        Parameters(p): Parameters<EditarMensajeParams>,
    ) -> Result<String, String> {
        let r = self
            .editar_mensaje_embed_json(
                &p.channel_id,
                &p.message_id,
                p.contenido.as_deref(),
                p.embed.as_ref(),
            )
            .await;
        self.salida(r)
    }

    #[tool(
        name = "crear_post_foro",
        description = "Creates a post (thread) in a forum channel as the active bot, with a title (up to 100 characters), a first message (up to 2000 characters) and optional `etiquetas` (up to 5 existing forum tags, by name or id). To reply inside a post, use enviar_mensaje with the post's ID as channel_id. Requires permission to create posts in that forum."
    )]
    async fn crear_post_foro(
        &self,
        Parameters(p): Parameters<CrearPostForoParams>,
    ) -> Result<String, String> {
        let r = self
            .crear_post_foro_json(
                &p.channel_id,
                &p.titulo,
                &p.contenido,
                p.etiquetas.as_deref().unwrap_or(&[]),
            )
            .await;
        self.salida(r)
    }

    #[tool(
        name = "etiquetas_foro",
        description = "Lists the tags of a forum channel (id, name, emoji, moderated). Read-only."
    )]
    async fn etiquetas_foro(
        &self,
        Parameters(p): Parameters<EtiquetasForoParams>,
    ) -> Result<String, String> {
        let r = self.etiquetas_foro_json(&p.channel_id).await;
        self.salida(r)
    }

    #[tool(
        name = "editar_etiquetas_foro",
        description = "Creates or renames tags of a forum channel. Each item has `nombre` (existing tag name/id to modify, or a new name to create; up to 20 characters), optional `nuevo_nombre` and optional `emoji` (Unicode or name:id). A forum has at most 20 tags. Requires «Manage Channels»."
    )]
    async fn editar_etiquetas_foro(
        &self,
        Parameters(p): Parameters<EditarEtiquetasForoParams>,
    ) -> Result<String, String> {
        let r = self.editar_etiquetas_foro_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "etiquetar_post",
        description = "Sets the tags of a forum post (replacing the current ones): up to 5 existing forum tags, by name or id. `post_id` is the post's thread ID."
    )]
    async fn etiquetar_post(
        &self,
        Parameters(p): Parameters<EtiquetarPostParams>,
    ) -> Result<String, String> {
        let r = self.etiquetar_post_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_eventos",
        description = "Lists the server's scheduled events (id, name, description, start, end, type voice/stage/external, status, channel, location). Read-only. `guild_id` is optional."
    )]
    async fn listar_eventos(
        &self,
        Parameters(p): Parameters<ServidorParams>,
    ) -> Result<String, String> {
        let r = self.listar_eventos_json(p.guild_id.as_deref()).await;
        self.salida(r)
    }

    #[tool(
        name = "crear_evento",
        description = "Creates a scheduled event (server members only). `inicio` and `fin` are ISO 8601 with a time zone (2030-05-01T20:00:00+02:00); the start must be in the future and the end after the start. Give either `lugar` (external location, needs `fin`) or `canal_id` (voice or stage channel). Requires «Create Events» or «Manage Events». `guild_id` is optional."
    )]
    async fn crear_evento(
        &self,
        Parameters(p): Parameters<CrearEventoParams>,
    ) -> Result<String, String> {
        let r = self.crear_evento_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "editar_evento",
        description = "Edits a scheduled event; only the given fields change. Same date rules as crear_evento (future start, end after start). `lugar` makes it external, `canal_id` moves it to a voice/stage channel. Requires «Create Events» or «Manage Events». `guild_id` is optional."
    )]
    async fn editar_evento(
        &self,
        Parameters(p): Parameters<EditarEventoParams>,
    ) -> Result<String, String> {
        let r = self.editar_evento_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "cancelar_evento",
        description = "Cancels (deletes) a scheduled event by its ID (irreversible). Requires «Create Events» or «Manage Events». `guild_id` is optional."
    )]
    async fn cancelar_evento(
        &self,
        Parameters(p): Parameters<CancelarEventoParams>,
    ) -> Result<String, String> {
        let r = self
            .cancelar_evento_json(p.guild_id.as_deref(), &p.evento_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "crear_encuesta",
        description = "Posts a native poll in a channel. `pregunta` ≤ 300 characters, 2–10 `respuestas` (`texto` ≤ 55, optional `emoji`), `duracion_horas` 1–768 (default 24), `voto_multiple` (default false). Requires «Create Polls». `guild_id` is optional."
    )]
    async fn crear_encuesta(
        &self,
        Parameters(p): Parameters<CrearEncuestaParams>,
    ) -> Result<String, String> {
        let r = self.crear_encuesta_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "resultados_encuesta",
        description = "Read-only: returns the vote count of each answer of a poll message (also total votes and whether it has ended)."
    )]
    async fn resultados_encuesta(
        &self,
        Parameters(p): Parameters<EncuestaMensajeParams>,
    ) -> Result<String, String> {
        let r = self.resultados_encuesta_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "cerrar_encuesta",
        description = "Ends a poll posted by the bot right away (Discord only lets the poll's author end it)."
    )]
    async fn cerrar_encuesta(
        &self,
        Parameters(p): Parameters<EncuestaMensajeParams>,
    ) -> Result<String, String> {
        let r = self.cerrar_encuesta_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "reaccionar",
        description = "Adds a reaction from the active bot to a message. emoji is a Unicode emoji (👍) or a custom one as name:id. Requires the Add Reactions permission."
    )]
    async fn reaccionar(
        &self,
        Parameters(p): Parameters<ReaccionParams>,
    ) -> Result<String, String> {
        let r = self.reaccion_json(&p, true).await;
        self.salida(r)
    }

    #[tool(
        name = "quitar_reaccion",
        description = "Removes the active bot's own reaction from a message (it never removes other users' reactions). emoji is a Unicode emoji or a custom one as name:id."
    )]
    async fn quitar_reaccion(
        &self,
        Parameters(p): Parameters<ReaccionParams>,
    ) -> Result<String, String> {
        let r = self.reaccion_json(&p, false).await;
        self.salida(r)
    }

    #[tool(
        name = "enviar_dm",
        description = "Sends a direct message (up to 2000 characters) to a user as the active bot. The user must allow direct messages."
    )]
    async fn enviar_dm(&self, Parameters(p): Parameters<EnviarDmParams>) -> Result<String, String> {
        let r = self.enviar_dm_json(&p.user_id, &p.contenido).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_mensaje",
        description = "Deletes a message from a channel. Requires «Manage Messages». `guild_id` is optional (defaults to the active server)."
    )]
    async fn borrar_mensaje(
        &self,
        Parameters(p): Parameters<MensajeParams>,
    ) -> Result<String, String> {
        let r = self
            .borrar_mensaje_json(p.guild_id.as_deref(), &p.channel_id, &p.message_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_mensajes",
        description = "Deletes several messages from a channel at once (between 2 and 100, all less than 14 days old). Requires «Manage Messages». `guild_id` is optional."
    )]
    async fn borrar_mensajes(
        &self,
        Parameters(p): Parameters<BorrarMensajesParams>,
    ) -> Result<String, String> {
        let r = self
            .borrar_mensajes_json(p.guild_id.as_deref(), &p.channel_id, &p.message_ids)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "fijar_mensaje",
        description = "Pins a message in its channel. Requires «Manage Messages». `guild_id` is optional."
    )]
    async fn fijar_mensaje(
        &self,
        Parameters(p): Parameters<MensajeParams>,
    ) -> Result<String, String> {
        let r = self
            .fijar_mensaje_json(p.guild_id.as_deref(), &p.channel_id, &p.message_id, true)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "desfijar_mensaje",
        description = "Unpins a message from its channel. Requires «Manage Messages». `guild_id` is optional."
    )]
    async fn desfijar_mensaje(
        &self,
        Parameters(p): Parameters<MensajeParams>,
    ) -> Result<String, String> {
        let r = self
            .fijar_mensaje_json(p.guild_id.as_deref(), &p.channel_id, &p.message_id, false)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "crear_canal",
        description = "Creates a channel or category (`tipo`: texto, voz, categoria, anuncios or foro). Requires «Manage Channels». `guild_id` is optional."
    )]
    async fn crear_canal(
        &self,
        Parameters(p): Parameters<CrearCanalParams>,
    ) -> Result<String, String> {
        let r = self.crear_canal_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "editar_canal",
        description = "Edits a channel's name, category, topic, NSFW flag or slowmode; only the given fields change. Requires «Manage Channels». `guild_id` is optional."
    )]
    async fn editar_canal(
        &self,
        Parameters(p): Parameters<EditarCanalParams>,
    ) -> Result<String, String> {
        let r = self.editar_canal_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_canal",
        description = "Deletes a channel or category (irreversible). Requires «Manage Channels». `guild_id` is optional."
    )]
    async fn borrar_canal(&self, Parameters(p): Parameters<CanalParams>) -> Result<String, String> {
        let r = self
            .borrar_canal_json(p.guild_id.as_deref(), &p.channel_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "ordenar_canales",
        description = "Changes the position of several channels at once with a list of {id, posicion}. Requires «Manage Channels». `guild_id` is optional."
    )]
    async fn ordenar_canales(
        &self,
        Parameters(p): Parameters<OrdenarParams>,
    ) -> Result<String, String> {
        let r = self
            .ordenar_canales_json(p.guild_id.as_deref(), &p.posiciones)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "crear_rol",
        description = "Creates a role. `permisos` accepts the bits text or a list of names (e.g. [\"ManageMessages\",\"ViewChannel\"]). Requires «Manage Roles». `guild_id` is optional."
    )]
    async fn crear_rol(&self, Parameters(p): Parameters<CrearRolParams>) -> Result<String, String> {
        let r = self.crear_rol_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "editar_rol",
        description = "Edits a role; only the given fields change. `permisos` (bits or list of names) replaces the current ones. Requires «Manage Roles» and the role must be below the bot's role. `guild_id` is optional."
    )]
    async fn editar_rol(
        &self,
        Parameters(p): Parameters<EditarRolParams>,
    ) -> Result<String, String> {
        let r = self.editar_rol_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_rol",
        description = "Deletes a role (irreversible). Requires «Manage Roles» and the role must be below the bot's role. `guild_id` is optional."
    )]
    async fn borrar_rol(&self, Parameters(p): Parameters<RolParams>) -> Result<String, String> {
        let r = self
            .borrar_rol_json(p.guild_id.as_deref(), &p.role_id)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "ordenar_roles",
        description = "Changes the position of several roles with a list of {id, posicion}. Source and target must stay below the bot's role. Requires «Manage Roles». `guild_id` is optional."
    )]
    async fn ordenar_roles(
        &self,
        Parameters(p): Parameters<OrdenarParams>,
    ) -> Result<String, String> {
        let r = self
            .ordenar_roles_json(p.guild_id.as_deref(), &p.posiciones)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "dar_rol",
        description = "Adds a role to a member. Discord requires «Manage Roles» and the role must be below the bot's role. `guild_id` is optional."
    )]
    async fn dar_rol(&self, Parameters(p): Parameters<RolMiembroParams>) -> Result<String, String> {
        let r = self
            .cambiar_rol_json(p.guild_id.as_deref(), &p.user_id, &p.role_id, true)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "quitar_rol",
        description = "Removes a role from a member. Discord requires «Manage Roles» and the role must be below the bot's role. `guild_id` is optional."
    )]
    async fn quitar_rol(
        &self,
        Parameters(p): Parameters<RolMiembroParams>,
    ) -> Result<String, String> {
        let r = self
            .cambiar_rol_json(p.guild_id.as_deref(), &p.user_id, &p.role_id, false)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "permiso_canal",
        description = "Sets the specific permissions of a role or member in a channel (`tipo`: rol or miembro; `permitir` and `denegar` as bits or a list of names). Requires «Manage Roles». `guild_id` is optional."
    )]
    async fn permiso_canal(
        &self,
        Parameters(p): Parameters<PermisoCanalParams>,
    ) -> Result<String, String> {
        let r = self.permiso_canal_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "quitar_permiso_canal",
        description = "Removes the specific permissions of a role or member in a channel (`tipo`: rol or miembro). Requires «Manage Roles». `guild_id` is optional."
    )]
    async fn quitar_permiso_canal(
        &self,
        Parameters(p): Parameters<QuitarPermisoCanalParams>,
    ) -> Result<String, String> {
        let r = self.quitar_permiso_canal_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "avisar",
        description = "Warns a member: saves the warning (reason, optional `mensaje_id`) and, unless `dm` is false, sends them a direct message with the reason and how many warnings they have. If the DM fails the warning is saved anyway and the result says so. `user_id` accepts an ID or a name. `guild_id` is optional."
    )]
    async fn avisar(&self, Parameters(p): Parameters<AvisarParams>) -> Result<String, String> {
        let r = self.avisar_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "listar_avisos",
        description = "Lists the warnings of the server: count per user and the latest warnings. `user_id` (ID or name) is optional and limits it to one user. `guild_id` is optional."
    )]
    async fn listar_avisos(
        &self,
        Parameters(p): Parameters<ListarAvisosParams>,
    ) -> Result<String, String> {
        let r = self
            .listar_avisos_json(p.guild_id.as_deref(), p.user_id.as_deref())
            .await;
        self.salida(r)
    }

    #[tool(
        name = "expulsar",
        description = "Kicks a member from the server. The `motivo` goes to the audit log. Requires «Kick Members»; the owner and anyone with a role equal to or above the bot's are rejected. `guild_id` is optional."
    )]
    async fn expulsar(&self, Parameters(p): Parameters<ExpulsarParams>) -> Result<String, String> {
        let r = self.expulsar_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "banear",
        description = "Bans a user (even if not in the server), with a `motivo` for the audit log and `dias_mensajes` (0–7) of messages to delete. Requires «Ban Members»; the owner and anyone with a role equal to or above the bot's are rejected. `guild_id` is optional."
    )]
    async fn banear(&self, Parameters(p): Parameters<BanearParams>) -> Result<String, String> {
        let r = self.banear_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "desbanear",
        description = "Unbans a user, with an optional `motivo` for the audit log. Requires «Ban Members». `guild_id` is optional."
    )]
    async fn desbanear(&self, Parameters(p): Parameters<ExpulsarParams>) -> Result<String, String> {
        let r = self
            .desbanear_json(p.guild_id.as_deref(), &p.user_id, p.motivo.as_deref())
            .await;
        self.salida(r)
    }

    #[tool(
        name = "silenciar",
        description = "Times out a member for `minutos` (maximum 40320 = 28 days); 0 removes the timeout. The `motivo` goes to the audit log. Requires «Timeout Members»; the owner and anyone with a role equal to or above the bot's are rejected. `guild_id` is optional."
    )]
    async fn silenciar(
        &self,
        Parameters(p): Parameters<SilenciarParams>,
    ) -> Result<String, String> {
        let r = self.silenciar_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "cambiar_apodo",
        description = "Changes a member's nickname on the server (up to 32 characters); without `apodo` it is cleared. The `motivo` goes to the audit log. Requires «Manage Nicknames»; the owner and anyone with a role equal to or above the bot's are rejected (the bot itself always can). `guild_id` is optional."
    )]
    async fn cambiar_apodo(
        &self,
        Parameters(p): Parameters<CambiarApodoParams>,
    ) -> Result<String, String> {
        let r = self.cambiar_apodo_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "editar_servidor",
        description = "Edits the general server settings (name, description, verification level, content filter, default notifications, system channel, AFK channel and AFK timeout); only the given fields change and an empty description or channel clears it. Requires «Manage Server». `guild_id` is optional."
    )]
    async fn editar_servidor(
        &self,
        Parameters(p): Parameters<EditarServidorParams>,
    ) -> Result<String, String> {
        let r = self.editar_servidor_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "crear_invitacion",
        description = "Creates an invite to a channel (`caducidad_segundos` 0–604800, default 86400; `usos_maximos` 0–100, default unlimited). Requires «Create Invite». `guild_id` is optional."
    )]
    async fn crear_invitacion(
        &self,
        Parameters(p): Parameters<CrearInvitacionParams>,
    ) -> Result<String, String> {
        let r = self.crear_invitacion_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_invitacion",
        description = "Deletes an invite by its code. Requires «Manage Server». `guild_id` is optional."
    )]
    async fn borrar_invitacion(
        &self,
        Parameters(p): Parameters<BorrarInvitacionParams>,
    ) -> Result<String, String> {
        let r = self
            .borrar_invitacion_json(p.guild_id.as_deref(), &p.codigo)
            .await;
        self.salida(r)
    }

    #[tool(
        name = "crear_webhook",
        description = "Creates a webhook in a channel and returns its full URL (shown only this once: it contains the token, keep it safe). Requires «Manage Webhooks». `guild_id` is optional."
    )]
    async fn crear_webhook(
        &self,
        Parameters(p): Parameters<CrearWebhookParams>,
    ) -> Result<String, String> {
        let r = self.crear_webhook_json(&p).await;
        self.salida(r)
    }

    #[tool(
        name = "borrar_webhook",
        description = "Deletes a webhook by its ID (irreversible). Requires «Manage Webhooks». `guild_id` is optional."
    )]
    async fn borrar_webhook(
        &self,
        Parameters(p): Parameters<BorrarWebhookParams>,
    ) -> Result<String, String> {
        let r = self
            .borrar_webhook_json(p.guild_id.as_deref(), &p.webhook_id)
            .await;
        self.salida(r)
    }
}

#[tool_handler]
impl ServerHandler for ArgusMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("argus-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCCIONES)
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
        let nombre = request.name.to_string();
        if self.herramientas_conectores().await.contains(&nombre) {
            let args = request
                .arguments
                .map(serde_json::Value::Object)
                .unwrap_or_else(|| json!({}));
            let (_tx, rx) = tokio::sync::watch::channel(false);
            let r = self
                .llamar_conector(&nombre, args, rx)
                .await
                .map(|v| v.to_string())
                .map_err(|e| self.limpiar(e));
            return rmcp::handler::server::tool::IntoCallToolResult::into_call_tool_result(r);
        }
        let ctx = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router().call(ctx).await
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListToolsResult::with_all_items(
            self.herramientas_publicas().await,
        ))
    }
}

/// Names of every tool registered by the MCP server (without connector tools).
pub fn nombres_registrados() -> Vec<String> {
    ArgusMcp::tool_router()
        .list_all()
        .iter()
        .map(|t| t.name.to_string())
        .collect()
}

/// Instructions returned by `get_info`: what the server can do and how writes work.
pub const INSTRUCCIONES: &str = "Manage Discord with the bots registered in ARGUS. \
Start with `estado` to see the active bot and server.\n\
- Writes are proposals: the user approves them in ARGUS and ARGUS runs exactly what was approved. \
Never claim a change is done before that.\n\
- Result verification: an accepted request is not a verified change. Operation results are \
correct, partial, failed, cancelled or unverified; report partial or unverified results \
honestly and re-read the server to check the outcome.\n\
- History: ARGUS keeps a persistent history of operations per bot, server and operation; \
do not repeat an operation that already appears there as done.\n\
- Diagnostics: ARGUS can check connection, permissions, agents, MCP and storage without \
changing Discord; ask the user to open Diagnostics when a tool fails.\n\
- Templates: `listar_plantillas` and `consultar_plantilla` read the user's library; \
`proponer_plantilla` prepares the proposal to apply one to the active server; \
`guardar_plantilla_desde_plan` and `guardar_plantilla_desde_snapshot` save one (only as an \
approved proposal action).\n\
- Permission audit: `informe_permisos` reports partial evidence and never proves a server is \
safe; `proponer_correccion` prepares the correction of one finding as a proposal for the user to approve.\n\
- Connectors: when the user enabled connectors for other bots, their read-only tools appear \
as `connector__tool`. They only read; configuring another bot is a proposal the user approves.";

#[cfg(test)]
#[path = "mcp_tests_nuevo.rs"]
mod tests_mcp_nuevo;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use crate::db::{self, BotConfig};
    use chrono::Utc;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TOKEN: &str = "token-secreto-de-prueba.XYZ";

    async fn snapshot_mcp() -> (tempfile_dir::Dir, ArgusMcp, MockServer) {
        let (dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let server = discord_falso().await;
        for (suffix, data) in [
            ("", json!({"id":"555","name":"Fixture guild"})),
            (
                "/roles",
                json!([{"id":"555","name":"@everyone","permissions":"1024","position":0}]),
            ),
            ("/channels", json!([])),
            ("/members", json!([])),
            ("/emojis", json!([])),
            ("/stickers", json!([])),
            ("/webhooks", json!([])),
            ("/bans", json!([])),
        ] {
            get_json(&server, &format!("/api/v10/guilds/555{suffix}"), data).await;
        }
        let mcp = servidor(pool, &server).await;
        mcp.elegir_servidor_json("555").await.unwrap();
        (dir, mcp, server)
    }

    async fn assert_no_discord_writes(server: &MockServer) {
        assert!(server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method.as_str() == "GET"));
    }

    #[tokio::test]
    async fn snapshot_tool_lists_only_active_scope() {
        let (_dir, mcp, server) = snapshot_mcp().await;
        assert_eq!(
            v(mcp.listar_snapshots().await.unwrap())["snapshots"],
            json!([])
        );
        mcp.crear_snapshot().await.unwrap();
        assert_eq!(
            v(mcp.listar_snapshots().await.unwrap())["snapshots"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        mcp.state.config.lock().unwrap().active_guild_id = Some("556".into());
        mcp.state.save_config_to_pool(&mcp.pool).await.unwrap();
        assert_eq!(
            v(mcp.listar_snapshots().await.unwrap())["snapshots"],
            json!([])
        );
        assert_no_discord_writes(&server).await;
    }

    #[tokio::test]
    async fn snapshot_tool_creates_local_capture_without_changing_discord() {
        let (_dir, mcp, server) = snapshot_mcp().await;
        let created = v(mcp.crear_snapshot().await.unwrap());
        let saved = crate::snapshots::load(
            &mcp.pool,
            "123456789",
            "555",
            created["snapshot_id"].as_str().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(saved.guild_id, "555");
        assert_no_discord_writes(&server).await;
    }

    #[tokio::test]
    async fn snapshot_tool_compares_without_applying_and_rejects_other_scope() {
        let (_dir, mcp, server) = snapshot_mcp().await;
        let created = v(mcp.crear_snapshot().await.unwrap());
        let id = created["snapshot_id"].as_str().unwrap().to_string();
        let comparison = v(mcp
            .comparar_snapshot(Parameters(SnapshotParams {
                snapshot_id: id.clone(),
            }))
            .await
            .unwrap());
        assert_eq!(comparison["counts"]["missing"], 0);
        mcp.state.config.lock().unwrap().active_guild_id = Some("556".into());
        mcp.state.save_config_to_pool(&mcp.pool).await.unwrap();
        assert!(mcp.comparar_snapshot_json(&id).await.is_err());
        assert_no_discord_writes(&server).await;
    }

    #[tokio::test]
    async fn snapshot_tool_proposes_conflicts_and_direct_apply_cannot_execute() {
        let (_dir, mcp, server) = snapshot_mcp().await;
        let created = v(mcp.crear_snapshot().await.unwrap());
        let id = created["snapshot_id"].as_str().unwrap();
        let mut saved = crate::snapshots::load(&mcp.pool, "123456789", "555", id)
            .await
            .unwrap();
        saved
            .roles
            .push(json!({"id":"7","name":"Missing integration","managed":true,"permissions":"0"}));
        let id = crate::snapshots::store(&mcp.pool, "123456789", "555", &saved, false)
            .await
            .unwrap();
        let preview = v(mcp
            .info_restauracion(Parameters(RestoreParams {
                snapshot_id: Some(id),
                undo: false,
                delete_extra: false,
            }))
            .await
            .unwrap());
        assert_eq!(preview["requires_confirmation"], true);
        assert!(preview["unavailable"]
            .to_string()
            .contains("Managed role cannot be recreated"));
        let proposal: Propuesta = serde_json::from_value(preview["propuesta"].clone()).unwrap();
        assert_eq!(proposal.acciones[0].herramienta, "aplicar_restauracion");
        assert!(mcp
            .aplicar_restauracion(Parameters(ApplyRestoreParams {
                id: preview["id"].as_str().unwrap().into(),
                confirmation: "yes".into(),
            }))
            .await
            .is_err());
        assert_no_discord_writes(&server).await;
        // Simula el límite de procesos: importar la propuesta persistida sin reconstruirla.
        crate::snapshots::restore::forget_for_test(preview["id"].as_str().unwrap());
        crate::snapshots::restore::load_reviewed(
            &mcp.pool,
            &mcp.state,
            preview["id"].as_str().unwrap(),
        )
        .await
        .unwrap();
        assert_no_discord_writes(&server).await;
    }

    #[test]
    fn color_acepta_hex_como_lo_devuelve_listar_roles_y_numero() {
        let de = |v: serde_json::Value| serde_json::from_value::<ColorParam>(v).unwrap();
        assert_eq!(de(json!("#5865F2")).a_u32().unwrap(), 0x5865F2);
        assert_eq!(de(json!("5865f2")).a_u32().unwrap(), 0x5865F2);
        assert_eq!(de(json!(255)).a_u32().unwrap(), 255);
        assert!(de(json!("azul")).a_u32().is_err());
        assert!(de(json!(0x0100_0000)).a_u32().is_err());
    }

    async fn bd_temporal() -> (tempfile_dir::Dir, SqlitePool) {
        let dir = tempfile_dir::Dir::nuevo();
        let pool = db::abrir_bd(&dir.0.join("argus.db")).await.unwrap();
        (dir, pool)
    }

    /// Carpeta temporal que se borra al soltarla (sin dependencias nuevas).
    mod tempfile_dir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn nuevo() -> Self {
                let p = std::env::temp_dir().join(format!("argus-mcp-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    async fn guardar_bot(pool: &SqlitePool) {
        let now = Utc::now().to_rfc3339();
        db::upsert_bot(
            pool,
            &BotConfig {
                id: "123456789".into(),
                name: "Centinela".into(),
                token_ciphertext: crypto::encrypt(TOKEN).unwrap(),
                avatar: None,
                application_id: None,
                allowed_guilds: Vec::new(),
                created_at: now.clone(),
                updated_at: now,
            },
        )
        .await
        .unwrap();
    }

    async fn discord_falso() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v10/users/@me/guilds"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"id": "555", "name": "Taller", "icon": null, "owner": false,
                 "permissions": "8", "features": []},
                {"id": "556", "name": "Casa", "icon": null, "owner": false,
                 "permissions": "8", "features": []}
            ])))
            .mount(&server)
            .await;
        server
    }

    async fn servidor(pool: SqlitePool, discord: &MockServer) -> ArgusMcp {
        ArgusMcp::nuevo(pool, Some(discord.address().to_string()))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn sin_bot_activo_todas_las_herramientas_lo_dicen() {
        let (_dir, pool) = bd_temporal().await;
        let discord = discord_falso().await;
        let mcp = servidor(pool, &discord).await;
        assert_eq!(mcp.estado_json().await.unwrap_err(), SIN_BOT);
        assert_eq!(mcp.listar_servidores_json().await.unwrap_err(), SIN_BOT);
        assert_eq!(mcp.elegir_servidor_json("555").await.unwrap_err(), SIN_BOT);
    }

    #[tokio::test]
    async fn los_mensajes_salen_en_el_idioma_de_la_config() {
        let (_dir, pool) = bd_temporal().await;
        let discord = discord_falso().await;
        let mcp = servidor(pool.clone(), &discord).await;

        // Inglés por defecto.
        assert_eq!(
            mcp.estado().await.unwrap_err(),
            "No active bot in ARGUS: add or choose one in the app"
        );

        // Español con `ui_language = "es"` guardado en la base.
        mcp.state.config.lock().unwrap().ui_language = "es".to_string();
        mcp.state.save_config_to_pool(&pool).await.unwrap();
        assert_eq!(
            mcp.estado().await.unwrap_err(),
            "No hay bot activo en ARGUS: añade o elige uno en la app"
        );
        let err = mcp
            .elegir_servidor(Parameters(ElegirServidorParams {
                guild_id: "555".into(),
            }))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            "No hay bot activo en ARGUS: añade o elige uno en la app"
        );

        // Con bot activo: el servidor activo falta y el mensaje también se traduce.
        guardar_bot(&pool).await;
        assert_eq!(
            mcp.listar_canales(Parameters(ServidorParams::default()))
                .await
                .unwrap_err(),
            "No hay servidor activo en ARGUS: indica `guild_id` o elige uno con `elegir_servidor`"
        );
        mcp.state.config.lock().unwrap().ui_language = "en".to_string();
        mcp.state.save_config_to_pool(&pool).await.unwrap();
        assert_eq!(
            mcp.listar_canales(Parameters(ServidorParams::default()))
                .await
                .unwrap_err(),
            "No active server in ARGUS: pass `guild_id` or choose one with `elegir_servidor`"
        );
    }

    #[test]
    fn las_descripciones_de_las_herramientas_estan_en_ingles() {
        for t in ArgusMcp::tool_router().list_all() {
            let d = t.description.as_deref().unwrap_or_default();
            assert!(
                !d.contains("Requiere") && !d.contains("opcional."),
                "{}",
                t.name
            );
            assert!(!d.contains("Solo lectura"), "{}", t.name);
        }
    }

    #[tokio::test]
    async fn estado_muestra_bot_y_servidor_sin_token() {
        let (_dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let discord = discord_falso().await;
        let mcp = servidor(pool, &discord).await;
        let v: serde_json::Value = serde_json::from_str(&mcp.estado_json().await.unwrap()).unwrap();
        assert_eq!(v["bot_activo"]["nombre"], "Centinela");
        assert!(v["servidor_activo"].is_null());
        assert!(!v.to_string().contains(TOKEN));
        assert!(!v.to_string().contains("token"));
    }

    #[tokio::test]
    async fn listar_y_elegir_servidor_persiste_en_la_bd() {
        let (_dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let discord = discord_falso().await;
        let mcp = servidor(pool.clone(), &discord).await;

        let lista: serde_json::Value =
            serde_json::from_str(&mcp.listar_servidores_json().await.unwrap()).unwrap();
        assert_eq!(lista["servidores"].as_array().unwrap().len(), 2);
        assert_eq!(lista["servidores"][0]["nombre"], "Taller");
        assert_eq!(lista["servidores"][0]["activo"], false);

        let elegido = mcp.elegir_servidor_json("556").await.unwrap();
        assert!(elegido.contains("Casa"));

        // Otra instancia (otro proceso) ve el cambio: está en la base.
        let otra = ArgusMcp::nuevo(pool, Some(discord.address().to_string()))
            .await
            .unwrap();
        let est: serde_json::Value =
            serde_json::from_str(&otra.estado_json().await.unwrap()).unwrap();
        assert_eq!(est["servidor_activo"]["id"], "556");
        assert_eq!(est["servidor_activo"]["nombre"], "Casa");
        let lista: serde_json::Value =
            serde_json::from_str(&otra.listar_servidores_json().await.unwrap()).unwrap();
        assert_eq!(lista["servidores"][1]["activo"], true);
    }

    #[tokio::test]
    async fn elegir_servidor_desconocido_falla_y_no_cambia_nada() {
        let (_dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let discord = discord_falso().await;
        let mcp = servidor(pool, &discord).await;
        let err = mcp.elegir_servidor_json("999").await.unwrap_err();
        assert!(err.contains("999"));
        let v: serde_json::Value = serde_json::from_str(&mcp.estado_json().await.unwrap()).unwrap();
        assert!(v["servidor_activo"].is_null());
    }

    #[tokio::test]
    async fn las_herramientas_tienen_descripcion_y_esquema() {
        let tools = ArgusMcp::tool_router().list_all();
        let nombres: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        for n in [
            "estado",
            "listar_servidores",
            "elegir_servidor",
            "mapa_servidor",
            "listar_canales",
            "listar_roles",
            "listar_miembros",
            "info_miembro",
            "permisos_bot",
            "info_servidor",
            "ajustes_servidor",
            "registro_auditoria",
            "leer_mensajes",
            "buscar_mensajes",
            "listar_baneos",
            "listar_invitaciones",
            "listar_webhooks",
        ] {
            assert!(nombres.contains(&n.to_string()), "falta {n}");
        }
        for t in &tools {
            assert!(t.description.as_deref().is_some_and(|d| !d.is_empty()));
            assert_eq!(t.input_schema["type"], "object");
        }
        let elegir = tools.iter().find(|t| t.name == "elegir_servidor").unwrap();
        assert!(elegir.input_schema["properties"]["guild_id"].is_object());
        // `guild_id` es opcional en las herramientas de lectura.
        let canales = tools.iter().find(|t| t.name == "listar_canales").unwrap();
        assert!(canales.input_schema["properties"]["guild_id"].is_object());
        let requeridos = canales.input_schema.get("required");
        assert!(requeridos.is_none_or(|r| !r.to_string().contains("guild_id")));
    }

    // ---------------------------------------------------- herramientas de lectura

    const WEBHOOK_TOKEN: &str = "wh-token-secreto";

    fn rol(id: &str, nombre: &str, pos: u32, bits: u64) -> serde_json::Value {
        json!({"id": id, "name": nombre, "color": 255, "hoist": false, "flags": 0,
               "position": pos, "permissions": bits.to_string(), "managed": false,
               "mentionable": false})
    }

    fn miembro(id: &str, nombre: &str, nick: Option<&str>, roles: &[&str]) -> serde_json::Value {
        json!({"user": {"id": id, "username": nombre, "discriminator": "0",
                        "avatar": null, "bot": id == "123456789"},
               "nick": nick, "roles": roles,
               "joined_at": "2024-01-02T03:04:05.000000+00:00",
               "deaf": false, "mute": false, "flags": 0})
    }

    async fn get_json(server: &MockServer, ruta: &str, cuerpo: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(ruta))
            .respond_with(ResponseTemplate::new(200).set_body_json(cuerpo))
            .mount(server)
            .await;
    }

    /// Servidor 555 completo; el bot (rol 600) tiene `bits` de permisos.
    async fn servidor_555(server: &MockServer, bits: u64) {
        get_json(
            server,
            "/api/v10/guilds/555",
            json!({
                "id": "555", "name": "Taller", "owner_id": "9", "description": "Una comunidad",
                "icon": null, "approximate_member_count": 3,
                "roles": [], "emojis": [], "features": [], "afk_timeout": 300,
                "afk_channel_id": "12", "system_channel_id": "11",
                "default_message_notifications": 1, "explicit_content_filter": 2,
                "verification_level": 3, "mfa_level": 0, "nsfw_level": 0,
                "premium_tier": 0, "system_channel_flags": 0
            }),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/roles",
            json!([
                rol("555", "@everyone", 0, 0),
                rol("600", "Bot", 5, bits),
                rol("700", "Moderadores", 3, 8192)
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/channels",
            json!([
                {"id": "10", "type": 4, "name": "Texto", "position": 0,
                 "permission_overwrites": []},
                {"id": "11", "type": 0, "name": "general", "position": 1, "parent_id": "10",
                 "topic": "Charla del taller",
                 "permission_overwrites": [{"id": "700", "type": 0, "allow": "1024", "deny": "0"}]},
                {"id": "12", "type": 2, "name": "Voz", "position": 2, "parent_id": "10",
                 "permission_overwrites": []},
                {"id": "13", "type": 0, "name": "suelto", "position": 3,
                 "permission_overwrites": []}
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/members",
            json!([
                miembro("123456789", "centinela", None, &["600"]),
                miembro("1001", "ana", Some("Anita"), &["700"]),
                miembro("1002", "beto", None, &[])
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/members/123456789",
            miembro("123456789", "centinela", None, &["600"]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/members/1001",
            miembro("1001", "ana", Some("Anita"), &["700"]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/bans",
            json!([{"reason": "spam", "user": {"id": "1004", "username": "malo",
                    "discriminator": "0", "avatar": null}}]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/audit-logs",
            json!({
                "audit_log_entries": [{"id": "1", "user_id": "7", "target_id": "88",
                                       "action_type": 12, "changes": []}],
                "users": [{"id": "7", "username": "mod", "discriminator": "0", "avatar": null}],
                "webhooks": [], "integrations": [], "threads": [],
                "application_commands": [], "auto_moderation_rules": [],
                "guild_scheduled_events": []
            }),
        )
        .await;
        get_json(
            server,
            "/api/v10/channels/11/messages",
            json!([
                {"id": "302", "channel_id": "11", "content": "segundo", "pinned": false,
                 "timestamp": "2026-10-27T10:00:00.000000+00:00",
                 "author": {"id": "1001", "username": "ana", "discriminator": "0",
                            "avatar": null}},
                {"id": "301", "channel_id": "11", "content": "primero", "pinned": false,
                 "timestamp": "2026-10-27T09:00:00.000000+00:00",
                 "author": {"id": "1002", "username": "beto", "discriminator": "0",
                            "avatar": null}}
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/channels/13/messages",
            json!([
                {"id": "402", "channel_id": "13", "content": "nada de nada", "pinned": false,
                 "timestamp": "2026-10-27T12:00:00.000000+00:00",
                 "author": {"id": "1002", "username": "beto", "discriminator": "0",
                            "avatar": null}},
                {"id": "401", "channel_id": "13", "content": "Reunión mañana a las 10",
                 "pinned": false, "timestamp": "2026-10-27T11:00:00.000000+00:00",
                 "author": {"id": "1001", "username": "ana", "discriminator": "0",
                            "avatar": null}}
            ]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/invites",
            json!([{"code": "abc123", "uses": 2, "max_uses": 0, "max_age": 0,
                    "channel": {"id": "11", "name": "general"},
                    "inviter": {"id": "1001", "username": "ana"}}]),
        )
        .await;
        get_json(
            server,
            "/api/v10/guilds/555/webhooks",
            json!([{"id": "900", "name": "Avisos", "channel_id": "11",
                    "token": WEBHOOK_TOKEN}]),
        )
        .await;
    }

    const TODO: u64 = 8; // Administrator

    async fn mcp_con_servidor(
        bits: u64,
        activo: bool,
    ) -> (tempfile_dir::Dir, ArgusMcp, MockServer) {
        let (dir, pool) = bd_temporal().await;
        guardar_bot(&pool).await;
        let discord = discord_falso().await;
        servidor_555(&discord, bits).await;
        // Crear un canal dentro de una categoría lee antes sus permisos para
        // copiarlos: respuesta genérica (sin permisos propios) de baja prioridad.
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex(r"^/api/v10/channels/\d+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "10", "type": 4, "guild_id": "555", "name": "Categoría",
                "position": 0, "permission_overwrites": []
            })))
            .with_priority(10)
            .mount(&discord)
            .await;
        let mcp = servidor(pool, &discord).await;
        if activo {
            mcp.elegir_servidor_json("555").await.unwrap();
        }
        (dir, mcp, discord)
    }

    fn v(texto: String) -> serde_json::Value {
        serde_json::from_str(&texto).unwrap()
    }

    #[tokio::test]
    async fn mapa_y_listas_usan_el_servidor_activo_con_nombres() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        let mapa = v(mcp.mapa_servidor_json(None).await.unwrap());
        assert_eq!(mapa["servidor"]["nombre"], "Taller");
        assert_eq!(mapa["categorias"][0]["nombre"], "Texto");
        assert_eq!(mapa["categorias"][0]["canales"][0]["nombre"], "general");
        assert_eq!(mapa["categorias"][0]["canales"][1]["tipo"], "voz");
        assert_eq!(mapa["sin_categoria"][0]["nombre"], "suelto");
        assert_eq!(mapa["miembros_cargados"], 3);
        let mods = mapa["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "700")
            .unwrap();
        assert_eq!(mods["miembros"], 1);

        let canales = v(mcp.listar_canales_json(None).await.unwrap());
        let general = &canales["canales"][1];
        assert_eq!(general["categoria"], "Texto");
        assert_eq!(general["overwrites"][0]["tipo"], "rol");
        // Legibles: nombre del rol y nombres de la API, como los que acepta permiso_canal.
        assert_eq!(general["overwrites"][0]["nombre"], "Moderadores");
        assert_eq!(general["overwrites"][0]["permitir"], json!(["ViewChannel"]));
        assert_eq!(general["overwrites"][0]["denegar"], json!([]));
        // La descripción del canal también se ve (para reescribirla o revisarla).
        assert_eq!(general["tema"], "Charla del taller");

        let roles = v(mcp.listar_roles_json(None).await.unwrap());
        assert_eq!(roles["roles"][0]["nombre"], "Bot");
        assert_eq!(roles["roles"][0]["color"], "#0000ff");
        assert_eq!(roles["roles"][1]["permisos"][0], "Gestionar mensajes");

        let info = v(mcp.info_servidor_json(None).await.unwrap());
        assert_eq!(
            (info["nombre"].as_str(), info["miembros_aprox"].as_u64()),
            (Some("Taller"), Some(3))
        );
    }

    #[tokio::test]
    async fn miembros_con_busqueda_y_limite_e_info_miembro() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        let todos = v(mcp.listar_miembros_json(None, None, None).await.unwrap());
        assert_eq!(todos["total_servidor"], 3);
        assert_eq!(todos["devueltos"], 3);

        let limitado = v(mcp.listar_miembros_json(None, None, Some(1)).await.unwrap());
        assert_eq!(
            (
                limitado["devueltos"].as_u64(),
                limitado["coincidencias"].as_u64()
            ),
            (Some(1), Some(3))
        );

        let por_apodo = v(mcp
            .listar_miembros_json(None, Some("ANIT"), None)
            .await
            .unwrap());
        assert_eq!(por_apodo["coincidencias"], 1);
        assert_eq!(por_apodo["miembros"][0]["usuario"], "ana");
        assert_eq!(
            por_apodo["miembros"][0]["roles"][0]["nombre"],
            "Moderadores"
        );

        let por_id = v(mcp
            .listar_miembros_json(None, Some("1002"), None)
            .await
            .unwrap());
        assert_eq!(por_id["miembros"][0]["usuario"], "beto");

        let uno = v(mcp.info_miembro_json(None, "1001").await.unwrap());
        assert_eq!(uno["apodo"], "Anita");
        assert_eq!(uno["roles"][0]["id"], "700");
        assert_eq!(uno["roles"][0]["nombre"], "Moderadores");
        assert!(mcp.info_miembro_json(None, "abc").await.is_err());
    }

    #[tokio::test]
    async fn permisos_ajustes_y_auditoria() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        let permisos = v(mcp.permisos_bot_json(None).await.unwrap());
        assert_eq!(permisos["bot"]["nombre"], "Centinela");
        assert!(permisos["permisos"].to_string().contains("Banear miembros"));

        let ajustes = v(mcp.ajustes_servidor_json(None).await.unwrap());
        assert_eq!(ajustes["nombre"], "Taller");
        assert_eq!(ajustes["canal_del_sistema"]["nombre"], "general");
        assert_eq!(ajustes["canal_afk"]["nombre"], "Voz");
        assert_eq!(ajustes["afk_segundos"], 300);

        let auditoria = v(mcp.registro_auditoria_json(None).await.unwrap());
        assert_eq!(auditoria["entradas"][0]["accion"], "Canal eliminado");
        assert_eq!(auditoria["entradas"][0]["autor"], "mod");
    }

    #[tokio::test]
    async fn mensajes_baneos_invitaciones_y_webhooks_sin_token() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        let todos = v(mcp.leer_mensajes_json(None, "11", None).await.unwrap());
        assert_eq!(todos["mensajes"].as_array().unwrap().len(), 2);
        assert_eq!(todos["mensajes"][0]["author_name"], "ana");
        let uno = v(mcp.leer_mensajes_json(None, "11", Some(1)).await.unwrap());
        assert_eq!(uno["mensajes"].as_array().unwrap().len(), 1);
        assert_eq!(uno["mensajes"][0]["content"], "segundo");
        // Más de 50 se recorta a 50 (Discord entrega 2 aquí) y 0 sube a 1.
        let cero = v(mcp.leer_mensajes_json(None, "11", Some(0)).await.unwrap());
        assert_eq!(cero["mensajes"].as_array().unwrap().len(), 1);
        assert_eq!(
            v(mcp.leer_mensajes_json(None, "11", Some(500)).await.unwrap())["mensajes"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        let bans = v(mcp.listar_baneos_json(None).await.unwrap());
        assert_eq!(bans["baneos"][0]["username"], "malo");
        assert_eq!(bans["baneos"][0]["reason"], "spam");

        let inv = v(mcp.listar_invitaciones_json(None).await.unwrap());
        assert_eq!(inv["invitaciones"][0]["code"], "abc123");
        assert_eq!(inv["invitaciones"][0]["channel_name"], "general");

        let whs = mcp.listar_webhooks_json(None).await.unwrap();
        assert!(!whs.contains(WEBHOOK_TOKEN));
        assert!(!whs.contains("discord.com/api/webhooks"));
        let whs = v(whs);
        assert_eq!(whs["webhooks"][0]["nombre"], "Avisos");
        assert_eq!(whs["webhooks"][0]["canal"], "general");
    }

    #[tokio::test]
    async fn buscar_mensajes_en_varios_canales_sin_tildes_ni_mayusculas() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;

        // «REUNION» (sin tilde, en mayúsculas) encuentra «Reunión» en #suelto.
        let r = v(mcp
            .buscar_mensajes_json(None, "REUNION", None, None, None)
            .await
            .unwrap());
        assert_eq!(r["total"], 1);
        let m = &r["coincidencias"][0];
        assert_eq!(m["canal"], "suelto");
        assert_eq!(m["canal_id"], "13");
        assert_eq!(m["autor"]["id"], "1001");
        assert_eq!(m["autor"]["nombre"], "ana");
        assert_eq!(m["mensaje_id"], "401");
        assert_eq!(m["enlace"], "https://discord.com/channels/555/13/401");
        assert_eq!(m["fecha"], "2026-10-27T11:00:00.000000+00:00");
        assert_eq!(m["fragmento"], "Reunión mañana a las 10");
        // Solo los canales de texto (el de voz no se lee) y cuántos se revisaron.
        let rev = r["revisados_por_canal"].as_array().unwrap();
        assert_eq!(rev.len(), 2);
        assert_eq!(rev[0]["canal"], "general");
        assert_eq!(rev[0]["revisados"], 2);
        assert_eq!(rev[1]["canal"], "suelto");
        assert_eq!(rev[1]["revisados"], 2);

        // Coincidencias en dos canales a la vez.
        let r = v(mcp
            .buscar_mensajes_json(None, "r", None, None, None)
            .await
            .unwrap());
        let canales: Vec<_> = r["coincidencias"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["canal"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(canales, ["general", "suelto"]);

        // Canales por nombre (con #) o por ID.
        let pedidos = ["#General".to_string(), "13".to_string()];
        let r = v(mcp
            .buscar_mensajes_json(None, "r", Some(&pedidos), None, None)
            .await
            .unwrap());
        assert_eq!(r["total"], 2);
        let solo = ["suelto".to_string()];
        let r = v(mcp
            .buscar_mensajes_json(None, "r", Some(&solo), None, None)
            .await
            .unwrap());
        assert_eq!(r["total"], 1);
        assert_eq!(r["revisados_por_canal"].as_array().unwrap().len(), 1);
        let mal = ["no-existe".to_string()];
        assert!(mcp
            .buscar_mensajes_json(None, "r", Some(&mal), None, None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn buscar_mensajes_filtra_por_autor() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;
        // «e» aparece en mensajes de ana y de beto; con autor solo salen los suyos.
        let todos = v(mcp
            .buscar_mensajes_json(None, "e", None, None, None)
            .await
            .unwrap());
        assert_eq!(todos["total"], 4);
        let ana = v(mcp
            .buscar_mensajes_json(None, "e", None, Some("ANA"), None)
            .await
            .unwrap());
        assert_eq!(ana["total"], 2);
        for c in ana["coincidencias"].as_array().unwrap() {
            assert_eq!(c["autor"]["nombre"], "ana");
        }
        let por_id = v(mcp
            .buscar_mensajes_json(None, "e", None, Some("1002"), None)
            .await
            .unwrap());
        assert_eq!(por_id["total"], 2);
        assert_eq!(por_id["coincidencias"][0]["autor"]["id"], "1002");
    }

    #[tokio::test]
    async fn buscar_mensajes_sin_resultados_y_con_limite() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;
        let r = v(mcp
            .buscar_mensajes_json(None, "zzzz", None, None, None)
            .await
            .unwrap());
        assert_eq!(r["total"], 0);
        assert!(r["coincidencias"].as_array().unwrap().is_empty());
        // Aun sin resultados se informa de lo revisado.
        assert_eq!(r["revisados_por_canal"][0]["revisados"], 2);

        // Con límite 1 solo se revisa el mensaje más reciente de cada canal.
        let r = v(mcp
            .buscar_mensajes_json(None, "r", None, None, Some(1))
            .await
            .unwrap());
        assert_eq!(r["revisados_por_canal"][0]["revisados"], 1);
        assert_eq!(r["revisados_por_canal"][1]["revisados"], 1);
        // Lo más nuevo de #general es «segundo» (sin «r») y de #suelto «nada de nada».
        assert_eq!(r["total"], 0);
        // 0 sube a 1 y un límite enorme se recorta (aquí solo hay 2 por canal).
        let r = v(mcp
            .buscar_mensajes_json(None, "r", None, None, Some(0))
            .await
            .unwrap());
        assert_eq!(r["revisados_por_canal"][0]["revisados"], 1);
        let r = v(mcp
            .buscar_mensajes_json(None, "r", None, None, Some(9999))
            .await
            .unwrap());
        assert_eq!(r["revisados_por_canal"][0]["revisados"], 2);

        // Texto vacío y sin servidor activo.
        assert!(mcp
            .buscar_mensajes_json(None, "  ", None, None, None)
            .await
            .is_err());
    }

    #[test]
    fn plegar_y_fragmento() {
        assert_eq!(plegar("  CanciÓn Ñu "), "cancion ñu");
        let largo = format!("{} Árbol {}", "x".repeat(300), "y".repeat(300));
        let f = fragmento_de(&largo, "arbol").unwrap();
        assert!(f.starts_with('…') && f.ends_with('…'));
        assert!(f.contains("Árbol"));
        assert!(f.chars().count() <= FRAGMENTO_MAX + 2);
        assert!(fragmento_de("nada", "zzz").is_none());
    }

    #[tokio::test]
    async fn guild_id_explicito_funciona_sin_servidor_activo() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, false).await;
        let roles = v(mcp.listar_roles_json(Some("555")).await.unwrap());
        assert_eq!(roles["roles"].as_array().unwrap().len(), 3);
        let info = v(mcp.info_servidor_json(Some(" 555 ")).await.unwrap());
        assert_eq!(info["nombre"], "Taller");
    }

    #[tokio::test]
    async fn sin_servidor_activo_las_herramientas_lo_dicen() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, false).await;
        let resultados = [
            mcp.mapa_servidor_json(None).await,
            mcp.listar_canales_json(None).await,
            mcp.listar_roles_json(None).await,
            mcp.listar_miembros_json(None, None, None).await,
            mcp.info_miembro_json(None, "1001").await,
            mcp.permisos_bot_json(None).await,
            mcp.info_servidor_json(None).await,
            mcp.ajustes_servidor_json(None).await,
            mcp.registro_auditoria_json(None).await,
            mcp.leer_mensajes_json(None, "11", None).await,
            mcp.buscar_mensajes_json(None, "hola", None, None, None)
                .await,
            mcp.listar_baneos_json(None).await,
            mcp.listar_invitaciones_json(None).await,
            mcp.listar_webhooks_json(None).await,
        ];
        for r in resultados {
            assert_eq!(r.unwrap_err(), SIN_SERVIDOR);
        }
    }

    #[tokio::test]
    async fn sin_bot_activo_las_herramientas_de_lectura_lo_dicen() {
        let (_dir, pool) = bd_temporal().await;
        let discord = discord_falso().await;
        let mcp = servidor(pool, &discord).await;
        assert_eq!(
            mcp.mapa_servidor_json(Some("555")).await.unwrap_err(),
            SIN_BOT
        );
        assert_eq!(mcp.listar_canales_json(None).await.unwrap_err(), SIN_BOT);
        assert_eq!(
            mcp.leer_mensajes_json(None, "11", None).await.unwrap_err(),
            SIN_BOT
        );
        assert_eq!(mcp.listar_webhooks_json(None).await.unwrap_err(), SIN_BOT);
    }

    #[tokio::test]
    async fn falta_de_permiso_se_informa_como_en_la_app() {
        let (_d, mcp, _s) = mcp_con_servidor(0, true).await;
        assert_eq!(
            mcp.listar_baneos_json(None).await.unwrap_err(),
            "The bot is missing the permission «Banear miembros»"
        );
        assert_eq!(
            mcp.registro_auditoria_json(None).await.unwrap_err(),
            "The bot is missing the permission «Ver registro de auditoría»"
        );
        assert_eq!(
            mcp.listar_webhooks_json(None).await.unwrap_err(),
            "The bot is missing the permission «Gestionar webhooks»"
        );
        assert_eq!(
            mcp.leer_mensajes_json(None, "11", None).await.unwrap_err(),
            "The bot is missing the permission «Leer el historial de mensajes»"
        );
    }

    // ------------------------------------------- escritura: mensajes, canales, roles

    const GESTIONAR_MENSAJES: u64 = 1 << 13;
    const GESTIONAR_CANALES: u64 = 1 << 4;
    const GESTIONAR_ROLES: u64 = 1 << 28;
    const JERARQUIA: &str =
        "That role is above the bot's role: move it up in Server settings → Roles";

    fn mensaje_discord(id: &str, canal: &str) -> serde_json::Value {
        json!({
            "id": id, "channel_id": canal, "content": "hola",
            "timestamp": "2026-10-27T10:00:00.000000+00:00",
            "edited_timestamp": null, "tts": false, "mention_everyone": false,
            "mentions": [], "mention_roles": [], "attachments": [], "embeds": [],
            "pinned": false, "type": 0,
            "author": {"id": "1", "username": "bot", "discriminator": "0", "avatar": null}
        })
    }

    /// Acepta una petición de cambio exactamente `veces` veces.
    async fn aceptar(
        server: &MockServer,
        metodo: &str,
        ruta: &str,
        estado: u16,
        cuerpo: Option<serde_json::Value>,
        veces: u64,
    ) {
        let mut r = ResponseTemplate::new(estado);
        if let Some(c) = cuerpo {
            r = r.set_body_json(c);
        }
        Mock::given(method(metodo))
            .and(path(ruta))
            .respond_with(r)
            .expect(veces)
            .mount(server)
            .await;
    }

    /// Ninguna petición de cambio debe llegar al Discord falso.
    async fn prohibir_cambios(server: &MockServer) {
        for metodo in ["POST", "PATCH", "PUT", "DELETE"] {
            Mock::given(method(metodo))
                .respond_with(ResponseTemplate::new(200))
                .expect(0)
                .mount(server)
                .await;
        }
    }

    /// Añade al servidor falso un rol «Alto» por encima del rol del bot.
    async fn con_rol_alto(server: &MockServer, bits: u64) {
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                rol("555", "@everyone", 0, 0),
                rol("600", "Bot", 5, bits),
                rol("700", "Moderadores", 3, 8192),
                rol("900", "Alto", 9, 0)
            ])))
            .with_priority(1)
            .mount(server)
            .await;
    }

    fn canal_nuevo(nombre: &str, tipo: Option<&str>) -> CrearCanalParams {
        CrearCanalParams {
            permisos: None,
            guild_id: None,
            nombre: nombre.into(),
            tipo: tipo.map(str::to_string),
            categoria_id: None,
            tema: None,
            nsfw: None,
            slowmode_segundos: None,
        }
    }

    fn editar_canal_nombre(id: &str, nombre: &str) -> EditarCanalParams {
        EditarCanalParams {
            guild_id: None,
            channel_id: id.into(),
            nombre: Some(nombre.into()),
            categoria_id: None,
            tema: None,
            nsfw: None,
            slowmode_segundos: None,
        }
    }

    fn rol_nuevo(nombre: &str) -> CrearRolParams {
        CrearRolParams {
            guild_id: None,
            nombre: nombre.into(),
            color: None,
            separado: None,
            mencionable: None,
            permisos: None,
        }
    }

    fn editar_rol_nombre(id: &str, nombre: &str) -> EditarRolParams {
        EditarRolParams {
            guild_id: None,
            role_id: id.into(),
            nombre: Some(nombre.into()),
            color: None,
            separado: None,
            mencionable: None,
            permisos: None,
        }
    }

    fn pos(id: &str, p: u64) -> Posicion {
        Posicion {
            id: id.into(),
            posicion: p,
        }
    }

    fn permiso(target: &str, tipo: &str) -> PermisoCanalParams {
        PermisoCanalParams {
            guild_id: None,
            channel_id: "11".into(),
            target_id: target.into(),
            tipo: tipo.into(),
            permitir: None,
            denegar: None,
        }
    }

    fn quitar_permiso(target: &str, tipo: &str) -> QuitarPermisoCanalParams {
        QuitarPermisoCanalParams {
            guild_id: None,
            channel_id: "11".into(),
            target_id: target.into(),
            tipo: tipo.into(),
        }
    }

    fn snowflake_hace_dias(dias: i64) -> String {
        let ms = chrono::Utc::now().timestamp_millis() - dias * 86_400_000;
        (((ms - 1_420_070_400_000) as u64) << 22).to_string()
    }

    fn avisar_a(user: &str, motivo: &str, dm: Option<bool>) -> AvisarParams {
        AvisarParams {
            guild_id: None,
            user_id: user.into(),
            motivo: motivo.into(),
            mensaje_id: None,
            dm,
        }
    }

    #[tokio::test]
    async fn avisar_guarda_cuenta_y_envia_dm_con_el_numero() {
        let (_d, mcp, s) = mcp_con_servidor(TODO, true).await;
        aceptar(
            &s,
            "POST",
            "/api/v10/users/@me/channels",
            200,
            Some(json!({"id": "4242", "type": 1})),
            2,
        )
        .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/4242/messages"))
            .and(wiremock::matchers::body_string_contains(
                "Warnings so far: 2",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(mensaje_discord("902", "4242")))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "POST",
            "/api/v10/channels/4242/messages",
            200,
            Some(mensaje_discord("901", "4242")),
            1,
        )
        .await;
        let r = v(mcp
            .avisar_json(&avisar_a("1001", "spam", None))
            .await
            .unwrap());
        assert_eq!(r["total_avisos"], 1);
        assert_eq!(r["dm"]["enviado"], true);
        let mut p = avisar_a("1001", "insultos", None);
        p.mensaje_id = Some("301".into());
        let r = v(mcp.avisar_json(&p).await.unwrap());
        assert_eq!(r["total_avisos"], 2);
        assert_eq!(r["aviso"]["mensaje_id"], "301");
        assert_eq!(r["aviso"]["autor"], "123456789");
        assert_eq!(
            crate::db::contar_avisos(&mcp.pool, "555", "1001")
                .await
                .unwrap(),
            2
        );
        assert!(mcp
            .avisar_json(&avisar_a("1001", "  ", None))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn avisar_con_dm_fallido_guarda_el_aviso_y_lo_informa() {
        let (_d, mcp, s) = mcp_con_servidor(TODO, true).await;
        aceptar(&s, "POST", "/api/v10/users/@me/channels", 403, None, 1).await;
        let r = v(mcp
            .avisar_json(&avisar_a("1002", "spam", Some(true)))
            .await
            .unwrap());
        assert_eq!(r["total_avisos"], 1);
        assert_eq!(r["dm"]["enviado"], false);
        assert!(r["dm"]["error"].is_string());
        assert_eq!(
            crate::db::contar_avisos(&mcp.pool, "555", "1002")
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn avisar_sin_dm_y_por_nombre_y_listar_avisos() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, true).await;
        // Por nombre de usuario y por apodo; sin DM no se llama a Discord para ello.
        let r = v(mcp
            .avisar_json(&avisar_a("Anita", "a", Some(false)))
            .await
            .unwrap());
        assert_eq!(r["dm"]["omitido"], true);
        mcp.ejecutar_accion(
            "avisar",
            json!({"user_id": "ana", "motivo": "b", "dm": false}),
        )
        .await
        .unwrap();
        mcp.ejecutar_accion(
            "avisar",
            json!({"user_id": "beto", "motivo": "c", "dm": false}),
        )
        .await
        .unwrap();
        let e = mcp
            .avisar_json(&avisar_a("fantasma", "x", Some(false)))
            .await
            .unwrap_err();
        assert!(e.contains("fantasma"), "{e}");

        let l = v(mcp.listar_avisos_json(None, None).await.unwrap());
        assert_eq!(l["total"], 3);
        assert_eq!(l["por_usuario"][0]["user_id"], "1001");
        assert_eq!(l["por_usuario"][0]["avisos"], 2);
        assert_eq!(l["por_usuario"][1]["avisos"], 1);
        assert_eq!(l["ultimos"][0]["motivo"], "c");
        assert_eq!(l["ultimos"].as_array().unwrap().len(), 3);

        let solo = v(mcp.listar_avisos_json(None, Some("Anita")).await.unwrap());
        assert_eq!(solo["total"], 2);
        assert_eq!(solo["por_usuario"].as_array().unwrap().len(), 1);
        let nadie = v(mcp.listar_avisos_json(None, Some("1002")).await.unwrap());
        assert_eq!(nadie["total"], 1);
        let otro = v(mcp.listar_avisos_json(Some("999"), None).await.unwrap());
        assert_eq!(otro["total"], 0);
    }

    #[tokio::test]
    async fn envia_mensaje_y_dm() {
        let (_d, mcp, s) = mcp_con_servidor(TODO, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/11/messages"))
            .and(wiremock::matchers::body_partial_json(
                json!({"content": "hola"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(mensaje_discord("900", "11")))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "POST",
            "/api/v10/users/@me/channels",
            200,
            Some(json!({"id": "4242", "type": 1})),
            1,
        )
        .await;
        aceptar(
            &s,
            "POST",
            "/api/v10/channels/4242/messages",
            200,
            Some(mensaje_discord("901", "4242")),
            1,
        )
        .await;
        let r = v(mcp.enviar_mensaje_json("11", "hola", None).await.unwrap());
        assert!(r["mensaje"].as_str().unwrap().contains("900"));
        let r = v(mcp.enviar_dm_json("1001", "hola").await.unwrap());
        assert!(r["mensaje"].as_str().unwrap().contains("901"));
        // Mismos errores que la app: vacío o demasiado largo no llegan a Discord.
        let largo = "a".repeat(2001);
        let e = mcp
            .enviar_mensaje_json("11", &largo, None)
            .await
            .unwrap_err();
        assert!(e.contains("too long"), "{e}");
        assert!(mcp.enviar_dm_json("1001", "  ").await.is_err());
    }

    #[tokio::test]
    async fn borra_y_fija_mensajes() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/channels/11/messages/301",
            204,
            None,
            1,
        )
        .await;
        aceptar(&s, "PUT", "/api/v10/channels/11/pins/301", 204, None, 1).await;
        aceptar(&s, "DELETE", "/api/v10/channels/11/pins/301", 204, None, 1).await;
        let r = mcp.borrar_mensaje_json(None, "11", "301").await.unwrap();
        assert!(r.contains("deleted"));
        let r = mcp
            .fijar_mensaje_json(None, "11", "301", true)
            .await
            .unwrap();
        assert!(r.contains("pinned"));
        let r = mcp
            .fijar_mensaje_json(None, "11", "301", false)
            .await
            .unwrap();
        assert!(r.contains("unpinned"));
    }

    #[tokio::test]
    async fn borrado_en_masa_valida_cantidad_y_antiguedad() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        let (a, b) = (snowflake_hace_dias(1), snowflake_hace_dias(2));
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/11/messages/bulk-delete"))
            .and(wiremock::matchers::body_partial_json(
                json!({"messages": [a, b]}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        let ok = mcp.borrar_mensajes_json(None, "11", &[a.clone(), b]).await;
        assert!(ok.unwrap().contains("2 messages deleted"));
        // Con un solo mensaje se borra ese (Discord exige 2–100 para el masivo).
        Mock::given(method("DELETE"))
            .and(path(format!("/api/v10/channels/11/messages/{a}")))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        mcp.borrar_mensajes_json(None, "11", &[a.clone()])
            .await
            .unwrap();
        // Uno de más de 14 días: error de la app, sin llamar a Discord.
        let e = mcp
            .borrar_mensajes_json(None, "11", &[a, snowflake_hace_dias(20)])
            .await
            .unwrap_err();
        assert!(e.contains("14 days"), "{e}");
    }

    #[tokio::test]
    async fn mensajes_sin_permiso_no_llegan_a_discord() {
        let (_d, mcp, s) = mcp_con_servidor(1 << 10, true).await;
        prohibir_cambios(&s).await;
        // `borrar_mensaje` sí llama a Discord sin el permiso: el bot puede borrar
        // los suyos (ver tests_mensajes.rs).
        let falta = "The bot is missing the permission «Gestionar mensajes»";
        let e = mcp.fijar_mensaje_json(None, "11", "301", true).await;
        assert_eq!(e.unwrap_err(), falta);
        let ids = [snowflake_hace_dias(1), snowflake_hace_dias(2)];
        let e = mcp.borrar_mensajes_json(None, "11", &ids).await;
        assert_eq!(e.unwrap_err(), falta);
    }

    #[tokio::test]
    async fn reintento_reconcilia_rol_por_id_y_overwrite_sin_repetir_escrituras() {
        let (_d, mcp, server) = mcp_con_servidor(TODO, true).await;
        roles_con_nuevos(
            &server,
            &[rol("1200", "Repetido", 1, 0), rol("1201", "Repetido", 2, 0)],
        )
        .await;
        get_json(
            &server,
            "/api/v10/channels/11",
            json!({
                "id":"11", "type":0, "guild_id":"555", "name":"general", "position":1,
                "permission_overwrites":[{"id":"1200", "type":0, "allow":"1024", "deny":"0"}]
            }),
        )
        .await;
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![
                accion(
                    "crear_rol",
                    json!({"nombre":"Repetido", "color":255, "permisos":"0"}),
                ),
                accion(
                    "permiso_canal",
                    json!({"channel_id":"11", "target_id":"1200", "tipo":"rol", "permitir":"1024"}),
                ),
            ],
        };
        let anteriores = vec![
            ResultadoAccion { indice:0, herramienta:"crear_rol".into(), descripcion:String::new(), estado:EstadoAccion::Ok,
                mensaje:json!({"rol":{"id":"1200", "nombre":"Repetido", "color":"#0000ff", "permisos_bits":"0", "posicion":1}}).to_string() },
            ResultadoAccion { indice:1, herramienta:"permiso_canal".into(), descripcion:String::new(), estado:EstadoAccion::Ok, mensaje:"{}".into() },
        ];
        let result = mcp
            .ejecutar_propuesta_reanudable(&p, &AtomicBool::new(false), &mut |_, _| {}, &anteriores)
            .await
            .unwrap();
        assert_eq!(result.hechas, 2);
        let bot = mcp.state.get_active_bot().unwrap();
        let history = crate::operaciones::listar(&mcp.pool, &bot.id, "555")
            .await
            .unwrap();
        assert_eq!(history[0].status, "correct");
        assert!(server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method.as_str() == "GET"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cambio_de_servidor_a_mitad_verifica_rol_y_overwrite_en_el_original() {
        let (_d, mcp, server) = mcp_con_servidor(TODO, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Mods"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1000", "Mods", 1, 0)))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Guias"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1001", "Guias", 1, 0)))
            .expect(1)
            .mount(&server)
            .await;
        roles_con_nuevos(
            &server,
            &[rol("1000", "Mods", 1, 0), rol("1001", "Guias", 1, 0)],
        )
        .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/11/permissions/700"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        get_json(
            &server,
            "/api/v10/channels/11",
            json!({
                "id":"11", "type":0, "guild_id":"555", "name":"general", "position":1,
                "permission_overwrites":[{"id":"700", "type":0, "allow":"1024", "deny":"0"}]
            }),
        )
        .await;
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![
                accion(
                    "crear_rol",
                    json!({"nombre":"Mods", "color":255, "permisos":"0"}),
                ),
                // Se crea y se verifica después del cambio de servidor.
                accion(
                    "crear_rol",
                    json!({"nombre":"Guias", "color":255, "permisos":"0"}),
                ),
                accion(
                    "permiso_canal",
                    json!({"channel_id":"11", "target_id":"700", "tipo":"rol", "permitir":"1024"}),
                ),
            ],
        };
        let state = mcp.state.clone();
        let pool = mcp.pool.clone();
        let bot = state.get_active_bot().unwrap();
        let mut cambiado = false;
        let resultado = mcp
            .ejecutar_propuesta_con(&p, &AtomicBool::new(false), &mut |_, r| {
                if !cambiado {
                    assert_eq!(r.indice, 0);
                    cambiado = true;
                    state.set_active_guild("556");
                    let state = state.clone();
                    let pool = pool.clone();
                    std::thread::spawn(move || {
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap()
                            .block_on(state.save_config_to_pool(&pool))
                            .unwrap();
                    })
                    .join()
                    .unwrap();
                }
            })
            .await
            .unwrap();
        assert!(cambiado);
        assert_eq!(resultado.hechas, 3);
        // Todo se leyó y escribió en el servidor de la operación, nunca en el activo.
        assert!(server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| !r.url.path().contains("/guilds/556")));
        let history = crate::operaciones::listar(&mcp.pool, &bot.id, "555")
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert!(
            history[0].steps.iter().all(|s| s.status == "correct"),
            "{:?}",
            history[0].steps
        );
        assert_eq!(history[0].status, "correct");
        assert!(crate::operaciones::listar(&mcp.pool, &bot.id, "556")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn plantilla_con_rol_fallido_no_da_permisos_al_rol_viejo_del_mismo_nombre() {
        let (_d, mcp, server) = mcp_con_servidor(TODO, true).await;
        // El servidor ya tiene «Moderadores» (700); crear el de la plantilla falla.
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_json(json!({"code":50013,"message":"Missing Permissions"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id":"31", "type":0, "guild_id":"555", "name":"chat", "position":1
            })))
            .mount(&server)
            .await;
        let documento = crate::template_transfer::parse(r#"{"format":"argus-template","version":1,"name":"T","description":"","roles":[{"key":"mods","name":"Moderadores","color":12,"permissions":0}],"channels":[{"key":"chat","name":"chat","kind":"text","parent":null,"overwrites":[{"role":"mods","allow":1024,"deny":0}]}]}"#).unwrap();
        let p = crate::template_aplicar::propuesta_de_documento(&documento);
        mcp.ejecutar_propuesta_con(&p, &AtomicBool::new(false), &mut |_, _| {})
            .await
            .unwrap();
        let tocan_700: Vec<_> = server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.as_str() != "GET")
            .filter(|r| {
                r.url.path().ends_with("/700")
                    || String::from_utf8_lossy(&r.body).contains("\"700\"")
            })
            .map(|r| format!("{} {}", r.method, r.url.path()))
            .collect();
        assert!(
            tocan_700.is_empty(),
            "the old role must not get the template's permissions: {tocan_700:?}"
        );
    }

    #[tokio::test]
    async fn plantilla_aplicada_se_detiene_al_cancelar_sin_mas_escrituras() {
        let (_d, mcp, server) = mcp_con_servidor(TODO, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1000", "Staff", 1, 8192)))
            .expect(1)
            .mount(&server)
            .await;
        roles_con_nuevos(&server, &[rol("1000", "Staff", 1, 8192)]).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id":"31", "type":4, "guild_id":"555", "name":"Grupo", "position":1
            })))
            .expect(0)
            .mount(&server)
            .await;
        let documento = crate::template_transfer::parse(r#"{"format":"argus-template","version":1,"name":"T","description":"","roles":[{"key":"staff","name":"Staff","color":12,"permissions":8192}],"channels":[{"key":"group","name":"Grupo","kind":"category","parent":null,"overwrites":[{"role":"staff","allow":1024,"deny":0}]},{"key":"chat","name":"chat","kind":"text","parent":"group","overwrites":[]}]}"#).unwrap();
        let p = crate::template_aplicar::propuesta_de_documento(&documento);
        assert_eq!(p.acciones.len(), 3);
        let cancelar = AtomicBool::new(false);
        let resumen = mcp
            .ejecutar_propuesta_con(&p, &cancelar, &mut |_, r| {
                assert_eq!(r.herramienta, "crear_rol");
                cancelar.store(true, Ordering::SeqCst);
            })
            .await
            .unwrap();
        assert_eq!(resumen.hechas, 1);
        assert_eq!(resumen.canceladas, 2, "{resumen:?}");
        let bot = mcp.state.get_active_bot().unwrap();
        let history = crate::operaciones::listar(&mcp.pool, &bot.id, "555")
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_ne!(
            history[0].status, "correct",
            "a cancelled template is not a success"
        );
    }

    #[tokio::test]
    async fn propuesta_verifica_canal_por_id_devuelto() {
        let (_d, mcp, server) = mcp_con_servidor(GESTIONAR_CANALES, true).await;
        let channel = json!({"id":"801", "type":0, "guild_id":"555", "name":"nuevo", "position":1});
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel.clone()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/channels/801"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel))
            .expect(1)
            .mount(&server)
            .await;
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![accion("crear_canal", json!({"nombre":"nuevo"}))],
        };
        mcp.ejecutar_propuesta(&p).await.unwrap();
        let bot = mcp.state.get_active_bot().unwrap();
        let history = crate::operaciones::listar(&mcp.pool, &bot.id, "555")
            .await
            .unwrap();
        assert_eq!(history[0].status, "correct");
    }

    #[tokio::test]
    async fn propuesta_canal_no_reflejado_queda_sin_verificar() {
        let (_d, mcp, server) = mcp_con_servidor(GESTIONAR_CANALES, true).await;
        let accepted =
            json!({"id":"802", "type":0, "guild_id":"555", "name":"nuevo", "position":1});
        let stale =
            json!({"id":"802", "type":0, "guild_id":"555", "name":"anterior", "position":1});
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(accepted))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/channels/802"))
            .respond_with(ResponseTemplate::new(200).set_body_json(stale))
            .expect(1)
            .mount(&server)
            .await;
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![accion("crear_canal", json!({"nombre":"nuevo"}))],
        };
        mcp.ejecutar_propuesta(&p).await.unwrap();
        let bot = mcp.state.get_active_bot().unwrap();
        let history = crate::operaciones::listar(&mcp.pool, &bot.id, "555")
            .await
            .unwrap();
        assert_eq!(history[0].status, "unverified");
        assert!(history[0].steps[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("nombre"));
    }

    #[tokio::test]
    async fn canal_creado_con_otro_tipo_queda_sin_verificar() {
        let (_d, mcp, server) = mcp_con_servidor(GESTIONAR_CANALES, true).await;
        // Discord answers with a text channel although a voice one was requested.
        let texto = json!({"id":"803", "type":0, "guild_id":"555", "name":"sala", "position":1});
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(texto.clone()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/channels/803"))
            .respond_with(ResponseTemplate::new(200).set_body_json(texto))
            .mount(&server)
            .await;
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![accion(
                "crear_canal",
                json!({"nombre":"sala", "tipo":"voz"}),
            )],
        };
        mcp.ejecutar_propuesta(&p).await.unwrap();
        let bot = mcp.state.get_active_bot().unwrap();
        let history = crate::operaciones::listar(&mcp.pool, &bot.id, "555")
            .await
            .unwrap();
        assert_eq!(history[0].status, "unverified");
        assert!(history[0].steps[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("tipo"));
    }

    #[tokio::test]
    async fn crea_edita_borra_y_ordena_canales() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES, true).await;
        let canal = |id: &str, nombre: &str, tipo: u8| {
            json!({"id": id, "type": tipo, "guild_id": "555", "name": nombre,
                   "position": 4, "parent_id": "10"})
        };
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "avisos", "type": 0, "parent_id": "10", "topic": "t"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(canal("20", "avisos", 0)))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/channels/20"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "noticias"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(canal("20", "noticias", 0)))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_json(json!([
                {"id": "20", "position": 1}, {"id": "11", "position": 0}
            ])))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/channels/20",
            200,
            Some(canal("20", "x", 0)),
            1,
        )
        .await;

        let mut p = canal_nuevo("avisos", Some("texto"));
        p.categoria_id = Some("10".into());
        p.tema = Some("t".into());
        let creado = v(mcp.crear_canal_json(&p).await.unwrap());
        assert_eq!(creado["canal"]["id"], "20");
        let editado = v(mcp
            .editar_canal_json(&editar_canal_nombre("20", "noticias"))
            .await
            .unwrap());
        assert_eq!(editado["canal"]["nombre"], "noticias");
        let r = mcp
            .ordenar_canales_json(None, &[pos("20", 1), pos("11", 0)])
            .await
            .unwrap();
        assert!(r.contains("2 channels reordered"));
        let r = mcp.borrar_canal_json(None, "20").await.unwrap();
        assert!(r.contains("Channel 20 deleted"));
        // Tipo desconocido: error propio, sin llamar a Discord.
        let e = mcp
            .crear_canal_json(&canal_nuevo("x", Some("holograma")))
            .await
            .unwrap_err();
        assert!(e.contains("Unsupported channel type"));
    }

    #[tokio::test]
    async fn canales_sin_permiso_no_llegan_a_discord() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let falta = "The bot is missing the permission «Gestionar canales»";
        let e = mcp.crear_canal_json(&canal_nuevo("x", None)).await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp.borrar_canal_json(None, "11").await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp.ordenar_canales_json(None, &[pos("11", 0)]).await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp.editar_canal_json(&editar_canal_nombre("11", "y")).await;
        assert_eq!(e.unwrap_err(), falta);
    }

    #[test]
    fn permisos_como_bits_o_como_nombres() {
        let lista = PermisosParam::Nombres(vec!["ManageMessages".into(), "ViewChannel".into()]);
        let esperado = ((1u64 << 13) | (1 << 10)).to_string();
        assert_eq!(lista.a_bits().unwrap(), esperado);
        // Mayúsculas con guion bajo y nombres en español.
        let otra = PermisosParam::Nombres(vec!["MANAGE_MESSAGES".into(), "Ver canales".into()]);
        assert_eq!(otra.a_bits().unwrap(), esperado);
        assert_eq!(
            PermisosParam::Bits(" 8192 ".into()).a_bits().unwrap(),
            "8192"
        );
        assert!(PermisosParam::Bits("abc".into()).a_bits().is_err());
        let e = PermisosParam::Nombres(vec!["Volar".into()])
            .a_bits()
            .unwrap_err();
        assert!(e.contains("Volar"));
        // Se leen ambas formas desde JSON.
        let a: PermisosParam = serde_json::from_value(json!("8")).unwrap();
        let b: PermisosParam = serde_json::from_value(json!(["Administrator"])).unwrap();
        assert_eq!(a.a_bits().unwrap(), b.a_bits().unwrap());
    }

    #[tokio::test]
    async fn crea_edita_borra_y_ordena_roles() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_ROLES, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(json!({
                "name": "Mods", "color": 255, "hoist": true, "permissions": "9216"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1000", "Mods", 1, 9216)))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/roles/700"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Mods2", "permissions": "8192"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("700", "Mods2", 3, 8192)))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(&s, "DELETE", "/api/v10/guilds/555/roles/700", 204, None, 1).await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .expect(1)
            .mount(&s)
            .await;

        let mut p = rol_nuevo("Mods");
        p.color = Some(ColorParam::Numero(255));
        p.separado = Some(true);
        p.permisos = Some(PermisosParam::Nombres(vec![
            "ManageMessages".into(),
            "ViewChannel".into(),
        ]));
        let creado = v(mcp.crear_rol_json(&p).await.unwrap());
        assert_eq!(creado["rol"]["id"], "1000");
        assert_eq!(creado["rol"]["permisos"][0], "Ver canales");
        let mut p = editar_rol_nombre("700", "Mods2");
        p.permisos = Some(PermisosParam::Bits("8192".into()));
        let editado = v(mcp.editar_rol_json(&p).await.unwrap());
        assert_eq!(editado["rol"]["nombre"], "Mods2");
        let r = mcp
            .ordenar_roles_json(None, &[pos("700", 2)])
            .await
            .unwrap();
        assert!(r.contains("1 roles reordered"));
        let r = mcp.borrar_rol_json(None, "700").await.unwrap();
        assert!(r.contains("Role 700 deleted"));
    }

    #[tokio::test]
    async fn roles_sin_permiso_o_sobre_la_jerarquia_no_llegan_a_discord() {
        // Sin «Gestionar roles».
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let falta = "The bot is missing the permission «Gestionar roles»";
        assert_eq!(mcp.borrar_rol_json(None, "700").await.unwrap_err(), falta);
        let e = mcp.crear_rol_json(&rol_nuevo("x")).await;
        assert_eq!(e.unwrap_err(), falta);

        // Con permiso, pero con un rol por encima del bot.
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_ROLES, true).await;
        con_rol_alto(&s, GESTIONAR_ROLES).await;
        prohibir_cambios(&s).await;
        assert_eq!(
            mcp.borrar_rol_json(None, "900").await.unwrap_err(),
            JERARQUIA
        );
        let e = mcp.editar_rol_json(&editar_rol_nombre("900", "y")).await;
        assert_eq!(e.unwrap_err(), JERARQUIA);
        let e = mcp.ordenar_roles_json(None, &[pos("700", 9)]).await;
        assert_eq!(e.unwrap_err(), JERARQUIA);
        let e = mcp.permiso_canal_json(&permiso("900", "rol")).await;
        assert_eq!(e.unwrap_err(), JERARQUIA);
        let e = mcp
            .quitar_permiso_canal_json(&quitar_permiso("900", "rol"))
            .await;
        assert_eq!(e.unwrap_err(), JERARQUIA);
    }

    #[tokio::test]
    async fn da_y_quita_roles() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_ROLES, true).await;
        aceptar(
            &s,
            "PUT",
            "/api/v10/guilds/555/members/1001/roles/700",
            204,
            None,
            1,
        )
        .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/guilds/555/members/1001/roles/700",
            204,
            None,
            1,
        )
        .await;
        let r = mcp
            .cambiar_rol_json(None, "1001", "700", true)
            .await
            .unwrap();
        assert!(r.contains("added"));
        let r = mcp
            .cambiar_rol_json(None, "1001", "700", false)
            .await
            .unwrap();
        assert!(r.contains("removed"));
    }

    #[tokio::test]
    async fn fija_y_quita_permisos_de_canal() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_ROLES, true).await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/11/permissions/700"))
            .and(wiremock::matchers::body_partial_json(
                json!({"allow": "9216", "deny": "2048", "type": 0}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/11/permissions/1001"))
            .and(wiremock::matchers::body_partial_json(
                json!({"allow": "0", "deny": "2048", "type": 1}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/channels/11/permissions/700",
            204,
            None,
            1,
        )
        .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/channels/11/permissions/1001",
            204,
            None,
            1,
        )
        .await;

        let mut p = permiso("700", "rol");
        p.permitir = Some(PermisosParam::Nombres(vec![
            "ManageMessages".into(),
            "ViewChannel".into(),
        ]));
        p.denegar = Some(PermisosParam::Bits("2048".into()));
        let r = mcp.permiso_canal_json(&p).await.unwrap();
        assert!(r.contains("updated"));
        let mut p = permiso("1001", "miembro");
        p.denegar = Some(PermisosParam::Nombres(vec!["SendMessages".into()]));
        mcp.permiso_canal_json(&p).await.unwrap();
        for (objetivo, tipo) in [("700", "rol"), ("1001", "miembro")] {
            let r = mcp
                .quitar_permiso_canal_json(&quitar_permiso(objetivo, tipo))
                .await
                .unwrap();
            assert!(r.contains("removed"));
        }
        // Tipo inválido y permiso desconocido: errores propios.
        let e = mcp
            .quitar_permiso_canal_json(&quitar_permiso("700", "cosa"))
            .await
            .unwrap_err();
        assert!(e.contains("«rol» or «miembro»"));
        let mut p = permiso("700", "rol");
        p.permitir = Some(PermisosParam::Nombres(vec!["Volar".into()]));
        let e = mcp.permiso_canal_json(&p).await.unwrap_err();
        assert!(e.contains("Volar"));
    }

    #[tokio::test]
    async fn permisos_de_canal_sin_permiso_no_llegan_a_discord() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let falta = "The bot is missing the permission «Gestionar roles»";
        let e = mcp.permiso_canal_json(&permiso("700", "rol")).await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp
            .quitar_permiso_canal_json(&quitar_permiso("700", "rol"))
            .await;
        assert_eq!(e.unwrap_err(), falta);
    }

    #[tokio::test]
    async fn escritura_sin_servidor_o_sin_bot_lo_dice() {
        let (_d, mcp, _s) = mcp_con_servidor(TODO, false).await;
        let e = mcp.borrar_canal_json(None, "11").await;
        assert_eq!(e.unwrap_err(), SIN_SERVIDOR);
        let e = mcp.cambiar_rol_json(None, "1", "2", true).await;
        assert_eq!(e.unwrap_err(), SIN_SERVIDOR);
        let (_dir, pool) = bd_temporal().await;
        let discord = discord_falso().await;
        let sin_bot = servidor(pool, &discord).await;
        let e = sin_bot.enviar_mensaje_json("11", "hola", None).await;
        assert_eq!(e.unwrap_err(), SIN_BOT);
        let e = sin_bot.enviar_dm_json("1", "hola").await;
        assert_eq!(e.unwrap_err(), SIN_BOT);
        let e = sin_bot.borrar_rol_json(Some("555"), "7").await;
        assert_eq!(e.unwrap_err(), SIN_BOT);
    }

    #[tokio::test]
    async fn agente_consulta_biblioteca_sin_modificarla() {
        let (_dir, pool) = bd_temporal().await;
        let data = r#"{"format":"argus-template","version":1,"name":"Example","description":"","roles":[],"channels":[]}"#;
        crate::template_transfer::import(&pool, data, false)
            .await
            .unwrap();
        let mcp = ArgusMcp::nuevo(pool.clone(), None).await.unwrap();
        let list: serde_json::Value =
            serde_json::from_str(&mcp.listar_plantillas().await.unwrap()).unwrap();
        let id = list[0]["id"].as_str().unwrap().to_string();
        let detail = mcp
            .consultar_plantilla(Parameters(PlantillaParams { id }))
            .await
            .unwrap();
        assert!(detail.contains("Example"));
        assert_eq!(crate::db::list_templates(&pool).await.unwrap().len(), 1);
        assert!(HERRAMIENTAS_LECTURA.contains(&"listar_plantillas"));
        assert!(HERRAMIENTAS_LECTURA.contains(&"consultar_plantilla"));
    }

    #[tokio::test]
    async fn las_herramientas_de_escritura_estan_registradas() {
        let tools = ArgusMcp::tool_router().list_all();
        let nombres: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        for n in [
            "enviar_mensaje",
            "editar_mensaje",
            "crear_post_foro",
            "editar_etiquetas_foro",
            "etiquetar_post",
            "crear_evento",
            "editar_evento",
            "cancelar_evento",
            "crear_encuesta",
            "resultados_encuesta",
            "cerrar_encuesta",
            "reaccionar",
            "quitar_reaccion",
            "enviar_dm",
            "borrar_mensaje",
            "borrar_mensajes",
            "fijar_mensaje",
            "desfijar_mensaje",
            "crear_canal",
            "editar_canal",
            "borrar_canal",
            "ordenar_canales",
            "crear_rol",
            "editar_rol",
            "borrar_rol",
            "ordenar_roles",
            "dar_rol",
            "quitar_rol",
            "permiso_canal",
            "quitar_permiso_canal",
        ] {
            assert!(nombres.contains(&n.to_string()), "falta {n}");
        }
        let crear = tools.iter().find(|t| t.name == "crear_rol").unwrap();
        assert!(crear.input_schema["properties"]["permisos"].is_object());
    }

    #[test]
    fn cada_herramienta_esta_en_exactamente_una_lista() {
        let registradas: Vec<String> = ArgusMcp::tool_router()
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        for n in &registradas {
            let veces = HERRAMIENTAS_LECTURA.iter().filter(|x| **x == n).count()
                + HERRAMIENTAS_ESCRITURA.iter().filter(|x| **x == n).count();
            assert_eq!(veces, 1, "{n} debe estar en exactamente una lista");
        }
        for n in HERRAMIENTAS_LECTURA.iter().chain(HERRAMIENTAS_ESCRITURA) {
            assert!(registradas.iter().any(|r| r == n), "{n} no está registrada");
        }
    }

    #[test]
    fn las_herramientas_de_lectura_no_modifican_nada() {
        const PERMITIDAS: [&str; 6] = [
            "informe_permisos",
            "estado",
            "mapa_servidor",
            "permisos_bot",
            "ajustes_servidor",
            "registro_auditoria",
        ];
        for n in HERRAMIENTAS_LECTURA {
            let ok = PERMITIDAS.contains(n)
                || *n == "listar_servidores"
                || *n == "leer_mensajes"
                || *n == "buscar_mensajes"
                || *n == "etiquetas_foro"
                || *n == "resultados_encuesta"
                || *n == "comparar_snapshot"
                || *n == "consultar_plantilla"
                || *n == "proponer_plantilla"
                || *n == "proponer_correccion"
                || n.starts_with("listar_")
                || n.starts_with("info_");
            assert!(ok, "{n} no es una herramienta de lectura conocida");
            assert!(
                !HERRAMIENTAS_ESCRITURA.contains(n),
                "{n} está en ambas listas"
            );
        }
        for n in HERRAMIENTAS_ESCRITURA {
            assert!(!HERRAMIENTAS_LECTURA.contains(n));
        }
    }

    #[test]
    fn auditoria_herencia_overwrites_y_administrator() {
        use crate::state::{Channel, Member, Overwrite, Role};
        let role = |id: &str, bits: u64| Role {
            id: id.into(),
            name: id.into(),
            color: 0,
            position: 0,
            permissions: bits.to_string(),
            permissions_bitfield: Some(bits.to_string()),
            member_count: 0,
            mentionable: false,
            hoist: false,
        };
        let mut roles = vec![role("guild", 1 << 10), role("role", 1 << 11)];
        let member = Member {
            user_id: "member".into(),
            username: "member".into(),
            nickname: None,
            roles: vec!["role".into()],
            bot: false,
            joined_at: None,
            avatar: None,
            flags: None,
        };
        let overwrite = |id: &str, type_, allow: u64, deny: u64| Overwrite {
            id: id.into(),
            type_,
            allow: allow.to_string(),
            deny: deny.to_string(),
        };
        let mut channel = Channel {
            id: "channel".into(),
            name: "channel".into(),
            channel_type: 0,
            parent_id: Some("category".into()),
            position: 0,
            overwrites: Some(vec![]),
            permissions_bitfield: None,
            topic: None,
        };
        let calculate = |roles: &[Role], channel: &Channel| {
            audit_channel_permissions("guild", Some("owner"), &member, roles, channel)
        };
        assert_eq!(
            calculate(&roles, &channel) & ((1 << 10) | (1 << 11)),
            (1 << 10) | (1 << 11)
        );
        channel.overwrites = Some(vec![
            overwrite("guild", 0, 0, 1 << 10),
            overwrite("role", 0, (1 << 10) | (1 << 13), 1 << 11),
            overwrite("member", 1, 1 << 11, 1 << 13),
        ]);
        let bits = calculate(&roles, &channel);
        assert_ne!(bits & (1 << 10), 0);
        assert_ne!(bits & (1 << 11), 0);
        assert_eq!(bits & (1 << 13), 0);
        roles[1] = role("role", 1 << 3);
        assert_ne!(calculate(&roles, &channel) & (1 << 13), 0);
    }

    #[tokio::test]
    async fn informe_permisos_es_parcial_y_solo_lee() {
        let (_d, mcp, server) = mcp_con_servidor(TODO, true).await;
        prohibir_cambios(&server).await;
        let inicio = server.received_requests().await.unwrap().len();
        let informe = v(mcp.informe_permisos_json(None).await.unwrap());
        assert_eq!(informe["status"], "partial");
        assert!(informe["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["permission"] == "ManageMessages"));
        assert!(informe["channel_overwrites"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["allow"] == "1024"));
        assert!(informe["limits"]
            .to_string()
            .contains("does not establish security"));
        assert!(informe["correction_workflow"]
            .as_str()
            .unwrap()
            .contains("Apply"));
        assert!(HERRAMIENTAS_LECTURA.contains(&"informe_permisos"));
        assert!(validar_accion("informe_permisos", &json!({})).is_err());
        let requests = server.received_requests().await.unwrap();
        assert!(requests[inicio..]
            .iter()
            .all(|r| r.method.as_str() == "GET"));
        assert_eq!(mcp.state.get_active_guild().as_deref(), Some("555"));
    }

    #[tokio::test]
    async fn correccion_de_rol_sale_del_motor_sin_tocar_discord() {
        let (_d, mcp, server) = mcp_con_servidor(TODO, true).await;
        prohibir_cambios(&server).await;
        let informe = v(mcp.informe_permisos_json(None).await.unwrap());
        assert_eq!(informe["incomplete"], false, "{informe}");
        let hallazgo = informe["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["tipo"] == "role_grant" && f["role_id"] == "700")
            .unwrap()
            .clone();
        let c = mcp
            .preparar_correccion_json("555", &hallazgo)
            .await
            .unwrap();
        let accion = &c["propuesta"]["acciones"][0];
        assert_eq!(accion["herramienta"], "editar_rol");
        assert_eq!(accion["args"]["role_id"], "700");
        assert_eq!(accion["args"]["permisos"], "0");
        assert_eq!(c["antes"], json!(["ManageMessages"]));
        assert_eq!(c["despues"], json!([]));
        assert!(
            c["validacion"]["errores"].as_array().unwrap().is_empty(),
            "{c}"
        );
        // Preparar no escribe nada: la propuesta espera la aprobación.
        assert!(server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method.as_str() == "GET"));
        // Un hallazgo que ya no está (descartado o resuelto) no se prepara.
        let mut viejo = hallazgo.clone();
        viejo["permission_bit"] = json!(3);
        assert!(mcp
            .preparar_correccion_json("555", &viejo)
            .await
            .unwrap_err()
            .contains("no longer present"));
    }

    #[tokio::test]
    async fn correccion_que_supera_la_jerarquia_del_bot_no_se_ofrece() {
        let (_d, mcp, server) = mcp_con_servidor(TODO, true).await;
        prohibir_cambios(&server).await;
        // «Alto» está por encima del rol del bot y concede ManageMessages.
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                rol("555", "@everyone", 0, 0),
                rol("600", "Bot", 5, TODO),
                rol("700", "Moderadores", 3, 0),
                rol("900", "Alto", 9, 8192)
            ])))
            .with_priority(1)
            .mount(&server)
            .await;
        let informe = v(mcp.informe_permisos_json(None).await.unwrap());
        let hallazgo = informe["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["tipo"] == "role_grant" && f["role_id"] == "900")
            .unwrap()
            .clone();
        let e = mcp
            .preparar_correccion_json("555", &hallazgo)
            .await
            .unwrap_err();
        assert!(e.contains("above the bot"), "{e}");
    }

    #[tokio::test]
    async fn correccion_sin_gestionar_roles_no_se_ofrece() {
        let (_d, mcp, server) = mcp_con_servidor(GESTIONAR_CANALES, true).await;
        prohibir_cambios(&server).await;
        let hallazgo = json!({"tipo":"role_grant","role_id":"700","permission_bit":13});
        let informe = v(mcp.informe_permisos_json(None).await.unwrap());
        if informe["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["role_id"] == "700" && f["permission_bit"] == 13)
        {
            let e = mcp
                .preparar_correccion_json("555", &hallazgo)
                .await
                .unwrap_err();
            assert!(e.contains("Gestionar roles"), "{e}");
            assert!(server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.method.as_str() == "GET"));
        } else {
            panic!("fixture should report the Moderadores grant: {informe}");
        }
    }

    #[tokio::test]
    async fn auditoria_compara_solo_expectativas_explicitas() {
        let (_dir, mcp, server) = mcp_con_servidor(TODO, true).await;
        prohibir_cambios(&server).await;
        let expectation = |member: &str| ExpectativaPrivacidad {
            channel_id: "11".into(),
            member_id: member.into(),
            can_view: false,
            source: FuenteExpectativa::Plan,
            evidence: "Plan: restricted access".into(),
        };
        let fulfilled = v(mcp
            .informe_permisos_con_expectativas_json(None, &[expectation("1002")])
            .await
            .unwrap());
        assert!(!fulfilled["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["tipo"] == "privacy_discrepancy"));
        let violated = v(mcp
            .informe_permisos_con_expectativas_json(None, &[expectation("1001")])
            .await
            .unwrap());
        let findings = violated["findings"].as_array().unwrap();
        let finding = findings
            .iter()
            .find(|f| f["tipo"] == "privacy_discrepancy")
            .unwrap();
        assert_eq!(finding["channel_id"], "11");
        assert_eq!(finding["member_id"], "1001");
        assert_eq!(finding["expectation_source"], "plan");
        assert_eq!(finding["expectation_evidence"], "Plan: restricted access");
        assert_eq!(finding["actual_can_view"], true);
        assert_eq!(finding["expected_can_view"], false);
        assert!(finding["permissions_bits"].as_str().is_some());
        assert!(
            chrono::DateTime::parse_from_rfc3339(finding["checked_at"].as_str().unwrap()).is_ok()
        );
        assert!(findings
            .iter()
            .all(|f| f["checked_at"] == violated["checked_at"]));
        Mock::given(method("GET")).and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id":"11","type":0,"name":"private","position":0,"permission_overwrites":[{"id":"700","type":0,"allow":"1024","deny":"0"}]}])))
            .with_priority(1).mount(&server).await;
        let no_expectation = v(mcp.informe_permisos_json(None).await.unwrap());
        assert!(no_expectation["channel_access"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["member_id"] == "1001" && a["view"] == true));
        assert!(!no_expectation["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["tipo"] == "privacy_discrepancy"));
    }

    #[tokio::test]
    async fn auditoria_lectura_parcial_no_inventa_accesos() {
        let (_dir, mcp, server) = mcp_con_servidor(TODO, true).await;
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/members"))
            .respond_with(ResponseTemplate::new(403))
            .with_priority(1)
            .mount(&server)
            .await;
        let report = v(mcp.informe_permisos_json(None).await.unwrap());
        assert_eq!(report["incomplete"], true);
        assert!(report["missing_data"].to_string().contains("members"));
        assert!(report["channel_access"].as_array().unwrap().is_empty());
        assert!(report["bot_missing_build_permissions"].is_null());
    }

    #[tokio::test]
    async fn auditoria_resalta_permisos_sensibles_del_overwrite() {
        let (_dir, mcp, server) = mcp_con_servidor(TODO, true).await;
        let bits = (1_u64 << 13) | (1 << 17) | (1 << 29);
        Mock::given(method("GET")).and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id":"10","type":0,"name":"test","position":0,"permission_overwrites":[{"id":"700","type":0,"allow":bits.to_string(),"deny":"0"}]}])))
            .with_priority(1).mount(&server).await;
        let report = v(mcp.informe_permisos_json(None).await.unwrap());
        for permission in ["ManageMessages", "MentionEveryone", "ManageWebhooks"] {
            assert!(report["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["tipo"] == "channel_overwrite_grant" && f["permission"] == permission));
        }
    }

    // ------------------------- moderación, servidor, invitaciones y webhooks

    const KICK: u64 = 1 << 1;
    const BAN: u64 = 1 << 2;
    const GESTIONAR_SERVIDOR: u64 = 1 << 5;
    const CREAR_INVITACION: u64 = 1;
    const GESTIONAR_APODOS: u64 = 1 << 27;
    const GESTIONAR_WEBHOOKS: u64 = 1 << 29;
    const MODERAR: u64 = 1 << 40;
    const DUENO: &str = "That cannot be done to the server owner";
    const JERARQUIA_MIEMBRO: &str = "That member has a role equal to or above the bot's: move the bot's role up in Server settings → Roles";

    fn expulsar_a(user: &str) -> ExpulsarParams {
        ExpulsarParams {
            guild_id: None,
            user_id: user.into(),
            motivo: None,
        }
    }

    fn banear_a(user: &str, dias: Option<u32>) -> BanearParams {
        BanearParams {
            guild_id: None,
            user_id: user.into(),
            motivo: None,
            dias_mensajes: dias,
        }
    }

    fn silenciar_a(user: &str, minutos: u32) -> SilenciarParams {
        SilenciarParams {
            guild_id: None,
            user_id: user.into(),
            minutos,
            motivo: None,
        }
    }

    fn apodo_de(user: &str, apodo: Option<&str>) -> CambiarApodoParams {
        CambiarApodoParams {
            guild_id: None,
            user_id: user.into(),
            apodo: apodo.map(Into::into),
            motivo: None,
        }
    }

    fn servidor_vacio() -> EditarServidorParams {
        EditarServidorParams {
            guild_id: None,
            nombre: None,
            descripcion: None,
            nivel_verificacion: None,
            filtro_contenido_explicito: None,
            notificaciones_por_defecto: None,
            canal_del_sistema_id: None,
            canal_afk_id: None,
            afk_segundos: None,
        }
    }

    #[tokio::test]
    async fn modera_con_el_motivo_en_la_auditoria() {
        let (_d, mcp, s) = mcp_con_servidor(KICK | BAN | MODERAR | GESTIONAR_APODOS, true).await;
        Mock::given(method("DELETE"))
            .and(path("/api/v10/guilds/555/members/1001"))
            .and(header("x-audit-log-reason", "spam"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/guilds/555/bans/1001"))
            .and(header("x-audit-log-reason", "insultos"))
            .and(wiremock::matchers::body_partial_json(
                json!({"delete_message_seconds": 2 * 86400}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/v10/guilds/555/bans/1004"))
            .and(header("x-audit-log-reason", "perdonado"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;
        let ana = miembro("1001", "ana", Some("Anita"), &["700"]);
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/members/1001"))
            .and(header("x-audit-log-reason", "calma"))
            .and(wiremock::matchers::body_partial_json(
                json!({"communication_disabled_until": null}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(ana.clone()))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555/members/1001"))
            .and(header("x-audit-log-reason", "normas"))
            .and(wiremock::matchers::body_partial_json(
                json!({"nick": "Nuevo"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(ana))
            .expect(1)
            .mount(&s)
            .await;

        let mut p = expulsar_a("1001");
        p.motivo = Some("spam".into());
        assert!(mcp.expulsar_json(&p).await.unwrap().contains("kicked"));
        let mut p = banear_a("1001", Some(2));
        p.motivo = Some("insultos".into());
        assert!(mcp.banear_json(&p).await.unwrap().contains("banned"));
        let r = mcp
            .desbanear_json(None, "1004", Some("perdonado"))
            .await
            .unwrap();
        assert!(r.contains("Ban of 1004 removed"));
        let mut p = silenciar_a("1001", 0);
        p.motivo = Some("calma".into());
        assert!(mcp.silenciar_json(&p).await.unwrap().contains("removed"));
        let mut p = apodo_de("1001", Some("Nuevo"));
        p.motivo = Some("normas".into());
        assert!(mcp
            .cambiar_apodo_json(&p)
            .await
            .unwrap()
            .contains("Nickname"));
    }

    #[tokio::test]
    async fn silencia_con_duracion() {
        let (_d, mcp, s) = mcp_con_servidor(MODERAR, true).await;
        let ana = miembro("1001", "ana", Some("Anita"), &["700"]);
        aceptar(
            &s,
            "PATCH",
            "/api/v10/guilds/555/members/1001",
            200,
            Some(ana),
            1,
        )
        .await;
        let r = mcp.silenciar_json(&silenciar_a("1001", 10)).await.unwrap();
        assert!(r.contains("timed out for 10 minutes"), "{r}");
        let peticiones = s.received_requests().await.unwrap();
        let patch = peticiones
            .iter()
            .find(|r| r.method.as_str() == "PATCH")
            .unwrap();
        let cuerpo: serde_json::Value = serde_json::from_slice(&patch.body).unwrap();
        assert!(cuerpo["communication_disabled_until"].is_string());
        // Más de 28 días: error de la app, sin llamar a Discord.
        let e = mcp
            .silenciar_json(&silenciar_a("1001", 40321))
            .await
            .unwrap_err();
        assert!(e.contains("28 days"), "{e}");
    }

    #[tokio::test]
    async fn moderacion_rechaza_dueno_y_jerarquia_con_el_mensaje_de_la_app() {
        let (_d, mcp, s) = mcp_con_servidor(KICK | BAN | MODERAR | GESTIONAR_APODOS, true).await;
        con_rol_alto(&s, KICK | BAN | MODERAR | GESTIONAR_APODOS).await;
        get_json(
            &s,
            "/api/v10/guilds/555/members/1002",
            miembro("1002", "beto", None, &["900"]),
        )
        .await;
        prohibir_cambios(&s).await;
        // El dueño es «9».
        assert_eq!(
            mcp.expulsar_json(&expulsar_a("9")).await.unwrap_err(),
            DUENO
        );
        assert_eq!(
            mcp.banear_json(&banear_a("9", None)).await.unwrap_err(),
            DUENO
        );
        assert_eq!(
            mcp.silenciar_json(&silenciar_a("9", 5)).await.unwrap_err(),
            DUENO
        );
        let e = mcp.cambiar_apodo_json(&apodo_de("9", Some("x"))).await;
        assert_eq!(e.unwrap_err(), DUENO);
        // «1002» tiene un rol por encima del bot.
        assert_eq!(
            mcp.expulsar_json(&expulsar_a("1002")).await.unwrap_err(),
            JERARQUIA_MIEMBRO
        );
        assert_eq!(
            mcp.banear_json(&banear_a("1002", None)).await.unwrap_err(),
            JERARQUIA_MIEMBRO
        );
        assert_eq!(
            mcp.silenciar_json(&silenciar_a("1002", 5))
                .await
                .unwrap_err(),
            JERARQUIA_MIEMBRO
        );
        let e = mcp.cambiar_apodo_json(&apodo_de("1002", Some("x"))).await;
        assert_eq!(e.unwrap_err(), JERARQUIA_MIEMBRO);
        // Días de mensajes fuera de 0–7.
        let e = mcp.banear_json(&banear_a("1001", Some(8))).await;
        assert_eq!(
            e.unwrap_err(),
            "The days of messages to delete must be between 0 and 7"
        );
        let e = mcp.banear_json(&banear_a("1001", Some(300))).await;
        assert!(e.unwrap_err().contains("between 0 and 7"));
    }

    #[tokio::test]
    async fn moderacion_sin_permiso_no_llega_a_discord() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let falta = |p: &str| format!("The bot is missing the permission «{p}»");
        let e = mcp.expulsar_json(&expulsar_a("1001")).await;
        assert_eq!(e.unwrap_err(), falta("Expulsar miembros"));
        let e = mcp.banear_json(&banear_a("1001", None)).await;
        assert_eq!(e.unwrap_err(), falta("Banear miembros"));
        let e = mcp.desbanear_json(None, "1004", None).await;
        assert_eq!(e.unwrap_err(), falta("Banear miembros"));
        let e = mcp.silenciar_json(&silenciar_a("1001", 5)).await;
        assert_eq!(e.unwrap_err(), falta("Moderar miembros"));
        let e = mcp.cambiar_apodo_json(&apodo_de("1001", Some("x"))).await;
        assert_eq!(e.unwrap_err(), falta("Gestionar apodos"));
        let e = mcp.editar_servidor_json(&servidor_vacio()).await;
        assert_eq!(e.unwrap_err(), falta("Gestionar servidor"));
        let p = CrearInvitacionParams {
            guild_id: None,
            channel_id: "11".into(),
            caducidad_segundos: None,
            usos_maximos: None,
        };
        let e = mcp.crear_invitacion_json(&p).await;
        assert_eq!(e.unwrap_err(), falta("Crear invitaciones"));
        let e = mcp.borrar_invitacion_json(None, "abc123").await;
        assert_eq!(e.unwrap_err(), falta("Gestionar servidor"));
        let p = CrearWebhookParams {
            guild_id: None,
            channel_id: "11".into(),
            nombre: "Avisos".into(),
        };
        let e = mcp.crear_webhook_json(&p).await;
        assert_eq!(e.unwrap_err(), falta("Gestionar webhooks"));
        let e = mcp.borrar_webhook_json(None, "900").await;
        assert_eq!(e.unwrap_err(), falta("Gestionar webhooks"));
    }

    #[tokio::test]
    async fn edita_el_servidor_cambiando_solo_los_campos_indicados() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_SERVIDOR, true).await;
        let guild = json!({
            "id": "555", "name": "Taller 2", "owner_id": "9", "description": null,
            "roles": [], "emojis": [], "features": [], "afk_timeout": 300,
            "afk_channel_id": null, "system_channel_id": "11",
            "default_message_notifications": 1, "explicit_content_filter": 2,
            "verification_level": 1, "mfa_level": 0, "nsfw_level": 0,
            "premium_tier": 0, "system_channel_flags": 0
        });
        Mock::given(method("PATCH"))
            .and(path("/api/v10/guilds/555"))
            .and(wiremock::matchers::body_partial_json(json!({
                "name": "Taller 2", "verification_level": 1, "afk_channel_id": null,
                "system_channel_id": "11", "explicit_content_filter": 2
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(guild))
            .expect(1)
            .mount(&s)
            .await;
        let mut p = servidor_vacio();
        p.nombre = Some("Taller 2".into());
        p.nivel_verificacion = Some(1);
        p.canal_afk_id = Some(String::new());
        let r = v(mcp.editar_servidor_json(&p).await.unwrap());
        assert_eq!(r["servidor"]["nombre"], "Taller 2");
        assert_eq!(r["servidor"]["nivel_verificacion"], 1);
        assert!(r["servidor"]["canal_afk_id"].is_null());
        // Validaciones de la app: sin llamar a Discord.
        let mut p = servidor_vacio();
        p.nivel_verificacion = Some(9);
        assert!(mcp.editar_servidor_json(&p).await.is_err());
        let mut p = servidor_vacio();
        p.nombre = Some("x".into());
        assert!(mcp.editar_servidor_json(&p).await.is_err());
    }

    #[tokio::test]
    async fn crea_y_borra_invitaciones() {
        let (_d, mcp, s) = mcp_con_servidor(CREAR_INVITACION | GESTIONAR_SERVIDOR, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/11/invites"))
            .and(wiremock::matchers::body_partial_json(
                json!({"max_age": 3600, "max_uses": 5}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"code": "abc123", "uses": 0, "max_uses": 5, "max_age": 3600,
                       "channel": {"id": "11", "name": "general"}}),
            ))
            .expect(1)
            .mount(&s)
            .await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/invites/abc123",
            200,
            Some(json!({"code": "abc123"})),
            1,
        )
        .await;
        let p = CrearInvitacionParams {
            guild_id: None,
            channel_id: "11".into(),
            caducidad_segundos: Some(3600),
            usos_maximos: Some(5),
        };
        let r = v(mcp.crear_invitacion_json(&p).await.unwrap());
        assert_eq!(r["invitacion"]["code"], "abc123");
        assert_eq!(r["invitacion"]["url"], "https://discord.gg/abc123");
        let r = mcp.borrar_invitacion_json(None, "abc123").await.unwrap();
        assert!(r.contains("abc123"));
        // Más de 7 días: error de la app.
        let p = CrearInvitacionParams {
            caducidad_segundos: Some(604_801),
            ..p
        };
        let e = mcp.crear_invitacion_json(&p).await.unwrap_err();
        assert!(e.contains("7 days"), "{e}");
    }

    #[tokio::test]
    async fn eventos_sin_permiso_no_llegan_a_discord_y_con_permiso_si() {
        let falta = "The bot is missing the permission «Crear/Gestionar eventos»";
        let crear = |inicio: &str| CrearEventoParams {
            guild_id: None,
            nombre: "Torneo".into(),
            descripcion: None,
            inicio: inicio.into(),
            fin: Some("2099-05-01T22:00:00+02:00".into()),
            lugar: Some("Plaza".into()),
            canal_id: None,
        };
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let e = mcp
            .crear_evento_json(&crear("2099-05-01T20:00:00+02:00"))
            .await;
        assert_eq!(e.unwrap_err(), falta);
        let e = mcp.cancelar_evento_json(None, "9").await;
        assert_eq!(e.unwrap_err(), falta);

        let (_d, mcp, s) = mcp_con_servidor(1 << 33, true).await;
        aceptar(
            &s,
            "DELETE",
            "/api/v10/guilds/555/scheduled-events/9",
            204,
            None,
            1,
        )
        .await;
        let r = mcp.cancelar_evento_json(None, "9").await.unwrap();
        assert!(r.contains('9'), "{r}");
        let e = mcp
            .crear_evento_json(&crear("2001-05-01T20:00:00+02:00"))
            .await;
        assert_eq!(e.unwrap_err(), "The event start must be in the future");
    }

    #[tokio::test]
    async fn encuestas_sin_permiso_no_llegan_a_discord_y_con_permiso_si() {
        let params = CrearEncuestaParams {
            guild_id: None,
            channel_id: "30".into(),
            pregunta: "¿Pizza?".into(),
            respuestas: vec![
                RespuestaEncuestaParams {
                    texto: "Sí".into(),
                    emoji: Some("🍕".into()),
                },
                RespuestaEncuestaParams {
                    texto: "No".into(),
                    emoji: None,
                },
            ],
            duracion_horas: None,
            voto_multiple: None,
        };
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let e = mcp.crear_encuesta_json(&params).await;
        assert_eq!(
            e.unwrap_err(),
            "The bot is missing the permission «Crear encuestas»"
        );

        let (_d, mcp, s) = mcp_con_servidor(1 << 49, true).await;
        aceptar(
            &s,
            "POST",
            "/api/v10/channels/30/messages",
            200,
            Some(json!({"id": "88"})),
            1,
        )
        .await;
        let r = mcp.crear_encuesta_json(&params).await.unwrap();
        assert!(r.contains("88"), "{r}");
        let mut mala = params;
        mala.duracion_horas = Some(769);
        assert_eq!(
            mcp.crear_encuesta_json(&mala).await.unwrap_err(),
            "The poll duration must be between 1 and 768 hours"
        );
    }

    #[tokio::test]
    async fn crea_y_borra_webhooks_y_solo_al_crear_da_la_url() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_WEBHOOKS, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/11/webhooks"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Avisos"}),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id": "901", "name": "Avisos", "channel_id": "11",
                       "token": WEBHOOK_TOKEN, "type": 1})),
            )
            .expect(1)
            .mount(&s)
            .await;
        aceptar(&s, "DELETE", "/api/v10/webhooks/900", 204, None, 1).await;
        let p = CrearWebhookParams {
            guild_id: None,
            channel_id: "11".into(),
            nombre: "Avisos".into(),
        };
        let r = v(mcp.crear_webhook_json(&p).await.unwrap());
        assert_eq!(r["webhook"]["id"], "901");
        assert_eq!(
            r["webhook"]["url"],
            format!("https://discord.com/api/webhooks/901/{WEBHOOK_TOKEN}")
        );
        let r = mcp.borrar_webhook_json(None, "900").await.unwrap();
        assert!(r.contains("900"));
        // Nombre vacío: error de la app.
        let vacio = CrearWebhookParams {
            nombre: "  ".into(),
            ..p
        };
        assert!(mcp.crear_webhook_json(&vacio).await.is_err());
        // El listado sigue sin token.
        let lista = mcp.listar_webhooks_json(None).await.unwrap();
        assert!(!lista.contains(WEBHOOK_TOKEN));
    }

    #[tokio::test]
    async fn las_herramientas_de_moderacion_estan_registradas() {
        let tools = ArgusMcp::tool_router().list_all();
        let nombres: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        for n in [
            "expulsar",
            "banear",
            "desbanear",
            "silenciar",
            "cambiar_apodo",
            "editar_servidor",
            "crear_invitacion",
            "borrar_invitacion",
            "crear_webhook",
            "borrar_webhook",
        ] {
            assert!(nombres.contains(&n.to_string()), "falta {n}");
        }
        let banear = tools.iter().find(|t| t.name == "banear").unwrap();
        assert!(banear.input_schema["properties"]["dias_mensajes"].is_object());
        assert!(banear.input_schema["properties"]["motivo"].is_object());
        let silenciar = tools.iter().find(|t| t.name == "silenciar").unwrap();
        assert!(silenciar.input_schema["properties"]["minutos"].is_object());
    }

    #[tokio::test]
    async fn ejecutar_accion_usa_la_logica_del_mcp() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES | GESTIONAR_ROLES, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Mods", "permissions": "9216"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1000", "Mods", 1, 9216)))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "avisos", "type": 0, "topic": "t"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id": "20", "type": 0, "guild_id": "555", "name": "avisos", "position": 4}),
            ))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/11/permissions/700"))
            .and(wiremock::matchers::body_partial_json(
                json!({"allow": "9216", "deny": "2048", "type": 0}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;

        let r = v(mcp
            .ejecutar_accion(
                "crear_rol",
                json!({"nombre": "Mods", "permisos": ["ManageMessages", "ViewChannel"]}),
            )
            .await
            .unwrap());
        assert_eq!(r["rol"]["id"], "1000");
        let r = v(mcp
            .ejecutar_accion("crear_canal", json!({"nombre": "avisos", "tema": "t"}))
            .await
            .unwrap());
        assert_eq!(r["canal"]["id"], "20");
        let r = mcp
            .ejecutar_accion(
                "permiso_canal",
                json!({"channel_id": "11", "target_id": "700", "tipo": "rol",
                       "permitir": ["ManageMessages", "ViewChannel"], "denegar": "2048"}),
            )
            .await
            .unwrap();
        assert!(r.contains("updated"));

        // Nombre inventado, herramienta de lectura y argumentos inválidos.
        let e = mcp.ejecutar_accion("inventada", json!({})).await;
        assert!(e.unwrap_err().contains("Unknown action"));
        let e = mcp.ejecutar_accion("listar_roles", json!({})).await;
        assert!(e.unwrap_err().contains("read-only"));
        let e = mcp.ejecutar_accion("crear_rol", json!({"color": 1})).await;
        assert!(e.unwrap_err().contains("Invalid arguments"));
    }

    #[tokio::test]
    async fn ejecutar_propuesta_resuelve_nombres_a_ids() {
        // Como en la app: el MCP sin servidor activo; la app pone su guild_id.
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES | GESTIONAR_ROLES, false).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Nueva", "type": 4}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id": "30", "type": 4, "guild_id": "555", "name": "Nueva", "position": 5}),
            ))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "avisos", "type": 0, "parent_id": "30"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id": "31", "type": 0, "guild_id": "555", "name": "avisos", "position": 6,
                       "parent_id": "30"}),
            ))
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/31/permissions/700"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&s)
            .await;

        let acc = |h: &str, args: serde_json::Value| crate::propuesta::Accion {
            herramienta: h.to_string(),
            args,
            descripcion: h.to_string(),
        };
        // Desordenada: el permiso y el canal van antes que su categoría.
        let p = Propuesta {
            resumen: "x".to_string(),
            acciones: vec![
                acc(
                    "permiso_canal",
                    json!({"channel_id": "avisos", "target_id": "Moderadores", "tipo": "rol",
                           "permitir": "1024"}),
                ),
                acc("dar_rol", json!({"user_id": "1001", "role_id": "Fantasma"})),
                acc(
                    "crear_canal",
                    json!({"nombre": "avisos", "parent": "Nueva"}),
                ),
                acc(
                    "crear_canal",
                    json!({"nombre": "Nueva", "tipo": "categoria"}),
                ),
            ],
        };
        let p = crate::propuesta::con_servidor(&p, "555");
        let r = mcp.ejecutar_propuesta(&p).await.unwrap();
        let h: Vec<_> = r.iter().map(|x| x.herramienta.as_str()).collect();
        assert_eq!(
            h,
            ["crear_canal", "crear_canal", "permiso_canal", "dar_rol"]
        );
        assert_eq!(r[0].descripcion, "crear_canal");
        assert!(r[..3].iter().all(|x| x.estado == EstadoAccion::Ok), "{r:?}");
        // La referencia que no se resuelve se omite, sin tocar Discord.
        assert_eq!(r[3].estado, EstadoAccion::Omitida);
        assert!(r[3].mensaje.contains("«Fantasma»"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cambio_de_servidor_durante_propuesta_sin_guild_id() {
        let (_dir, mcp, discord) = mcp_con_servidor(GESTIONAR_CANALES, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id":"31", "type":0, "guild_id":"555", "name":"test", "position":1
            })))
            .expect(2)
            .mount(&discord)
            .await;
        let propuesta = Propuesta {
            resumen: String::new(),
            acciones: vec![
                accion("crear_canal", json!({"nombre":"uno"})),
                accion("crear_canal", json!({"nombre":"dos"})),
            ],
        };
        let state = mcp.state.clone();
        let pool = mcp.pool.clone();
        let config = state.config.lock().unwrap().clone();
        let bot_original = config.active_bot_id.clone().unwrap();
        assert_eq!(config.active_guild_id.as_deref(), Some("555"));
        let mut cambiado = false;
        let resultado = mcp
            .ejecutar_propuesta_con(
                &propuesta,
                &AtomicBool::new(false),
                &mut |total, resultado| {
                    if !cambiado {
                        assert_eq!(total, 2);
                        assert_eq!(resultado.indice, 0);
                        assert_eq!(resultado.estado, EstadoAccion::Ok);
                        cambiado = true;
                        assert_eq!(state.get_active_guild().as_deref(), Some("555"));
                        state.set_active_guild("556");
                        let state = state.clone();
                        let pool = pool.clone();
                        std::thread::spawn(move || {
                            tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .unwrap()
                                .block_on(state.save_config_to_pool(&pool))
                                .unwrap();
                        })
                        .join()
                        .unwrap();
                    }
                },
            )
            .await
            .unwrap();
        assert!(cambiado);
        assert_eq!(state.get_active_guild().as_deref(), Some("556"));
        assert_eq!(resultado.hechas, 2);
        let writes: Vec<_> = discord
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.method.as_str() == "POST")
            .map(|r| r.url.path().to_string())
            .collect();
        assert_eq!(writes, vec!["/api/v10/guilds/555/channels"; 2]);
        let history = crate::operaciones::listar(&mcp.pool, &bot_original, "555")
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "unverified");
        assert!(history[0].steps.iter().all(|s| s.status == "unverified"));
        assert!(crate::operaciones::listar(&mcp.pool, &bot_original, "556")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn crear_canal_permisos_combina_categoria_y_rol_nuevo() {
        let (_d, mcp, s) = mcp_con_servidor(TODO, true).await;
        let creado = std::sync::Arc::new(AtomicBool::new(false));
        let para_roles = creado.clone();
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(move |_: &wiremock::Request| {
                let mut roles = vec![
                    rol("555", "@everyone", 0, 0),
                    rol("600", "Bot", 5, TODO),
                    rol("700", "Moderadores", 3, 8192),
                ];
                if para_roles.load(Ordering::SeqCst) {
                    roles.push(rol("1200", "Nuevo", 1, 0));
                }
                ResponseTemplate::new(200).set_body_json(roles)
            })
            .with_priority(1)
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(move |_: &wiremock::Request| {
                creado.store(true, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(rol("1200", "Nuevo", 1, 0))
            })
            .expect(1)
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v10/channels/20"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "20", "type": 4, "guild_id": "555", "name": "Categoria",
                "permission_overwrites": [
                    {"id": "555", "type": 0, "allow": "0", "deny": "1024"},
                    {"id": "700", "type": 0, "allow": "1024", "deny": "0"}
                ]
            })))
            .with_priority(1)
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_json(
                json!({"name": "privado", "type": 0, "parent_id": "20",
                "permission_overwrites": [
                    {"id": "700", "type": 0, "allow": "1024", "deny": "0"},
                    {"id": "555", "type": 0, "allow": "1024", "deny": "0"},
                    {"id": "1200", "type": 0, "allow": "2048", "deny": "0"}
                ]}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "50", "type": 0, "guild_id": "555", "name": "privado", "parent_id": "20"
            })))
            .expect(1)
            .mount(&s)
            .await;
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![
                accion("crear_rol", json!({"nombre": "Nuevo"})),
                accion(
                    "crear_canal",
                    json!({"nombre": "privado", "categoria_id": "20", "permisos": [
                        {"objetivo": "@everyone", "tipo": "rol", "permitir": ["ViewChannel"]},
                        {"objetivo": "Nuevo", "tipo": "rol", "permitir": ["SendMessages"]}
                    ]}),
                ),
            ],
        };
        assert!(crate::propuesta::validar_acciones(&p, &Nombres::default())
            .errores
            .is_empty());
        let r = mcp.ejecutar_propuesta(&p).await.unwrap();
        assert!(r.iter().all(|r| r.estado == EstadoAccion::Ok), "{r:?}");
    }

    #[tokio::test]
    async fn crear_canal_permisos_sin_categoria_resuelve_miembro_y_rol() {
        let (_d, mcp, s) = mcp_con_servidor(TODO, true).await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .and(wiremock::matchers::body_json(
                json!({"name": "privado", "type": 0,
                "permission_overwrites": [
                    {"id": "1001", "type": 1, "allow": "1024", "deny": "2048"},
                    {"id": "700", "type": 0, "allow": "1024", "deny": "0"}
                ]}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "50", "type": 0, "guild_id": "555", "name": "privado"
            })))
            .expect(2)
            .mount(&s)
            .await;
        for objetivo in ["Anita", "1001"] {
            let p = Propuesta {
                resumen: String::new(),
                acciones: vec![accion(
                    "crear_canal",
                    json!({
                        "nombre": "privado", "permisos": [
                            {"objetivo": objetivo, "tipo": "miembro", "permitir": ["ViewChannel"], "denegar": ["SendMessages"]},
                            {"objetivo": "700", "tipo": "rol", "permitir": ["ViewChannel"]}
                        ]
                    }),
                )],
            };
            let r = mcp.ejecutar_propuesta(&p).await.unwrap();
            assert_eq!(r[0].estado, EstadoAccion::Ok, "{r:?}");
        }
    }

    #[tokio::test]
    async fn crear_canal_rechaza_objetivo_inexistente_y_permiso_desconocido() {
        let (_d, mcp, s) = mcp_con_servidor(TODO, true).await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&s)
            .await;
        for objetivo in ["Fantasma", "999999"] {
            let args = json!({"nombre": "privado", "permisos": [
                {"objetivo": objetivo, "tipo": "rol", "permitir": ["ViewChannel"]}
            ]});
            let p = Propuesta {
                resumen: String::new(),
                acciones: vec![accion("crear_canal", args.clone())],
            };
            let v = crate::propuesta::validar_acciones(&p, &Nombres::default());
            assert!(v.errores[0].mensaje.contains(objetivo));
            let e = mcp
                .crear_canal_json(&serde_json::from_value(args).unwrap())
                .await
                .unwrap_err();
            assert!(e.contains(objetivo) && e.contains("does not exist"), "{e}");
        }
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![accion(
                "crear_canal",
                json!({
                    "nombre": "privado", "permisos": [{"objetivo": "@everyone", "tipo": "rol", "denegar": ["ImaginaryPermission"]}]
                }),
            )],
        };
        let v = crate::propuesta::validar_acciones(&p, &Nombres::default());
        assert!(v.errores[0]
            .mensaje
            .contains("Unknown permission: «ImaginaryPermission»"));
        assert!(firmas_escritura().contains("objetivo: nombre o id"));
    }

    /// El Discord falso responde a `GET roles` con los roles de siempre más estos
    /// (los que la propuesta acaba de crear).
    async fn roles_con_nuevos(s: &MockServer, nuevos: &[serde_json::Value]) {
        let mut roles = vec![
            rol("555", "@everyone", 0, 0),
            rol("600", "Bot", 5, TODO),
            rol("700", "Moderadores", 3, 8192),
        ];
        roles.extend(nuevos.iter().cloned());
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(roles))
            .with_priority(1)
            .mount(s)
            .await;
    }

    fn accion(h: &str, args: serde_json::Value) -> crate::propuesta::Accion {
        crate::propuesta::Accion {
            herramienta: h.to_string(),
            descripcion: format!("{h} {args}"),
            args,
        }
    }

    #[tokio::test]
    async fn archivo_de_sitio_19_aplica_con_siete_peticiones_de_escritura_antes_25() {
        let (_d, mcp, s) = mcp_con_servidor(TODO, true).await;
        let creado = std::sync::Arc::new(AtomicBool::new(false));
        let para_roles = creado.clone();
        Mock::given(method("GET"))
            .and(path("/api/v10/guilds/555/roles"))
            .respond_with(move |_: &wiremock::Request| {
                let mut roles = vec![
                    rol("555", "@everyone", 0, 0),
                    rol("600", "Bot", 5, TODO),
                    rol("700", "Muteado", 1, 0),
                ];
                if para_roles.load(Ordering::SeqCst) {
                    roles.push(rol("1200", "Archivista", 2, 0));
                }
                ResponseTemplate::new(200).set_body_json(roles)
            })
            .with_priority(1)
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Archivista"}),
            ))
            .respond_with(move |_: &wiremock::Request| {
                creado.store(true, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(rol("1200", "Archivista", 2, 0))
            })
            .expect(1)
            .mount(&s)
            .await;
        let overwrites = json!([
            {"id": "555", "type": 0, "allow": "0", "deny": "1024"},
            {"id": "1200", "type": 0, "allow": "3072", "deny": "0"},
            {"id": "700", "type": 0, "allow": "0", "deny": "1024"}
        ]);
        for i in 0..=5 {
            let nombre = if i == 0 {
                "Archivo".to_string()
            } else {
                format!("expediente-{i}")
            };
            let tipo = if i == 0 { 4 } else { 0 };
            let mut cuerpo = json!({"name": nombre, "type": tipo,
                "permission_overwrites": overwrites});
            if i > 0 {
                cuerpo["parent_id"] = json!("40");
            }
            let mut respuesta = cuerpo.clone();
            respuesta["id"] = json!((40 + i).to_string());
            respuesta["guild_id"] = json!("555");
            Mock::given(method("POST"))
                .and(path("/api/v10/guilds/555/channels"))
                .and(wiremock::matchers::body_partial_json(cuerpo))
                .respond_with(ResponseTemplate::new(201).set_body_json(respuesta))
                .expect(1)
                .mount(&s)
                .await;
        }
        let roles = commands::leer_roles(&mcp.state, "555", mcp.url())
            .await
            .unwrap();
        let canales = commands::leer_canales(&mcp.state, "555", mcp.url())
            .await
            .unwrap();
        // Mismo contrato que el test del compilador: 1 rol, 1 categoría,
        // 5 canales, con 3 overwrites por categoría/canal (18 en total).
        let plan = crate::plan_estructurado::analizar(
            "# Archivo de Sitio-19\n\
            ## Rol: Archivista\n\
            ## Categoría: Archivo | ver: Archivista | escribir: Archivista | ocultar a: Muteado\n\
            - texto: expediente-{1..5}",
        );
        let compilado = crate::plan_compilar::compilar(
            &plan,
            &crate::plan_compilar::Existentes {
                roles: &roles,
                canales: &canales,
            },
        );
        assert!(compilado.avisos.is_empty(), "{:?}", compilado.avisos);
        let inicio = s.received_requests().await.unwrap().len();
        let resultados = mcp.ejecutar_propuesta(&compilado.propuesta).await.unwrap();
        assert_eq!(resultados.len(), 7);
        assert!(
            resultados.iter().all(|r| r.estado == EstadoAccion::Ok),
            "{resultados:?}"
        );
        let recibidas = s.received_requests().await.unwrap();
        let aplicacion = &recibidas[inicio..];
        let escrituras: Vec<_> = aplicacion
            .iter()
            .filter(|r| r.method.as_str() != "GET")
            .map(|r| format!("{} {}", r.method, r.url.path()))
            .collect();
        // Antes: 7 creaciones + 18 PUT de permisos = 25 escrituras.
        // Contar HTTP real detecta también peticiones extra dentro de una acción.
        // Las lecturas de validación/categoría siguen existiendo, fuera de este presupuesto.
        let lecturas = aplicacion.len() - escrituras.len();
        eprintln!(
            "Archivo de Sitio-19: {} escrituras (antes 25), {lecturas} lecturas",
            escrituras.len()
        );
        assert_eq!(
            escrituras.len(),
            7,
            "Regresión del presupuesto HTTP: {escrituras:?}"
        );
        assert_eq!(escrituras[0], "POST /api/v10/guilds/555/roles");
        assert!(escrituras[1..]
            .iter()
            .all(|r| r == "POST /api/v10/guilds/555/channels"));
    }

    #[tokio::test]
    async fn aplicar_propuesta_crea_en_orden_con_avance() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES | GESTIONAR_ROLES, true).await;
        for (id, nombre) in [("1100", "Staff"), ("1101", "Miembro")] {
            Mock::given(method("POST"))
                .and(path("/api/v10/guilds/555/roles"))
                .and(wiremock::matchers::body_partial_json(
                    json!({"name": nombre}),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(rol(id, nombre, 1, 0)))
                .expect(1)
                .mount(&s)
                .await;
        }
        let canal = |id: &str, tipo: u8, nombre: &str, padre: Option<&str>| {
            let mut c = json!({"id": id, "type": tipo, "guild_id": "555", "name": nombre,
                               "position": 9});
            if let Some(p) = padre {
                c["parent_id"] = json!(p);
            }
            c
        };
        let creados = [
            canal("40", 4, "Comunidad", None),
            canal("41", 0, "bienvenida", Some("40")),
            canal("42", 0, "normas", Some("40")),
            canal("43", 0, "charla", Some("40")),
        ];
        for c in creados {
            let mut cuerpo = json!({"name": c["name"], "type": c["type"]});
            if c["type"] == 0 {
                cuerpo["parent_id"] = json!("40");
            }
            Mock::given(method("POST"))
                .and(path("/api/v10/guilds/555/channels"))
                .and(wiremock::matchers::body_partial_json(cuerpo))
                .respond_with(ResponseTemplate::new(200).set_body_json(c))
                .expect(1)
                .mount(&s)
                .await;
        }
        roles_con_nuevos(
            &s,
            &[rol("1100", "Staff", 1, 0), rol("1101", "Miembro", 1, 0)],
        )
        .await;
        for (canal, rol) in [("41", "1100"), ("42", "1101")] {
            Mock::given(method("PUT"))
                .and(path(format!("/api/v10/channels/{canal}/permissions/{rol}")))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&s)
                .await;
        }

        // Desordenada a propósito: permisos y canales antes que sus roles/categoría.
        let p = Propuesta {
            resumen: "x".to_string(),
            acciones: vec![
                accion(
                    "permiso_canal",
                    json!({"channel_id": "bienvenida", "target_id": "Staff", "tipo": "rol",
                           "permitir": "1024"}),
                ),
                accion(
                    "permiso_canal",
                    json!({"channel_id": "normas", "target_id": "Miembro", "tipo": "rol",
                           "denegar": "2048"}),
                ),
                accion(
                    "crear_canal",
                    json!({"nombre": "bienvenida", "parent": "Comunidad"}),
                ),
                accion(
                    "crear_canal",
                    json!({"nombre": "normas", "parent": "Comunidad"}),
                ),
                accion(
                    "crear_canal",
                    json!({"nombre": "charla", "parent": "Comunidad"}),
                ),
                accion(
                    "crear_canal",
                    json!({"nombre": "Comunidad", "tipo": "categoria"}),
                ),
                accion("crear_rol", json!({"nombre": "Staff"})),
                accion("crear_rol", json!({"nombre": "Miembro"})),
            ],
        };
        let cancelar = AtomicBool::new(false);
        let mut avisos = Vec::new();
        let resumen = mcp
            .ejecutar_propuesta_con(&p, &cancelar, &mut |total, r| {
                avisos.push((total, r.indice, r.estado.clone()))
            })
            .await
            .unwrap();
        assert_eq!(
            (
                resumen.total,
                resumen.hechas,
                resumen.fallidas,
                resumen.omitidas
            ),
            (8, 8, 0, 0),
            "{resumen:?}"
        );
        assert_eq!(avisos.len(), 8);
        assert!(avisos
            .iter()
            .all(|(t, _, e)| *t == 8 && *e == EstadoAccion::Ok));
        // Cada avance lleva la posición ORIGINAL de su acción (la interfaz
        // empareja así cada fila), en el orden en que se ejecutan.
        let indices: Vec<usize> = avisos.iter().map(|(_, i, _)| *i).collect();
        assert_eq!(indices, crate::propuesta::orden_de_ejecucion(&p));
        assert_eq!(p.acciones[indices[0]].herramienta, "crear_rol");
        assert_eq!(p.acciones[indices[2]].args["nombre"], "Comunidad");
        // Orden en Discord: roles, categoría, canales, permisos (los `expect`
        // de los mocks comprueban parent_id y los IDs de los roles creados).
        let orden: Vec<String> = s
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method.as_str() != "GET")
            .map(|r| format!("{} {}", r.method, r.url.path()))
            .collect();
        let dest = |i: usize| orden[i].as_str();
        assert_eq!(dest(0), "POST /api/v10/guilds/555/roles");
        assert_eq!(dest(1), "POST /api/v10/guilds/555/roles");
        assert!(orden[2..6]
            .iter()
            .all(|o| o == "POST /api/v10/guilds/555/channels"));
        assert_eq!(dest(6), "PUT /api/v10/channels/41/permissions/1100");
        assert_eq!(dest(7), "PUT /api/v10/channels/42/permissions/1101");
    }

    #[tokio::test]
    async fn aplicar_propuesta_sigue_tras_un_fallo_omite_dependientes_y_cancela() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES | GESTIONAR_ROLES, true).await;
        // Crear el rol «Roto» falla; «Bueno» y el canal se crean.
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Roto"}),
            ))
            .respond_with(ResponseTemplate::new(500))
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/roles"))
            .and(wiremock::matchers::body_partial_json(
                json!({"name": "Bueno"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(rol("1200", "Bueno", 1, 0)))
            .mount(&s)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v10/guilds/555/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"id": "50", "type": 0, "guild_id": "555", "name": "sala", "position": 9}),
            ))
            .mount(&s)
            .await;
        roles_con_nuevos(&s, &[rol("1200", "Bueno", 1, 0)]).await;
        Mock::given(method("PUT"))
            .and(path("/api/v10/channels/50/permissions/1200"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&s)
            .await;
        let p = Propuesta {
            resumen: String::new(),
            acciones: vec![
                accion("crear_rol", json!({"nombre": "Roto"})),
                accion("crear_rol", json!({"nombre": "Bueno"})),
                accion("crear_canal", json!({"nombre": "sala"})),
                accion(
                    "permiso_canal",
                    json!({"channel_id": "sala", "target_id": "Roto", "tipo": "rol",
                           "permitir": "1024"}),
                ),
                accion(
                    "permiso_canal",
                    json!({"channel_id": "sala", "target_id": "Bueno", "tipo": "rol",
                           "permitir": "1024"}),
                ),
            ],
        };
        let cancelar = AtomicBool::new(false);
        let r = mcp
            .ejecutar_propuesta_con(&p, &cancelar, &mut |_, _| {})
            .await
            .unwrap();
        let e: Vec<_> = r.resultados.iter().map(|x| x.estado.clone()).collect();
        assert_eq!(
            e,
            [
                EstadoAccion::Error,
                EstadoAccion::Ok,
                EstadoAccion::Ok,
                EstadoAccion::Omitida,
                EstadoAccion::Ok
            ]
        );
        assert_eq!(
            (r.hechas, r.fallidas, r.omitidas, r.canceladas),
            (3, 1, 1, 0)
        );
        let bot = mcp.state.get_active_bot().unwrap();
        let historial = crate::operaciones::listar(&mcp.pool, &bot.id, "555")
            .await
            .unwrap();
        assert_eq!(historial[0].status, "partial");
        assert_eq!(r.operation_id.as_deref(), Some(historial[0].id.as_str()));
        assert_eq!(r.operation_status.as_deref(), Some("partial"));

        let fallida = Propuesta {
            resumen: String::new(),
            acciones: vec![accion("crear_rol", json!({"nombre":"Roto"}))],
        };
        let resultado = mcp
            .ejecutar_propuesta_con(&fallida, &AtomicBool::new(false), &mut |_, _| {})
            .await
            .unwrap();
        assert_eq!((resultado.hechas, resultado.fallidas), (0, 1));
        let historial = crate::operaciones::listar(&mcp.pool, &bot.id, "555")
            .await
            .unwrap();
        assert!(historial.iter().any(|op| op.status == "failed"
            && op.steps.len() == 1
            && op.steps[0].status == "failed"
            && op.steps[0].reason.as_deref()
                == Some("Discord returned an unreadable response; the outcome may be unknown")));

        // Cancelar tras la primera acción: el resto no se intenta.
        let cancelar = AtomicBool::new(false);
        let r = mcp
            .ejecutar_propuesta_con(&p, &cancelar, &mut |_, _| {
                cancelar.store(true, Ordering::SeqCst)
            })
            .await
            .unwrap();
        assert_eq!((r.resultados.len(), r.canceladas, r.total), (1, 4, 5));
    }

    #[tokio::test]
    async fn parar_en_peticion_termina_la_actual_y_reanudar_no_repite_hechas() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_CANALES, true).await;
        let cancelar = std::sync::Arc::new(AtomicBool::new(false));
        for (i, nombre) in ["primero", "segundo", "tercero"].iter().enumerate() {
            let parada = cancelar.clone();
            let nombre = nombre.to_string();
            Mock::given(method("POST"))
                .and(path("/api/v10/guilds/555/channels"))
                .and(wiremock::matchers::body_partial_json(
                    json!({"name": nombre}),
                ))
                .respond_with(move |_: &wiremock::Request| {
                    // Stop llega con la petición ya en vuelo, antes de responder.
                    if i == 0 {
                        parada.store(true, Ordering::SeqCst);
                    }
                    ResponseTemplate::new(200)
                        .set_delay(std::time::Duration::from_millis(20))
                        .set_body_json(json!({"id": (80 + i).to_string(), "type": 0,
                            "guild_id": "555", "name": nombre, "position": i}))
                })
                .expect(1)
                .mount(&s)
                .await;
        }
        let p = Propuesta {
            resumen: String::new(),
            acciones: ["primero", "segundo", "tercero"]
                .iter()
                .map(|nombre| accion("crear_canal", json!({"nombre": nombre})))
                .collect(),
        };
        let mut avances = Vec::new();
        let r = mcp
            .ejecutar_propuesta_con(&p, &cancelar, &mut |_, r| avances.push(r.indice))
            .await
            .unwrap();
        assert_eq!(
            (r.hechas, r.fallidas, r.omitidas, r.canceladas),
            (1, 0, 0, 2)
        );
        assert_eq!(avances, [0]);
        assert_eq!(r.resultados[0].indice, 0);
        assert_eq!(r.resultados[0].estado, EstadoAccion::Ok);
        let peticiones = s.received_requests().await.unwrap();
        let escrituras: Vec<_> = peticiones
            .iter()
            .filter(|r| r.method.as_str() == "POST")
            .collect();
        assert_eq!(escrituras.len(), 1, "Las restantes no se intentaron");
        assert_eq!(
            escrituras[0].body_json::<serde_json::Value>().unwrap()["name"],
            "primero"
        );

        cancelar.store(false, Ordering::SeqCst);
        avances.clear();
        let finalizado = mcp
            .ejecutar_propuesta_reanudable(
                &p,
                &cancelar,
                &mut |_, r| avances.push(r.indice),
                &r.resultados,
            )
            .await
            .unwrap();
        assert_eq!(avances, [1, 2]);
        assert_eq!((finalizado.hechas, finalizado.canceladas), (3, 0));
        s.verify().await; // Una sola petición por acción, incluso al reanudar.
    }

    #[tokio::test]
    async fn ejecutar_accion_respeta_los_permisos() {
        let (_d, mcp, s) = mcp_con_servidor(GESTIONAR_MENSAJES, true).await;
        prohibir_cambios(&s).await;
        let e = mcp
            .ejecutar_accion("crear_canal", json!({"nombre": "x"}))
            .await;
        assert_eq!(
            e.unwrap_err(),
            "The bot is missing the permission «Manage Channels»"
        );
    }
}

#[cfg(test)]
mod tests_firmas {
    use super::*;

    #[test]
    fn firmas_cubren_todas_las_de_escritura_con_sus_obligatorios() {
        let f = firmas_escritura();
        println!("{f}");
        for h in HERRAMIENTAS_ESCRITURA
            .iter()
            .filter(|h| **h != "elegir_servidor")
        {
            assert!(f.contains(&format!("- `{h}`:")), "falta {h}");
            assert!(esquema_args(h).is_some(), "sin esquema {h}");
        }
        // La que falló en la prueba real: `posiciones` es obligatorio.
        let ordenar = f
            .lines()
            .find(|l| l.starts_with("- `ordenar_roles`"))
            .unwrap();
        assert!(
            ordenar.contains("posiciones (") && ordenar.contains("required"),
            "{ordenar}"
        );
        let canal = f
            .lines()
            .find(|l| l.starts_with("- `crear_canal`"))
            .unwrap();
        assert!(canal.contains("nombre (string, required)"), "{canal}");
        assert!(!f.contains("guild_id"));
    }
}
