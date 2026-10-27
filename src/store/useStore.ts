import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { Config, BotStatus, ServerMap, Channel, Member, Role, Guild } from '../types';

interface AppState {
  // Config
  config: Config;
  setConfig: (config: Partial<Config>) => void;
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
  logs: string[];
  addLog: (log: string) => void;
  clearLogs: () => void;

  // Notifications
  notification: { message: string; type: 'info' | 'success' | 'error' | 'warning' } | null;
  showNotification: (message: string, type: 'info' | 'success' | 'error' | 'warning') => void;
  hideNotification: () => void;

  // Loading states
  isLoading: boolean;
  setIsLoading: (loading: boolean) => void;
}

export const useStore = create<AppState>()(
  persist(
    (set, get) => ({
      // Config
      config: {
        discord_token: '',
        server_id: '1541513296375717921',
        auto_start: false,
        minimize_to_tray: true,
      },
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
        guild_name: null,
        guild_id: null,
        member_count: null,
        latency_ms: null,
        last_error: null,
      },
      setBotStatus: (status) => set((state) => ({ botStatus: { ...state.botStatus, ...status } })),

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
      addLog: (log) => set((state) => ({ logs: [...state.logs.slice(-999), log] })),
      clearLogs: () => set({ logs: [] }),

      // Notifications
      notification: null,
      showNotification: (message, type) => set({ notification: { message, type } }),
      hideNotification: () => set({ notification: null }),

      // Loading
      isLoading: false,
      setIsLoading: (loading) => set({ isLoading: loading }),
    }),
    {
      name: 'argus-storage',
      partialize: (state) => ({
        config: state.config,
        activeGuildId: state.activeGuildId,
        activeTab: state.activeTab,
      }),
    }
  )
);

// Selectors
export const useConfig = () => useStore((s) => s.config);
export const useBotStatus = () => useStore((s) => s.botStatus);
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
export const useNotification = () => useStore((s) => s.notification);
export const useShowNotification = () => useStore((s) => s.showNotification);
export const useHideNotification = () => useStore((s) => s.hideNotification);
export const useIsLoading = () => useStore((s) => s.isLoading);