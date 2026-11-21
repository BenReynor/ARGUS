import { test } from 'node:test';
import assert from 'node:assert/strict';
import { filterOperations, readFilters, saveFilters } from '../src/activity.ts';
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
