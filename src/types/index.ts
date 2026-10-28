// Refleja src-tauri/src/state.rs y db.rs. Un `Option<T>` de Rust llega como `T | null`.
export interface BotConfig {
  id: string;
  name: string;
  token_ciphertext: string;
  avatar: string | null;
  application_id: string | null;
  allowed_guilds: string[];
  created_at: string;
  updated_at: string;
}

export interface CliAgentConfig {
  id: string;
  name: string;
  mode: string; // "auto" | "manual"
  binary_path: string | null;
  model: string | null;
  config: unknown;
  enabled: boolean;
}

export interface VigilanceConfig {
  preset: string;
  enabled_events: string[];
  filters: unknown;
}

export interface ResponderTriggers {
  mention: boolean;
  slash_command: boolean;
  prefix: string | null;
  allowed_channels: string[];
  blocked_channels: string[];
  respond_to_bots: boolean;
  respond_in_dms: boolean;
}

export interface AlertsConfig {
  enabled: boolean;
  sound: boolean;
  only_when_unfocused: boolean;
  telegram_enabled: boolean;
  telegram_events: string[];
}

export interface BackupConfig {
  enabled: boolean;
  interval_hours: number;
  retention_count: number;
}

export interface Config {
  bots: BotConfig[];
  active_bot_id: string | null;
  active_guild_id: string | null;
  guilds_cache: Guild[];
  multi_guild_mode: boolean;

  llm_chat_provider: string;
  llm_chat_model: string;
  llm_chat_url: string;
  llm_chat_api_key_ciphertext: string;
  plan_provider: string;
  ask_provider: string;
  cli_agents: Record<string, CliAgentConfig>;

  context_limit_per_user: number;
  context_limit_per_channel: number;
  auto_cleanup_enabled: boolean;
  archive_retention_months: number;

  vigilance: VigilanceConfig;
  backup: BackupConfig;

  language: string;
  auto_update: boolean;
  telemetry_opt_in: boolean;
  anti_spam_cooldown_secs: number;
  responder_triggers: ResponderTriggers;
  alerts: AlertsConfig;

  auto_sync_map: boolean;
  sync_interval_secs: number;
  bot_engine: string;

  ui_theme: string;
  ui_mode: string;
}

// Ajustes solo de la interfaz: no existen en la Config de Rust.
export interface UiSettings {
  auto_start: boolean;
  minimize_to_tray: boolean;
}

export interface BotStatus {
  running: boolean;
  connected: boolean;
  bot_name: string | null;
  guild_name: string | null;
  guild_id: string | null;
  member_count: number | null;
  latency_ms: number | null;
  last_error: string | null;
  warning: string | null;
}

export interface BotRegistrado extends BotConfig {
  warning: string | null;
}

/** Ajustes propios de un servidor (sincronizado con `GuildSettings` de db.rs). */
export interface GuildSettings {
  alias: string | null;
  notes: string;
  vigilance_enabled: boolean;
  personality_enabled: boolean;
  templates_enabled: boolean;
}

export interface Guild {
  id: string;
  name: string;
  member_count: number;
  icon: string | null;
  joined_at: string | null;
}

export interface Channel {
  id: string;
  name: string;
  channel_type: number;
  parent_id: string | null;
  position: number;
  overwrites: Overwrite[] | null;
  permissions_bitfield: string | null;
}

export interface Overwrite {
  id: string;
  type_: number; // 0 = rol, 1 = miembro
  allow: string;
  deny: string;
}

export interface Member {
  user_id: string;
  username: string;
  nickname: string | null;
  roles: string[];
  bot: boolean;
  joined_at: string | null;
  avatar: string | null;
  flags: number | null;
}

export interface Role {
  id: string;
  name: string;
  color: number;
  position: number;
  permissions: string;
  permissions_bitfield: string | null;
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