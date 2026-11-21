import React, { useEffect, useState } from 'react';
import { permissionAudit } from '../api/tauri';
import { filterFindings, findingRole, findingSeverity } from '../auditFindings';
import type { AuditReport, Finding } from '../auditFindings';
import { useActiveGuildId, useServerMap, useStore } from '../store/useStore';
import { Button, Card, Cargando, ErrorPanel, EstadoVacio, Select } from './ui';
import { t } from '../i18n';


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
  useEffect(() => {
    let current = true;
    setReport(null); setError(''); setChannel(''); setRole(''); setSeverity('');
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
  const renderFinding = (f: Finding, index: number) => {
    const roleId = findingRole(f);
    return <Card key={index}>
      <h3>{f.tipo === 'privacy_discrepancy' ? t('Privacy expectation differs from access') : t('Sensitive permission granted')}</h3>
      <p><strong>{t('Severity')}:</strong> {findingSeverity(f) === 'high' ? t('High') : t('Medium')}</p>
      {f.channel_id && <p><strong>{t('Channel')}:</strong> {channelName(f.channel_id)}</p>}
      {roleId && <p><strong>{t('Role')}:</strong> {f.role_name ?? roleName(roleId)}</p>}
      {f.target_id && f.target_type === 1 && <p>{t('Member ID')}: {f.target_id}</p>}
      {f.member_id && <p>{t('Member ID')}: {f.member_id}</p>}
      {f.permission && <p>{t('Permission')}: {f.permission} · {t('Permission bit')}: {f.permission_bit} · {t('Evidence bits')}: {f.evidence_bits}</p>}
      {f.tipo === 'privacy_discrepancy' && <>
        <p>{t('Expected access')}: {f.expected_can_view ? t('Allowed') : t('Denied')} · {t('Observed access')}: {f.actual_can_view ? t('Allowed') : t('Denied')}</p>
        <p>{t('Expectation source')}: {f.expectation_source} · {t('Evidence')}: {f.expectation_evidence} · {t('Effective permission bits')}: {f.permissions_bits}</p>
      </>}
      <p><strong>{t('Impact')}:</strong> {f.tipo === 'privacy_discrepancy'
        ? f.actual_can_view ? t('This member can view a channel expected to be private.') : t('This member cannot view a channel they were expected to access.')
        : impact(f)}</p>
      <p>{t('Checked at')}: <time dateTime={f.checked_at}>{new Date(f.checked_at).toLocaleString()}</time></p>
    </Card>;
  };
  if (!guild || !bot) return <EstadoVacio titulo={t('Select a bot and server to run the permission audit')} />;
  const filtered = report ? filterFindings(report.findings, { channel, role, severity }) : [];
  const channelIds = [...new Set(report?.findings.flatMap(f => f.channel_id ? [f.channel_id] : []) ?? [])];
  const roleIds = [...new Set(report?.findings.flatMap(f => findingRole(f) ? [findingRole(f)!] : []) ?? [])];
  return <div className="audit-findings">
    <h2>{t('Permission audit')}</h2>
    <p>{t('A point-in-time observation; no findings does not guarantee security.')}</p>
    <Button variante="secundario" onClick={retry} disabled={loading}>{t('Refresh')}</Button>
    {loading && <Cargando />}
    {error && <ErrorPanel mensaje={error} onReintentar={retry} />}
    {report && <>
      {report.incomplete && <p role="status">{t('Audit incomplete. Missing data: {data}', { data: report.missing_data.map(missingDataLabel).join(', ') })}</p>}
      <div className="audit-findings__filters">
        <Select etiqueta={t('Channel')} value={channel} onChange={e => setChannel(e.target.value)}><option value="">{t('All channels')}</option>{channelIds.map(id => <option key={id} value={id}>{channelName(id)}</option>)}</Select>
        <Select etiqueta={t('Role')} value={role} onChange={e => setRole(e.target.value)}><option value="">{t('All roles')}</option>{roleIds.map(id => <option key={id} value={id}>{roleName(id)}</option>)}</Select>
        <Select etiqueta={t('Severity')} value={severity} onChange={e => setSeverity(e.target.value)}><option value="">{t('All severities')}</option><option value="high">{t('High')}</option><option value="medium">{t('Medium')}</option></Select>
      </div>
      {filtered.length ? filtered.map(renderFinding) : <EstadoVacio titulo={t('No findings match these filters')} />}
    </>}
  </div>;
};
