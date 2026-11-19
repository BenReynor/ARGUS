import React, { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { applySnapshotRestore, serverSnapshots, stopSnapshotRestore, type RestorePreview, type ServerSnapshotItem } from '../api/tauri';
import type { AvanceAccion, ResumenPropuesta } from '../types';
import { t } from '../i18n';
import { Button, Card, Input } from './ui';

export const SnapshotRestore: React.FC<{botId: string; guildId: string; snapshot: ServerSnapshotItem; onClose: () => void}> = ({botId, guildId, snapshot, onClose}) => {
  const [undo, setUndo] = useState(false);
  const [extra, setExtra] = useState(false);
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [confirmation, setConfirmation] = useState('');
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [applying, setApplying] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState('');
  const [progress, setProgress] = useState<AvanceAccion | null>(null);
  const [states, setStates] = useState<Record<number, AvanceAccion['estado']>>({});
  const [summary, setSummary] = useState<ResumenPropuesta | null>(null);
  const mounted = useRef(true);
  const lock = useRef(false);
  const active = useRef<string | null>(null);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; if (active.current) void stopSnapshotRestore(active.current); };
  }, []);
  const generate = async () => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError(''); setPreview(null); setProgress(null); setStates({}); setSummary(null); setConfirmation(''); setAcknowledged(false);
    try {
      const result = await serverSnapshots(botId, guildId, {op: 'restore', id: snapshot.id, undo, delete_extra: extra});
      if (mounted.current) setPreview(result.restore ?? null);
    } catch (e) { if (mounted.current) setError(String(e)); }
    finally { lock.current = false; if (mounted.current) setBusy(false); }
  };
  useEffect(() => { void generate(); }, []);
  const apply = async () => {
    if (lock.current || !preview) return;
    lock.current = true; setApplying(true); setError(''); setStopping(false); active.current = preview.id;
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<{id: string; progress: AvanceAccion}>('snapshot-restore-progress', event => {
        if (mounted.current && event.payload.id === preview.id) {
          const advance = event.payload.progress;
          setProgress(advance); setStates(old => ({...old, [advance.indice]: advance.estado}));
        }
      });
      if (!mounted.current) return;
      const result = await applySnapshotRestore(preview.id, confirmation);
      if (mounted.current) {
        setSummary(result); setStates(Object.fromEntries(result.resultados.map(r => [r.indice, r.estado])));
      }
    } catch (e) { if (mounted.current) setError(String(e)); }
    finally { unlisten?.(); active.current = null; lock.current = false; if (mounted.current) { setApplying(false); setStopping(false); } }
  };
  const stop = async () => {
    if (!preview) return;
    setStopping(true);
    try { await stopSnapshotRestore(preview.id); }
    catch (e) { if (mounted.current) { setError(String(e)); setStopping(false); } }
  };
  const kinds: Record<string, string> = {role_create: t('Create role'), role_update: t('Update role'), role_order: t('Restore role order'), channel_create: t('Create channel'), channel_update: t('Update channel'), channel_order: t('Restore channel order'), member_add: t('Add member role'), member_remove: t('Remove member role'), guild_update: t('Restore server settings'), widget_update: t('Restore widget settings'), channel_delete: t('Delete channel'), role_delete: t('Delete role')};
  const disabled = busy || applying;
  const complete = summary?.hechas === preview?.steps.length;
  return <Card>
    <h3>{t('Restore snapshot: {name}', {name: snapshot.name})}</h3>
    <p>{t('Restore generates a proposal. Review it before pressing Apply.')}</p>
    <label><input type="radio" name="restore-mode" checked={!undo} disabled={disabled} onChange={() => { setUndo(false); setPreview(null); }} /> {t('Only missing items')}</label>
    <label><input type="radio" name="restore-mode" checked={undo} disabled={disabled} onChange={() => { setUndo(true); setPreview(null); }} /> {t('Also undo changes')}</label>
    <label className="ajustes__interruptor"><input type="checkbox" checked={extra} disabled={disabled} onChange={e => { setExtra(e.target.checked); setPreview(null); }} /><span>{t('Delete items not in the snapshot')}</span></label>
    <Button disabled={disabled} onClick={() => void generate()}>{t('Generate restore proposal')}</Button>
    {preview && <section aria-label={t('Proposal')}>
      <p>{t('Estimated duration: {seconds} seconds ({count} actions). Discord rate limits may increase this.', {seconds: preview.estimated_seconds, count: preview.steps.length})}</p>
      <h4>{t('Cannot be restored')}</h4>
      <ul>{preview.unavailable.map(w => <li key={w}>{w}</li>)}</ul>
      <ol>{preview.steps.map((s, i) => <li key={i}><details><summary>{kinds[s.kind]} · {s.description} · {states[i] === 'ok' ? t('Completed') : states[i] === 'error' ? t('Failed') : t('Pending')}</summary><pre>{JSON.stringify(s.data, null, 2)}</pre></details></li>)}</ol>
      {preview.delete_extra && <div role="alert">
        <p>{t('Extra channels and roles will be permanently deleted. Messages in deleted channels cannot be recovered.')}</p>
        <Input etiqueta={t('Type the server name: {name}', {name: preview.guild_name})} value={confirmation} disabled={applying} onChange={e => setConfirmation(e.target.value)} />
        <label><input type="checkbox" checked={acknowledged} disabled={applying} onChange={e => setAcknowledged(e.target.checked)} /> {t('I understand that these deletions cannot be undone.')}</label>
      </div>}
      {progress && <div role="status"><progress max={progress.total} value={progress.indice + 1} /> {progress.indice + 1}/{progress.total} · {progress.descripcion}{progress.mensaje && <p role="alert">{progress.mensaje}</p>}</div>}
      {summary && <p role="status">{t('Done: {done} · Failed: {failed} · Remaining: {remaining}', {done: summary.hechas, failed: summary.fallidas, remaining: summary.canceladas})}</p>}
      {applying ? <Button variante="peligro" disabled={stopping} onClick={() => void stop()}>{stopping ? t('Stopping after the current action…') : t('Stop')}</Button> : <Button disabled={busy || complete || preview.steps.length === 0 || (preview.delete_extra && (confirmation !== preview.guild_name || !acknowledged))} onClick={() => void apply()}>{summary ? t('Continue restore') : t('Apply')}</Button>}
    </section>}
    {error && <p role="alert">{error}</p>}
    <Button variante="secundario" disabled={disabled} onClick={onClose}>{t('Close')}</Button>
  </Card>;
};
