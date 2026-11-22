import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ShieldAlert, TriangleAlert } from 'lucide-react';
import { aplicarPropuesta, permissionAudit } from '../api/tauri';
import { canCorrect, correctionOutcome, filterFindings, findingRole, findingSeverity, groupFindings, summarize } from '../auditFindings';
import type { AuditReport, Finding, PreparedCorrection } from '../auditFindings';
import { useActiveGuildId, useServerMap, useStore } from '../store/useStore';
import { Badge, Button, Card, Cargando, ErrorPanel, EstadoVacio, Select } from './ui';
import { CabeceraVista } from './EstadoVista';
import { nombrePermisoApi } from '../i18n/permisos';
import { idiomaActual, t } from '../i18n';
import './Vistas.css';

function missingDataLabel(value: string): string {
  switch (value) {
    case 'roles': return t('Roles');
    case 'channels': return t('Channels');
    case 'members': return t('Members');
    case 'owner': return t('Server owner');
    case 'role permissions': return t('Role permissions');
    case 'channel overwrites': return t('Channel overwrites');
    case 'member roles': return t('Member roles');
    case 'bot permissions': return t('Bot permissions');
    default: return value;
  }
}

function impact(f: Finding): string {
  switch (f.permission) {
    case 'Administrator': return t('Administrator bypasses channel permission restrictions.');
    case 'ManageMessages': return t('The target can manage other members’ messages where permitted.');
    case 'ManageChannels': return t('The target can create, edit or delete channels where permitted.');
    case 'ManageRoles': return t('The target can manage roles and permissions within Discord hierarchy limits.');
    case 'ManageWebhooks': return t('The target can create and manage webhooks where permitted.');
    case 'KickMembers': return t('The target can kick members subject to Discord hierarchy limits.');
    case 'BanMembers': return t('The target can ban members subject to Discord hierarchy limits.');
    case 'MentionEveryone': return t('The target can mention everyone or all members of a role where permitted.');
    default: return t('This grant allows the target to exercise this sensitive permission within its scope.');
  }
}

export const AuditFindingsView: React.FC = () => {
  const guild = useActiveGuildId();
  const bot = useStore(s => s.config.active_bot_id);
  const map = useServerMap();
  const [report, setReport] = useState<AuditReport | null>(null);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [channel, setChannel] = useState('');
  const [role, setRole] = useState('');
  const [severity, setSeverity] = useState('');
  const [correction, setCorrection] = useState<PreparedCorrection | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ kind: 'verified' | 'partial' | 'error'; text: string } | null>(null);
  useEffect(() => {
    let current = true;
    setReport(null); setError(''); setChannel(''); setRole(''); setSeverity(''); setCorrection(null); setResult(null);
    if (!guild || !bot) { setLoading(false); return () => { current = false; }; }
    setLoading(true);
    permissionAudit(guild).then(result => { if (current) setReport(result); })
      .catch(e => { if (current) setError(typeof e === 'string' ? e : t('Could not run the permission audit')); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [guild, bot, refresh]);
  const channels = map?.guild.id === guild ? map.channels : [];
  const roles = map?.guild.id === guild ? map.roles : [];
  const channelName = (id: string) => channels.find(c => c.id === id)?.name ?? id;
  const roleName = (id: string) => roles.find(r => r.id === id)?.name ?? id;
  const retry = () => setRefresh(n => n + 1);
  const prepare = async (f: Finding) => {
    if (!guild || busy) return;
    setBusy(true); setResult(null); setCorrection(null);
    try { setCorrection(await invoke<PreparedCorrection>('prepare_audit_correction', { guildId: guild, finding: f })); }
    catch (e) { setResult({ kind: 'error', text: `${t('No correction can be offered.')} ${String(e)}` }); }
    finally { setBusy(false); }
  };
  const approve = async () => {
    if (!correction || !guild || busy) return;
    const prepared = correction;
    setBusy(true); setResult(null);
    let done = 0; let total = 0;
    try { const r = await aplicarPropuesta(prepared.propuesta); done = r.hechas; total = r.total; } catch { /* resultado parcial */ }
    setCorrection(null);
    let after: AuditReport | null = null;
    try { after = await permissionAudit(guild); setReport(after); } catch { after = null; }
    const outcome = correctionOutcome(after, prepared.hallazgo, done, total);
    setResult(outcome === 'verified'
      ? { kind: 'verified', text: t('Correction verified: the finding is no longer present.') }
      : { kind: 'partial', text: t('Partial result: the correction or its verification could not be completed.') });
    setBusy(false);
  };
  const renderFinding = (f: Finding, index: number) => {
    const roleId = findingRole(f);
    const high = findingSeverity(f) === 'high';
    const permiso = nombrePermisoApi(f.permission, f.permission_bit, idiomaActual());
    const SevIcon: any = high ? ShieldAlert : TriangleAlert;
    return <Card key={index} className="audit-card">
      <div className="audit-card__top">
        <h4 className="audit-card__titulo">{f.tipo === 'privacy_discrepancy' ? t('Privacy expectation differs from access') : permiso}</h4>
        <Badge tono={high ? 'peligro' : 'aviso'}><SevIcon size={12} aria-hidden="true" /> {high ? t('High') : t('Medium')}</Badge>
      </div>
      <p className="audit-card__texto">{f.tipo === 'privacy_discrepancy'
        ? f.actual_can_view ? t('This member can view a channel expected to be private.') : t('This member cannot view a channel they were expected to access.')
        : impact(f)}</p>
      <div className="audit-card__pie">
        {canCorrect(f) && <Button variante="secundario" tamano="sm" disabled={busy} onClick={() => void prepare(f)}>{t('Prepare correction')}</Button>}
        <details className="audit-card__detalle">
          <summary>{t('Technical details')}</summary>
          {f.channel_id && <p>{t('Channel')}: {channelName(f.channel_id)}</p>}
          {roleId && <p>{t('Role')}: {f.role_name ?? roleName(roleId)}</p>}
          {f.target_id && f.target_type === 1 && <p>{t('Member ID')}: {f.target_id}</p>}
          {f.member_id && <p>{t('Member ID')}: {f.member_id}</p>}
          {f.permission && <p>{t('Permission')}: {f.permission} · {t('Permission bit')}: {f.permission_bit} · {t('Evidence bits')}: {f.evidence_bits}</p>}
          {f.tipo === 'privacy_discrepancy' && <>
            <p>{t('Expected access')}: {f.expected_can_view ? t('Allowed') : t('Denied')} · {t('Observed access')}: {f.actual_can_view ? t('Allowed') : t('Denied')}</p>
            <p>{t('Expectation source')}: {f.expectation_source} · {t('Evidence')}: {f.expectation_evidence} · {t('Effective permission bits')}: {f.permissions_bits}</p>
          </>}
          <p>{t('Checked at')}: <time dateTime={f.checked_at}>{new Date(f.checked_at).toLocaleString()}</time></p>
        </details>
      </div>
    </Card>;
  };
  if (!guild || !bot) return <EstadoVacio titulo={t('Select a bot and server to run the permission audit')} />;
  const filtered = report ? filterFindings(report.findings, { channel, role, severity }) : [];
  const groups = groupFindings(filtered);
  const summary = report ? summarize(report) : null;
  const channelIds = [...new Set(report?.findings.flatMap(f => f.channel_id ? [f.channel_id] : []) ?? [])];
  const roleIds = [...new Set(report?.findings.flatMap(f => findingRole(f) ? [findingRole(f)!] : []) ?? [])];
  return <div className="vista audit-findings">
    <CabeceraVista titulo={t('Permission audit')}>
      <Button variante="secundario" onClick={retry} cargando={loading}>{t('Refresh')}</Button>
    </CabeceraVista>
    <p className="vista__nota">{t('A point-in-time observation; no findings does not guarantee security.')}</p>
    {loading && <Cargando />}
    {error && <ErrorPanel mensaje={error} onReintentar={retry} />}
    {report && summary && <>
      <Card className="audit-resumen">
        <div className="audit-resumen__fila" role="status">
          <Badge tono={summary.high ? 'peligro' : 'neutro'}><ShieldAlert size={12} aria-hidden="true" /> {summary.high} {t('High')}</Badge>
          <Badge tono={summary.medium ? 'aviso' : 'neutro'}><TriangleAlert size={12} aria-hidden="true" /> {summary.medium} {t('Medium')}</Badge>
          <Badge tono={summary.complete ? 'exito' : 'aviso'}>{summary.complete ? t('Report complete') : t('Report incomplete')}</Badge>
        </div>
        {report.incomplete && <p>{t('Audit incomplete. Missing data: {data}', { data: report.missing_data.map(missingDataLabel).join(', ') })}</p>}
      </Card>
      <div className="audit-findings__filters">
        <Select etiqueta={t('Channel')} value={channel} onChange={e => setChannel(e.target.value)}><option value="">{t('All channels')}</option>{channelIds.map(id => <option key={id} value={id}>{channelName(id)}</option>)}</Select>
        <Select etiqueta={t('Role')} value={role} onChange={e => setRole(e.target.value)}><option value="">{t('All roles')}</option>{roleIds.map(id => <option key={id} value={id}>{roleName(id)}</option>)}</Select>
        <Select etiqueta={t('Severity')} value={severity} onChange={e => setSeverity(e.target.value)}><option value="">{t('All severities')}</option><option value="high">{t('High')}</option><option value="medium">{t('Medium')}</option></Select>
      </div>
      {result && <p role={result.kind === 'verified' ? 'status' : 'alert'}>{result.text}</p>}
      {correction && <Card>
        <h3>{t('Proposed correction')}</h3>
        <p>{correction.propuesta.resumen}</p>
        <p><strong>{t('Before')}:</strong> {correction.antes.join(', ') || t('None')}</p>
        <p><strong>{t('After')}:</strong> {correction.despues.join(', ') || t('None')}</p>
        {correction.validacion.errores.length > 0 && <ul role="alert">{correction.validacion.errores.map((e, i) => <li key={i}>{e.mensaje}</li>)}</ul>}
        <Button disabled={busy || correction.validacion.errores.length > 0} onClick={() => void approve()}>{t('Approve and apply')}</Button>{' '}
        <Button variante="secundario" disabled={busy} onClick={() => setCorrection(null)}>{t('Discard')}</Button>
      </Card>}
      {groups.length ? groups.map(g => <section key={`${g.kind}:${g.key}`} className="audit-grupo">
        <h3 className="audit-grupo__titulo">{g.kind === 'role' ? `${t('Role')}: ${roleName(g.key)}` : g.kind === 'channel' ? `${t('Channel')}: ${channelName(g.key)}` : t('Other findings')}</h3>
        <div className="audit-grupo__lista">{g.findings.map(renderFinding)}</div>
      </section>) : <EstadoVacio titulo={t('No findings match these filters')} />}
    </>}
  </div>;
};
