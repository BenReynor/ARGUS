import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { allCorrect, initialChecks, mergeChecks } from '../src/diagnostics.ts';

test('partial and unchecked results never report complete success', () => {
  const checks = initialChecks();
  const partial = mergeChecks(checks, [{ name: checks[0].name, status: 'correct' }]);
  assert.equal(allCorrect(partial), false);
  assert.equal(allCorrect(partial.map(c => ({ ...c, status: 'correct' }))), true);
});
test('cancellation preserves already completed results', () => {
  const checks = initialChecks();
  const completed = mergeChecks(checks, [{ name: checks[0].name, status: 'correct' }]);
  const cancelled = completed.map(c => c.status === 'pending' ? { ...c, status: 'cancelled' } : c);
  assert.equal(cancelled[0].status, 'correct');
  assert.equal(allCorrect(cancelled), false);
});
test('retry replaces only the selected result', () => {
  const checks = initialChecks();
  const retried = mergeChecks(checks, [{ name: checks[0].name, status: 'correct' }, { name: checks[1].name, status: 'failed' }], checks[0].name);
  assert.equal(retried[0].status, 'correct');
  assert.equal(retried[1].status, 'pending');
});

test('retrying one check calls only the command of its group', async () => {
  const { groupsToRun } = await import('../src/diagnostics.ts');
  assert.deepEqual(groupsToRun('configuration'), ['diagnostico_local']);
  assert.deepEqual(groupsToRun('build_permissions'), ['diagnostico_discord']);
  assert.deepEqual(groupsToRun('codex_response'), ['diagnostico_agentes']);
  assert.deepEqual(groupsToRun(), ['diagnostico_local', 'diagnostico_discord', 'diagnostico_agentes']);
});
