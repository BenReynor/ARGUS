import React, { useCallback, useEffect, useState } from 'react';
import { CheckCircle, AlertCircle, CircleDashed, Link2, Unlink } from 'lucide-react';
import { t } from '../i18n';
import { Button, Card, Input } from './ui';
import { externalServices, connectorStatus } from '../externalServices';
import type { ConnectorStatus } from '../externalServices';
import { connectorStates, connectConnector, disconnectConnector } from '../api/tauri';
import type { ConnectorStateDto } from '../api/tauri';

const LABEL: Record<ConnectorStatus, string> = {
  not_implemented: 'Not implemented',
  untested: 'Not tested',
  connected: 'Connected',
  expired: 'Expired',
  revoked: 'Revoked',
};
const ICON: Record<ConnectorStatus, any> = {
  not_implemented: CircleDashed,
  untested: CircleDashed,
  connected: CheckCircle,
  expired: AlertCircle,
  revoked: AlertCircle,
};

interface Props {
  guildId: string | null;
  notify: (msg: string, kind: 'success' | 'error') => void;
  /** Ids of the connectors enabled for the active server. */
  enabled: readonly string[];
  toggleDisabled: boolean;
  onToggle: (id: string, on: boolean) => void | Promise<void>;
}

/**
 * One card per external service: status, capabilities, enable switch and connect/disconnect.
 * The pasted credential is cleared right after sending and never read back.
 */
export function ConnectorsPanel({ guildId, notify, enabled, toggleDisabled, onToggle }: Props) {
  const [states, setStates] = useState<ConnectorStateDto[]>([]);
  const [account, setAccount] = useState<Record<string, string>>({});
  const [secret, setSecret] = useState<Record<string, string>>({});
  const [confirming, setConfirming] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    if (!guildId) { setStates([]); return; }
    try { setStates(await connectorStates(guildId)); } catch { setStates([]); }
  }, [guildId]);
  useEffect(() => { void refresh(); }, [refresh]);

  return <div className="conectores">
    {!guildId && <p>{t('Select a server to enable connectors.')}</p>}
    {externalServices.map(service => {
      const st = states.find(s => s.service === service.id);
      const status = connectorStatus(service, st?.status);
      const canConnect = st?.accepts_credential ?? false;
      const Icono = ICON[status];
      return <Card key={service.id} className="ajustes__seccion ajustes__seccion--ancha conector">
        <header>
          <h3>{service.service}</h3>
          <p className="conector__estado" data-testid={`connector-status-${service.id}`}><Icono size={14} aria-hidden="true" /> {t(LABEL[status])}</p>
        </header>
        {st?.account && st.status !== 'none' && <p>{t('Account')}: {st.account}</p>}
        <dl className="conector__capacidades">
          <dt>{t('Method')}</dt><dd>{t(service.method)}</dd>
          <dt>{t('Requirements')}</dt><dd>{t(service.requirements)}</dd>
          <dt>{t('Permissions')}</dt><dd>{t(service.permissions)}</dd>
          <dt>{t('Read tools')}</dt><dd>{t(service.readTools)}</dd>
          <dt>{t('Write tools')}</dt><dd>{t(service.writeTools)}</dd>
          <dt>{t('Limitations')}</dt><dd>{t(service.limitations)}</dd>
        </dl>
        <label>
          <input type="checkbox" disabled={!guildId || toggleDisabled} checked={enabled.includes(service.id)}
            onChange={e => void onToggle(service.id, e.target.checked)} /> {t('Enable for this server')}
        </label>
        {guildId && st && !canConnect && <p>{t('This service does not accept credentials in ARGUS. ARGUS never asks for another bot’s token.')}</p>}
        {guildId && canConnect && st?.status === 'none' && <form onSubmit={async e => {
          e.preventDefault();
          const acc = (account[service.id] ?? '').trim();
          const sec = secret[service.id] ?? '';
          if (!acc || !sec.trim()) return;
          setBusy(true);
          try {
            await connectConnector(service.id, acc, guildId, sec);
            notify(t('Connector connected.'), 'success');
          } catch (err) { notify(String(err), 'error'); }
          finally { setSecret(s => ({ ...s, [service.id]: '' })); setBusy(false); await refresh(); }
        }}>
          <Input aria-label={t('Account')} placeholder={t('Account')} value={account[service.id] ?? ''} onChange={(e: React.ChangeEvent<HTMLInputElement>) => setAccount(a => ({ ...a, [service.id]: e.target.value }))} />
          <Input type="password" autoComplete="off" aria-label={t('Service credential')} placeholder={t('Paste your own API key or service token')} value={secret[service.id] ?? ''} onChange={(e: React.ChangeEvent<HTMLInputElement>) => setSecret(s => ({ ...s, [service.id]: e.target.value }))} />
          <p>{t('The credential is stored encrypted and is never shown again.')}</p>
          <Button type="submit" disabled={busy}><Link2 size={14} /> {t('Connect')}</Button>
        </form>}
        {guildId && st && st.status !== 'none' && st.account && (confirming === service.id
          ? <p>{t('Disconnect this connector and delete its credential?')} <Button disabled={busy} onClick={async () => {
              setBusy(true);
              try { await disconnectConnector(service.id, st.account as string, guildId); notify(t('Connector disconnected.'), 'success'); }
              catch (err) { notify(String(err), 'error'); }
              finally { setConfirming(null); setBusy(false); await refresh(); }
            }}>{t('Confirm disconnect')}</Button> <Button onClick={() => setConfirming(null)}>{t('Cancel')}</Button></p>
          : <div><Button onClick={() => setConfirming(service.id)}><Unlink size={14} /> {t('Disconnect')}</Button></div>)}
      </Card>;
    })}
  </div>;
}
