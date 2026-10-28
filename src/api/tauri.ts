import { invoke } from '@tauri-apps/api/core';
import { listen, Event } from '@tauri-apps/api/event';

import type { BotConfig, BotRegistrado, BotStatus, Config, Guild, GuildSettings, Channel, Member, Role, ServerMap } from '../types';

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
export const listChannels =(guildId: string) => invoke<Channel[]>('list_channels', { guild_id: guildId });
export const listMembers = (guildId: string) => invoke<Member[]>('list_members', { guild_id: guildId });
export const listRoles = (guildId: string) => invoke<Role[]>('list_roles', { guild_id: guildId });
export const sendMessage = (channelId: string, content: string) => invoke<string>('send_message', { channel_id: channelId, content });

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
export const sendDM = (userId: string, content: string) => invoke<string>('send_dm', { user_id: userId, content });
export const getMemberInfo = (guildId: string, userId: string) => invoke<Member>('get_member_info', { guild_id: guildId, user_id: userId });
export const addRole = (guildId: string, userId: string, roleId: string) => invoke<string>('add_role', { guild_id: guildId, user_id: userId, role_id: roleId });
export const removeRole = (guildId: string, userId: string, roleId: string) => invoke<string>('remove_role', { guild_id: guildId, user_id: userId, role_id: roleId });
export const timeoutMember = (guildId: string, userId: string, durationMinutes: number) => invoke<string>('timeout_member', { guild_id: guildId, user_id: userId, duration_minutes: durationMinutes });

// Config
export const getConfig = () => invoke<Config>('get_config');
export const saveConfig = (config: Config) => invoke<string>('save_config', { config });

// Bots: el backend cifra el token (AES-GCM); viaja una sola vez y no se guarda en la UI
export const addBot = (token: string) => invoke<BotRegistrado>('add_bot', { token });
export const updateBot = (bot: BotConfig) => invoke<string>('update_bot', { bot });
export const setActiveBot = (botId: string) => invoke<string>('set_active_bot', { botId });
export const inviteUrl = () => invoke<string>('invite_url');
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