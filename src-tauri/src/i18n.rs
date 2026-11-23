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
    ("The action is not a valid typed connector action", "La acción no es una acción tipada válida de conector"),
    ("The action targets a different server", "La acción apunta a otro servidor"),
    ("The action must name an account", "La acción debe indicar una cuenta"),
    ("The action has no changes", "La acción no tiene cambios"),
    ("The connector does not support this change type", "El conector no admite este tipo de cambio"),
    ("No adapter is available for this connector", "No hay adaptador disponible para este conector"),
    ("The active server changed since the action was approved", "El servidor activo cambió desde que se aprobó la acción"),
    ("The connector is not connected for this account and server", "El conector no está conectado para esta cuenta y servidor"),
    ("There is no pending action with that id", "No hay ninguna acción pendiente con ese id"),
    ("The active bot Gateway is connected.", "El Gateway del bot activo está conectado."),
    ("The active bot Gateway is disconnected. Start or reconnect the bot.", "El Gateway del bot activo está desconectado. Inicia o reconecta el bot."),
    ("Permission check cancelled.", "Comprobación de permisos cancelada."),
    ("Binary not found. Install or configure it.", "No se encontró el ejecutable. Instálalo o configúralo."),
    ("Session was not checked.", "No se comprobó la sesión."),
    ("Response was not checked.", "No se comprobó la respuesta."),
    ("Binary found.", "Ejecutable encontrado."),
    ("MCP does not require an agent login session.", "MCP no requiere una sesión de agente."),
    ("Session available.", "Sesión disponible."),
    ("MCP session unavailable. Check its initialization.", "Sesión MCP no disponible. Comprueba su inicialización."),
    ("No session available. Sign in separately if needed.", "No hay sesión disponible. Inicia sesión por separado si es necesario."),
    ("Process responded.", "El proceso respondió."),
    ("Process did not respond. Check its installation.", "El proceso no respondió. Comprueba su instalación."),
    ("Process check cancelled.", "Comprobación del proceso cancelada."),
    ("Process check timed out. Retry or check the installation.", "La comprobación del proceso agotó el tiempo. Reintenta o comprueba la instalación."),
    ("Process could not be started. Check its installation.", "No se pudo iniciar el proceso. Comprueba su instalación."),
    ("The bot has the server permissions required to build.", "El bot tiene los permisos de servidor necesarios para construir."),
    ("Missing building permissions. Grant the bot:", "Faltan permisos para construir. Concede al bot:"),
    ("View Channels", "Ver canales"),
    ("Manage Channels", "Gestionar canales"),
    ("Manage Roles", "Gestionar roles"),
    ("Could not read bot permissions. Check the network and server access.", "No se pudieron leer los permisos del bot. Comprueba la red y el acceso al servidor."),
    ("Permission check timed out. Check the network and retry.", "La comprobación de permisos agotó el tiempo. Comprueba la red y vuelve a intentarlo."),
    ("Could not access the bot credential. Check the keyring.", "No se pudo acceder a la credencial del bot. Comprueba el llavero."),
    ("Database is accessible.", "Se puede acceder a la base de datos."),
    ("Database is unavailable. Check access to the application database and restart ARGUS.", "La base de datos no está disponible. Comprueba el acceso a la base de datos de la aplicación y reinicia ARGUS."),
    ("Configuration could not be checked because the database is unavailable.", "No se pudo comprobar la configuración porque la base de datos no está disponible."),
    ("Saved configuration is valid.", "La configuración guardada es válida."),
    ("No saved configuration exists; defaults are in use.", "No hay configuración guardada; se usan los valores predeterminados."),
    ("Saved configuration is invalid. Review and save the application settings.", "La configuración guardada no es válida. Revisa y guarda los ajustes de la aplicación."),
    ("Configuration could not be read. Check the application database.", "No se pudo leer la configuración. Comprueba la base de datos de la aplicación."),
    ("The system keyring was available when encryption was initialized.", "El llavero del sistema estaba disponible al inicializar el cifrado."),
    ("The system keyring was unavailable when encryption was initialized. Unlock it and restart ARGUS.", "El llavero del sistema no estaba disponible al inicializar el cifrado. Desbloquéalo y reinicia ARGUS."),
    // <CATALOGO>
    ("Unknown connector", "Conector desconocido"),
    ("This service does not accept credentials in ARGUS", "Este servicio no admite credenciales en ARGUS"),
    ("Account, server and credential are required", "La cuenta, el servidor y la credencial son obligatorios"),
    ("The credential has already expired", "La credencial ya ha caducado"),
    ("The service rejected the credential", "El servicio rechazó la credencial"),
    ("OAuth is not supported yet", "OAuth aún no está soportado"),
    ("Unsupported credential type", "Tipo de credencial no soportado"),
    ("The owner, this bot and your verified Discord account are always protected. Administrator roles are excluded by default.", "El dueño, este bot y tu cuenta de Discord verificada siempre están protegidos. Los roles con Administrator se excluyen por defecto."),
    ("Verify your account using the code sent by this bot to your Discord DMs. The code expires after five minutes. If DMs are unavailable, member removal is blocked.", "Verifica tu cuenta con el código que este bot envía por DM en Discord. El código caduca a los cinco minutos. Si no se pueden enviar DM, se bloquea la expulsión de miembros."),
    ("Send verification code", "Enviar código de verificación"),
    ("Discord DM verification code", "Código de verificación recibido por DM en Discord"),
    ("Verification required", "Verificación necesaria"),
    ("Verify your Discord identity again", "Vuelve a verificar tu identidad de Discord"),
    ("ARGUS member removal verification code: {code}. Enter it only in ARGUS. Never share this code.", "Código de verificación para expulsión de miembros en ARGUS: {code}. Introdúcelo solo en ARGUS. Nunca compartas este código."),
    ("Kick all members", "Expulsar a todos los miembros"),
    ("App only. An automatic snapshot must succeed before any member is kicked. Kicked members cannot be added back automatically.", "Solo desde la app. Debe guardarse un snapshot automático antes de expulsar a nadie. Los miembros expulsados no pueden volver a añadirse automáticamente."),
    ("The owner, this bot and your selected member ID are always protected. Administrator roles are excluded by default.", "El dueño, este bot y el ID de miembro que indiques siempre están protegidos. Los roles con Administrator se excluyen por defecto."),
    ("Your Discord member ID", "Tu ID de miembro de Discord"),
    ("Review member removal", "Revisar expulsión de miembros"),
    ("{done} of {total}", "{done} de {total}"),
    ("Preparing", "Preparando"),
    ("Ready", "Listo"),
    ("Running", "En curso"),
    ("Stopped", "Detenido"),
    ("Completed", "Completado"),
    ("Failed", "Fallido"),
    ("Stop prevents the next kick. A request already in progress may finish.", "Stop impide la siguiente expulsión. Una petición que ya está en curso puede terminar."),
    ("Automatic snapshot saved", "Snapshot automático guardado"),
    ("Excluded roles", "Roles excluidos"),
    ("Review again after changing excluded roles.", "Vuelve a revisar después de cambiar los roles excluidos."),
    ("Members that could not be kicked", "Miembros que no se pudieron expulsar"),
    ("Confirm member removal", "Confirmar expulsión de miembros"),
    ("{count} members will be kicked from {server}.", "Se expulsará a {count} miembros de {server}."),
    ("Type the server name to confirm", "Escribe el nombre del servidor para confirmar"),
    ("Select your own server member ID", "Selecciona tu propio ID de miembro del servidor"),
    ("A member removal is already running", "Ya hay una expulsión de miembros en curso"),
    ("Invalid member removal request", "Solicitud de expulsión de miembros no válida"),
    ("Review the member removal again", "Vuelve a revisar la expulsión de miembros"),
    ("Member now has an excluded role", "El miembro ahora tiene un rol excluido"),

    ("Departed members", "Miembros que se fueron"),
    ("Discord cannot add members back. An invitation can only be sent if the bot can DM them, normally through another shared server. Failed DMs are listed below.", "Discord no permite volver a meter miembros. Solo se envía una invitación si el bot puede escribirles por DM, normalmente mediante otro servidor compartido. Los DM fallidos aparecen abajo."),
    ("Return their roles when they rejoin", "Devolver sus roles al volver"),
    ("Uses the last completed restoration for this bot and server. Enabled by default after restoring. Keep ARGUS and the bot running.", "Usa la última restauración completada de este bot y servidor. Se activa por defecto tras restaurar. Mantén ARGUS y el bot encendidos."),
    ("Departed members: {count}", "Miembros que se fueron: {count}"),
    ("No roles", "Sin roles"),
    ("Invitation channel", "Canal de invitación"),
    ("Choose an invitation channel", "Elige un canal de invitación"),
    ("Invite", "Invitar"),
    ("Export departed members", "Exportar miembros que se fueron"),
    ("Invitation sent", "Invitación enviada"),
    ("Could not send invitation: {error}", "No se pudo enviar la invitación: {error}"),
    ("Restored role {role} to returning member {user}", "Rol {role} devuelto al miembro que vuelve {user}"),
    ("Could not restore role {role} to returning member {user}: {error}", "No se pudo devolver el rol {role} al miembro que vuelve {user}: {error}"),
    ("Role is unavailable or above the bot", "El rol no está disponible o está por encima del bot"),
    ("Returning member roles failed: {error}", "Falló la devolución de roles al miembro: {error}"),

    ("The bot lacks the Manage Roles permission", "Al bot le falta el permiso Gestionar roles"),
    ("Mass deletion protection", "Protección ante borrados masivos"),
    ("Save an emergency snapshot and alert while the bot is running.", "Guarda un snapshot de emergencia y avisa mientras el bot está encendido."),
    ("Deletion threshold", "Umbral de borrados"),
    ("Detection window (seconds)", "Ventana de detección (segundos)"),
    ("Automatically remove actor roles", "Quitar automáticamente los roles al responsable"),
    ("Off by default. Requires audit log access and Manage Roles; Discord role hierarchy still applies.", "Desactivado por defecto. Requiere acceso a auditoría y Gestionar roles; se respeta la jerarquía de Discord."),
    ("Mass deletion detected", "Borrado masivo detectado"),
    ("Emergency snapshots contain the previous live map: channels, roles, permissions and member roles. Other data is not included.", "Los snapshots de emergencia contienen el mapa en vivo anterior: canales, roles, permisos y roles de miembros. No incluyen otros datos."),
    ("Mass deletion detected: {count} items. Actor: {actor}", "Borrado masivo detectado: {count} elementos. Responsable: {actor}"),
    ("Unknown actor (audit log unavailable or delayed)", "Responsable desconocido (auditoría no disponible o retrasada)"),
    ("Snapshot not found", "Snapshot no encontrado"),
    ("Emergency snapshot saved", "Snapshot de emergencia guardado"),
    ("Emergency protection failed: {error}", "Falló la protección de emergencia: {error}"),
    ("Audit log unavailable: {error}", "Auditoría no disponible: {error}"),
    ("Audit log request timed out", "La solicitud de auditoría superó el tiempo máximo"),
    ("Roles removed from {actor}", "Roles retirados a {actor}"),
    ("Could not remove roles from {actor}: {error}", "No se pudieron quitar los roles a {actor}: {error}"),

    ("Use restore <snapshot-id> [undo]", "Usa restore <snapshot-id> [undo]"),
    ("Restore requires structure and role deletion permissions for Discord orders", "Restaurar requiere permisos de estructura y borrado de roles para las órdenes de Discord"),
    ("Restore snapshot", "Restaurar snapshot"),
    ("Apply the reviewed restore proposal", "Aplicar la propuesta de restauración revisada"),
    ("Channel permissions unresolved; access closed: {name}", "Permisos del canal sin resolver; acceso cerrado: {name}"),
    ("Channel type cannot be restored: {name}", "No se puede restaurar el tipo de canal: {name}"),
    ("Parent category is unavailable: {name}", "La categoría superior no está disponible: {name}"),
    ("Server setting references an unavailable channel: {field}", "El ajuste del servidor hace referencia a un canal no disponible: {field}"),
    ("Restore role order", "Restaurar orden de roles"),
    ("Restore channel order", "Restaurar orden de canales"),
    ("Restore server settings", "Restaurar ajustes del servidor"),
    ("Restore widget settings", "Restaurar ajustes del widget"),
    ("Messages cannot be restored", "No se pueden restaurar los mensajes"),
    ("Emojis, stickers, webhooks and bans are not restored by this proposal", "Esta propuesta no restaura emojis, stickers, webhooks ni baneos"),
    ("Managed role cannot be recreated: {name}", "No se puede recrear el rol gestionado: {name}"),
    ("Ambiguous role name: {name}", "Nombre de rol ambiguo: {name}"),
    ("Channel name already exists: {name}", "Ya existe un canal con el mismo nombre: {name}"),
    ("Unsupported channel type: {name}", "Tipo de canal no compatible: {name}"),
    ("Member is no longer in the server: {name}", "El miembro ya no está en el servidor: {name}"),
    ("Permission role unavailable: {name}", "Rol de permisos no disponible: {name}"),
    ("Restore dependency is unavailable", "Una dependencia de la restauración no está disponible"),
    ("Restore requires confirmation in ARGUS", "La restauración requiere confirmación en ARGUS"),
    ("Saving a template requires approval in ARGUS", "Guardar una plantilla requiere aprobación en ARGUS"),
    ("Restore proposal is unavailable", "La propuesta de restauración no está disponible"),
    ("Type the server name to confirm deletion of extra items", "Escribe el nombre del servidor para confirmar el borrado de los elementos sobrantes"),
    ("A restore is already running", "Ya hay una restauración en curso"),

    ("Invalid snapshot file or different server", "Archivo de snapshot no válido o de otro servidor"),
    ("Snapshot file is too large", "El archivo de snapshot es demasiado grande"),
    ("Snapshot not found", "Snapshot no encontrado"),
    ("Choose a snapshot interval of 6, 12 or 24 hours", "Elige un intervalo de snapshots de 6, 12 o 24 horas"),
    ("Keep between 1 and 365 snapshots", "Conserva entre 1 y 365 snapshots"),
    ("Invalid response from Discord", "Respuesta de Discord no válida"),

    ("Use a backup password with at least 8 characters", "Usa una contraseña de al menos 8 caracteres para la copia"),
    ("Automatic configuration backup failed", "Falló la copia automática de configuración"),
    ("Invalid or unsupported backup file", "Archivo de copia no válido o incompatible"),
    ("Incorrect backup password or damaged file", "Contraseña de copia incorrecta o archivo dañado"),
    ("Could not derive the backup key", "No se pudo derivar la clave de la copia"),
    ("Backup file is too large", "El archivo de copia es demasiado grande"),
    ("Choose a .argusbak file", "Elige un archivo .argusbak"),
    ("Confirm the backup import before continuing", "Confirma la importación de la copia antes de continuar"),
    ("Stop the bot before importing a backup", "Detén el bot antes de importar una copia"),
    ("Preview the backup before importing", "Revisa la copia antes de importarla"),

    ("AI time limits must be between 1 and 30 minutes", "Los límites de tiempo de IA deben estar entre 1 y 30 minutos"),
    ("The AI time limit was exceeded and the agent was stopped. Try a more specific request or fewer messages per channel", "Se superó el tiempo máximo de IA y se detuvo el agente. Prueba una petición más concreta o menos mensajes por canal"),

    ("Daily reply budget unavailable", "El presupuesto diario de respuestas no está disponible"),
    ("Daily reply limit reached", "Se alcanzó el tope diario de respuestas"),
    ("Daily reply limit reached. I will reply again tomorrow (00:00 UTC).", "Se alcanzó el tope diario de respuestas. Volveré a responder mañana (00:00 UTC)."),
    ("[WARN] Daily reply limit reached in {server} #{channel}: {max}", "[WARN] Se alcanzó el tope diario de respuestas en {server} #{channel}: {max}"),

    ("Confirm the description generation cost before continuing", "Confirma el coste de generar las descripciones antes de continuar"),
    ("Description generation cancelled; no AI calls made", "Generación de descripciones cancelada; no se hicieron llamadas a la IA"),
    ("Generate {n} descriptions with approximately {calls} AI calls? This may incur costs. Reply **yes** within 5 minutes to generate, or **no** to cancel", "¿Generar {n} descripciones con aproximadamente {calls} llamadas a la IA? Esto puede tener un coste. Responde **sí** en 5 minutos para generar, o **no** para cancelar"),
    ("Unlock the system keyring and restart ARGUS to access migrated tokens", "Desbloquea el llavero del sistema y reinicia ARGUS para acceder a los tokens migrados"),
    ("Invalid keyring key length", "La clave del llavero tiene una longitud no válida"),
    ("Encryption is already initialized", "El cifrado ya está inicializado"),
    ("Encryption is not initialized", "El cifrado no está inicializado"),
    ("Action «{h}» is not allowed from Discord. Check Allow from Discord in Discord replies", "La acción «{h}» no está permitida desde Discord. Revisa Permitir desde Discord en Respuestas de Discord"),
    ("Permission target «{texto}» ({tipo}) does not exist and is not created before this action", "El objetivo de permisos «{texto}» ({tipo}) no existe y no se crea antes de esta acción"),
    ("Invalid permission overwrite: {e}", "Sobrescritura de permisos no válida: {e}"),
    ("Undefined variable «{nombre}»", "Variable sin definir «{nombre}»"),
    ("Invalid variable: use «@var name = value»", "Variable no válida: usa «@var nombre = valor»"),
    ("Invalid variable name «{nombre}»", "Nombre de variable no válido «{nombre}»"),
    ("Invalid color «{valor}» (use #RRGGBB)", "Color no válido «{valor}» (usa #RRGGBB)"),
    ("Invalid range «{texto}»: use two whole numbers, the first not greater than the second", "Rango no válido «{texto}»: usa dos números enteros, el primero no mayor que el segundo"),
    ("Range «{texto}» is too large (maximum {max} items)", "El rango «{texto}» es demasiado grande (máximo {max} elementos)"),
    ("Only one range per line is supported", "Solo se admite un rango por línea"),
    ("Missing name after «{clave}»", "Falta el nombre después de «{clave}»"),
    ("Unknown attribute «{clave}»", "Atributo desconocido «{clave}»"),
    ("Invalid value «{valor}» for «{clave}» (use yes or no)", "Valor no válido «{valor}» para «{clave}» (usa sí o no)"),
    ("Invalid description: the quoted text is not closed", "Descripción no válida: el texto entre comillas no se cierra"),
    ("Empty hint after «generar:»", "Indicación vacía después de «generar:»"),
    ("Invalid description: write the text in quotes or use «generar:» followed by a hint", "Descripción no válida: escribe el texto entre comillas o usa «generar:» con una indicación"),
    ("[ORDER] Order from {autor} in {servidor} #{canal}: {orden}", "[ORDEN] Orden de {autor} en {servidor} #{canal}: {orden}"),
    ("[ORDER] Proposal for {autor} in {servidor} #{canal}: {n} actions in {ms} ms", "[ORDEN] Propuesta para {autor} en {servidor} #{canal}: {n} acciones en {ms} ms"),
    ("[WARN] Could not prepare the proposal in {servidor} #{canal} for {autor}: {motivo}", "[WARN] No se pudo preparar la propuesta en {servidor} #{canal} para {autor}: {motivo}"),
    ("[ORDER] Proposal discarded by {autor} in {servidor} #{canal}", "[ORDEN] Propuesta descartada por {autor} en {servidor} #{canal}"),
    ("[ORDER] Applying the proposal of {autor} in {servidor} #{canal}: {n} actions", "[ORDEN] Aplicando la propuesta de {autor} en {servidor} #{canal}: {n} acciones"),
    ("[ORDER] Applied the proposal of {autor} in {servidor} #{canal}: {hechas} done, {fallos} with errors, {canceladas} not attempted in {ms} ms", "[ORDEN] Propuesta de {autor} aplicada en {servidor} #{canal}: {hechas} hechas, {fallos} con error, {canceladas} sin intentar en {ms} ms"),
    ("[WARN] Could not apply the proposal in {servidor} #{canal} for {autor}: {motivo}", "[WARN] No se pudo aplicar la propuesta en {servidor} #{canal} para {autor}: {motivo}"),
    ("The proposal has {n} actions, exceeding the Discord order limit of {max}", "La propuesta tiene {n} acciones y supera el límite de {max} por orden de Discord"),
    ("Order stopped: no changes made; proposal preparation was cancelled", "Orden detenida: no se hicieron cambios; se canceló la preparación de la propuesta"),
    ("Proposal discarded", "Propuesta descartada"),
    ("A proposal is being applied in this server", "Se está aplicando una propuesta en este servidor"),
    ("I could not apply the proposal: {motivo}", "No pude aplicar la propuesta: {motivo}"),
    ("Done: {n} changes, {m} with errors", "Hecho: {n} cambios, {m} con error"),
    ("Done: 1 change, {m} with errors", "Hecho: 1 cambio, {m} con error"),
    ("Channels to delete: {n}", "Canales a borrar: {n}"),
    ("Roles to delete: {n}", "Roles a borrar: {n}"),
    ("…and {n} more errors", "…y {n} errores más"),
    ("{n} actions were not attempted", "{n} acciones sin intentar"),
    ("[ORDER] File {archivo} from {autor} in {servidor} #{canal}", "[ORDEN] Archivo {archivo} de {autor} en {servidor} #{canal}"),
    ("[WARN] Could not build from a file in {servidor} #{canal} for {autor}: {motivo}", "[WARN] No se pudo construir desde un archivo en {servidor} #{canal} para {autor}: {motivo}"),
    ("You are not allowed to give orders from Discord", "No tienes permitido dar órdenes desde Discord"),
    ("The file is too large (the maximum is 200 KB)", "El archivo es demasiado grande (el máximo es 200 KB)"),
    ("The file is not valid text", "El archivo no es texto válido"),
    ("Invalid file address", "Dirección de archivo no válida"),
    ("Download failed: {motivo}", "No se pudo descargar el archivo: {motivo}"),
    ("The file has no structure to build from. Read the manual: {enlace}", "El archivo no tiene estructura para construir. Lee el manual: {enlace}"),
    ("The plan is not valid: {herramienta}: {motivo}", "El plan no es válido: {herramienta}: {motivo}"),
    ("The descriptions were not written: the AI agent is not installed", "No se escribieron las descripciones: el agente de IA no está instalado"),
    ("Permissions included in creation: {n}", "Permisos incluidos en la creación: {n}"),
    ("Build from the structured plan: {n} actions", "Construir desde el plan estructurado: {n} acciones"),
    ("Apply template «{p}»: {n} actions", "Aplicar la plantilla «{p}»: {n} acciones"),
    ("Create role «{r}»", "Crear el rol «{r}»"),
    ("Remove {p} from role «{r}»", "Quitar {p} del rol «{r}»"),
    ("Remove {p} from a channel overwrite", "Quitar {p} de un permiso de canal"),
    ("Create category «{c}»", "Crear la categoría «{c}»"),
    ("Create channel «{c}» in «{cat}»", "Crear el canal «{c}» en «{cat}»"),
    ("Create channel «{c}»", "Crear el canal «{c}»"),
    ("Set permissions of «{o}» on «{c}»", "Permisos de «{o}» en «{c}»"),
    ("Line {linea}: {motivo}", "Línea {linea}: {motivo}"),
    ("Line {linea}: channel «{canal}» is repeated in «{categoria}»; skipped", "Línea {linea}: el canal «{canal}» está repetido en «{categoria}»; se omite"),
    ("Line {linea}: channel «{canal}» has the same name as another channel; its permissions may be applied to the wrong one", "Línea {linea}: el canal «{canal}» se llama igual que otro; sus permisos podrían aplicarse al equivocado"),
    ("Role «{rol}» is not in the plan nor on the server; its permissions on «{canal}» were skipped", "El rol «{rol}» no está en el plan ni en el servidor; se omitieron sus permisos en «{canal}»"),
    ("Category «{categoria}» can hold at most 50 channels (Discord's limit); {n} channels were skipped. Split them into several categories", "La categoría «{categoria}» admite como mucho 50 canales (límite de Discord); se omitieron {n} canales. Repártelos en varias categorías"),
    ("The server would have {n} channels and categories; Discord allows at most 500, so Apply will stop when it reaches the limit", "El servidor tendría {n} canales y categorías; Discord permite como mucho 500, así que Aplicar se detendrá al llegar al límite"),
    ("Warnings:", "Avisos:"),
    ("…and {n} more warnings", "…y {n} avisos más"),
    ("The database is not available", "La base de datos no está disponible"),
    ("Preparing the proposal…", "Preparando la propuesta…"),
    ("I could not prepare the proposal: {motivo}", "No pude preparar la propuesta: {motivo}"),
    ("Another proposal is already being prepared in this server", "Ya se está preparando otra propuesta en este servidor"),
    ("…and {n} more changes", "…y {n} cambios más"),
    ("Reply **yes** within the next 5 minutes to apply it", "Responde **sí** en los próximos 5 minutos para aplicarla"),
    ("Contains irreversible actions: {acciones}", "Contiene acciones irreversibles: {acciones}"),
    ("[CHAT] Replied in {servidor} #{canal} to {autor} with {modelo} in {ms} ms", "[CHAT] Respondido en {servidor} #{canal} a {autor} con {modelo} en {ms} ms"),
    ("[WARN] No reply in {servidor} #{canal} to {autor}: {motivo}", "[WARN] Sin respuesta en {servidor} #{canal} a {autor}: {motivo}"),
    ("The AI agent took too long to reply", "El agente de IA tardó demasiado en responder"),
    ("The AI agent returned an empty reply", "El agente de IA devolvió una respuesta vacía"),
    ("The AI agent «{agente}» is not installed", "El agente de IA «{agente}» no está instalado"),
    ("Roles to create: {n}", "Roles a crear: {n}"),
    ("Roles to edit: {n}", "Roles a editar: {n}"),
    ("Categories to create: {n}", "Categorías a crear: {n}"),
    ("Channels to create: {n}", "Canales a crear: {n}"),
    ("Channels to edit: {n}", "Canales a editar: {n}"),
    ("Channel permissions to change: {n}", "Permisos de canal a cambiar: {n}"),
    ("Member roles to change: {n}", "Roles de miembros a cambiar: {n}"),
    ("Other actions: {n}", "Otras acciones: {n}"),
    ("The {que} «{texto}» does not exist and is not created before this action", "El {que} «{texto}» no existe y no se crea antes de esta acción"),
    ("Unknown action: «{nombre}»", "Acción desconocida: «{nombre}»"),
    ("«{nombre}» is a read-only tool, not an action", "«{nombre}» es una herramienta de lectura, no una acción"),
    ("Invalid arguments for «{nombre}»: {e}", "Argumentos no válidos para «{nombre}»: {e}"),
    ("Enter the text to search for", "Escribe el texto que buscar"),
    ("Text channel not found: «{texto}»", "Canal de texto no encontrado: «{texto}»"),
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
    ("Announcement channels need Community enabled on the server (Server Settings → Enable Community); use a text channel instead", "Los canales de anuncios necesitan la Comunidad activada en el servidor (Ajustes del servidor → Activar Comunidad); usa un canal de texto"),
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
    ("The service is not running", "El servicio no está en marcha"),
    ("Service authentication failed", "Falló la autenticación con el servicio"),
    ("Enable the service before starting it", "Habilita el servicio antes de iniciarlo"),
    ("The executable path is not valid", "La ruta del ejecutable no es válida"),
    ("The executable path contains unsupported characters", "La ruta del ejecutable contiene caracteres no admitidos"),
    ("Could not change the startup setting", "No se pudo cambiar el arranque con el sistema"),
    ("Starting with the system is only available on Linux and Windows", "El arranque con el sistema solo está disponible en Linux y Windows"),
    ("The service's Gateway is connected.", "El Gateway del servicio está conectado."),
    ("The service's Gateway is down. Check the bot token and the network, then restart the service.", "El Gateway del servicio está caído. Revisa el token del bot y la red y reinicia el servicio."),
    ("The service did not report its Gateway state.", "El servicio no informó del estado de su Gateway."),
    ("The service is running.", "El servicio está en marcha."),
    ("The interface is connected to the service.", "La interfaz está conectada al servicio."),
    ("The service is stopped. Start it from Settings → Service.", "El servicio está detenido. Inícialo desde Ajustes → Servicio."),
    ("Not checked: the service is stopped.", "Sin comprobar: el servicio está detenido."),
    ("The service process could not be confirmed.", "No se pudo confirmar el proceso del servicio."),
    ("The interface is disconnected from the service. Stop and start the service again.", "La interfaz está desconectada del servicio. Detén e inicia el servicio de nuevo."),
    ("Not checked: the interface cannot reach the service.", "Sin comprobar: la interfaz no alcanza el servicio."),
    ("This bot already has a Gateway connection in another ARGUS process (service or interface)", "Este bot ya tiene una conexión al Gateway en otro proceso de ARGUS (servicio o interfaz)"),
    ("Could not find the data folder", "No se encontró la carpeta de datos"),
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
    ("Message {id} edited in channel {channel_id}", "Mensaje {id} editado en el canal {channel_id}"),
    ("The bot can only edit its own messages", "El bot solo puede editar sus propios mensajes"),
    ("Reaction {emoji} added to message {message_id}", "Reacción {emoji} añadida al mensaje {message_id}"),
    ("Reaction {emoji} removed from message {message_id}", "Reacción {emoji} quitada del mensaje {message_id}"),
    ("Invalid emoji: use a Unicode emoji (👍) or a custom one as name:id", "Emoji no válido: usa un emoji Unicode (👍) o uno personalizado como nombre:id"),
    ("Unknown emoji: it does not exist or the bot cannot use it", "Emoji desconocido: no existe o el bot no puede usarlo"),
    ("Post created in forum {channel_id} (id {id})", "Post creado en el foro {channel_id} (id {id})"),
    ("Forum {channel_id} now has {n} tags", "El foro {channel_id} ahora tiene {n} etiquetas"),
    ("Post {post_id} tagged with {n} tags", "Post {post_id} etiquetado con {n} etiquetas"),
    ("The channel is not a forum", "El canal no es un foro"),
    ("The channel is not a forum post", "El canal no es un post de foro"),
    ("Unknown forum tag: {name}", "Etiqueta de foro desconocida: {name}"),
    ("A post can have at most 5 tags", "Un post puede tener como máximo 5 etiquetas"),
    ("A forum can have at most 20 tags", "Un foro puede tener como máximo 20 etiquetas"),
    ("The tag name cannot be empty", "El nombre de la etiqueta no puede estar vacío"),
    ("The tag name is too long (the maximum is 20 characters)", "El nombre de la etiqueta es demasiado largo (el máximo es 20 caracteres)"),
    ("Provide at least one tag", "Indica al menos una etiqueta"),
    ("Event created (id {id})", "Evento creado (id {id})"),
    ("Event {event_id} updated", "Evento {event_id} actualizado"),
    ("Event {event_id} canceled", "Evento {event_id} cancelado"),
    ("Invalid event ID", "ID de evento inválido"),
    ("Invalid {que} date: use ISO 8601 with a time zone (2030-05-01T20:00:00+02:00)", "Fecha de {que} no válida: usa ISO 8601 con zona horaria (2030-05-01T20:00:00+02:00)"),
    ("The event start must be in the future", "El inicio del evento debe ser futuro"),
    ("The event end must be after its start", "El fin del evento debe ser posterior a su inicio"),
    ("The event name cannot be empty", "El nombre del evento no puede estar vacío"),
    ("The event name is too long (the maximum is 100 characters)", "El nombre del evento es demasiado largo (el máximo es 100 caracteres)"),
    ("The event description is too long (the maximum is 1000 characters)", "La descripción del evento es demasiado larga (el máximo es 1000 caracteres)"),
    ("The event location must be between 1 and 100 characters", "El lugar del evento debe tener entre 1 y 100 caracteres"),
    ("The event channel must be a voice or stage channel", "El canal del evento debe ser de voz o escenario"),
    ("Provide either an external location or a voice/stage channel", "Indica un lugar externo o un canal de voz/escenario (solo uno)"),
    ("An event with an external location needs an end time", "Un evento con lugar externo necesita fecha de fin"),
    ("Poll created in channel {channel_id} (message {id})", "Encuesta creada en el canal {channel_id} (mensaje {id})"),
    ("Poll {message_id} closed", "Encuesta {message_id} cerrada"),
    ("The poll question cannot be empty", "La pregunta de la encuesta no puede estar vacía"),
    ("The poll question is too long (the maximum is 300 characters)", "La pregunta de la encuesta es demasiado larga (el máximo es 300 caracteres)"),
    ("A poll needs between 2 and 10 answers", "Una encuesta necesita entre 2 y 10 respuestas"),
    ("The poll duration must be between 1 and 768 hours", "La duración de la encuesta debe estar entre 1 y 768 horas"),
    ("A poll answer cannot be empty", "Una respuesta de la encuesta no puede estar vacía"),
    ("A poll answer is too long (the maximum is 55 characters)", "Una respuesta de la encuesta es demasiado larga (el máximo es 55 caracteres)"),
    ("That message has no poll", "Ese mensaje no tiene encuesta"),
    ("Invalid response from Discord: missing message id", "Respuesta de Discord no válida: falta el id del mensaje"),
    ("Could not reach Discord: {e}", "No se pudo conectar con Discord: {e}"),
    ("Provide at least one field to change","Indica al menos un campo que cambiar"),
    ("Invalid response from Discord: missing event id", "Respuesta de Discord no válida: falta el id del evento"),
    ("The post title cannot be empty", "El título del post no puede estar vacío"),
    ("The post title is too long (the maximum is 100 characters)", "El título del post es demasiado largo (el máximo es 100 caracteres)"),
    ("DM sent to user {user_id} (id {id})", "DM enviado a usuario {user_id} (id {id})"),
    ("You have received a warning: {motivo}", "Has recibido un aviso: {motivo}"),
    ("Warnings so far: {total}", "Avisos hasta ahora: {total}"),
    ("Warning {total} saved for user {user}", "Aviso {total} guardado para el usuario {user}"),
    ("The warning was saved, but the direct message could not be sent", "El aviso se guardó, pero no se pudo enviar el mensaje directo"),
    ("The warning needs a reason", "El aviso necesita un motivo"),
    ("The user is empty", "El usuario está vacío"),
    ("No member named «{texto}» in the server", "No hay ningún miembro llamado «{texto}» en el servidor"),
    ("More than one member is named «{texto}»: use the user ID", "Hay más de un miembro llamado «{texto}»: usa el ID del usuario"),
    ("Could not save the warning: {e}", "No se pudo guardar el aviso: {e}"),
    ("Could not read the warnings: {e}", "No se pudieron leer los avisos: {e}"),
    ("Role {role_id} added to user {user_id}", "Rol {role_id} añadido a usuario {user_id}"),
    ("Role {role_id} removed from user {user_id}", "Rol {role_id} quitado a usuario {user_id}"),
    ("Timeout removed from user {user_id}", "Silencio quitado a usuario {user_id}"),
    ("User {user_id} timed out for {minutos} minutes", "Usuario {user_id} silenciado por {minutos} minutos"),
    ("Settings saved", "Configuración guardada"),
    ("Full settings saved", "Configuración completa guardada"),
    ("Could not locate ARGUS: {e}", "No se pudo localizar ARGUS: {e}"),
    ("Unknown agent", "Agente desconocido"),
    ("The skill already exists: confirm to overwrite it", "La skill ya existe: confirma para sobrescribirla"),
    ("Invalid skill path", "Ruta de skill no válida"),
    ("Could not create the skills folder: {e}", "No se pudo crear la carpeta de skills: {e}"),
    ("Could not write the skill: {e}", "No se pudo escribir la skill: {e}"),
    ("Could not find your home folder", "No se encontró tu carpeta personal"),
    ("Could not locate the ARGUS folder", "No se pudo localizar la carpeta de ARGUS"),
    ("Could not detect AI agents: {e}", "No se pudo detectar los agentes de IA: {e}"),
    ("The AI agent is not signed in. Sign in with its command line and try again", "El agente de IA no tiene la sesión iniciada. Inicia sesión desde su línea de comandos e inténtalo de nuevo"),
    ("The AI agent has no quota left. Wait for it to reset or pick another model", "El agente de IA se ha quedado sin cuota. Espera a que se restablezca o elige otro modelo"),
    ("The selected model is not valid for this agent", "El modelo elegido no es válido para este agente"),
    ("AI agent error: {e}", "Error del agente de IA: {e}"),
    ("The AI agent stopped unexpectedly", "El agente de IA se detuvo de forma inesperada"),
    ("An AI agent is already running", "Ya hay un agente de IA en ejecución"),
    ("Could not start the AI agent: {e}", "No se pudo iniciar el agente de IA: {e}"),
    ("Could not prepare the AI agent folder: {e}", "No se pudo preparar la carpeta del agente de IA: {e}"),
    ("Unknown assistant mode: {modo}", "Modo del asistente desconocido: {modo}"),
    ("Unknown assistant type: {otro}", "Tipo de asistente desconocido: {otro}"),
    ("The moderation assistant has no Plan mode", "El asistente de moderación no tiene modo Plan"),
    ("The moderation assistant can only propose moderation actions, not {herramienta}", "El asistente de moderación solo puede proponer acciones de moderación, no {herramienta}"),
    ("{agente} is not installed", "{agente} no está instalado"),
    ("Unknown AI agent: {otro}", "Agente de IA desconocido: {otro}"),
    ("No active server: choose one first", "No hay servidor activo: elige uno primero"),
    ("The assistant did not include a plan in its answer", "El asistente no incluyó ningún plan en su respuesta"),
    ("The assistant's plan is empty", "El plan del asistente está vacío"),
    ("The assistant did not include a proposal in its answer", "El asistente no incluyó ninguna propuesta en su respuesta"),
    ("The assistant's proposal is not valid JSON: {e}", "La propuesta del asistente no es un JSON válido: {e}"),
    ("The assistant's proposal has no actions", "La propuesta del asistente no tiene acciones"),
    ("The assistant's proposal is not valid: {e}", "La propuesta del asistente no es válida: {e}"),
    ("The plan has no structure to build from", "El plan no tiene estructura desde la que construir"),
    ("Could not write {n} channel descriptions: {e}", "No se pudieron escribir {n} descripciones de canal: {e}"),
    ("The AI did not write a description for: {canales}", "La IA no escribió descripción para: {canales}"),
    ("The AI reply has no JSON object", "La respuesta de la IA no contiene un objeto JSON"),
    ("The AI reply is not valid JSON: {e}", "La respuesta de la IA no es un JSON válido: {e}"),
    ("Bot updated","Bot actualizado"),
    ("Bot deleted", "Bot eliminado"),
    ("The bot is not registered", "El bot no está registrado"),
    ("Active bot changed", "Bot activo cambiado"),
    ("Template saved", "Plantilla guardada"),
    ("Template deleted", "Plantilla eliminada"),
    ("Template not found", "Plantilla no encontrada"),
    ("Monitoring settings saved", "Configuración de vigilancia guardada"),
    ("Server settings saved", "Ajustes del servidor guardados"),
    ("CLI agent saved", "Agente CLI guardado"),
    ("Context cleared", "Contexto limpiado"),
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
    ("Añadir reacciones", "Add Reactions"),
    ("Expulsar miembros", "Kick Members"),
    ("Banear miembros", "Ban Members"),
    ("Moderar miembros", "Timeout Members"),
    ("Ver registro de auditoría", "View Audit Log"),
    ("Crear/Gestionar eventos", "Create/Manage Events"),
    ("Crear encuestas", "Create Polls"),
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
    fn los_avisos_del_plan_compilado_salen_en_espanol() {
        assert_eq!(
            tr(
                "es",
                "Line 4: channel «sala» is repeated in «Zona»; skipped"
            ),
            "Línea 4: el canal «sala» está repetido en «Zona»; se omite"
        );
        assert_eq!(
            tr(
                "es",
                "Category «Grande» can hold at most 50 channels (Discord's limit); 70 channels were skipped. Split them into several categories"
            ),
            "La categoría «Grande» admite como mucho 50 canales (límite de Discord); se omitieron 70 canales. Repártelos en varias categorías"
        );
        assert!(tr(
            "es",
            "Role «Fantasma» is not in the plan nor on the server; its permissions on «x» were skipped"
        )
        .starts_with("El rol «Fantasma» no está"));
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

#[cfg(test)]
mod tests_textos_del_plan {
    use super::tr;

    #[test]
    fn las_acciones_del_plan_se_traducen() {
        assert_eq!(
            tr("es", "Create channel «lab-01» in «Labs»"),
            "Crear el canal «lab-01» en «Labs»"
        );
        assert_eq!(
            tr("es", "Set permissions of «Guardia» on «celda-1»"),
            "Permisos de «Guardia» en «celda-1»"
        );
        assert_eq!(tr("es", "Create role «O5»"), "Crear el rol «O5»");
        assert_eq!(
            tr("es", "Create category «Labs»"),
            "Crear la categoría «Labs»"
        );
        assert_eq!(
            tr("es", "Build from the structured plan: 87 actions"),
            "Construir desde el plan estructurado: 87 acciones"
        );
        assert_eq!(tr("en", "Create role «O5»"), "Create role «O5»");
    }
    #[test]
    fn hecho_en_singular_y_grupos_de_borrado() {
        assert_eq!(
            tr("es", "Done: 1 change, 0 with errors"),
            "Hecho: 1 cambio, 0 con error"
        );
        assert_eq!(
            tr("es", "Done: 3 changes, 1 with errors"),
            "Hecho: 3 cambios, 1 con error"
        );
        assert_eq!(tr("es", "Channels to delete: 1"), "Canales a borrar: 1");
    }
}
