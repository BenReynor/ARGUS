import { test } from 'node:test';
import { strictEqual, deepStrictEqual } from 'node:assert';
import { readFileSync } from 'node:fs';
import { latestSnapshot, readAuditSummary, recentOperations, saveAuditSummary } from '../src/panel.ts';
import type { Operation } from '../src/activity.ts';

const memoria = () => {
  const d = new Map<string, string>();
  return { getItem: (k: string) => d.get(k) ?? null, setItem: (k: string, v: string) => { d.set(k, v); } };
};
const op = (id: string, guild: string, started_at: string): Operation => ({ id, bot_id: 'b', guild_id: guild, status: 'correct', started_at, steps: [] });

test('recent operations: filtered by server, newest first, limited', () => {
  const ops = [op('1', 'g', '2026-01-01T00:00:00Z'), op('2', 'g', '2026-01-03T00:00:00Z'), op('3', 'x', '2026-01-04T00:00:00Z'), op('4', 'g', '2026-01-02T00:00:00Z'), op('5', 'g', '2026-01-05T00:00:00Z')];
  deepStrictEqual(recentOperations(ops, 'g').map(o => o.id), ['5', '2', '4']);
  deepStrictEqual(recentOperations(ops, 'g', 1).map(o => o.id), ['5']);
});

test('recent operations: empty state', () => {
  deepStrictEqual(recentOperations([], 'g'), []);
  deepStrictEqual(recentOperations([op('1', 'x', '2026-01-01T00:00:00Z')], 'g'), []);
});

test('latest snapshot picks the newest one; none gives null', () => {
  const s = (id: string, created_at: string) => ({ id, name: id, created_at, size_bytes: 0, automatic: false, summary: { channels: 0, roles: 0, members: 0, emojis: 0, stickers: 0, webhooks: 0, bans: 0 } });
  strictEqual(latestSnapshot([s('a', '2026-01-01T00:00:00Z'), s('b', '2026-02-01T00:00:00Z'), s('c', '2026-01-15T00:00:00Z')])?.id, 'b');
  strictEqual(latestSnapshot([]), null);
});

test('audit summary is stored per bot and server; empty or corrupt storage gives null', () => {
  const st = memoria();
  strictEqual(readAuditSummary(st, 'b', 'g'), null);
  saveAuditSummary(st, 'b', 'g', { guild_id: 'g', checked_at: '2026-01-01T00:00:00Z', incomplete: false, missing_data: [], findings: [{ tipo: 'privacy_discrepancy', checked_at: '' }, { tipo: 'role_grant', permission: 'Manage Roles', checked_at: '' }] });
  const a = readAuditSummary(st, 'b', 'g');
  deepStrictEqual(a?.summary, { high: 1, medium: 1, total: 2, complete: true });
  strictEqual(readAuditSummary(st, 'b', 'other'), null);
  st.setItem('last-audit:b:bad', '{oops');
  strictEqual(readAuditSummary(st, 'b', 'bad'), null);
});

test('Panel links each card to its view, reuses local data and adds no Discord-calling audit', () => {
  const src = readFileSync('src/components/Dashboard.tsx', 'utf8');
  for (const tab of ["'settings'", "'backups'", "'permission-audit'", "'activity'"]) strictEqual(src.includes(`setActiveTab(${tab})`), true, tab);
  strictEqual(src.includes('permissionAudit('), false);
  strictEqual(src.includes("op: 'list'"), true);
  strictEqual(src.includes('startBot()'), true);
  strictEqual(readFileSync('src/components/Dashboard.css', 'utf8').includes('repeat(2, minmax(0, 1fr))'), true);
});
