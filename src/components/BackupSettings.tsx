import React, { useEffect, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { backupWindowPreferences, backupSchedule, exportConfiguration, previewConfiguration, importConfiguration, type BackupPreview, type BackupSchedule } from '../api/tauri';
import { t } from '../i18n';
import { Button, Card, Dialogo, Input, Select } from './ui';
import { useUiSettings, useShowNotification } from '../store/useStore';

export const BackupSettings: React.FC = () => {
  const notify = useShowNotification();
  const uiSettings = useUiSettings();
  const [password, setPassword] = useState('');
  const [repeat, setRepeat] = useState('');
  const [conversations, setConversations] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [preview, setPreview] = useState<BackupPreview | null>(null);
  const [replace, setReplace] = useState(false);
  const [schedule, setSchedule] = useState<BackupSchedule | null>(null);
  useEffect(() => {
    backupSchedule().then(setSchedule).catch(e => setError(String(e)));
  }, [notify]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true); setError('');
    try { await action(); } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const validNewPassword = () => {
    if (password.length < 8) throw new Error(t('Use a backup password with at least 8 characters'));
    if (password !== repeat) throw new Error(t('Backup passwords do not match'));
  };
  const filters = [{ name: 'ARGUS', extensions: ['argusbak'] }];
  const exportFile = () => run(async () => {
    validNewPassword();
    const chosen = await save({ defaultPath: 'configuration.argusbak', filters });
    if (!chosen) return;
    const path = chosen.endsWith('.argusbak') ? chosen : `${chosen}.argusbak`;
    await backupWindowPreferences(uiSettings);
    await exportConfiguration(path, password, conversations);
    setPassword(''); setRepeat('');
    notify(t('Configuration exported'), 'success');
  });
  const importFile = () => run(async () => {
    const path = await open({ multiple: false, directory: false, filters });
    if (!path) return;
    setPreview(await previewConfiguration(path, password));
    setReplace(false); setPassword(''); setRepeat('');
  });
  const restore = () => run(async () => {
    if (!preview) return;
    await importConfiguration(preview.id, replace);
    setPreview(null);
    // Recarga también formularios, idioma, servidor activo y cachés del frontend.
    window.location.reload();
  });
  const changeSchedule = () => run(async () => {
    const enabled = !schedule?.enabled;
    if (enabled) { validNewPassword(); await backupWindowPreferences(uiSettings); }
    setSchedule(await backupSchedule(enabled, enabled ? password : undefined, conversations));
    setPassword(''); setRepeat('');
    notify(t('Backup schedule saved'), 'success');
  });

  return <Card className="ajustes__seccion ajustes__seccion--ancha">
    <header><h3>{t('Backup')}</h3><p>{t('Portable configuration backups encrypted with your password. Keep the password safe: it cannot be recovered.')}</p></header>
    <div className="ajustes__fila">
      <Input type="password" autoComplete="new-password" etiqueta={t('Backup password')} value={password} onChange={e => setPassword(e.target.value)} disabled={busy} />
      <Input type="password" autoComplete="new-password" etiqueta={t('Confirm backup password (export or automatic backups)')} value={repeat} onChange={e => setRepeat(e.target.value)} disabled={busy} />
    </div>
    <label className="ajustes__interruptor"><input type="checkbox" checked={conversations} disabled={busy} onChange={e => setConversations(e.target.checked)} /><span>{t('Include conversations')}<small>{t('Includes assistant conversations and saved message history.')}</small></span></label>
    <div className="ajustes__fila">
      <Button disabled={busy || password.length < 8} onClick={exportFile}>{t('Export configuration')}</Button>
      <Button variante="secundario" disabled={busy || !password} onClick={importFile}>{t('Import configuration')}</Button>
    </div>
    <p>{t('Includes bots, server personality, context, replies, orders, plans, templates and preferences.')}</p>
    <label className="ajustes__interruptor"><input type="checkbox" checked={schedule?.enabled ?? false} disabled={busy || !schedule} onChange={changeSchedule} /><span>{t('Daily automatic backup')}<small>{t('While ARGUS is open, keeps the last 7 daily backups in the data folder. Uses the password and conversation option selected when enabled.')}</small></span></label>
    {schedule && <p>{t('Backup folder: {path}', { path: schedule.folder })}</p>}
    {busy && <p role="status">{t('Processing backup…')}</p>}
    {error && <p role="alert" className="ajustes__aviso">{error}</p>}
    <Dialogo abierto={preview !== null} titulo={t('Confirm configuration import')} onCerrar={() => { if (!busy) setPreview(null); }}
      pie={<><Button variante="secundario" disabled={busy} onClick={() => setPreview(null)}>{t('Cancel')}</Button><Button cargando={busy} onClick={restore}>{t('Confirm import')}</Button></>}>
      {preview && <>
        <p>{t('Backup created: {date}', { date: new Date(preview.created_at).toLocaleString() })}</p>
        <p>{t('Bots: {names}', { names: preview.bots.join(', ') || t('None') })}</p>
        <ul>
          <li>{t('Bots: {n}', { n: preview.counts.bots ?? 0 })}</li>
          <li>{t('Server settings: {n}', { n: preview.counts.guild_settings ?? 0 })}</li>
          <li>{t('Plans: {n}', { n: preview.counts.plan_servidor ?? 0 })}</li>
          <li>{t('Server contexts: {n}', { n: preview.counts.contexto_servidor ?? 0 })}</li>
          <li>{t('Templates: {n}', { n: preview.counts.templates ?? 0 })}</li>
          <li>{t('Preferences: {n}', { n: (preview.counts.config ?? 0) + (preview.counts.settings ?? 0) })}</li>
          <li>{t('Assistant conversations: {n}', { n: preview.counts.conversacion_asistente ?? 0 })}</li>
          <li>{t('Saved messages: {n}', { n: preview.counts.messages ?? 0 })}</li>
        </ul>
        <Select etiqueta={t('Import mode')} value={replace ? 'replace' : 'merge'} onChange={e => setReplace(e.target.value === 'replace')} disabled={busy}>
          <option value="merge">{t('Merge (keep existing records on conflicts)')}</option>
          <option value="replace">{t('Replace existing configuration')}</option>
        </Select>
        <p>{replace ? t('Replace deletes existing configuration and conversations before restoring this backup. This cannot be undone.') : t('Merge adds missing records and keeps local records with the same key. Server settings and preferences are kept as complete records.')}</p>
        {!preview.conversations && <p>{t('This backup does not include conversations.')}</p>}
        <p>{t('Stop the bot before importing. Confirm only after reviewing this summary.')}</p>
        {error && <p role="alert">{error}</p>}
      </>}
    </Dialogo>
  </Card>;
};
