import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';

const vista = readFileSync(new URL('../src/components/BackupsView.tsx', import.meta.url), 'utf8');
const masskick = readFileSync(new URL('../src/components/MassKick.tsx', import.meta.url), 'utf8');
const css = readFileSync(new URL('../src/components/Vistas.css', import.meta.url), 'utf8');

test('the danger zone starts collapsed, sits last and has a warning', () => {
  const m = vista.match(/<details className="zona-peligro"[^>]*>/);
  assert.ok(m, 'danger zone is a <details>');
  assert.ok(!/\bopen\b/.test(m[0]), 'no open attribute: collapsed by default');
  assert.ok(vista.indexOf('zona-peligro') > vista.indexOf('data?.snapshots.map'), 'after the snapshot list');
  assert.ok(vista.indexOf('<MassKick') > vista.indexOf('zona-peligro'), 'MassKick lives inside the zone');
  assert.ok(vista.includes("t('Danger zone')") && vista.includes('zona-peligro__aviso'));
  assert.match(css, /\.zona-peligro\s*\{[^}]*border:[^}]*--danger/);
});

test('snapshot controls stay in their own section, outside the danger zone', () => {
  const zona = vista.indexOf('zona-peligro');
  for (const k of ["t('Create snapshot')", "t('Import snapshot')", "t('Automatic snapshots')", "t('Save schedule')"])
    assert.ok(vista.indexOf(k) < zona, k);
});

test('mass kick keeps its verification and double confirmation steps', () => {
  for (const k of ["op: 'identify'", "op: 'prepare'", "op: 'start'", "t('Send verification code')", "t('Confirm member removal')",
    "t('Type the server name to confirm')", 'confirmation !== status.preview?.guild_name', 'step === 1', 'step === 2'])
    assert.ok(masskick.includes(k), k);
});
