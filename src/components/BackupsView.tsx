import React, { useEffect, useRef, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { serverSnapshots, type ServerSnapshotItem, type ServerSnapshotListing, type ServerSnapshotRequest } from '../api/tauri';
import { useActiveGuildId, useConfig } from '../store/useStore';
import { Button, Card, Cargando, Dialogo, ErrorPanel, EstadoVacio, Input } from './ui';
import { t } from '../i18n';
import './Vistas.css';
import { DiffView } from './DiffView';
import { DepartedMembers } from './DepartedMembers';
import { MassKick } from './MassKick';
import { SnapshotRestore } from './SnapshotRestore';

export const BackupsView: React.FC = () => {
  const guildId = useActiveGuildId();
  const botId = useConfig().active_bot_id;
  // Remontar al cambiar el ámbito evita aplicar respuestas de otro servidor.
  if (!botId || !guildId) return <EstadoVacio titulo={t('No server selected')} descripcion={t('Choose a server to manage its snapshots.')} />;
  return <ServerBackups key={`${botId}:${guildId}`} botId={botId} guildId={guildId} />;
};

const ServerBackups: React.FC<{ botId: string; guildId: string }> = ({ botId, guildId }) => {
  const [data, setData] = useState<ServerSnapshotListing | null>(null);
  const [restoring, setRestoring] = useState<ServerSnapshotItem | null>(null);
  const [departed, setDeparted] = useState<ServerSnapshotItem | null>(null);
  const [compared, setCompared] = useState<ServerSnapshotItem | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [keep, setKeep] = useState(7);
  const [rename, setRename] = useState<ServerSnapshotItem | null>(null);
  const [name, setName] = useState('');
  const [deleting, setDeleting] = useState<ServerSnapshotItem | null>(null);
  const mounted = useRef(true);
  const lock = useRef(false);
  const filters = [{ name: 'ARGUS', extensions: ['argussnapshot'] }];
  useEffect(() => {
    mounted.current = true;
    const refresh = () => {
      if (lock.current) return;
      serverSnapshots(botId, guildId, { op: 'list' }).then(value => {
        if (mounted.current) { setData(value); setKeep(value.schedule.keep_last); }
      }).catch(e => { if (mounted.current) setError(String(e)); });
    };
    refresh();
    const subscription = listen<{ bot_id: string; guild_id: string }>('snapshots-updated', event => {
      if (event.payload.bot_id === botId && event.payload.guild_id === guildId) refresh();
    });
    return () => { mounted.current = false; void subscription.then(unlisten => unlisten()); };
  }, [botId, guildId]);

  const run = async (action: () => Promise<ServerSnapshotRequest | null>) => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError('');
    try {
      const request = await action();
      if (!request || !mounted.current) return;
      const result = await serverSnapshots(botId, guildId, request);
      if (mounted.current) {
        setCompared(request.op === 'compare' ? data?.snapshots.find(s => s.id === request.id) ?? null : null);
        setData(result); setKeep(result.schedule.keep_last); setRename(null); setDeleting(null);
      }
    } catch (e) { if (mounted.current) setError(String(e)); }
    finally { lock.current = false; if (mounted.current) setBusy(false); }
  };
  const exportFile = (id: string) => run(async () => {
    const chosen = await save({ defaultPath: 'server.argussnapshot', filters });
    return chosen ? { op: 'export', id, path: chosen.endsWith('.argussnapshot') ? chosen : `${chosen}.argussnapshot` } : null;
  });
  const importFile = () => run(async () => {
    const path = await open({ multiple: false, directory: false, filters });
    return path ? { op: 'import', path } : null;
  });
  return <div className="vista">
    <header className="vista__cabecera"><h2>{t('Backups')}</h2><p>{t('Save a complete server snapshot locally. Imports add a saved snapshot for this server.')}</p></header>
    <Card>
      <div className="ajustes__fila">
        <Button disabled={busy} onClick={() => run(async () => ({ op: 'create' }))}>{t('Create snapshot')}</Button>
        <Button variante="secundario" disabled={busy} onClick={importFile}>{t('Import snapshot')}</Button>
      </div>
      <p>{t('Snapshots include member information and bans. Store exported files securely.')}</p>
      <label className="ajustes__interruptor"><input type="checkbox" checked={data?.schedule.enabled ?? false} disabled={busy || !data} onChange={e => { const enabled = e.target.checked; void run(async () => ({ op: 'schedule', enabled, keep_last: keep })); }} /><span>{t('Daily automatic snapshots')}<small>{t('Runs while ARGUS is open. Retention applies only to automatic snapshots for this server.')}</small></span></label>
      <div className="ajustes__fila">
        <Input type="number" min={1} max={365} etiqueta={t('Automatic snapshots to keep')} value={keep} disabled={busy || !data} onChange={e => setKeep(Number(e.target.value))} />
        <Button variante="secundario" disabled={busy || !data || !Number.isInteger(keep) || keep < 1 || keep > 365} onClick={() => run(async () => ({ op: 'schedule', enabled: data?.schedule.enabled ?? false, keep_last: keep }))}>{t('Save schedule')}</Button>
      </div>
      {data?.schedule.last_error && <p role="alert">{t('Last automatic snapshot failed: {error}', { error: data.schedule.last_error })}</p>}
    </Card>
    <MassKick botId={botId} guildId={guildId} />
    {busy && <p role="status">{t('Processing snapshot…')}</p>}
    {error && <ErrorPanel mensaje={error} onReintentar={() => run(async () => ({ op: 'list' }))} />}
    {!data && !error && <Cargando />}
    {data && data.snapshots.length === 0 && <EstadoVacio titulo={t('No snapshots yet')} descripcion={t('Create a snapshot to save this server’s current configuration.')} />}
    {departed && <DepartedMembers key={departed.id} botId={botId} guildId={guildId} snapshot={departed} onClose={() => setDeparted(null)} />}
    {restoring && <SnapshotRestore key={restoring.id} botId={botId} guildId={guildId} snapshot={restoring} onClose={() => setRestoring(null)} />}
    {compared && data?.comparison && <Card>
      <h3>{t('Snapshot comparison: {name}', { name: compared.name })}</h3>
      <p>{t('Snapshot → current server. Removed lines belong to the snapshot; added lines belong to the current server.')}</p>
      <p role="status">{t('Missing: {missing} · Changed: {changed} · Extra: {extra}', { ...data.comparison.counts })}</p>
      {data.comparison.groups.every(g => g.entries.length === 0) && <p>{t('No differences in channels, roles, permissions or member roles.')}</p>}
      {data.comparison.groups.map(group => <section key={group.kind}>
        <h4>{({ channels: t('Channels'), roles: t('Roles'), permissions: t('Permissions'), member_roles: t('Member roles') })[group.kind]}</h4>
        <p>{t('Missing: {missing} · Changed: {changed} · Extra: {extra}', { ...group.counts })}</p>
        {(['missing', 'changed', 'extra'] as const).map(status => <div key={status}>
          <h5>{({ missing: t('Missing from current server'), changed: t('Changed since snapshot'), extra: t('Extra in current server') })[status]} ({group.counts[status]})</h5>
          {group.entries.filter(e => e.status === status).map(entry => <details key={entry.id}>
            <summary>{entry.label}</summary>
            <DiffView lineas={entry.lineas} label={entry.label} />
          </details>)}
        </div>)}
      </section>)}
      <Button variante="secundario" onClick={() => setCompared(null)}>{t('Close')}</Button>
    </Card>}
    {data?.snapshots.map(snapshot => <Card key={snapshot.id}>
      <h3>{snapshot.name}</h3>
      <p>{new Date(snapshot.created_at).toLocaleString()} · {t('{size} KiB', { size: (snapshot.size_bytes / 1024).toFixed(1) })} · {snapshot.automatic ? t('Automatic') : t('Manual')}</p>
      <p>{t('{channels} channels, {roles} roles, {members} members', snapshot.summary)}</p>
      <p>{t('{emojis} emojis, {stickers} stickers, {webhooks} webhooks, {bans} bans', snapshot.summary)}</p>
      <div className="ajustes__fila">
        <Button variante="secundario" disabled={busy} onClick={() => { setCompared(null); void run(async () => ({ op: 'compare', id: snapshot.id })); }}>{t('Compare')}</Button>
        <Button variante="secundario" disabled={busy} onClick={() => setDeparted(snapshot)}>{t('Departed members')}</Button>
        <Button variante="secundario" disabled={busy || restoring !== null} onClick={() => setRestoring(snapshot)}>{t('Restore')}</Button>
        <Button variante="secundario" disabled={busy} onClick={() => { setRename(snapshot); setName(snapshot.name); }}>{t('Rename')}</Button>
        <Button variante="secundario" disabled={busy} onClick={() => exportFile(snapshot.id)}>{t('Export snapshot')}</Button>
        <Button variante="peligro" disabled={busy} onClick={() => setDeleting(snapshot)}>{t('Delete')}</Button>
      </div>
    </Card>)}
    <Dialogo abierto={rename !== null} titulo={t('Rename snapshot')} onCerrar={() => { if (!busy) setRename(null); }} pie={<Button disabled={!name.trim() || busy} onClick={() => run(async () => rename ? { op: 'rename', id: rename.id, name } : null)}>{t('Save')}</Button>}>
      <Input etiqueta={t('Snapshot name')} maxLength={100} value={name} disabled={busy} onChange={e => setName(e.target.value)} />
      {error && <p role="alert">{error}</p>}
    </Dialogo>
    <Dialogo abierto={deleting !== null} titulo={t('Delete snapshot')} onCerrar={() => { if (!busy) setDeleting(null); }} pie={<><Button variante="secundario" disabled={busy} onClick={() => setDeleting(null)}>{t('Cancel')}</Button><Button variante="peligro" disabled={busy} onClick={() => run(async () => deleting ? { op: 'delete', id: deleting.id } : null)}>{t('Delete')}</Button></>}>
      <p>{t('Delete snapshot “{name}”? This cannot be undone.', { name: deleting?.name ?? '' })}</p>
      {error && <p role="alert">{error}</p>}
    </Dialogo>
  </div>;
};
