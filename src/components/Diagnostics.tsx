import { useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { t } from '../i18n';
import { operationProgress } from '../activity';
import { allCorrect, groups, groupsToRun, initialChecks, mergeChecks } from '../diagnostics';
import type { DiagnosticCheck } from '../diagnostics';
import { Button, Card } from './ui';
import { CheckCircle, XCircle, AlertCircle, Loader2, CircleDashed, RefreshCw } from 'lucide-react';

function statusIcon(status: string): any {
  switch (status) {
    case 'correct': return CheckCircle;
    case 'failed': return XCircle;
    case 'partial': case 'unverified': return AlertCircle;
    case 'in_progress': return Loader2;
    default: return CircleDashed;
  }
}
function label(name: string) {
 const labels: Record<string, string> = {
  configuration: t('Configuration'), database: t('Database'), keyring: t('Keyring'),
  gateway: t('Gateway'), build_permissions: t('Build permissions'),
  binary: t('Binary'), session: t('Session'), response: t('Response'),
};
  const parts = name.split('_');
  if (labels[name]) return labels[name];
  return `${parts[0].toUpperCase()} · ${labels[parts[1]] ?? parts[1]}`;
}
export function Diagnostics() {
  const [checks, setChecks] = useState(initialChecks);
  const [busy, setBusy] = useState(false);
  const cancelled = useRef(false);
  const run = async (only?: string) => {
    cancelled.current = false; setBusy(true);
    const selected = groupsToRun(only);
    if (!only) setChecks(initialChecks());
    for (const group of selected) {
      if (cancelled.current) break;
      const names: readonly string[] = only ? [only] : groups[group];
      setChecks(old => mergeChecks(old, names.map(name => ({ name, status: 'in_progress' }))));
      try {
        const result = await invoke<DiagnosticCheck[]>(group, { name: only ?? null });
        setChecks(old => mergeChecks(old, result, only));
        setChecks(old => mergeChecks(old, names.filter(name => !result.some(r => r.name === name)).map(name => ({ name, status: 'unverified', explanation: t('This check was not completed.') }))));
      } catch {
        setChecks(old => mergeChecks(old, names.map(name => ({ name, status: 'unverified', explanation: t('Could not run this check. Review the relevant settings and retry.') }))));
      }
    }
    if (cancelled.current) setChecks(old => old.map(c => c.status === 'pending' || c.status === 'in_progress' ? { ...c, status: 'cancelled' } : c));
    setBusy(false);
  };
  const progress = operationProgress(checks);
  return <Card><h3>{t('Diagnostics')}</h3>
    <Button disabled={busy} onClick={() => void run()}>{t('Run diagnostics')}</Button>{' '}
    {busy && <Button onClick={() => { cancelled.current = true; void invoke('cancelar_diagnostico_discord'); }}>{t('Cancel')}</Button>}
    <p role="status">{t('Checks completed: {done} of {total}', { done: progress.done, total: progress.total })}</p>
    {allCorrect(checks) && <p>{t('All checks passed.')}</p>}
    <table className="diag-tabla">
      <thead><tr>
        <th scope="col">{t('Component')}</th>
        <th scope="col">{t('Status')}</th>
        <th scope="col">{t('Last checked')}</th>
        <th scope="col"><span className="vista__solo-lector">{t('Retry check')}</span></th>
      </tr></thead>
      <tbody>{checks.map(c => {
        const Icono = statusIcon(c.status);
        return <tr key={c.name} data-testid={`diag-${c.name}`}>
          <th scope="row">{label(c.name)}{c.explanation && <small className="diag-tabla__nota">{c.explanation}</small>}</th>
          <td><span className={`diag-estado diag-estado--${c.status}`}><Icono size={14} aria-hidden="true" /> {t(c.status)}</span></td>
          <td>{c.checked_at ? new Date(c.checked_at).toLocaleString() : '—'}</td>
          <td><Button variante="secundario" tamano="sm" disabled={busy} title={t('Retry check')} aria-label={`${t('Retry check')}: ${label(c.name)}`} onClick={() => void run(c.name)}><RefreshCw size={14} /></Button></td>
        </tr>;
      })}</tbody>
    </table>
  </Card>;
}
