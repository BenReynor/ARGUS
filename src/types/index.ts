export interface Config {
  discord_token: string;
  server_id: string;
  auto_start: boolean;
  minimize_to_tray: boolean;
}

export interface BotStatus {
  running: boolean;
  connected: boolean;
  guild_name: string | null;
  guild_id: string | null;
  member_count: number | null;
  latency_ms: number | null;
  last_error: string | null;
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

export type ChannelType = 
  | 'GUILD_TEXT' 
  | 'DM' 
  | 'GUILD_VOICE' 
  | 'GROUP_DM' 
  | 'GUILD_CATEGORY' 
  | 'GUILD_NEWS' 
  | 'GUILD_STORE' 
  | 'GUILD_NEWS_THREAD' 
  | 'GUILD_PUBLIC_THREAD' 
  | 'GUILD_PRIVATE_THREAD' 
  | 'GUILD_STAGE_VOICE' 
  | 'GUILD_DIRECTORY' 
  | 'GUILD_FORUM' 
  | 'GUILD_MEDIA';

export const CHANNEL_TYPE_LABELS: Record<number, string> = {
  0: 'Texto',
  2: 'Voz',
  4: 'Categoría',
  5: 'Anuncio',
  10: 'Hilo de anuncio',
  11: 'Hilo público',
  12: 'Hilo privado',
  13: 'Escenario',
  15: 'Foro',
  16: 'Media',
};