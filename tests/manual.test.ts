import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { buscarManual, seccionesDeVista, seccionesManual } from '../src/manual.ts';

const secciones = seccionesManual((s) => s);

test('cada vista de la barra lateral tiene su sección en el manual', () => {
  const app = readFileSync(new URL('../src/App.tsx', import.meta.url), 'utf8');
  const ids = [...app.matchAll(/\{ id: '([\w-]+)', label: t\(/g)].map((m) => m[1]);
  assert.ok(ids.includes('manual') && ids.length >= 12, 'se esperan las vistas de la barra');
  for (const id of ids.filter((i) => i !== 'manual')) {
    assert.ok(seccionesDeVista(secciones, id).length > 0, `la vista ${id} no tiene sección`);
  }
});

test('el manual cubre todas las áreas con qué hace, cómo se usa y límites', () => {
  assert.ok(secciones.length >= 14);
  for (const s of secciones) assert.ok(s.title && s.que && s.como && s.limites, s.id);
  assert.equal(new Set(secciones.map((s) => s.id)).size, secciones.length);
});

test('la búsqueda encuentra por título y por contenido, sin tildes ni mayúsculas', () => {
  assert.deepEqual(buscarManual(secciones, 'PERMISSION audit').map((s) => s.id), ['audit']);
  assert.ok(buscarManual(secciones, 'tray').some((s) => s.id === 'service'));
  assert.ok(buscarManual(secciones, 'encrypted').length > 1);
  assert.equal(buscarManual(secciones, 'zzzz-nada').length, 0);
  assert.equal(buscarManual(secciones, '  ').length, secciones.length);
});
