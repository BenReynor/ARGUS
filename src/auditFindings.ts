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
