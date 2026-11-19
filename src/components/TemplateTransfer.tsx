import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button, Card } from './ui';
import { t } from '../i18n';

type Template = { id: string; name: string };
type Document = { name: string; description: string; roles: { key: string; name: string }[]; channels: { key: string; name: string; kind: string; parent: string | null; overwrites: { role: string }[] }[] };
type Preview = { document: Document; name_exists: boolean };
export function TemplateTransfer() {
  const [items, setItems] = useState<Template[]>([]);
  const [data, setData] = useState('');
  const [preview, setPreview] = useState<Preview | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const refresh = async () => setItems(await invoke<Template[]>('list_templates'));
  useEffect(() => { refresh().catch(() => setError(t('Could not load templates.'))); }, []);
  const choose = async (file: File | undefined) => {
    setPreview(null); setData(''); setError('');
    if (!file) return;
    if (file.size > 1048576) { setError(t('Invalid or unsupported template file')); return; }
    setBusy(true);
    try { const text = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer()); const result = await invoke<Preview>('preview_template_file', { data: text }); setData(text); setPreview(result); }
    catch { setError(t('Invalid or unsupported template file')); } finally { setBusy(false); }
  };
  const save = async () => {
    if (!preview || (preview.name_exists && !window.confirm(t('A template with this name already exists. Import another copy?')))) return;
    setBusy(true); setError('');
    try { await invoke('import_template_file', { data, confirmed: preview.name_exists }); setData(''); setPreview(null); await refresh(); }
    catch { setError(t('Could not import the template. Check for duplicate names and try again.')); } finally { setBusy(false); }
  };
  const download = async (id: string) => {
    setError('');
    try { const text = await invoke<string>('export_template_file', { id }); const url = URL.createObjectURL(new Blob([text], { type: 'application/json' })); const a = document.createElement('a'); a.href = url; a.download = 'argus-template.json'; a.click(); URL.revokeObjectURL(url); }
    catch { setError(t('This template cannot be exported in the portable format.')); }
  };
  return <Card><h3>{t('Template library')}</h3><p>{t('Import and export portable templates.')}</p>
    {error && <p role="alert">{error}</p>}
    {items.map(item => <p key={item.id}>{item.name} <Button variante="secundario" disabled={busy} onClick={() => void download(item.id)}>{t('Export')}</Button></p>)}
    <label>{t('Choose a template file')} <input type="file" accept="application/json,.json" disabled={busy} onChange={e => { void choose(e.target.files?.[0]); e.target.value = ''; }} /></label>
    {preview && <div><h4>{t('Import preview')}</h4><strong>{preview.document.name}</strong><p>{preview.document.description}</p>
      {preview.name_exists && <p>{t('A template with this name already exists.')}</p>}
      <h5>{t('Roles')}</h5><ul>{preview.document.roles.map(r => <li key={r.key}>{r.name} ({r.key})</li>)}</ul>
      <h5>{t('Channels')}</h5><ul>{preview.document.channels.map(c => <li key={c.key}>{c.name} ({c.kind}) {c.parent && `→ ${c.parent}`} {c.overwrites.map(o => o.role).join(', ')}</li>)}</ul>
      <pre>{JSON.stringify(preview.document, null, 2)}</pre>
      <Button disabled={busy} onClick={() => void save()}>{t('Import template')}</Button> <Button variante="secundario" onClick={() => { setData(''); setPreview(null); }}>{t('Cancel')}</Button></div>}
  </Card>;
}
