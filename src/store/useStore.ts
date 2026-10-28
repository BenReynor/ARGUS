import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { guildInfo, botPermissions, getGuildSettings } from '../api/tauri';
import type { GuildSettings, Config, UiSettings, BotStatus, ServerMap, Channel, Member, Role, Guild } from '../types';

/** Datos del servidor activo y permisos del bot en él. */
export interface GuildPanel {
  loading: boolean;
  guild: Guild | null;
  /** Permisos efectivos del bot; null mientras no se conocen. */
  permissions: string[] | null;
  error: string | null;
}

/** Permisos que ARGUS pide al invitar al bot (nombres en español, como los da Rust). */
export const PERMISOS_REQUERIDOS: string[] = [
  'Ver canales',
  'Enviar mensajes',
  'Leer el historial de mensajes',
  'Gestionar canales',
  'Gestionar roles',
  'Moderar miembros',
  'Expulsar miembros',
  'Banear miembros',
];

/** Texto que explica por qué una acción está desactivada, o null si se puede usar. */
export const motivoPermisoAusente = (permissions: string[] | null, requerido: string): string | null =>
  permissions && !permissions.includes(requerido)
    ? `El bot no tiene el permiso «${requerido}» en este servidor`
    : null;

export type ToastType = 'info' | 'success' | 'error' | 'warning';
export interface ToastItem { id: number; message: string; type: ToastType }
export interface LogEntry { ts: string; texto: string }

let siguienteToast = 1;
const MAX_TOASTS = 5;

interface AppState {
  // Panel del servidor activo
  guildPanel: GuildPanel;
  loadGuildPanel: (guildId: string | null) => Promise<void>;

  // Ajustes propios del servidor activo
  guildSettings: GuildSettings | null;
  setGuildSettings: (settings: GuildSettings | null) => void;
  loadGuildSettings: (guildId: string | null) => Promise<void>;

  // Config
  config: Config;
  setConfig: (config: Partial<Config>) => void;
  uiSettings: UiSettings;
  setUiSettings: (settings: Partial<UiSettings>) => void;
  loadConfig: () => Promise<void>;
  saveConfig: () => Promise<void>;

  // Bot status
  botStatus: BotStatus;
  setBotStatus: (status: Partial<BotStatus>) => void;

  // Guild activa
  activeGuildId: string | null;
  setActiveGuildId: (id: string | null) => void;

  // Server data
  serverMap: ServerMap | null;
  setServerMap: (map: ServerMap) => void;
  refreshServerMap: () => Promise<void>;

  // UI state
  activeTab: 'dashboard' | 'channels' | 'members' | 'roles' | 'settings' | 'logs';
  setActiveTab: (tab: AppState['activeTab']) => void;
  
  selectedChannel: Channel | null;
  setSelectedChannel: (channel: Channel | null) => void;

  selectedMember: Member | null;
  setSelectedMember: (member: Member | null) => void;

  // Logs
  logs: LogEntry[];
  addLog: (log: string) => void;
  clearLogs: () => void;

  // Notifications
  notifications: ToastItem[];
  showNotification: (message: string, type: ToastType) => void;
  hideNotification: (id: number) => void;

  // Loading states
  isLoading: boolean;
  setIsLoading: (loading: boolean) => void;
}

// Debe coincidir con Config::default() de src-tauri/src/state.rs
const defaultConfig = (): Config => ({
  bots: [],
  active_bot_id: null,
  active_guild_id: null,
  guilds_cache: [],
  multi_guild_mode: false,

  llm_chat_provider: 'nemotron',
  llm_chat_model: 'nvidia/nemotron-3-ultra',
  llm_chat_url: 'https://openrouter.ai/api/v1',
  llm_chat_api_key_ciphertext: '',
  plan_provider: 'auto',
  ask_provider: 'auto',
  cli_agents: {
    opencode: { id: 'opencode', name: 'opencode', mode: 'auto', binary_path: null, model: 'auto', config: {}, enabled: true },
    codex: { id: 'codex', name: 'codex', mode: 'auto', binary_path: null, model: null, config: {}, enabled: true },
    claude: { id: 'claude', name: 'claude', mode: 'auto', binary_path: null, model: 'sonnet', config: {}, enabled: true },
  },

  context_limit_per_user: 10,
  context_limit_per_channel: 50,
  auto_cleanup_enabled: true,
  archive_retention_months: 12,

  vigilance: { preset: '', enabled_events: [], filters: null },
  backup: { enabled: true, interval_hours: 24, retention_count: 30 },

  language: 'es',
  auto_update: true,
  telemetry_opt_in: false,
  anti_spam_cooldown_secs: 30,
  responder_triggers: {
    mention: true,
    slash_command: true,
    prefix: '!',
    allowed_channels: [],
    blocked_channels: [],
    respond_to_bots: false,
    respond_in_dms: true,
  },
  alerts: {
    enabled: true,
    sound: true,
    only_when_unfocused: true,
    telegram_enabled: false,
    telegram_events: ['critical', 'ban', 'role_change'],
  },

  auto_sync_map: true,
  sync_interval_secs: 300,
  bot_engine: 'rust',

  ui_theme: 'discord',
  ui_mode: '',
});

export const useStore = create<AppState>()(
  persist(
    (set, get) => ({
      // Config
      config: defaultConfig(),
      uiSettings: {
        auto_start: false,
        minimize_to_tray: true,
      },
      setUiSettings: (settings) => set((state) => ({ uiSettings: { ...state.uiSettings, ...settings } })),
      setConfig: (config) => set((state) => ({ config: { ...state.config, ...config } })),
      loadConfig: async () => {
        // Cargar desde tauri store
      },
      saveConfig: async () => {
        // Guardar en tauri store
      },

      // Bot status
      botStatus: {
        running: false,
        connected: false,
        bot_name: null,
        guild_name: null,
        guild_id: null,
        member_count: null,
        latency_ms: null,
        last_error: null,
        warning: null,
      },
      setBotStatus: (status) => set((state) => ({ botStatus: { ...state.botStatus, ...status } })),

      // Panel del servidor activo
      guildPanel: { loading: false, guild: null, permissions: null, error: null },
      loadGuildPanel: async (guildId) => {
        if (!guildId) {
          set({ guildPanel: { loading: false, guild: null, permissions: null, error: null } });
          return;
        }
        set({ guildPanel: { loading: true, guild: null, permissions: null, error: null } });
        try {
          const [guild, permissions] = await Promise.all([guildInfo(guildId), botPermissions(guildId)]);
          if (get().activeGuildId !== guildId) return; // el usuario cambió de servidor
          set({ guildPanel: { loading: false, guild, permissions, error: null } });
        } catch (e) {
          if (get().activeGuildId !== guildId) return;
          set({ guildPanel: { loading: false, guild: null, permissions: null, error: String(e) } });
        }
      },

      // Ajustes propios del servidor activo
      guildSettings: null,
      setGuildSettings: (settings) => set({ guildSettings: settings }),
      loadGuildSettings: async (guildId) => {
        set({ guildSettings: null });
        if (!guildId) return;
        try {
          const settings = await getGuildSettings(guildId);
          if (get().activeGuildId !== guildId) return; // el usuario cambió de servidor
          set({ guildSettings: settings });
        } catch {
          // sin ajustes legibles la sección muestra el error al guardar
        }
      },

      // Guild activa
      activeGuildId: null,
      setActiveGuildId: (id) => set({ activeGuildId: id }),

      // Server data
      serverMap: null,
      setServerMap: (map) => set({ serverMap: map }),
      refreshServerMap: async () => {
        // Invocar comando tauri
      },

      // UI state
      activeTab: 'dashboard',
      setActiveTab: (tab) => set({ activeTab: tab }),
      selectedChannel: null,
      setSelectedChannel: (channel) => set({ selectedChannel: channel }),
      selectedMember: null,
      setSelectedMember: (member) => set({ selectedMember: member }),

      // Logs
      logs: [],
      addLog: (log) => set((state) => ({ logs: [...state.logs.slice(-999), { ts: new Date().toISOString(), texto: log }] })),
      clearLogs: () => set({ logs: [] }),

      // Notifications
      notifications: [],
      showNotification: (message, type) =>
        set((state) => ({
          notifications: [...state.notifications, { id: siguienteToast++, message, type }].slice(-MAX_TOASTS),
        })),
      hideNotification: (id) => set((state) => ({ notifications: state.notifications.filter((n) => n.id !== id) })),

      // Loading
      isLoading: false,
      setIsLoading: (loading) => set({ isLoading: loading }),
    }),
    {
      name: 'argus-storage',
      version: 1,
      // v0 guardaba una Config con otra forma (discord_token, server_id...)
      migrate: (persisted: any, version) => {
        if (version < 1 && persisted) {
          const { config: old, ...rest } = persisted;
          return {
            ...rest,
            // el token en claro de v0 se descarta: se reintroduce y el backend lo cifra
            config: { ...defaultConfig(), active_guild_id: old?.server_id ?? defaultConfig().active_guild_id },
            uiSettings: {
              auto_start: old?.auto_start ?? false,
              minimize_to_tray: old?.minimize_to_tray ?? true,
            },
          };
        }
        return persisted;
      },
      partialize: (state) => ({
        config: state.config,
        uiSettings: state.uiSettings,
        activeGuildId: state.activeGuildId,
        activeTab: state.activeTab,
      }),
    }
  )
);

// Selectors
export const useConfig = () => useStore((s) => s.config);
export const useUiSettings = () => useStore((s) => s.uiSettings);
export const useBotStatus = () => useStore((s) => s.botStatus);
export const useGuildPanel = () => useStore((s) => s.guildPanel);
export const useGuildSettings = () => useStore((s) => s.guildSettings);
export const useLoadGuildPanel =() => useStore((s) => s.loadGuildPanel);
export const useActiveGuildId = () => useStore((s) => s.activeGuildId);
export const useSetActiveGuildId = () => useStore((s) => s.setActiveGuildId);
export const useServerMap = () => useStore((s) => s.serverMap);
export const useActiveTab = () => useStore((s) => s.activeTab);
export const useSetActiveTab = () => useStore((s) => s.setActiveTab);
export const useSelectedChannel = () => useStore((s) => s.selectedChannel);
export const useSetSelectedChannel = () => useStore((s) => s.setSelectedChannel);
export const useSelectedMember = () => useStore((s) => s.selectedMember);
export const useSetSelectedMember = () => useStore((s) => s.setSelectedMember);
export const useLogs = () => useStore((s) => s.logs);
export const useAddLog = () => useStore((s) => s.addLog);
export const useClearLogs = () => useStore((s) => s.clearLogs);
export const useNotifications = () => useStore((s) => s.notifications);
export const useShowNotification = () => useStore((s) => s.showNotification);
export const useHideNotification = () => useStore((s) => s.hideNotification);
export const useIsLoading = () => useStore((s) => s.isLoading);