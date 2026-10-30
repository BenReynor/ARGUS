//! Textos del backend en el idioma elegido (`ui_language`).
//!
//! El texto fuente de todo mensaje que Rust devuelve a la UI está en inglés. Los
//! comandos lo traducen al salir con `tr`: busca el texto en el catálogo español
//! y, si no hay entrada, lo deja en inglés. Una entrada del catálogo puede tener
//! huecos `{nombre}` (o `{}`): se rellenan, en orden, con lo que haya en el mensaje.

use crate::state::{AppState, BotStatus};

/// Idioma por defecto de la interfaz.
pub const IDIOMA_DEFECTO: &str = "en";

/// Catálogo español: (texto fuente en inglés, traducción).
const CATALOGO_ES: &[(&str, &str)] = &[
    // <CATALOGO>
    ("No active bot in ARGUS: add or choose one in the app", "No hay bot activo en ARGUS: añade o elige uno en la app"),
    ("No active server in ARGUS: pass `guild_id` or choose one with `elegir_servidor`", "No hay servidor activo en ARGUS: indica `guild_id` o elige uno con `elegir_servidor`"),
    ("Permissions must be a bits number or a list of names", "Los permisos deben ser un número de bits o una lista de nombres"),
    ("Invalid color: «{t}» (use #RRGGBB or an RGB number)", "Color no válido: «{t}» (usa #RRGGBB o un número RGB)"),
    ("Color out of range: {valor} (maximum #FFFFFF)", "Color fuera de rango: {valor} (máximo #FFFFFF)"),
    ("Could not read the ARGUS configuration: {e}", "No se pudo leer la configuración de ARGUS: {e}"),
    ("The active bot is not in server {guild_id}: use `listar_servidores`", "El bot activo no está en el servidor {guild_id}: usa `listar_servidores`"),
    ("Could not save the active server: {e}", "No se pudo guardar el servidor activo: {e}"),
    ("Unsupported channel type: use texto, voz, categoria, anuncios or foro", "Tipo de canal no admitido: usa texto, voz, categoria, anuncios o foro"),
    ("The type must be «rol» or «miembro»", "El tipo debe ser «rol» o «miembro»"),
    ("Invalid response from Discord: {e}", "Respuesta de Discord inválida: {e}"),
    ("Enable «Server Members Intent» in the Developer Portal → Bot", "Activa «Server Members Intent» en el Developer Portal → Bot"),
    ("Invalid token", "Token inválido"),
    ("The bot is missing the permission «{p}»", "Al bot le falta el permiso «{p}»"),
    ("The bot is missing the permission «{permiso}»", "Al bot le falta el permiso «{permiso}»"),
    ("The bot is missing a permission to do this on the server", "Al bot le falta un permiso para hacer esto en el servidor"),
    ("Not found: the server, channel, role or member does not exist or the bot has no access", "No encontrado: el servidor, canal, rol o miembro no existe o el bot no tiene acceso"),
    ("Discord rate limited the requests; try again in {:.0} s", "Discord limitó las peticiones; vuelve a intentarlo en {:.0} s"),
    ("Discord rate limited the requests; try again in a few seconds", "Discord limitó las peticiones; vuelve a intentarlo en unos segundos"),
    ("Discord responded with an error ({status})", "Discord respondió con un error ({status})"),
    ("Discord responded with an error ({status}): {texto}", "Discord respondió con un error ({status}): {texto}"),
    ("Could not reach Discord: {e}", "No se pudo contactar con Discord: {e}"),
    ("Invalid server ID", "ID de servidor inválido"),
    ("Invalid request: {e}", "Petición inválida: {e}"),
    ("Invalid user ID", "ID de usuario inválido"),
    ("The message cannot be empty", "El mensaje no puede estar vacío"),
    ("The message is too long ({n} characters; the maximum is {MAX_LONGITUD_MENSAJE})", "El mensaje es demasiado largo ({n} caracteres; el máximo es {MAX_LONGITUD_MENSAJE})"),
    ("Invalid {que} ID", "ID de {que} inválido"),
    ("Invalid message: {e}", "Mensaje inválido: {e}"),
    ("A timeout cannot exceed 28 days (40320 minutes)", "El silencio no puede superar los 28 días (40320 minutos)"),
    ("Invalid timeout date", "Fecha de silencio inválida"),
    ("Invalid timeout duration", "Duración de silencio inválida"),
    ("Invalid reason", "Motivo inválido"),
    ("The reason cannot exceed {MAX_MOTIVO} characters", "El motivo no puede superar los {MAX_MOTIVO} caracteres"),
    ("The days of messages to delete must be between 0 and 7", "Los días de mensajes a borrar deben estar entre 0 y 7"),
    ("The nickname cannot exceed 32 characters", "El apodo no puede superar los 32 caracteres"),
    ("Invalid nickname", "Apodo inválido"),
    ("The channel name must be between 1 and 100 characters", "El nombre del canal debe tener entre 1 y 100 caracteres"),
    ("Unsupported channel type", "Tipo de canal no admitido"),
    ("Slowmode cannot exceed 6 hours (21600 s)", "El modo lento no puede superar las 6 horas (21600 s)"),
    ("The channel topic cannot exceed 1024 characters", "El tema del canal no puede superar los 1024 caracteres"),
    ("The channel needs a name", "El canal necesita un nombre"),
    ("Invalid channel name", "Nombre de canal inválido"),
    ("A category cannot be inside another category", "Una categoría no puede estar dentro de otra categoría"),
    ("Invalid channel topic", "Tema de canal inválido"),
    ("Invalid slowmode", "Modo lento inválido"),
    ("The {que} permissions are not valid", "Los permisos {que} no son válidos"),
    ("A permission cannot be both allowed and denied", "Un mismo permiso no puede estar permitido y denegado a la vez"),
    ("There are no positions to change", "No hay posiciones que cambiar"),
    ("The role name must be between 1 and 100 characters", "El nombre del rol debe tener entre 1 y 100 caracteres"),
    ("The role color is not valid", "El color del rol no es válido"),
    ("The role permissions are not valid", "Los permisos del rol no son válidos"),
    ("The role needs a name", "El rol necesita un nombre"),
    ("Unknown permission: «{}»", "Permiso desconocido: «{}»"),
    ("The server name must be between 2 and 100 characters", "El nombre del servidor debe tener entre 2 y 100 caracteres"),
    ("The description cannot exceed 120 characters", "La descripción no puede superar los 120 caracteres"),
    ("Invalid verification level (0–4)", "Nivel de verificación inválido (0–4)"),
    ("Invalid content filter (0–2)", "Filtro de contenido inválido (0–2)"),
    ("Invalid default notifications (0–1)", "Notificaciones por defecto inválidas (0–1)"),
    ("The AFK timeout must be 60, 300, 900, 1800 or 3600 seconds", "El tiempo AFK debe ser 60, 300, 900, 1800 o 3600 segundos"),
    ("No server selected", "No hay servidor seleccionado"),
    ("Error decrypting the token: {e}", "Error descifrando el token: {e}"),
    ("Error decrypting the token: {}", "Error descifrando token: {}"),
    ("Error encrypting the token: {}", "Error cifrando token: {}"),
    ("No active bot. Choose one in Settings.", "No hay un bot activo. Elige uno en Configuración."),
    ("No bot selected", "No hay bot seleccionado"),
    ("Active server changed", "Guild activa cambiada"),
    ("Channel {channel_id} deleted", "Canal {channel_id} borrado"),
    ("{} channels reordered", "{} canales reordenados"),
    ("That role is above the bot's role: move it up in Server settings → Roles", "Ese rol está por encima del rol del bot: súbelo en Ajustes del servidor → Roles"),
    ("Role {id} does not exist on the server", "El rol {id} no existe en el servidor"),
    ("That cannot be done to the server owner", "No se puede hacer eso al dueño del servidor"),
    ("That member has a role equal to or above the bot's: move the bot's role up in Server settings → Roles", "Ese miembro tiene un rol igual o superior al del bot: sube el rol del bot en Ajustes del servidor → Roles"),
    ("User {user_id} kicked", "Usuario {user_id} expulsado"),
    ("User {user_id} banned", "Usuario {user_id} baneado"),
    ("Ban of {user_id} removed", "Ban de {user_id} quitado"),
    ("Nickname of {user_id} updated", "Apodo de {user_id} actualizado"),
    ("Message {message_id} deleted", "Mensaje {message_id} borrado"),
    ("{n} messages deleted", "{n} mensajes borrados"),
    ("Message {message_id} pinned", "Mensaje {message_id} fijado"),
    ("Message {message_id} unpinned", "Mensaje {message_id} desfijado"),
    ("Invite {code} deleted", "Invitación {code} eliminada"),
    ("Webhook {webhook_id} deleted", "Webhook {webhook_id} eliminado"),
    ("Role {role_id} deleted", "Rol {role_id} borrado"),
    ("{} roles reordered", "{} roles reordenados"),
    ("The overwrite type must be 0 (role) or 1 (member)", "El tipo de overwrite debe ser 0 (rol) o 1 (miembro)"),
    ("Permissions of {target_id} updated in channel {channel_id}", "Permisos de {target_id} actualizados en el canal {channel_id}"),
    ("Permissions of {target_id} removed from channel {channel_id}", "Permisos de {target_id} quitados del canal {channel_id}"),
    ("Message sent to channel {channel_id} (id {id})", "Mensaje enviado a canal {channel_id} (id {id})"),
    ("DM sent to user {user_id} (id {id})", "DM enviado a usuario {user_id} (id {id})"),
    ("Role {role_id} added to user {user_id}", "Rol {role_id} añadido a usuario {user_id}"),
    ("Role {role_id} removed from user {user_id}", "Rol {role_id} quitado a usuario {user_id}"),
    ("Timeout removed from user {user_id}", "Silencio quitado a usuario {user_id}"),
    ("User {user_id} timed out for {minutos} minutes", "Usuario {user_id} silenciado por {minutos} minutos"),
    ("Settings saved", "Configuración guardada"),
    ("Full settings saved", "Configuración completa guardada"),
    ("Could not locate ARGUS: {e}", "No se pudo localizar ARGUS: {e}"),
    ("Could not locate the ARGUS folder", "No se pudo localizar la carpeta de ARGUS"),
    ("Bot updated", "Bot actualizado"),
    ("Bot deleted", "Bot eliminado"),
    ("The bot is not registered", "El bot no está registrado"),
    ("Active bot changed", "Bot activo cambiado"),
    ("Template saved", "Plantilla guardada"),
    ("Template deleted", "Plantilla eliminada"),
    ("Template not found", "Plantilla no encontrada"),
    ("Template '{}' applied to server {}", "Plantilla '{}' aplicada a guild {}"),
    ("Monitoring settings saved", "Configuración de vigilancia guardada"),
    ("Server settings saved", "Ajustes del servidor guardados"),
    ("CLI agent saved", "Agente CLI guardado"),
    ("Context cleared", "Contexto limpiado"),
    ("Imported", "Importado"),
    ("Uninstall complete", "Desinstalación completa"),
    ("Bulk delete needs between 2 and 100 messages", "El borrado en masa necesita entre 2 y 100 mensajes"),
    ("Discord does not allow bulk-deleting messages older than 14 days: {viejos} of {} are older. Remove them from the selection or delete them one by one", "Discord no permite borrar en masa mensajes de más de 14 días: {viejos} de {} son más antiguos. Quítalos de la selección o bórralos uno a uno"),
    ("The expiry cannot exceed 7 days (604800 seconds)", "La caducidad no puede superar 7 días (604800 segundos)"),
    ("The maximum uses cannot exceed 100", "Los usos máximos no pueden superar 100"),
    ("Invalid invite code", "Código de invitación inválido"),
    ("The webhook name must be between 1 and 80 characters", "El nombre del webhook debe tener entre 1 y 80 caracteres"),
    ("The database is not open", "La base de datos no está abierta"),
    ("Configuration folder not found", "No se encontró la carpeta de configuración"),
    ("Gateway error: {e}", "Error del Gateway: {e}"),
    ("channel", "canal"),
    ("user", "usuario"),
    ("role", "rol"),
    ("category", "categoría"),
    ("member", "miembro"),
    ("server", "servidor"),
    ("allowed", "permitidos"),
    ("denied", "denegados"),
    // </CATALOGO>
];

/// Nombres de permiso (identificadores internos) con su nombre oficial en inglés.
const PERMISOS_EN: &[(&str, &str)] = &[
    ("Administrador", "Administrator"),
    ("Ver canales", "View Channels"),
    ("Enviar mensajes", "Send Messages"),
    ("Leer el historial de mensajes", "Read Message History"),
    ("Gestionar canales", "Manage Channels"),
    ("Gestionar roles", "Manage Roles"),
    ("Gestionar el servidor", "Manage Server"),
    ("Gestionar mensajes", "Manage Messages"),
    ("Expulsar miembros", "Kick Members"),
    ("Banear miembros", "Ban Members"),
    ("Moderar miembros", "Timeout Members"),
    ("Ver registro de auditoría", "View Audit Log"),
];

/// `"es"` si el idioma pedido es español; cualquier otro valor es inglés.
pub fn normalizar(idioma: &str) -> &'static str {
    if idioma.trim().eq_ignore_ascii_case("es") {
        "es"
    } else {
        IDIOMA_DEFECTO
    }
}

/// Idioma guardado en la config del estado.
pub fn idioma(state: &AppState) -> String {
    normalizar(&state.config.lock().unwrap().ui_language).to_string()
}

/// Trocea una plantilla en los textos fijos que hay entre sus huecos `{...}`.
/// Un hueco tiene que ser corto y sin espacios (`{e}`, `{}`, `{:.0}`).
fn trocear(plantilla: &str) -> Vec<&str> {
    let mut partes = Vec::new();
    let mut inicio = 0;
    let mut desde = 0;
    while let Some(i) = plantilla[desde..].find('{') {
        let abre = desde + i;
        match plantilla[abre..].find('}') {
            Some(j) if !plantilla[abre + 1..abre + j].contains(char::is_whitespace) => {
                partes.push(&plantilla[inicio..abre]);
                inicio = abre + j + 1;
                desde = inicio;
            }
            _ => desde = abre + 1,
        }
    }
    partes.push(&plantilla[inicio..]);
    partes
}

/// Si `texto` encaja con la plantilla, devuelve lo que ocupa cada hueco.
fn encajar<'a>(partes: &[&str], texto: &'a str) -> Option<Vec<&'a str>> {
    let (primera, resto) = partes.split_first()?;
    let mut pos = texto.len() - texto.strip_prefix(primera)?.len();
    let Some((ultima, medio)) = resto.split_last() else {
        return (pos == texto.len()).then(Vec::new);
    };
    let fin = texto.len().checked_sub(ultima.len())?;
    if fin < pos || !texto.ends_with(ultima) {
        return None;
    }
    let mut huecos = Vec::new();
    for parte in medio {
        let hasta = if parte.is_empty() {
            pos
        } else {
            pos + texto[pos..fin].find(parte)?
        };
        huecos.push(&texto[pos..hasta]);
        pos = hasta + parte.len();
    }
    if pos > fin {
        return None;
    }
    huecos.push(&texto[pos..fin]);
    Some(huecos)
}

fn montar(partes: &[&str], huecos: &[String]) -> String {
    let mut salida = String::new();
    for (i, parte) in partes.iter().enumerate() {
        salida.push_str(parte);
        if let Some(h) = huecos.get(i).filter(|_| i + 1 < partes.len()) {
            salida.push_str(h);
        }
    }
    salida
}

/// Traduce `texto` (inglés) al idioma `idioma`. Sin entrada en el catálogo, o con
/// un idioma que no sea el español, devuelve el texto en inglés.
pub fn tr(idioma: &str, texto: &str) -> String {
    let es = normalizar(idioma) == "es";
    let mut mejor: Option<(usize, Vec<&str>, &str, &str)> = None;
    for (fuente, traduccion) in CATALOGO_ES {
        let partes = trocear(fuente);
        let Some(huecos) = encajar(&partes, texto) else {
            continue;
        };
        let fijo: usize = partes.iter().map(|p| p.len()).sum();
        if mejor.as_ref().is_none_or(|(f, ..)| fijo > *f) {
            mejor = Some((fijo, huecos, fuente, traduccion));
        }
    }
    let Some((_, huecos, fuente, traduccion)) = mejor else {
        return texto.to_string();
    };
    let huecos: Vec<String> = huecos
        .into_iter()
        .map(|h| if es { palabra_es(h) } else { permiso_en(h) })
        .collect();
    montar(&trocear(if es { traduccion } else { fuente }), &huecos)
}

/// Un hueco que es una palabra suelta del catálogo (`channel`) se traduce también.
fn palabra_es(h: &str) -> String {
    CATALOGO_ES
        .iter()
        .find(|(fuente, _)| *fuente == h && !h.contains('{'))
        .map_or_else(|| h.to_string(), |(_, es)| es.to_string())
}

/// Un hueco que es un nombre de permiso interno se muestra con su nombre oficial.
fn permiso_en(h: &str) -> String {
    PERMISOS_EN
        .iter()
        .find(|(es, _)| *es == h)
        .map_or_else(|| h.to_string(), |(_, en)| en.to_string())
}

/// Traduce el error de un comando al idioma de la config.
pub fn localizar_error<T>(state: &AppState, r: Result<T, String>) -> Result<T, String> {
    r.map_err(|e| tr(&idioma(state), &e))
}

/// Traduce el error y el mensaje de éxito de un comando que devuelve texto.
pub fn localizar_texto(state: &AppState, r: Result<String, String>) -> Result<String, String> {
    let lang = idioma(state);
    r.map(|m| tr(&lang, &m)).map_err(|e| tr(&lang, &e))
}

/// Traduce el último error de un estado del bot.
pub fn estado_traducido(lang: &str, mut estado: BotStatus) -> BotStatus {
    estado.last_error = estado.last_error.map(|e| tr(lang, &e));
    estado
}

/// Traduce los errores de un comando que devuelve el estado del bot.
pub fn localizar_estado(
    state: &AppState,
    r: Result<BotStatus, String>,
) -> Result<BotStatus, String> {
    let lang = idioma(state);
    r.map(|s| estado_traducido(&lang, s))
        .map_err(|e| tr(&lang, &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texto_con_entrada_sale_en_espanol_solo_con_es() {
        assert_eq!(tr("es", "Invalid token"), "Token inválido");
        assert_eq!(tr("en", "Invalid token"), "Invalid token");
        assert_eq!(tr("", "Invalid token"), "Invalid token");
        assert_eq!(tr("fr", "Invalid token"), "Invalid token");
    }

    #[test]
    fn texto_sin_entrada_sale_en_ingles() {
        assert_eq!(tr("es", "Nothing like this"), "Nothing like this");
        assert_eq!(tr("en", "Nothing like this"), "Nothing like this");
    }

    #[test]
    fn los_huecos_se_rellenan_con_el_mensaje() {
        assert_eq!(
            tr("es", "Channel 42 deleted"),
            "Canal 42 borrado",
            "hueco con nombre"
        );
        assert_eq!(
            tr("es", "Invalid response from Discord: bad json"),
            "Respuesta de Discord inválida: bad json"
        );
        assert_eq!(
            tr("es", "Invalid channel ID"),
            "ID de canal inválido",
            "la palabra suelta también se traduce"
        );
        assert_eq!(
            tr("es", "Discord rate limited the requests; try again in 3 s"),
            "Discord limitó las peticiones; vuelve a intentarlo en 3 s"
        );
    }

    #[test]
    fn el_permiso_sale_con_su_nombre_oficial_en_ingles() {
        let falta = "The bot is missing the permission «Gestionar roles»";
        assert_eq!(
            tr("en", falta),
            "The bot is missing the permission «Manage Roles»"
        );
        assert_eq!(
            tr("es", falta),
            "Al bot le falta el permiso «Gestionar roles»"
        );
    }

    #[test]
    fn gana_la_entrada_mas_especifica() {
        assert_eq!(
            tr("es", "Discord responded with an error (500): boom"),
            "Discord respondió con un error (500): boom"
        );
        assert_eq!(
            tr("es", "Discord responded with an error (500)"),
            "Discord respondió con un error (500)"
        );
    }

    #[test]
    fn cada_entrada_del_catalogo_tiene_los_mismos_huecos() {
        for (fuente, es) in CATALOGO_ES {
            assert_eq!(
                trocear(fuente).len(),
                trocear(es).len(),
                "huecos distintos en {fuente:?}"
            );
        }
    }

    #[test]
    fn normalizar_solo_reconoce_es() {
        assert_eq!(normalizar("es"), "es");
        assert_eq!(normalizar(" ES "), "es");
        assert_eq!(normalizar("en"), "en");
        assert_eq!(normalizar("xx"), "en");
    }
}
