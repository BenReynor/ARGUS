import { useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { t } from '../i18n';
import { operationProgress } from '../activity';
import { allCorrect, groups, groupsToRun, initialChecks, mergeChecks } from '../diagnostics';
import type { DiagnosticCheck } from '../diagnostics';
import { Button, Card } from './ui';

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
    <ul>{checks.map(c => <li key={c.name}><strong>{label(c.name)}</strong> — {t(c.status)}
      {c.checked_at && <> · {t('Last checked')}: {new Date(c.checked_at).toLocaleString()}</>}
      {c.explanation && <p>{c.explanation}</p>}
      <Button disabled={busy} onClick={() => void run(c.name)}>{t('Retry check')}</Button>
    </li>)}</ul>
  </Card>;
}
