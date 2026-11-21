import React, { useEffect, useState } from 'react';
import { AlertCircle, CheckCircle, Clock, HelpCircle, Loader, MinusCircle, XCircle } from 'lucide-react';
import { listOperations } from '../api/tauri';
import { emptyFilters, filterOperations, readFilters, saveFilters } from '../activity';
import type { ActivityFilters, Operation } from '../activity';
import { useStore } from '../store/useStore';
import { t } from '../i18n';
import { Badge, Button, Select } from './ui';
import './Vistas.css';

function Status({ status }: { status: string }) {
  const states: Record<string, { label: string; icon: any; tone: 'neutro' | 'info' | 'exito' | 'aviso' | 'peligro' }> = {
    pending: { label: t('Pending'), icon: Clock, tone: 'neutro' },
    in_progress: { label: t('In progress'), icon: Loader, tone: 'info' },
    verifying: { label: t('Verifying'), icon: Loader, tone: 'info' },
    correct: { label: t('Verified'), icon: CheckCircle, tone: 'exito' },
    unverified: { label: t('Unverified'), icon: HelpCircle, tone: 'aviso' },
    partial: { label: t('Partial'), icon: AlertCircle, tone: 'aviso' },
    failed: { label: t('Failed'), icon: XCircle, tone: 'peligro' },
    skipped: { label: t('Skipped'), icon: MinusCircle, tone: 'aviso' },
    cancelled: { label: t('Cancelled'), icon: XCircle, tone: 'neutro' },
    interrupted: { label: t('Interrupted'), icon: AlertCircle, tone: 'aviso' },
  };
  const item = states[status] ?? { label: t('Unknown status'), icon: HelpCircle, tone: 'neutro' as const };
  const Icon = item.icon;
  return <Badge tono={item.tone}><Icon size={14} aria-hidden="true" /> {item.label}</Badge>;
}

export function ActivityView() {
  const bot = useStore(s => s.config.active_bot_id);
  const guilds = useStore(s => s.config.guilds_cache);
  const [operations, setOperations] = useState<Operation[]>([]);
  const [filters, setFilters] = useState<ActivityFilters>(emptyFilters);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const [reload, setReload] = useState(0);
  useEffect(() => {
    setFilters(bot ? readFilters(localStorage, bot) : emptyFilters);
    setOperations([]);
    if (!bot) return;
    let active = true;
    setLoading(true); setError(false);
    listOperations().then(data => { if (active) setOperations(data); }).catch(() => { if (active) setError(true); }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [bot, reload]);
  const update = (key: keyof ActivityFilters, value: string) => {
    const next = { ...filters, [key]: value }; setFilters(next);
    if (bot) { try { saveFilters(localStorage, bot, next); } catch { /* Storage may be unavailable. */ } }
  };
  const types = [...new Set(operations.flatMap(op => op.steps.map(step => step.action)))].sort();
  const servers = [...new Set(operations.map(op => op.guild_id))].sort();
  const statuses = [...new Set(operations.map(op => op.status))].sort();
  return <div className="vista">
    <h2>{t('Activity')}</h2>
    {!bot ? <p>{t('Select a bot to see its activity.')}</p> : <>
      <div className="logs__filtros">
        <label>{t('Type')} <Select value={filters.type} onChange={e => update('type', e.target.value)}><option value="">{t('All types')}</option>{types.map(type => <option key={type} value={type}>{actionLabel(type)}</option>)}</Select></label>
        <label>{t('Server')} <Select value={filters.guild} onChange={e => update('guild', e.target.value)}><option value="">{t('All servers')}</option>{servers.map(id => <option key={id} value={id}>{guilds.find(g => g.id === id)?.name ?? id}</option>)}</Select></label>
        <label>{t('Status')} <Select value={filters.status} onChange={e => update('status', e.target.value)}><option value="">{t('All statuses')}</option>{statuses.map(status => <option key={status} value={status}>{statusLabel(status)}</option>)}</Select></label>
        <Button onClick={() => setReload(n => n + 1)}>{t('Refresh')}</Button>
      </div>
      {loading ? <p>{t('Loading activity…')}</p> : error ? <p role="alert">{t('Could not load activity.')}</p> : filterOperations(operations, filters).length === 0 ? <p>{t('No activity matches these filters.')}</p> : <ul>{filterOperations(operations, filters).map(op => <li key={op.id}>
        <p><Status status={op.status} /> · {guilds.find(g => g.id === op.guild_id)?.name ?? op.guild_id} · <time dateTime={op.started_at}>{new Date(op.started_at).toLocaleString()}</time></p>
        <ul>{op.steps.map(step => <li key={step.step_index}>{actionLabel(step.action)} — <Status status={step.status} /></li>)}</ul>
      </li>)}</ul>}
    </>}
  </div>;
}
function statusLabel(status: string): string {
  switch (status) {
    case 'pending': return t('Pending'); case 'in_progress': return t('In progress');
    case 'verifying': return t('Verifying'); case 'correct': return t('Verified');
    case 'unverified': return t('Unverified'); case 'partial': return t('Partial');
    case 'failed': return t('Failed'); case 'skipped': return t('Skipped');
    case 'cancelled': return t('Cancelled'); case 'interrupted': return t('Interrupted');
    default: return t('Unknown status');
  }
}

function actionLabel(action: string): string {
  switch (action) {
    case 'crear_canal': return t('Create channel');
    case 'editar_canal': return t('Edit channel');
    case 'borrar_canal': return t('Delete channel');
    case 'ordenar_canales': return t('Reorder channels');
    case 'crear_rol': return t('Create role');
    case 'editar_rol': return t('Edit role');
    case 'borrar_rol': return t('Delete role');
    case 'ordenar_roles': return t('Reorder roles');
    case 'dar_rol': return t('Assign role');
    case 'quitar_rol': return t('Remove role');
    case 'permiso_canal': return t('Set channel permission');
    case 'quitar_permiso_canal': return t('Remove channel permission');
    case 'expulsar': return t('Kick member');
    case 'banear': return t('Ban member');
    case 'desbanear': return t('Unban member');
    case 'silenciar': return t('Timeout member');
    case 'cambiar_apodo': return t('Change nickname');
    case 'editar_servidor': return t('Edit server');
    case 'crear_invitacion': return t('Create invite');
    case 'borrar_invitacion': return t('Delete invite');
    case 'crear_webhook': return t('Create webhook');
    case 'borrar_webhook': return t('Delete webhook');
    case 'borrar_mensaje': return t('Delete message');
    case 'borrar_mensajes': return t('Delete messages');
    case 'fijar_mensaje': return t('Pin message');
    case 'desfijar_mensaje': return t('Unpin message');
    case 'enviar_mensaje': return t('Send message');
    case 'aplicar_restauracion': return t('Apply restoration');
    default: return action;
  }
}
