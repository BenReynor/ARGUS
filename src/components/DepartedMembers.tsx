import React, { useEffect, useRef, useState } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
import { listChannels, serverSnapshots, type Channel, type ServerSnapshotItem, type ServerSnapshotListing, type ServerSnapshotRequest } from '../api/tauri';
import { Button, Card, Cargando, ErrorPanel, Select } from './ui';
import { t } from '../i18n';

export const DepartedMembers: React.FC<{ botId: string; guildId: string; snapshot: ServerSnapshotItem; onClose: () => void }> = ({ botId, guildId, snapshot, onClose }) => {
  const [data, setData] = useState<ServerSnapshotListing | null>(null);
  const [channels, setChannels] = useState<Channel[]>([]);
  const [channel, setChannel] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);
  const lock = useRef(false);
  const run = async (request: ServerSnapshotRequest) => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError('');
    try {
      const result = await serverSnapshots(botId, guildId, request);
      if (alive.current) setData(previous => ({ ...previous, ...result, departed: result.departed ?? previous?.departed }));
    } catch (e) { if (alive.current) setError(String(e)); }
    finally { lock.current = false; if (alive.current) setBusy(false); }
  };
  useEffect(() => {
    alive.current = true;
    void run({ op: 'departed', id: snapshot.id });
    listChannels(guildId).then(value => { if (alive.current) setChannels(value.filter(c => c.channel_type === 0 || c.channel_type === 5)); }).catch(e => { if (alive.current) setError(String(e)); });
    return () => { alive.current = false; };
  }, [botId, guildId, snapshot.id]);
  const exportMembers = async () => {
    try {
      const path = await save({ defaultPath: 'departed-members.json', filters: [{ name: 'JSON', extensions: ['json'] }] });
      if (path && alive.current) await run({ op: 'export_departed', id: snapshot.id, path });
    } catch (e) { if (alive.current) setError(String(e)); }
  };
  return <Card>
    <h3>{t('Departed members')}: {snapshot.name}</h3>
    <p>{t('Discord cannot add members back. An invitation can only be sent if the bot can DM them, normally through another shared server. Failed DMs are listed below.')}</p>
    {error && <ErrorPanel mensaje={error} onReintentar={() => run({ op: 'departed', id: snapshot.id })} />}
    {busy && <Cargando />}
    <label className="ajustes__interruptor"><input type="checkbox" checked={data?.return_roles ?? false} disabled={busy || !data?.return_roles_ready} onChange={e => run({ op: 'return_roles', enabled: e.target.checked })} /><span>{t('Return their roles when they rejoin')}<small>{t('Uses the last completed restoration for this bot and server. Enabled by default after restoring. Keep ARGUS and the bot running.')}</small></span></label>
    <p>{t('Departed members: {count}', { count: data?.departed?.length ?? 0 })}</p>
    <ul>{data?.departed?.map(m => <li key={m.id}>{m.name} ({m.id}) — {m.roles.map(r => `${r.name} (${r.id})`).join(', ') || t('No roles')}</li>)}</ul>
    <Select aria-label={t('Invitation channel')} value={channel} disabled={busy} onChange={e => setChannel(e.target.value)}><option value="">{t('Choose an invitation channel')}</option>{channels.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}</Select>
    <div className="ajustes__fila">
      <Button disabled={busy || !channel || !data?.departed?.length} onClick={() => run({ op: 'invite_departed', id: snapshot.id, channel_id: channel })}>{t('Invite')}</Button>
      <Button variante="secundario" disabled={busy || !data} onClick={exportMembers}>{t('Export departed members')}</Button>
      <Button variante="secundario" disabled={busy} onClick={onClose}>{t('Close')}</Button>
    </div>
    {data?.invitations && <ul aria-live="polite">{data.invitations.results.map(r => <li key={r.id}>{r.name} ({r.id}): {r.sent ? t('Invitation sent') : t('Could not send invitation: {error}', { error: r.error ?? '' })}</li>)}</ul>}
  </Card>;
};
