export interface Finding {
  tipo: 'role_grant' | 'channel_overwrite_grant' | 'privacy_discrepancy';
  role_id?: string; role_name?: string; channel_id?: string; target_id?: string; target_type?: number;
  permission?: string; permission_bit?: number; evidence_bits?: string; permissions_bits?: string;
  member_id?: string; expected_can_view?: boolean; actual_can_view?: boolean;
  expectation_source?: string; expectation_evidence?: string; checked_at: string;
}
export interface AuditReport {
  guild_id: string; checked_at: string; incomplete: boolean; missing_data: string[]; findings: Finding[];
}
export type Severity = 'high' | 'medium';
export interface FindingFilters { channel: string; role: string; severity: string }
export function findingSeverity(f: Finding): Severity {
  return f.tipo === 'privacy_discrepancy' || f.permission === 'Administrator' ? 'high' : 'medium';
}
export function findingRole(f: Finding): string | undefined {
  return f.tipo === 'role_grant' ? f.role_id : f.tipo === 'channel_overwrite_grant' && f.target_type === 0 ? f.target_id : undefined;
}
export function filterFindings(findings: Finding[], filters: FindingFilters): Finding[] {
  return findings.filter(f => (!filters.channel || f.channel_id === filters.channel)
    && (!filters.role || findingRole(f) === filters.role)
    && (!filters.severity || findingSeverity(f) === filters.severity));
}

export interface FindingGroup { kind: 'role' | 'channel' | 'other'; key: string; findings: Finding[] }
const rank = (s: Severity) => (s === 'high' ? 0 : 1);

/** Agrupa por rol (concesiones de rol y overwrites de rol) o, si no hay rol, por canal. */
export function groupFindings(findings: Finding[]): FindingGroup[] {
  const groups = new Map<string, FindingGroup>();
  for (const f of findings) {
    const role = findingRole(f);
    const kind = role ? 'role' : f.channel_id ? 'channel' : 'other';
    const key = role ?? f.channel_id ?? '';
    const id = `${kind}:${key}`;
    const g = groups.get(id) ?? { kind, key, findings: [] };
    g.findings.push(f);
    groups.set(id, g);
  }
  const worst = (g: FindingGroup) => Math.min(...g.findings.map(f => rank(findingSeverity(f))));
  const order = { role: 0, channel: 1, other: 2 };
  return [...groups.values()].sort((a, b) => worst(a) - worst(b) || order[a.kind] - order[b.kind]);
}

export interface AuditSummary { high: number; medium: number; total: number; complete: boolean }
export function summarize(report: AuditReport): AuditSummary {
  const high = report.findings.filter(f => findingSeverity(f) === 'high').length;
  return { high, medium: report.findings.length - high, total: report.findings.length, complete: !report.incomplete };
}

/** Corrección preparada por el backend con el motor de propuestas. */
export interface PreparedCorrection {
  hallazgo: Finding;
  propuesta: import('./types').Propuesta;
  validacion: import('./types').Validacion;
  antes: string[]; despues: string[];
}

/** Hallazgos que tienen corrección automática (los de privacidad no). */
export const canCorrect = (f: Finding) => f.tipo === 'role_grant' || f.tipo === 'channel_overwrite_grant';

const sameFinding = (a: Finding, b: Finding) =>
  a.tipo === b.tipo && a.role_id === b.role_id && a.channel_id === b.channel_id
  && a.target_id === b.target_id && a.permission_bit === b.permission_bit;

/** Resultado tras aplicar y volver a auditar: solo es «verified» si todo se
 * aplicó, la auditoría está completa y el hallazgo ya no aparece. */
export function correctionOutcome(after: AuditReport | null, finding: Finding, done: number, total: number): 'verified' | 'partial' {
  if (!after || after.incomplete || total === 0 || done !== total) return 'partial';
  return after.findings.some(f => sameFinding(f, finding)) ? 'partial' : 'verified';
}
