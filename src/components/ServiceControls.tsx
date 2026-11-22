import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { t } from '../i18n';
import { controlesServicio, estadoInicial, resumenServicio } from '../servicio';
import type { EstadoServicio } from '../servicio';
import type { DiagnosticCheck } from '../diagnostics';
import { Button } from './ui';

const mensajeError = (e: unknown) => (typeof e === 'string' ? e : e instanceof Error ? e.message : t('Unknown error'));

function etiqueta(name: string) {
  const labels: Record<string, string> = {
    service_process: t('Service'), service_interface: t('Interface link'), service_gateway: t('Service Gateway'),
  };
  return labels[name] ?? name;
}

/** Controles del servicio sin ventana: opcional, desactivado por defecto. */
export function ServiceControls() {
  const [estado, setEstado] = useState<EstadoServicio>(estadoInicial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [checks, setChecks] = useState<DiagnosticCheck[]>([]);

  const refrescar = useCallback(async () => {
    try { setEstado(await invoke<EstadoServicio>('servicio_estado')); setError(null); }
    catch (e) { setError(mensajeError(e)); }
  }, []);
  useEffect(() => { void refrescar(); }, [refrescar]);

  const accion = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try { await fn(); setError(null); } catch (e) { setError(mensajeError(e)); }
    // El proceso tarda un instante en abrir o cerrar el canal local.
    await new Promise(r => setTimeout(r, 1500));
    await refrescar();
    setBusy(false);
  };
  const diagnosticar = async () => {
    try { setChecks(await invoke<DiagnosticCheck[]>('diagnostico_servicio', { name: null })); }
    catch (e) { setError(mensajeError(e)); }
  };

  const resumen = resumenServicio(estado);
  const estados = {
    stopped: t('Stopped'),
    running_connected: t('Running · Gateway connected'),
    running_gateway_down: t('Running · Gateway down'),
    running_unknown: t('Running · Gateway state unknown'),
  };
  const { canStart, canStop } = controlesServicio(estado);

  return <div>
    <label className="ajustes__interruptor">
      <input type="checkbox" checked={estado.enabled} disabled={busy}
        onChange={e => void accion(() => invoke('set_service_enabled', { enabled: e.target.checked }))} />
      <span>{t('Enable the background service')}
        <small>{t('Off by default. Nothing runs in the background until you enable it and press Start.')}</small></span>
    </label>
    <label className="ajustes__interruptor">
      <input type="checkbox" checked={estado.autostart} disabled={busy}
        onChange={e => void accion(() => invoke('servicio_set_autoarranque', { enabled: e.target.checked }))} />
      <span>{t('Start the service when I sign in')}
        <small>{t('Off by default. Per-user startup entry: no system service and no administrator rights. Turn it off to remove it.')}</small></span>
    </label>
    <p role="status"><strong>{t('Service status')}:</strong> {estados[resumen]}</p>
    {estado.last_error && <p role="alert"><strong>{t('Last error')}:</strong> {estado.last_error}</p>}
    {error && <p role="alert">{error}</p>}
    <Button disabled={busy || !canStart} onClick={() => void accion(() => invoke('servicio_iniciar'))}>{t('Start service')}</Button>{' '}
    <Button disabled={busy || !canStop} onClick={() => void accion(() => invoke('servicio_detener'))}>{t('Stop service')}</Button>{' '}
    <Button disabled={busy} onClick={() => void refrescar()}>{t('Refresh status')}</Button>{' '}
    <Button disabled={busy} onClick={() => void diagnosticar()}>{t('Diagnose service')}</Button>
    {checks.length > 0 && <ul>{checks.map(c => <li key={c.name}><strong>{etiqueta(c.name)}</strong> — {t(c.status)}
      {c.explanation && <p>{c.explanation}</p>}</li>)}</ul>}
  </div>;
}
