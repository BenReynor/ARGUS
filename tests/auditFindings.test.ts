import { test } from 'node:test';
import { deepStrictEqual } from 'node:assert';
import { filterFindings, type Finding } from '../src/auditFindings.ts';

test('channel, role and severity filters intersect and distinguish member overwrites', () => {
  const findings: Finding[] = [
    { tipo: 'role_grant', role_id: 'r1', permission: 'Administrator', checked_at: 'now' },
    { tipo: 'channel_overwrite_grant', channel_id: 'c1', target_id: 'r1', target_type: 0, permission: 'ManageChannels', checked_at: 'now' },
    { tipo: 'channel_overwrite_grant', channel_id: 'c1', target_id: 'r1', target_type: 1, permission: 'Administrator', checked_at: 'now' },
    { tipo: 'channel_overwrite_grant', channel_id: 'c2', target_id: 'r2', target_type: 0, permission: 'Administrator', checked_at: 'now' },
  ];
  deepStrictEqual(filterFindings(findings, { channel: '', role: '', severity: '' }), findings);
  deepStrictEqual(filterFindings(findings, { channel: 'c1', role: '', severity: '' }), findings.slice(1, 3));
  deepStrictEqual(filterFindings(findings, { channel: '', role: 'r1', severity: '' }), findings.slice(0, 2));
  deepStrictEqual(filterFindings(findings, { channel: '', role: '', severity: 'high' }), [findings[0], findings[2], findings[3]]);
  deepStrictEqual(filterFindings(findings, { channel: 'c1', role: 'r1', severity: 'medium' }), [findings[1]]);
  deepStrictEqual(filterFindings(findings, { channel: 'c1', role: 'r1', severity: 'high' }), []);
});
