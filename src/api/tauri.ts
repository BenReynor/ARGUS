import { invoke } from '@tauri-apps/api/core';
import { listen, Event } from '@tauri-apps/api/event';

import type {
  BotConfig, BotRegistrado, BotStatus, Config, Guild, GuildSettings, Channel, Member, Role, ServerMap,
  AgenteIa, AvisoReciente, Propuesta, EventoPropuesta, ResumenPlan, ResultadoConstruccion, CambiosPlan, EventoPlan, AvanceAccion, ResumenPropuesta, EventoAsistente, ConversacionAsistente,
} from '../types';

export type { BotStatus, Config, Guild, Channel, Member, Role, ServerMap };

// Bot control
export const startBot = () => invoke<BotStatus>('start_bot');
export const stopBot = () => invoke<BotStatus>('stop_bot');
export const getBotStatus = () => invoke<BotStatus>('get_bot_status');

// Discord operations
export const listGuilds = () => invoke<Guild[]>('list_guilds');
export const getActiveGuild = () => invoke<string | null>('get_active_guild');
export const setActiveGuild = (guildId: string) => invoke<string>('set_active_guild', { guildId });
export const guildInfo = (guildId: string) => invoke<Guild>('guild_info', { guildId });
export const botPermissions = (guildId: string) => invoke<string[]>('bot_permissions', { guildId });
export const getGuildSettings = (guildId: string) => invoke<GuildSettings>('get_guild_settings', { guildId });
export const saveGuildSettings = (guildId: string, settings: GuildSettings) =>
  invoke<string>('save_guild_settings', { guildId, settings });
export const listChannels =(guildId: string) => invoke<Channel[]>('list_channels', { guildId: guildId });
export const listMembers = (guildId: string) => invoke<Member[]>('list_members', { guildId: guildId });
export const listRoles = (guildId: string) => invoke<Role[]>('list_roles', { guildId: guildId });
export const sendMessage = (channelId: string, content: string) => invoke<string>('send_message', { channelId: channelId, content });

/** Campos editables de un canal; solo se envían los definidos. `parent_id` vacío saca el canal de su categoría. */
export interface DatosCanal {
  name?: string;
  channel_type?: number;
  parent_id?: string;
  topic?: string;
  nsfw?: boolean;
  rate_limit_per_user?: number;
}
export const createChannel = (guildId: string, datos: DatosCanal) => invoke<Channel>('create_channel', { guildId, datos });
export const updateChannel = (guildId: string, channelId: string, datos: DatosCanal) =>
  invoke<Channel>('update_channel', { guildId, channelId, datos });
export const deleteChannel = (guildId: string, channelId: string) => invoke<string>('delete_channel', { guildId, channelId });
/** Campos editables de un rol; `permissions` es el campo de bits como texto. */
export interface DatosRol {
  name?: string | null;
  color?: number | null;
  hoist?: boolean | null;
  mentionable?: boolean | null;
  permissions?: string | null;
}
export const createRole = (guildId: string, datos: DatosRol) => invoke<Role>('create_role', { guildId, datos });
export const updateRole = (guildId: string, roleId: string, datos: DatosRol) => invoke<Role>('update_role', { guildId, roleId, datos });
export const deleteRole = (guildId: string, roleId: string) => invoke<string>('delete_role', { guildId, roleId });
export const reorderRoles = (guildId: string, positions: [string, number][]) => invoke<string>('reorder_roles', { guildId, positions });
/** Overwrites de canal: `kind` 0 = rol, 1 = miembro; `allow`/`deny` son campos de bits como texto. */
export const setChannelPermission = (guildId: string, channelId: string, targetId: string, kind: number, allow: string, deny: string) =>
  invoke<string>('set_channel_permission', { guildId, channelId, targetId, kind, allow, deny });
export const deleteChannelPermission = (guildId: string, channelId: string, targetId: string, kind: number) =>
  invoke<string>('delete_channel_permission', { guildId, channelId, targetId, kind });
export const sendDM = (userId: string, content: string) => invoke<string>('send_dm', { userId: userId, content });
export const getMemberInfo = (guildId: string, userId: string) => invoke<Member>('get_member_info', { guildId: guildId, userId: userId });
export const addRole = (guildId: string, userId: string, roleId: string) => invoke<string>('add_role', { guildId: guildId, userId: userId, roleId: roleId });
export const removeRole = (guildId: string, userId: string, roleId: string) => invoke<string>('remove_role', { guildId: guildId, userId: userId, roleId: roleId });
export const timeoutMember = (guildId: string, userId: string, durationMinutes: number, reason?: string) => invoke<string>('timeout_member', { guildId: guildId, userId: userId, durationMinutes: durationMinutes, reason: reason ?? null });

// Config
export const getConfig = () => invoke<Config>('get_config');
export const saveConfig = (config: Config) => invoke<string>('save_config', { config });

// Bots: el backend cifra el token (AES-GCM); viaja una sola vez y no se guarda en la UI
export const addBot = (token: string) => invoke<BotRegistrado>('add_bot', { token });
export const updateBot = (bot: BotConfig) => invoke<string>('update_bot', { bot });
export const setActiveBot = (botId: string) => invoke<string>('set_active_bot', { botId });
export const inviteUrl = () => invoke<string>('invite_url');
export const rutaMcp = () => invoke<string>('ruta_mcp');
export const deleteBot =(botId: string) => invoke<string>('delete_bot', { botId });

// Avatar oficial del bot en el CDN de Discord (el hash lo guarda el backend)
export const botAvatarUrl = (bot: BotConfig) =>
  bot.avatar ? `https://cdn.discordapp.com/avatars/${bot.id}/${bot.avatar}.png?size=64` : null;

// Server map
export const getServerMap = () => invoke<ServerMap | null>('get_server_map');
export const refreshServerMap = () => invoke<ServerMap>('refresh_server_map');
export const resyncMap = () => invoke<ServerMap>('resync_map');

// Event listeners
export const onBotStatusChange = (callback: (status: BotStatus) => void) => 
  listen<BotStatus>('bot-status-change', (event: Event<BotStatus>) => callback(event.payload));

export interface SyncProgress {
  guild_id: string;
  step: number;
  total: number;
  message: string;
}

export const onMapUpdated = (callback: () => void) =>
  listen<null>('map-updated', () => callback());

export const onSyncProgress = (callback: (progress: SyncProgress) => void) =>
  listen<SyncProgress>('sync-progress', (event: Event<SyncProgress>) => callback(event.payload));

export const onLog = (callback: (log: string) => void) =>
  listen<string>('bot-log', (event: Event<string>) => callback(event.payload));
// Moderación de miembros
export interface ContextoModeracion { owner_id: string | null; bot_id: string; bot_top_position: number }
export interface Baneo { user_id: string; username: string; reason: string | null }
export const moderationContext = (guildId: string) => invoke<ContextoModeracion>('moderation_context', { guildId });
export const kickMember = (guildId: string, userId: string, reason: string | null) =>
  invoke<string>('kick_member', { guildId, userId, reason });
export const banMember = (guildId: string, userId: string, reason: string | null, deleteMessageDays: number) =>
  invoke<string>('ban_member', { guildId, userId, reason, deleteMessageDays });
export const unbanMember = (guildId: string, userId: string, reason: string | null) =>
  invoke<string>('unban_member', { guildId, userId, reason });
export const listBans = (guildId: string) => invoke<Baneo[]>('list_bans', { guildId });
export const setNickname = (guildId: string, userId: string, nickname: string | null, reason: string | null) =>
  invoke<string>('set_nickname', { guildId, userId, nickname, reason });

// Ajustes del servidor en Discord y registro de auditoría
export const getGuildSettingsDiscord = (guildId: string) =>
  invoke<import('../types').AjustesServidor>('get_guild_settings_discord', { guildId });
export const updateGuildDiscord = (guildId: string, settings: import('../types').AjustesServidor) =>
  invoke<import('../types').AjustesServidor>('update_guild_discord', { guildId, settings });
export const auditLog = (guildId: string) =>
  invoke<import('../types').EntradaAuditoria[]>('audit_log', { guildId });

// Mensajes, invitaciones y webhooks de un canal
export interface MensajeCanal {
  id: string; content: string; author_id: string; author_name: string; author_bot: boolean;
  timestamp: string; pinned: boolean; attachments: { filename: string; url: string }[];
}
export interface Invitacion {
  code: string; url: string; channel_id: string | null; channel_name: string | null; inviter: string | null;
  uses: number; max_uses: number; max_age: number; expires_at: string | null;
}
export interface Webhook { id: string; name: string; channel_id: string | null; url: string | null }
export const channelMessages = (guildId: string, channelId: string) =>
  invoke<MensajeCanal[]>('channel_messages', { guildId, channelId });
export const deleteMessage = (guildId: string, channelId: string, messageId: string) =>
  invoke<string>('delete_message', { guildId, channelId, messageId });
export const bulkDeleteMessages = (guildId: string, channelId: string, messageIds: string[]) =>
  invoke<string>('bulk_delete_messages', { guildId, channelId, messageIds });
export const pinMessage = (guildId: string, channelId: string, messageId: string) =>
  invoke<string>('pin_message', { guildId, channelId, messageId });
export const unpinMessage = (guildId: string, channelId: string, messageId: string) =>
  invoke<string>('unpin_message', { guildId, channelId, messageId });
export const createInvite = (guildId: string, channelId: string, maxAge: number, maxUses: number) =>
  invoke<Invitacion>('create_invite', { guildId, channelId, maxAge, maxUses });
export const listInvites = (guildId: string) => invoke<Invitacion[]>('list_invites', { guildId });
export const deleteInvite = (guildId: string, code: string) => invoke<string>('delete_invite', { guildId, code });
export const listWebhooks = (guildId: string) => invoke<Webhook[]>('list_webhooks', { guildId });
export const createWebhook = (guildId: string, channelId: string, name: string) =>
  invoke<Webhook>('create_webhook', { guildId, channelId, name });
export const deleteWebhook = (guildId: string, webhookId: string) => invoke<string>('delete_webhook', { guildId, webhookId });

// Asistente IA
export const listarAgentesIa = () => invoke<AgenteIa[]>('listar_agentes_ia');
export const enviarAlAsistente = (mensaje: string, agente: string, modelo: string, modo: 'read' | 'plan' | 'execute', plan?: string, tipo?: TipoAsistente, limitePorCanal?: number) =>
  invoke<string | null>('enviar_al_asistente', { mensaje, agente, modelo, modo, plan, tipo, limitePorCanal });
export const listarAvisosRecientes = (guildId: string) => invoke<AvisoReciente[]>('listar_avisos_recientes', { guildId });
export const leerPlanServidor = (guildId: string) => invoke<string>('leer_plan_servidor', { guildId });
export const leerContextoServidor = (guildId: string) => invoke<string>('leer_contexto_servidor', { guildId });
export const guardarContextoServidor = (guildId: string, contenido: string) =>
  invoke<void>('guardar_contexto_servidor', { guildId, contenido });
export const guardarPlanServidor =(guildId: string, contenido: string) =>
  invoke<void>('guardar_plan_servidor', { guildId, contenido });
export type TipoAsistente = 'construccion' | 'moderacion';
export const leerConversacion = (guildId: string, agente?: string, tipo?: TipoAsistente) =>
  invoke<ConversacionAsistente | null>('leer_conversacion', { guildId, agente, tipo });
export const guardarConversacion = (guildId: string, agente: string, modelo: string, mensajes: unknown, tipo?: TipoAsistente) =>
  invoke<void>('guardar_conversacion', { guildId, agente, modelo, mensajes, tipo });
export const nuevaConversacion = (guildId: string, tipo?: TipoAsistente) =>
  invoke<void>('nueva_conversacion', { guildId, tipo });
export const aplicarPropuesta = (propuesta: Propuesta, anteriores: ResumenPropuesta['resultados'] = []) =>
  invoke<ResumenPropuesta>('aplicar_propuesta', { propuesta, anteriores });
export const resumirPlan = (plan: string) => invoke<ResumenPlan>('resumir_plan', { plan });
export interface CosteDescripciones { descripciones: number; llamadas: number; confirmar: boolean }
export const costeDescripcionesPlan = (plan: string) => invoke<CosteDescripciones>('coste_descripciones_plan', { plan });
export const construirDesdePlan = (agente: string, modelo: string, plan?: string, descripcionesConfirmadas?: number) =>
  invoke<ResultadoConstruccion>('construir_desde_plan', { agente, modelo, plan, descripcionesConfirmadas });
export const compararPlan = (actual: string, propuesto: string) =>
  invoke<CambiosPlan>('comparar_plan', { actual, propuesto });
export const onAsistentePlan = (cb: (e: EventoPlan) => void) =>
  listen<EventoPlan>('asistente-plan', (event: Event<EventoPlan>) => cb(event.payload));
export const cancelarAsistente =() => invoke<void>('cancelar_asistente');

export const onAsistenteEvento = (cb: (e: EventoAsistente) => void) =>
  listen<EventoAsistente>('asistente-evento', (event: Event<EventoAsistente>) => cb(event.payload));
export const onAsistentePropuesta = (cb: (e: EventoPropuesta) => void) =>
  listen<EventoPropuesta>('asistente-propuesta', (event: Event<EventoPropuesta>) => cb(event.payload));
export const onAsistenteAccion = (cb: (a: AvanceAccion) => void) =>
  listen<AvanceAccion>('asistente-accion', (event: Event<AvanceAccion>) => cb(event.payload));
export const onAsistenteResumen = (cb: (r: ResumenPropuesta) => void) =>
  listen<ResumenPropuesta>('asistente-resumen', (event: Event<ResumenPropuesta>) => cb(event.payload));

export const keyringAvailable = () => invoke<boolean>('keyring_available');

export interface EstimacionLectura {
  limite: number; canales: number; peticiones: number; requiere_confirmacion: boolean;
}
export const prepararLecturaModeracion = (guildId: string, agente: string, modelo: string, limite: number, confirmar = false, noPreguntar = false) =>
  invoke<EstimacionLectura>('preparar_lectura_moderacion', { guildId, agente, modelo, limite, confirmar, noPreguntar });

export interface BackupPreview {
  id: string; created_at: string; conversations: boolean;
  counts: Record<string, number>; bots: string[];
}
export interface BackupSchedule { enabled: boolean; conversations: boolean; folder: string }
export const exportConfiguration = (path: string, password: string, conversations: boolean) =>
  invoke<void>('export_configuration', { path, password, conversations });
export const previewConfiguration = (path: string, password: string) =>
  invoke<BackupPreview>('preview_configuration', { path, password });
export const importConfiguration = (id: string, replace: boolean) =>
  invoke<void>('import_configuration', { id, replace, confirmed: true });
export const backupSchedule = (enabled?: boolean, password?: string, conversations?: boolean) =>
  invoke<BackupSchedule>('backup_schedule', { enabled: enabled ?? null, password: password ?? null, conversations: conversations ?? null });

export const backupWindowPreferences = (preferences?: { auto_start: boolean; minimize_to_tray: boolean }) =>
  invoke<{ auto_start: boolean; minimize_to_tray: boolean } | null>('backup_window_preferences', { preferences: preferences ?? null });

export interface ServerSnapshotItem {
  id: string;
  name: string;
  created_at: string;
  size_bytes: number;
  summary: { channels: number; roles: number; members: number; emojis: number; stickers: number; webhooks: number; bans: number };
  automatic: boolean;
}
export interface SnapshotCounts { missing: number; changed: number; extra: number }
export interface SnapshotComparison {
  counts: SnapshotCounts;
  groups: {
    kind: 'channels' | 'roles' | 'permissions' | 'member_roles';
    counts: SnapshotCounts;
    entries: { id: string; label: string; status: 'missing' | 'changed' | 'extra'; lineas: import('../types').LineaDiff[] }[];
  }[];
}
export interface RestorePreview {
  id: string; guild_name: string; delete_extra: boolean; estimated_seconds: number;
  steps: { kind: string; target: string; data: unknown; description: string }[];
  unavailable: string[];
}
export const applySnapshotRestore = (id: string, confirmation: string) => invoke<ResumenPropuesta>('apply_snapshot_restore', { id, confirmation });
export const stopSnapshotRestore = (id: string) => invoke<void>('stop_snapshot_restore', { id });
export interface ServerSnapshotListing {
  return_roles: boolean;
  return_roles_ready: boolean;
  departed?: { id: string; name: string; roles: { id: string; name: string }[] }[];
  invitations?: { url: string; results: { id: string; name: string; sent: boolean; error: string | null }[] };
  restore?: RestorePreview;
  comparison?: SnapshotComparison;
  snapshots: ServerSnapshotItem[];
  schedule: { enabled: boolean; keep_last: number; interval_hours: 6 | 12 | 24; last_error: string | null };
}
export type ServerSnapshotRequest =
  | { op: 'list' | 'create' }
  | { op: 'departed'; id: string }
  | { op: 'invite_departed'; id: string; channel_id: string }
  | { op: 'export_departed'; id: string; path: string }
  | { op: 'return_roles'; enabled: boolean }
  | { op: 'rename'; id: string; name: string }
  | { op: 'delete' | 'compare'; id: string }
  | { op: 'restore'; id: string; undo: boolean; delete_extra: boolean }
  | { op: 'export'; id: string; path: string }
  | { op: 'import'; path: string }
  | { op: 'schedule'; enabled: boolean; keep_last: number; interval_hours: 6 | 12 | 24 };
export const serverSnapshots = (botId: string, guildId: string, request: ServerSnapshotRequest): Promise<ServerSnapshotListing> =>
  invoke('server_snapshots', { botId, guildId, request });

// Destructive bulk removal is deliberately exclusive to the desktop app.
export interface MassKickStatus {
  phase: 'verification' | 'preparing' | 'ready' | 'running' | 'completed' | 'stopped' | 'failed';
  key?: string;
  preview?: { guild_name: string; targets: { id: string; name: string }[]; skipped: { id: string; name: string; error: string }[]; roles: { id: string; name: string }[]; excluded_roles: string[] };
  done?: number;
  total?: number;
  results?: { id: string; name: string; ok: boolean; error: string | null }[];
  snapshot_id?: string;
  error?: string;
}
export const appMassKick = (botId: string, guildId: string, request: { op: 'identify' | 'prepare' | 'start' | 'stop' | 'status'; actor?: string; identity_key?: string; identity_code?: string; excluded_roles?: string[]; key?: string; confirmation?: string; count?: number }) => invoke<MassKickStatus | null>('app_mass_kick', { botId, guildId, request });
