import React, { useState } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { keyringAvailable } from '../api/tauri';
import { t } from '../i18n';
import { checkKeyring, diagnosticReport } from '../diagnosticReport';
import { Button, Card } from './ui';

export function DiagnosticReport() {
  const [report, setReport] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  async function prepare() {
    setBusy(true); setError(''); setReport('');
    const outcome = await checkKeyring(keyringAvailable);
    setReport(diagnosticReport(__APP_VERSION__, navigator.platform, outcome, t));
    setBusy(false);
  }
  async function exportReport() {
    try {
      const path = await save({ defaultPath: 'argus-diagnostic.txt', filters: [{ name: 'Text', extensions: ['txt'] }] });
      if (path) await writeTextFile(path, report);
    } catch { setError(t('Could not export the diagnostic report.')); }
  }
  return <Card className="ajustes__seccion ajustes__seccion--ancha">
    <h3>{t('Diagnostic report')}</h3>
    <p>{t('Review the report before saving and sharing it. Only the keyring is checked here.')}</p>
    <Button onClick={() => void prepare()} cargando={busy}>{t('Prepare diagnostic report')}</Button>
    {report && <><pre tabIndex={0} style={{ whiteSpace: 'pre-wrap' }}>{report}</pre>
      <Button onClick={() => void exportReport()}>{t('Export report')}</Button></>}
    {error && <p role="alert">{error}</p>}
  </Card>;
}
