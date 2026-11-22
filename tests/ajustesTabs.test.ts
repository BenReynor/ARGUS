import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { SETTINGS_TABS, SETTINGS_SECTIONS, readSettingsTab, saveSettingsTab, moveTab, DEFAULT_SETTINGS_TAB } from '../src/ajustesTabs.ts';

const store = () => { const m = new Map<string, string>(); return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) }; };
const vista = readFileSync(new URL('../src/components/SettingsView.tsx', import.meta.url), 'utf8');
const conectores = readFileSync(new URL('../src/components/ConnectorsPanel.tsx', import.meta.url), 'utf8');
const diag = readFileSync(new URL('../src/components/Diagnostics.tsx', import.meta.url), 'utf8');

test('the seven required tabs exist and each section belongs to exactly one', () => {
  assert.deepEqual([...SETTINGS_TABS], ['bots', 'appearance', 'ai', 'diagnostics', 'backups', 'connectors', 'service']);
  const all = SETTINGS_TABS.flatMap(tab => SETTINGS_SECTIONS[tab]);
  assert.equal(new Set(all).size, all.length);
});
test('last opened tab is remembered; invalid values fall back to the default', () => {
  const s = store();
  assert.equal(readSettingsTab(s), DEFAULT_SETTINGS_TAB);
  saveSettingsTab(s, 'connectors');
  assert.equal(readSettingsTab(s), 'connectors');
  s.setItem('argus.settings.tab', 'nope');
  assert.equal(readSettingsTab(s), DEFAULT_SETTINGS_TAB);
});
test('keyboard navigation wraps around the tabs', () => {
  assert.equal(moveTab('bots', 1), 'appearance');
  assert.equal(moveTab('bots', -1), 'service');
  assert.equal(moveTab('service', 1), 'bots');
});
test('every existing section is still rendered, in its tab', () => {
  const casos: [string, string][] = [
    ["t('Bots')", 'bots'], ["t('This server')", 'bots'], ["t('General')", 'bots'],
    ["t('Appearance')", 'appearance'], ["t('About')", 'appearance'],
    ['titulo="MCP"', 'ai'], ["t('AI time limit')", 'ai'],
    ['<Diagnostics />', 'diagnostics'], ['<DiagnosticReport />', 'diagnostics'],
    ['<BackupSettings />', 'backups'], ['<ConnectorsPanel', 'connectors'], ["t('Background service')", 'service'],
  ];
  for (const [marca, tab] of casos) {
    const i = vista.indexOf(marca);
    assert.ok(i >= 0, `missing ${marca}`);
    const linea = vista.slice(vista.lastIndexOf('\n', i), vista.indexOf('\n', i));
    assert.match(linea, new RegExp(`tab ?=== ?'${tab}'`), `${marca} not in tab ${tab}`);
  }
  assert.match(vista, /role="tablist"/);
  assert.match(vista, /readSettingsTab\(localStorage\)/);
});
test('diagnostics is a compact table with icon+text status and a small icon retry button', () => {
  assert.match(diag, /<table/);
  for (const col of ["'Component'", "'Status'", "'Last checked'"]) assert.ok(diag.includes(col));
  assert.match(diag, /tamano="sm"/);
  assert.match(diag, /RefreshCw/);
  assert.match(diag, /<Icono /);
});
test('external services and connectors are one card per service with status, capabilities and connect/disconnect', () => {
  assert.ok(!vista.includes("t('External services')"));
  assert.equal((conectores.match(/externalServices\.map/g) ?? []).length, 1);
  for (const k of ["'Read tools'", "'Write tools'", "'Limitations'", "'Connect'", "'Disconnect'", "'Enable for this server'", 'connector-status-']) assert.ok(conectores.includes(k), k);
});
