export const DOWNLOAD_URL = 'https://github.com/BenReynor/ARGUS/releases/latest';

// Orden SemVer: los metadatos no cambian la precedencia; release > prerelease.
export function newerVersion(candidate: string, current: string): boolean {
  const parse = (v: string) => /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([\da-zA-Z-]+(?:\.[\da-zA-Z-]+)*))?(?:\+[\da-zA-Z-]+(?:\.[\da-zA-Z-]+)*)?$/.exec(v.replace(/^v/, ''));
  const a = parse(candidate), b = parse(current);
  if (!a || !b) return false;
  if ([a, b].some((parts) => parts[4]?.split('.').some((id) => /^0\d+$/.test(id)))) return false;
  for (let i = 1; i <= 3; i++) {
    if (BigInt(a[i]) !== BigInt(b[i])) return BigInt(a[i]) > BigInt(b[i]);
  }
  if (!a[4] || !b[4]) return !!b[4] && !a[4];
  const x = a[4].split('.'), y = b[4].split('.');
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if (x[i] === undefined || y[i] === undefined) return y[i] === undefined;
    if (x[i] === y[i]) continue;
    const xn = /^\d+$/.test(x[i]), yn = /^\d+$/.test(y[i]);
    if (xn && yn) return BigInt(x[i]) > BigInt(y[i]);
    if (xn !== yn) return !xn;
    return x[i] > y[i];
  }
  return false;
}

export interface UpdatePackage {
  version: string;
  body?: string;
  downloadAndInstall: () => Promise<void>;
  close: () => Promise<void>;
}
export interface UpdateState {
  checking: boolean;
  installing: boolean;
  notice: { version: string; notes: string; manual: boolean } | null;
  status: 'idle' | 'current' | 'unavailable' | 'failed';
}

// El controlador permite probar el consentimiento, los fallos y el aviso sin red.
export function createUpdater(deps: {
  current: string;
  check: () => Promise<UpdatePackage | null>;
  supported: () => Promise<boolean>;
  restart: () => Promise<void>;
  changed: (state: UpdateState) => void;
}) {
  let state: UpdateState = { checking: false, installing: false, notice: null, status: 'idle' };
  let pending: UpdatePackage | null = null;
  const publish = (patch: Partial<UpdateState>) => { state = { ...state, ...patch }; deps.changed(state); };
  return {
    async check(manual = false) {
      if (state.checking || state.installing) return;
      publish({ checking: true, status: 'idle' });
      try {
        const supported = await deps.supported();
        const update = await deps.check();
        const previous = pending;
        pending = null;
        await previous?.close().catch(() => {});
        if (update && newerVersion(update.version, deps.current)) {
          pending = update;
          publish({ notice: { version: update.version, notes: update.body || '', manual: !supported } });
        } else {
          await update?.close();
          publish({ notice: null, status: manual ? 'current' : 'idle' });
        }
      } catch {
        // Sin toast, log ni diálogo al arrancar sin conexión.
        if (manual) publish({ status: 'unavailable' });
      } finally {
        publish({ checking: false });
      }
    },
    async update() {
      if (!pending || !state.notice || state.notice.manual || state.installing || state.checking) return;
      publish({ installing: true, status: 'idle' });
      try {
        // El plugin verifica la firma ANTES de instalar. Un fallo nunca reinicia.
        await pending.downloadAndInstall();
        await deps.restart();
      } catch {
        publish({ status: 'failed' });
      } finally {
        publish({ installing: false });
      }
    },
    dismiss() {
      if (state.installing) return;
      const discarded = pending;
      pending = null;
      publish({ notice: null });
      void discarded?.close().catch(() => {});
    },
  };
}
