import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { filterTemplates, readTemplateFile, splitTags } from '../src/templateLibrary.ts';

const item = (id: string, name: string, tags: string) => ({ id, name, description: '', tags, created_at: '2026-01-01T00:00:00Z' });
const lista = [item('1', 'Gaming hub', 'games, community'), item('2', 'Study group', 'school')];

test('the list filters by name or tag, ignoring case and spaces', () => {
  assert.equal(filterTemplates(lista, '').length, 2);
  assert.deepEqual(filterTemplates(lista, '  GAMING ').map((i) => i.id), ['1']);
  assert.deepEqual(filterTemplates(lista, 'school').map((i) => i.id), ['2']);
  assert.equal(filterTemplates(lista, 'nothing').length, 0);
});

test('tags split into clean badges', () => {
  assert.deepEqual(splitTags('games, community ,, '), ['games', 'community']);
  assert.deepEqual(splitTags(''), []);
});

test('importing a simulated file reads its text and asks for the preview', async () => {
  const texto = '{"name":"x"}';
  const bytes = new TextEncoder().encode(texto);
  const file = { size: bytes.length, arrayBuffer: async () => bytes.buffer as ArrayBuffer };
  const pedidos: string[] = [];
  const r = await readTemplateFile(file, async (d) => { pedidos.push(d); return { name_exists: false }; });
  assert.deepEqual(r, { ok: true, data: texto, preview: { name_exists: false } });
  assert.deepEqual(pedidos, [texto]);
});

test('invalid, oversized or rejected files fail without calling the backend wrongly', async () => {
  let llamadas = 0;
  const preview = async () => { llamadas++; return {}; };
  const grande = { size: 2_000_000, arrayBuffer: async () => new ArrayBuffer(0) };
  assert.deepEqual(await readTemplateFile(grande, preview), { ok: false });
  const binario = { size: 2, arrayBuffer: async () => new Uint8Array([0xff, 0xfe]).buffer as ArrayBuffer };
  assert.deepEqual(await readTemplateFile(binario, preview), { ok: false });
  assert.equal(llamadas, 0);
  const ok = { size: 2, arrayBuffer: async () => new TextEncoder().encode('{}').buffer as ArrayBuffer };
  assert.deepEqual(await readTemplateFile(ok, async () => { throw new Error('bad'); }), { ok: false });
});

test('the library uses design components and labelled selectors', () => {
  const v = readFileSync(new URL('../src/components/TemplateTransfer.tsx', import.meta.url), 'utf8');
  assert.ok(v.includes('plantillas__tarjeta') && v.includes('<Badge') && v.includes('<Search'));
  assert.ok(v.includes('type="file" hidden') && v.includes("t('Import template file')"));
  assert.ok(!v.includes("Choose a template file"));
  const a = readFileSync(new URL('../src/components/AsistenteView.tsx', import.meta.url), 'utf8');
  assert.ok(a.includes("<Select etiqueta={t('Agent')}") && a.includes("<Select etiqueta={t('Model')}"));
});
