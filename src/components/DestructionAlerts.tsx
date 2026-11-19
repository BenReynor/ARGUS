import React, { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useStore } from '../store/useStore';
import { serverSnapshots, type ServerSnapshotItem } from '../api/tauri';
import { SnapshotRestore } from './SnapshotRestore';
import { Button, Card } from './ui';
import { t } from '../i18n';

type Alert = { bot_id: string; guild_id: string; snapshot_id: string; count: number; actors: string[]; detail: string[]; created_at: string };
export const DestructionAlerts: React.FC = () => {
  const bot = useStore(s => s.config.active_bot_id);
  const guild = useStore(s => s.activeGuildId);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [restore, setRestore] = useState<ServerSnapshotItem | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const scope = `${bot}:${guild}`;
  const currentScope = useRef(scope);
  currentScope.current = scope;
  useEffect(() => {
    let alive = true;
    setAlerts([]); setRestore(null); setError(''); setBusy(false);
    if (!bot || !guild) return;
    const reload = () => invoke<Alert[]>('destruction_alerts').then(a => { if (alive) setAlerts(a); }).catch(e => { if (alive) setError(String(e)); });
    // Suscribirse antes de leer para no perder una alerta entre ambas operaciones.
    const subscription = listen<Alert>('destruction-alert', e => {
      if (e.payload.bot_id === bot && e.payload.guild_id === guild) void reload();
    });
    void subscription.then(reload);
    return () => { alive = false; void subscription.then(fn => fn()); };
  }, [bot, guild]);
  const open = async (alert: Alert) => {
    if (!bot || !guild) return;
    setBusy(true); setError('');
    try {
      const result = await serverSnapshots(bot, guild, { op: 'list' });
      if (currentScope.current !== scope) return;
      const snapshot = result.snapshots.find(s => s.id === alert.snapshot_id);
      if (!snapshot) throw new Error(t('Snapshot not found'));
      setRestore(snapshot);
    } catch (e) { if (currentScope.current === scope) setError(String(e)); }
    finally { if (currentScope.current === scope) setBusy(false); }
  };
  return <>
    {alerts.length > 0 && <Card>
      <details open>
        <summary>{t('Mass deletion detected')}</summary>
        <p>{t('Emergency snapshots contain the previous live map: channels, roles, permissions and member roles. Other data is not included.')}</p>
        {alerts.map(a => <div key={a.snapshot_id} role="alert">
          <p><time dateTime={a.created_at}>{new Date(a.created_at).toLocaleString()}</time> · {t('Mass deletion detected: {count} items. Actor: {actor}', { count: a.count, actor: a.actors.join(', ') || t('Unknown actor (audit log unavailable or delayed)') })}</p>
          {a.detail.map((detail, i) => <p key={i}>{detail}</p>)}
          <Button disabled={busy || !!restore} onClick={() => void open(a)}>{t('Restore')}</Button>
        </div>)}
      </details>
    </Card>}
    {error && <p role="alert">{error}</p>}
    {restore && bot && guild && <SnapshotRestore key={`${bot}:${guild}:${restore.id}`} botId={bot} guildId={guild} snapshot={restore} onClose={() => setRestore(null)} />}
  </>;
};
