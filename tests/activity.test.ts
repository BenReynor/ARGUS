import { test } from 'node:test';
import assert from 'node:assert/strict';
import { filterOperations, readFilters, saveFilters, operationProgress, operationNotice } from '../src/activity.ts';
import type { Operation } from '../src/activity.ts';

const operations: Operation[] = [
  { id: '1', bot_id: 'a', guild_id: 'one', status: 'correct', started_at: '', steps: [{ step_index: 0, action: 'create', status: 'correct' }] },
  { id: '2', bot_id: 'a', guild_id: 'two', status: 'failed', started_at: '', steps: [{ step_index: 0, action: 'delete', status: 'failed' }] },
  { id: '3', bot_id: 'a', guild_id: 'one', status: 'failed', started_at: '', steps: [{ step_index: 0, action: 'create', status: 'failed' }] },
];
test('filters intersect action type, server and operation status', () => {
  const all = { type: '', guild: '', status: '' };
  assert.equal(filterOperations(operations, all).length, 3);
  assert.deepEqual(filterOperations(operations, { ...all, type: 'create' }).map(o => o.id), ['1', '3']);
  assert.deepEqual(filterOperations(operations, { ...all, guild: 'two' }).map(o => o.id), ['2']);
  assert.deepEqual(filterOperations(operations, { ...all, status: 'failed' }).map(o => o.id), ['2', '3']);
  assert.deepEqual(filterOperations(operations, { type: 'create', guild: 'one', status: 'failed' }).map(o => o.id), ['3']);
});
test('filters reopen for the same bot and remain isolated', () => {
  const data = new Map<string, string>();
  const storage = { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); } };
  const filters = { type: 'create', guild: 'one', status: 'failed' };
  saveFilters(storage, 'a', filters);
  const reopened = readFilters(storage, 'a');
  assert.deepEqual(reopened, filters);
  assert.deepEqual(filterOperations(operations, reopened).map(o => o.id), ['3']);
  assert.deepEqual(readFilters(storage, 'b'), { type: '', guild: '', status: '' });
});

test('progress follows pending, running, verifying and terminal step transitions', () => {
  const steps = ['pending', 'pending', 'pending', 'pending'].map((status, step_index) => ({ step_index, action: 'create', status }));
  assert.deepEqual(operationProgress(steps), { done: 0, total: 4 });
  steps[0].status = 'in_progress'; steps[1].status = 'verifying';
  assert.equal(operationProgress(steps).done, 0);
  steps[0].status = 'correct'; steps[1].status = 'failed'; steps[2].status = 'skipped'; steps[3].status = 'cancelled';
  assert.deepEqual(operationProgress(steps), { done: 4, total: 4 });
});
test('partial and unverified outcomes never produce a success notice', () => {
  assert.equal(operationNotice('partial'), 'warning');
  assert.equal(operationNotice('unverified'), 'warning');
  assert.equal(operationNotice('failed'), 'error');
  assert.equal(operationNotice('correct'), 'success');
});
