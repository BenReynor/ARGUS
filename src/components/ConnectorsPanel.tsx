import React, { useCallback, useEffect, useState } from 'react';
import { t } from '../i18n';
import { Button, Input } from './ui';
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

/** Connect/disconnect screen. The pasted credential is cleared right after sending and never read back. */
export function ConnectorsPanel({ guildId, notify }: { guildId: string | null; notify: (msg: string, kind: 'success' | 'error') => void }) {
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

  if (!guildId) return null;
  return <div>
    <h4>{t('Connectors')}</h4>
    {externalServices.map(service => {
      const st = states.find(s => s.service === service.id);
      const status = connectorStatus(service, st?.status);
      const canConnect = st?.accepts_credential ?? false;
      return <article key={`conn-${service.id}`}>
        <h5>{service.service} — <span data-testid={`connector-status-${service.id}`}>{t(LABEL[status])}</span></h5>
        {st?.account && st.status !== 'none' && <p>{t('Account')}: {st.account}</p>}
        {st && !canConnect && <p>{t('This service does not accept credentials in ARGUS. ARGUS never asks for another bot’s token.')}</p>}
        {canConnect && st?.status === 'none' && <form onSubmit={async e => {
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
          <Button type="submit" disabled={busy}>{t('Connect')}</Button>
        </form>}
        {st && st.status !== 'none' && st.account && (confirming === service.id
          ? <p>{t('Disconnect this connector and delete its credential?')} <Button disabled={busy} onClick={async () => {
              setBusy(true);
              try { await disconnectConnector(service.id, st.account as string, guildId); notify(t('Connector disconnected.'), 'success'); }
              catch (err) { notify(String(err), 'error'); }
              finally { setConfirming(null); setBusy(false); await refresh(); }
            }}>{t('Confirm disconnect')}</Button> <Button onClick={() => setConfirming(null)}>{t('Cancel')}</Button></p>
          : <Button onClick={() => setConfirming(service.id)}>{t('Disconnect')}</Button>)}
      </article>;
    })}
  </div>;
}
