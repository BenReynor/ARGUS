import { create } from 'zustand';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { invoke } from '@tauri-apps/api/core';
import { createUpdater, type UpdateState } from '../actualizaciones';

export const useUpdates = create<UpdateState>(() => ({
  checking: false, installing: false, notice: null, status: 'idle',
}));
export const updates = createUpdater({
  current: __APP_VERSION__,
  check: async () => {
    const update = await check({ timeout: 10000 });
    return update && {
      version: update.version,
      body: update.body,
      close: () => update.close(),
      downloadAndInstall: () => update.downloadAndInstall(undefined, { timeout: 300000, restartAfterInstall: true }),
    };
  },
  supported: () => invoke<boolean>('updater_install_supported'),
  restart: relaunch,
  changed: (state) => useUpdates.setState(state),
});
