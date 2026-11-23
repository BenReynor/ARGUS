import React, { useEffect, useRef, useState } from 'react';
import { AlertCircle, CheckCircle, Clock, HelpCircle, Inbox, Loader, MinusCircle, XCircle } from 'lucide-react';
import { CabeceraVista } from './EstadoVista';
import { listOperations } from '../api/tauri';
import { emptyFilters, filterOperations, operationProgress, readFilters, saveFilters } from '../activity';
import type { ActivityFilters, Operation } from '../activity';
import { useStore } from '../store/useStore';
import { t } from '../i18n';
import { Badge, Button, EstadoVacio, Select } from './ui';
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
  const selectedId = useStore(s => s.activityOperationId);
  const selectOperation = useStore(s => s.openActivityOperation);
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
  useEffect(() => {
    if (!bot) return;
    let active = true;
    const timer = setInterval(() => {
      if (document.visibilityState === 'visible') listOperations().then(data => { if (active) setOperations(data); }).catch(() => {});
    }, 3000);
    return () => { active = false; clearInterval(timer); };
  }, [bot]);
  const selected = operations.find(op => op.id === selectedId);
  const update = (key: keyof ActivityFilters, value: string) => {
    const next = { ...filters, [key]: value }; setFilters(next);
    if (bot) { try { saveFilters(localStorage, bot, next); } catch { /* Storage may be unavailable. */ } }
  };
  const types = [...new Set(operations.flatMap(op => op.steps.map(step => step.action)))].sort();
  const servers = [...new Set(operations.map(op => op.guild_id))].sort();
  const statuses = [...new Set(operations.map(op => op.status))].sort();
  const visibles = filterOperations(operations, filters);
  return <div className="vista">
    <CabeceraVista titulo={t('Activity')} />
    {!bot ? <EstadoVacio icono={<Inbox size={32} aria-hidden="true" />} titulo={t('Select a bot to see its activity.')} /> : <>
      <div className="actividad__filtros" role="group" aria-label={t('Filters')}>
        <label>{t('Type')} <Select value={filters.type} onChange={e => update('type', e.target.value)}><option value="">{t('All types')}</option>{types.map(type => <option key={type} value={type}>{actionLabel(type)}</option>)}</Select></label>
        <label>{t('Server')} <Select value={filters.guild} onChange={e => update('guild', e.target.value)}><option value="">{t('All servers')}</option>{servers.map(id => <option key={id} value={id}>{guilds.find(g => g.id === id)?.name ?? id}</option>)}</Select></label>
        <label>{t('Status')} <Select value={filters.status} onChange={e => update('status', e.target.value)}><option value="">{t('All statuses')}</option>{statuses.map(status => <option key={status} value={status}>{statusLabel(status)}</option>)}</Select></label>
        <Button className="actividad__refrescar" onClick={() => setReload(n => n + 1)}>{t('Refresh')}</Button>
      </div>
      {loading ? <p>{t('Loading activity…')}</p> : error ? <p role="alert">{t('Could not load activity.')}</p> : visibles.length === 0 ? <EstadoVacio icono={<Inbox size={32} aria-hidden="true" />} titulo={t('No activity matches these filters.')} /> : <ul className="actividad__tarjetas">{visibles.map(op => <li key={op.id} className={`actividad__tarjeta${selectedId === op.id ? ' actividad__tarjeta--sel' : ''}`}>
        <Status status={op.status} />
        <span className="actividad__servidor">{guilds.find(g => g.id === op.guild_id)?.name ?? op.guild_id}</span>
        <time className="actividad__fecha" dateTime={op.started_at}>{new Date(op.started_at).toLocaleString()}</time>
        <Button variante="secundario" tamano="sm" onClick={() => selectOperation(op.id)} aria-expanded={selectedId === op.id}>{t('View operation')}</Button>
      </li>)}</ul>}
      {selectedId && !loading && !selected && <p role="status">{t('Operation not found for the selected bot.')}</p>}
      {selected && <OperationDetail operation={selected} />}
    </>}
  </div>;
}
function OperationDetail({ operation }: { operation: Operation }) {
  const { done, total } = operationProgress(operation.steps);
  const detail = useRef<HTMLElement>(null);
  useEffect(() => { detail.current?.focus(); }, [operation.id]);
  return <section ref={detail} aria-label={t('Operation details')} tabIndex={-1}>
    <h3>{t('Operation details')}</h3>
    <div role="status"><Status status={operation.status} /> {operation.reason && <p>{reasonText(operation.reason)}</p>}</div>
    <label>{t('Progress')} <progress value={done} max={total || 1} /> {t('{done} of {total}', { done, total })}</label>
    <ol>{operation.steps.map(step => <li key={step.step_index}>
      {actionLabel(step.action)} — <Status status={step.status} />
      {step.reason && <p role={step.status === 'failed' ? 'alert' : undefined}>{reasonText(step.reason)}</p>}
      {['failed', 'skipped', 'cancelled', 'unverified', 'interrupted'].includes(step.status) && <p>{suggestion(step.reason)}</p>}
    </li>)}</ol>
  </section>;
}
function reasonText(reason: string): string {
  if (reason.startsWith('Discord rejected the request (HTTP ')) return t('Discord rejected the request ({code})', { code: reason.slice('Discord rejected the request ('.length, -1) });
  switch (reason) {
    case 'The bot lacks permission for this action': return t('The bot lacks permission for this action');
    case 'The requested resource was not found or is inaccessible': return t('The requested resource was not found or is inaccessible');
    case 'The request timed out; its outcome may be unknown': return t('The request timed out; its outcome may be unknown');
    case 'A required reference could not be resolved': return t('A required reference could not be resolved');
    case "Discord returned an unreadable response; the outcome may be unknown": return t("Discord returned an unreadable response; the outcome may be unknown");
    case "Could not reach Discord; the outcome may be unknown": return t("Could not reach Discord; the outcome may be unknown");
    case "Discord rate limited the request": return t("Discord rate limited the request");
    case "Discord rejected the bot credential": return t("Discord rejected the bot credential");
    case "The action contains invalid parameters": return t("The action contains invalid parameters");
    case "The bot's role hierarchy does not allow this action": return t("The bot's role hierarchy does not allow this action");
    case "The action failed; its cause could not be safely recorded": return t("The action failed; its cause could not be safely recorded");
    case "Outcome was not recorded": return t("Outcome was not recorded");
    case "Not attempted because the operation stopped": return t("Not attempted because the operation stopped");
    case "Verification in progress": return t("Verification in progress");
    case "Result verified": return t("Result verified");
    case "All results verified": return t("All results verified");
    case "Some steps failed or were skipped": return t("Some steps failed or were skipped");
    case "Some results have not been verified": return t("Some results have not been verified");
    case "Interrupted before the outcome was recorded": return t("Interrupted before the outcome was recorded");
    case "Not attempted before interruption": return t("Not attempted before interruption");
    case "Interrupted before completion; it was not restarted": return t("Interrupted before completion; it was not restarted");
    case "Some actions were accepted; others were not completed": return t("Some actions were accepted; others were not completed");
    case "No actions were accepted; at least one failed": return t("No actions were accepted; at least one failed");
    case "Operation stopped before all steps were attempted": return t("Operation stopped before all steps were attempted");
    case "No actions could be attempted": return t("No actions could be attempted");
    case "Previously accepted; the result has not been verified": return t("Previously accepted; the result has not been verified");
    case "Discord accepted the action; the result has not been verified": return t("Discord accepted the action; the result has not been verified");
    case "The outcome of a timed-out creation is unknown": return t("The outcome of a timed-out creation is unknown");
    default: return reason;
  }
}
function suggestion(reason?: string | null): string {
  const value = (reason ?? '').toLowerCase();
  if (value.includes('permission') || value.includes('hierarchy') || value.includes('403')) return t('Check the bot permissions and role hierarchy, then review the operation before retrying.');
  if (value.includes('not found') || value.includes('reference') || value.includes('404')) return t('Check that the referenced resource exists and is accessible, then review the operation before retrying.');
  if (value.includes('timeout') || value.includes('timed out') || value.includes('unknown') || value.includes('unverified')) return t('Inspect the server to check what changed before retrying.');
  return t('Review the step and server state, correct the cause, and retry only the remaining changes.');
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
    case 'editar_mensaje': return t('Edit message');
    case 'crear_post_foro': return t('Create forum post');
    case 'aplicar_restauracion': return t('Apply restoration');
    default: return action;
  }
}
