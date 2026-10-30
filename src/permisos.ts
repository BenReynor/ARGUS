/**
 * Nombre oficial de Discord (en inglés, no se traduce) de cada permiso. La clave
 * es el nombre que devuelve el backend / usa GRUPOS_PERMISOS.
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
