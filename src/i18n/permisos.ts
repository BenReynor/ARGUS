/**
 * Nombre oficial de Discord (en inglés, no se traduce) de cada permiso. La clave
 * es el nombre en español que devuelve el backend / usa GRUPOS_PERMISOS.
 */
const NOMBRE_OFICIAL: Record<string, string> = {
  'Ver canales': 'View Channels',
  'Gestionar canales': 'Manage Channels',
  'Gestionar roles': 'Manage Roles',
  'Gestionar servidor': 'Manage Server',
  'Gestionar webhooks': 'Manage Webhooks',
  'Ver el registro de auditoría': 'View Audit Log',
  'Crear invitaciones': 'Create Invite',
  'Cambiar apodo': 'Change Nickname',
  'Gestionar apodos': 'Manage Nicknames',
  'Expulsar miembros': 'Kick Members',
  'Banear miembros': 'Ban Members',
  'Moderar miembros (timeout)': 'Timeout Members',
  'Moderar miembros': 'Timeout Members',
  'Enviar mensajes': 'Send Messages',
  'Enviar mensajes en hilos': 'Send Messages in Threads',
  'Crear hilos públicos': 'Create Public Threads',
  'Insertar enlaces': 'Embed Links',
  'Adjuntar archivos': 'Attach Files',
  'Añadir reacciones': 'Add Reactions',
  'Leer el historial': 'Read Message History',
  'Leer el historial de mensajes': 'Read Message History',
  'Mencionar @everyone': 'Mention @everyone, @here, and All Roles',
  'Gestionar mensajes': 'Manage Messages',
  'Gestionar hilos': 'Manage Threads',
  'Usar comandos de aplicación': 'Use Application Commands',
  'Conectar': 'Connect',
  'Hablar': 'Speak',
  'Vídeo y pantalla': 'Video',
  'Usar detección de voz': 'Use Voice Activity',
  'Silenciar miembros': 'Mute Members',
  'Ensordecer miembros': 'Deafen Members',
  'Mover miembros': 'Move Members',
  'Gestionar emojis y stickers': 'Manage Expressions',
  'Gestionar eventos': 'Manage Events',
  'Usar emojis externos': 'Use External Emojis',
  'Prioridad de palabra': 'Priority Speaker',
};

/** Nombre oficial en inglés del permiso; si no se conoce, devuelve el recibido. */
export const nombreOficialPermiso = (nombre: string): string => NOMBRE_OFICIAL[nombre] ?? nombre;

/** Permisos agrupados para el editor: [bit de Discord, nombre]. */
export const GRUPOS_PERMISOS: { grupo: string; permisos: [number, string][] }[] = [
  { grupo: 'General', permisos: [[10, 'Ver canales'], [4, 'Gestionar canales'], [28, 'Gestionar roles'], [5, 'Gestionar servidor'], [29, 'Gestionar webhooks'], [7, 'Ver el registro de auditoría']] },
  { grupo: 'Miembros', permisos: [[0, 'Crear invitaciones'], [26, 'Cambiar apodo'], [27, 'Gestionar apodos'], [1, 'Expulsar miembros'], [2, 'Banear miembros'], [40, 'Moderar miembros (timeout)']] },
  { grupo: 'Texto', permisos: [[11, 'Enviar mensajes'], [38, 'Enviar mensajes en hilos'], [35, 'Crear hilos públicos'], [14, 'Insertar enlaces'], [15, 'Adjuntar archivos'], [6, 'Añadir reacciones'], [16, 'Leer el historial'], [17, 'Mencionar @everyone'], [13, 'Gestionar mensajes'], [34, 'Gestionar hilos'], [31, 'Usar comandos de aplicación']] },
  { grupo: 'Voz', permisos: [[20, 'Conectar'], [21, 'Hablar'], [9, 'Vídeo y pantalla'], [25, 'Usar detección de voz'], [22, 'Silenciar miembros'], [23, 'Ensordecer miembros'], [24, 'Mover miembros']] },
  { grupo: 'Avanzado', permisos: [[30, 'Gestionar emojis y stickers'], [33, 'Gestionar eventos'], [18, 'Usar emojis externos'], [8, 'Prioridad de palabra']] },
];
