import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button, Card } from './ui';
import { t } from '../i18n';

type Template = { id: string; name: string; description: string; tags: string; data: string; version: number; created_at: string; updated_at: string };
type Document = { name: string; description: string; roles: { key: string; name: string }[]; channels: { key: string; name: string; kind: string; parent: string | null; overwrites: { role: string }[] }[] };
type TargetPreview = { document: Document; guild_id: string; differences: string[]; conflicts: string[]; actions: Record<string, unknown>[] };
type Guild = { id: string; name: string };
type Preview = { document: Document; name_exists: boolean };
export function TemplateTransfer() {
  const [guilds, setGuilds] = useState<Guild[]>([]);
  const [guildId, setGuildId] = useState('');
  const [target, setTarget] = useState<TargetPreview | null>(null);
  const [selected, setSelected] = useState('');
  const [items, setItems] = useState<Template[]>([]);
  const [data, setData] = useState('');
  const [preview, setPreview] = useState<Preview | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<Template | null>(null);
  const refresh = async () => setItems(await invoke<Template[]>('list_templates'));
  useEffect(() => { refresh().catch(() => setError(t('Could not load templates.'))); }, []);
  const selectTarget = async (id: string) => {
    setSelected(id); setTarget(null); setGuildId(''); setError('');
    try { setGuilds(await invoke<Guild[]>('list_guilds')); } catch { setError(t('Could not load servers.')); }
  };
  const previewTarget = async () => {
    setBusy(true); setTarget(null); setError('');
    try { setTarget(await invoke<TargetPreview>('preview_template_target', { templateId: selected, guildId })); }
    catch { setError(t('Could not preview this template for the selected server.')); } finally { setBusy(false); }
  };
  const [propuesta, setPropuesta] = useState('');
  const apply = async () => {
    setBusy(true); setError(''); setPropuesta('');
    try { await invoke('proponer_plantilla', { templateId: selected, guildId }); setPropuesta(t('The proposal is ready in the assistant above: review it and press Apply.')); }
    catch (e) { setError(`${t('Could not prepare the template proposal.')} ${String(e)}`); } finally { setBusy(false); }
  };
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
  const persist = async () => {
    if (!editing) return;
    setBusy(true); setError('');
    try {
      const document = JSON.parse(editing.data);
      document.name = editing.name; document.description = editing.description;
      const data = JSON.stringify(document);
      await invoke('preview_template_file', { data });
      await invoke('save_template', { template: { ...editing, data } });
      setEditing(null); await refresh();
    } catch { setError(t('Could not save template. Check the name and structure.')); }
    finally { setBusy(false); }
  };
  const duplicate = async (item: Template) => {
    setBusy(true); setError('');
    try {
      const now = new Date().toISOString();
      const copy = { ...item, id: crypto.randomUUID(), created_at: now, updated_at: now };
      await invoke('save_template', { template: copy });
      await refresh();
    } catch { setError(t('Could not duplicate template.')); }
    finally { setBusy(false); }
  };
  const remove = async (item: Template) => {
    if (!window.confirm(t('Delete this template?'))) return;
    setBusy(true); setError('');
    try { await invoke('delete_template', { templateId: item.id }); if (editing?.id === item.id) setEditing(null); await refresh(); }
    catch { setError(t('Could not delete template.')); }
    finally { setBusy(false); }
  };
  return <Card><h3>{t('Template library')}</h3><p>{t('Import and export portable templates.')}</p>
    {error && <p role="alert">{error}</p>}
    <label>{t('Search by name or tag')} <input value={query} onChange={e => setQuery(e.target.value)} /></label>
    {items.filter(item => [item.name, item.tags].some(value => value.toLocaleLowerCase().includes(query.toLocaleLowerCase().trim()))).map(item => <p key={item.id}>{item.name} <Button variante="secundario" disabled={busy} onClick={() => void selectTarget(item.id)}>{t('Preview for server')}</Button> {item.tags && <small>({item.tags})</small>} {item.description && <small>— {item.description}</small>} <small>· {new Date(item.created_at).toLocaleString()}</small> <Button variante="secundario" disabled={busy} onClick={() => setEditing({ ...item })}>{t('Edit')}</Button> <Button variante="secundario" disabled={busy} onClick={() => void duplicate(item)}>{t('Duplicate')}</Button> <Button variante="secundario" disabled={busy} onClick={() => void remove(item)}>{t('Delete')}</Button> <Button variante="secundario" disabled={busy} onClick={() => void download(item.id)}>{t('Export')}</Button></p>)}
    {selected && <div><h4>{t('Preview for server')}</h4>
      <label>{t('Server')} <select value={guildId} onChange={e => { setGuildId(e.target.value); setTarget(null); }}><option value="">{t('Select a server')}</option>{guilds.map(g => <option key={g.id} value={g.id}>{g.name}</option>)}</select></label>
      <Button disabled={!guildId || busy} onClick={() => void previewTarget()}>{t('Preview')}</Button>
      {target && <div><h5>{t('Template tree')}</h5><ul>{target.document.roles.map(r => <li key={r.key}>{r.name}</li>)}{target.document.channels.filter(c => !c.parent).map(c => <li key={c.key}>{c.name} ({c.kind})<ul>{target.document.channels.filter(child => child.parent === c.key).map(child => <li key={child.key}>{child.name} ({child.kind})</li>)}</ul></li>)}</ul>
      <h5>{t('Differences')}</h5><ul>{target.differences.map((d, i) => <li key={i}>{d}</li>)}</ul>
      <h5>{t('Conflicts')}</h5>{target.conflicts.length ? <ul>{target.conflicts.map((c, i) => <li key={i}>{c}</li>)}</ul> : <p>{t('No conflicts')}</p>}
      <h5>{t('Planned actions')}</h5><pre>{JSON.stringify(target.actions, null, 2)}</pre>
      <Button disabled={busy} onClick={() => void apply()}>{t('Prepare proposal to apply')}</Button>
      {propuesta && <p role="status">{propuesta}</p>}</div>}</div>}
    {editing && <div>
      <label>{t('Name')} <input value={editing.name} onChange={e => setEditing({ ...editing, name: e.target.value })} /></label>
      <label>{t('Description')} <input value={editing.description} onChange={e => setEditing({ ...editing, description: e.target.value })} /></label>
      <label>{t('Tags (comma separated)')} <input value={editing.tags} onChange={e => setEditing({ ...editing, tags: e.target.value })} /></label>
      <label>{t('Structure (portable JSON)')} <textarea rows={16} value={editing.data} onChange={e => setEditing({ ...editing, data: e.target.value })} /></label>
      <Button disabled={busy} onClick={() => void persist()}>{t('Save')}</Button> <Button variante="secundario" onClick={() => setEditing(null)}>{t('Cancel')}</Button>
    </div>}
    <label>{t('Choose a template file')} <input type="file" accept="application/json,.json" disabled={busy} onChange={e => { void choose(e.target.files?.[0]); e.target.value = ''; }} /></label>
    {preview && <div><h4>{t('Import preview')}</h4><strong>{preview.document.name}</strong><p>{preview.document.description}</p>
      {preview.name_exists && <p>{t('A template with this name already exists.')}</p>}
      <h5>{t('Roles')}</h5><ul>{preview.document.roles.map(r => <li key={r.key}>{r.name} ({r.key})</li>)}</ul>
      <h5>{t('Channels')}</h5><ul>{preview.document.channels.map(c => <li key={c.key}>{c.name} ({c.kind}) {c.parent && `→ ${c.parent}`} {c.overwrites.map(o => o.role).join(', ')}</li>)}</ul>
      <pre>{JSON.stringify(preview.document, null, 2)}</pre>
      <Button disabled={busy} onClick={() => void save()}>{t('Import template')}</Button> <Button variante="secundario" onClick={() => { setData(''); setPreview(null); }}>{t('Cancel')}</Button></div>}
  </Card>;
}
