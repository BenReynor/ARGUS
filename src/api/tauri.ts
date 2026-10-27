import { invoke } from '@tauri-apps/api/core';
import { listen, Event } from '@tauri-apps/api/event';

export interface BotStatus {
  running: boolean;
  connected: boolean;
  guild_name: string | null;
  guild_id: string | null;
  member_count: number | null;
  latency_ms: number | null;
  last_error: string | null;
}

export interface Config {
  discord_token: string;
  server_id: string;
  auto_start: boolean;
  minimize_to_tray: boolean;
}

export interface Guild {
  id: string;
  name: string;
  member_count: number;
  icon: string | null;
}

export interface Channel {
  id: string;
  name: string;
  channel_type: number;
  parent_id: string | null;
  position: number;
}

export interface Member {
  user_id: string;
  username: string;
  nickname: string | null;
  roles: string[];
  bot: boolean;
}

export interface Role {
  id: string;
  name: string;
  color: number;
  position: number;
  permissions: string;
  member_count: number;
  mentionable: boolean;
  hoist: boolean;
}

export interface ServerMap {
  guild: Guild;
  channels: Channel[];
  members: Member[];
  roles: Role[];
  categories: Record<string, Channel[]>;
  last_updated: string;
}

// Bot control
export const startBot = () => invoke<BotStatus>('start_bot');
export const stopBot = () => invoke<BotStatus>('stop_bot');
export const getBotStatus = () => invoke<BotStatus>('get_bot_status');

// Discord operations
export const listGuilds = () => invoke<Guild[]>('list_guilds');
export const getActiveGuild = () => invoke<string | null>('get_active_guild');
export const setActiveGuild = (guildId: string) => invoke<string>('set_active_guild', { guildId });
export const listChannels = (guildId: string) => invoke<Channel[]>('list_channels', { guild_id: guildId });
export const listMembers = (guildId: string) => invoke<Member[]>('list_members', { guild_id: guildId });
export const listRoles = (guildId: string) => invoke<Role[]>('list_roles', { guild_id: guildId });
export const sendMessage = (channelId: string, content: string) => invoke<string>('send_message', { channel_id: channelId, content });
export const sendDM = (userId: string, content: string) => invoke<string>('send_dm', { user_id: userId, content });
export const getMemberInfo = (guildId: string, userId: string) => invoke<Member>('get_member_info', { guild_id: guildId, user_id: userId });
export const addRole = (guildId: string, userId: string, roleId: string) => invoke<string>('add_role', { guild_id: guildId, user_id: userId, role_id: roleId });
export const removeRole = (guildId: string, userId: string, roleId: string) => invoke<string>('remove_role', { guild_id: guildId, user_id: userId, role_id: roleId });
export const timeoutMember = (guildId: string, userId: string, durationMinutes: number) => invoke<string>('timeout_member', { guild_id: guildId, user_id: userId, duration_minutes: durationMinutes });

// Config
export const getConfig = () => invoke<Config>('get_config');
export const saveConfig = (config: Config) => invoke<string>('save_config', { config });

// Server map
export const getServerMap = () => invoke<ServerMap | null>('get_server_map');
export const refreshServerMap = () => invoke<ServerMap>('refresh_server_map');

// Event listeners
export const onBotStatusChange = (callback: (status: BotStatus) => void) => 
  listen<BotStatus>('bot-status-change', (event: Event<BotStatus>) => callback(event.payload));

export const onLog = (callback: (log: string) => void) =>
  listen<string>('bot-log', (event: Event<string>) => callback(event.payload));