/** Pestañas de Config y qué secciones contiene cada una. Sin dependencias de UI para poder probarlo. */
export const SETTINGS_TABS = ['bots', 'appearance', 'ai', 'diagnostics', 'backups', 'connectors', 'service'] as const;
export type SettingsTab = (typeof SETTINGS_TABS)[number];
export const DEFAULT_SETTINGS_TAB: SettingsTab = 'bots';
export const SETTINGS_TAB_KEY = 'argus.settings.tab';

/** Secciones de la vista, por pestaña. Cada sección existente aparece en exactamente una. */
export const SETTINGS_SECTIONS: Record<SettingsTab, readonly string[]> = {
  bots: ['bots', 'server', 'general'],
  appearance: ['appearance', 'about'],
  ai: ['ai-limits', 'mcp'],
  diagnostics: ['diagnostics', 'diagnostic-report'],
  backups: ['backups'],
  connectors: ['connectors'],
  service: ['service'],
};

export const isSettingsTab = (v: unknown): v is SettingsTab => typeof v === 'string' && (SETTINGS_TABS as readonly string[]).includes(v);

export function readSettingsTab(storage: Pick<Storage, 'getItem'>): SettingsTab {
  try {
    const v = storage.getItem(SETTINGS_TAB_KEY);
    return isSettingsTab(v) ? v : DEFAULT_SETTINGS_TAB;
  } catch { return DEFAULT_SETTINGS_TAB; }
}
export function saveSettingsTab(storage: Pick<Storage, 'setItem'>, tab: SettingsTab): void {
  try { storage.setItem(SETTINGS_TAB_KEY, tab); } catch { /* sin almacenamiento: no se recuerda */ }
}
/** Pestaña vecina con las flechas del teclado (circular). */
export function moveTab(current: SettingsTab, delta: 1 | -1): SettingsTab {
  const i = SETTINGS_TABS.indexOf(current);
  return SETTINGS_TABS[(i + delta + SETTINGS_TABS.length) % SETTINGS_TABS.length];
}
