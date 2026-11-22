import React, { useEffect, useRef, useState } from 'react';
import { appMassKick, type MassKickStatus } from '../api/tauri';
import { Button, Card, Dialogo, Input } from './ui';
import { t } from '../i18n';

export const MassKick: React.FC<{ botId: string; guildId: string }> = ({ botId, guildId }) => {
  const [status, setStatus] = useState<MassKickStatus | null>(null);
  const [actor, setActor] = useState('');
  const [identityKey, setIdentityKey] = useState('');
  const [code, setCode] = useState('');
  const [excluded, setExcluded] = useState<string[] | undefined>();
  const [confirmation, setConfirmation] = useState('');
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const alive = useRef(true);
  const lock = useRef(false);
  const running = busy || status?.phase === 'running' || status?.phase === 'preparing';
  useEffect(() => {
    alive.current = true;
    let pending = false;
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try { const s = await appMassKick(botId, guildId, { op: 'status' }); if (alive.current) setStatus(s); }
      catch (e) { if (alive.current) setError(String(e)); }
      finally { pending = false; }
    };
    void refresh();
    const timer = window.setInterval(refresh, 700);
    return () => { alive.current = false; window.clearInterval(timer); };
  }, [botId, guildId]);
  const identify = async () => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError(''); setIdentityKey(''); setCode(''); setStep(0);
    try {
      const s = await appMassKick(botId, guildId, { op: 'identify', actor });
      if (alive.current) setIdentityKey(s?.key ?? '');
    } catch (e) { if (alive.current) setError(String(e)); }
    finally { lock.current = false; if (alive.current) setBusy(false); }
  };
  const prepare = async () => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError('');
    try {
      const s = await appMassKick(botId, guildId, { op: 'prepare', identity_key: identityKey, identity_code: code.trim(), excluded_roles: excluded });
      if (alive.current) { setStatus(s); setExcluded(s?.preview?.excluded_roles); setStep(1); setConfirmation(''); }
    } catch (e) { if (alive.current) setError(String(e)); }
    finally { lock.current = false; if (alive.current) setBusy(false); }
  };
  const start = async () => {
    if (lock.current || !status?.preview) return;
    lock.current = true; setBusy(true); setError(''); setStep(0);
    try {
      const s = await appMassKick(botId, guildId, { op: 'start', identity_key: identityKey, identity_code: code.trim(), key: status.key, confirmation, count: status.total });
      if (alive.current) setStatus(s);
    } catch (e) { if (alive.current) setError(String(e)); }
    finally { lock.current = false; if (alive.current) setBusy(false); }
  };
  const stop = async () => {
    try { await appMassKick(botId, guildId, { op: 'stop' }); }
    catch (e) { if (alive.current) setError(String(e)); }
  };
  return <Card>
    <h3>{t('Kick all members')}</h3>
    <p>{t('App only. An automatic snapshot must succeed before any member is kicked. Kicked members cannot be added back automatically.')}</p>
    <p>{t('The owner, this bot and your verified Discord account are always protected. Administrator roles are excluded by default.')}</p>
    <Input etiqueta={t('Your Discord member ID')} value={actor} disabled={running} onChange={e => { setActor(e.target.value); setIdentityKey(''); setCode(''); setStep(0); }} />
    <p>{t('Verify your account using the code sent by this bot to your Discord DMs. The code expires after five minutes. If DMs are unavailable, member removal is blocked.')}</p>
    <div className="ajustes__fila">
      <Button disabled={running || !/^[1-9]\d*$/.test(actor)} onClick={identify}>{t('Send verification code')}</Button>
    </div>
    {identityKey && <Input etiqueta={t('Discord DM verification code')} value={code} disabled={running} onChange={e => setCode(e.target.value)} autoComplete="off" />}
    <div className="ajustes__fila">
      <Button variante="peligro" disabled={running || !identityKey || !code.trim()} onClick={prepare}>{t('Review member removal')}</Button>
    </div>
    {(error || status?.error) && <p role="alert">{error || status?.error}</p>}
    {status && <>
      <p role="status" aria-live="polite">{t('{done} of {total}', { done: status.done ?? 0, total: status.total ?? 0 })} · {({ verification: t('Verification required'), preparing: t('Preparing'), ready: t('Ready'), running: t('Running'), stopped: t('Stopped'), completed: t('Completed'), failed: t('Failed') })[status.phase]}</p>
      {running && <><Button variante="peligro" onClick={stop}>{t('Stop')}</Button><p>{t('Stop prevents the next kick. A request already in progress may finish.')}</p></>}
      {status.snapshot_id && <p>{t('Automatic snapshot saved')}: {status.snapshot_id}</p>}
      {!running && status.preview && <>
        <h4>{t('Excluded roles')}</h4>
        {status.preview.roles.map(r => <label className="ajustes__interruptor" key={r.id}><input type="checkbox" checked={(excluded ?? status.preview?.excluded_roles ?? []).includes(r.id)} onChange={e => { const next = excluded ?? status.preview?.excluded_roles ?? []; setExcluded(e.target.checked ? [...next, r.id] : next.filter(id => id !== r.id)); setStep(0); }} /><span>{r.name}</span></label>)}
        <p>{t('Review again after changing excluded roles.')}</p>
      </>}
      {!running && <>
        <h4>{t('Members that could not be kicked')}</h4>
        <ul>{status.preview?.skipped.map(m => <li key={m.id}>{m.name} ({m.id}): {t('Role is unavailable or above the bot')}</li>)}{status.results?.filter(m => !m.ok).map(m => <li key={m.id}>{m.name} ({m.id}): {m.error}</li>)}</ul>
      </>}
    </>}
    <Dialogo abierto={step === 1} titulo={t('Confirm member removal')} onCerrar={() => setStep(0)} pie={<><Button variante="secundario" onClick={() => setStep(0)}>{t('Cancel')}</Button><Button variante="peligro" disabled={!status?.total} onClick={() => setStep(2)}>{t('Continue')}</Button></>}>
      <p>{t('{count} members will be kicked from {server}.', { count: status?.total ?? 0, server: status?.preview?.guild_name ?? '' })}</p>
    </Dialogo>
    <Dialogo abierto={step === 2} titulo={t('Type the server name to confirm')} onCerrar={() => setStep(0)} pie={<><Button variante="secundario" onClick={() => setStep(0)}>{t('Cancel')}</Button><Button variante="peligro" disabled={running || !status?.total || confirmation !== status.preview?.guild_name} onClick={start}>{t('Kick all members')}</Button></>}>
      <p>{t('{count} members will be kicked from {server}.', { count: status?.total ?? 0, server: status?.preview?.guild_name ?? '' })}</p>
      <Input etiqueta={t('Server name')} value={confirmation} onChange={e => setConfirmation(e.target.value)} />
    </Dialogo>
  </Card>;
};
