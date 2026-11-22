import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { buscarManual, seccionAbierta, seccionesDeVista, seccionesManual } from '../src/manual.ts';

const leer = (ruta: string) => readFileSync(new URL(ruta, import.meta.url), 'utf8');

test('el plegado: abierta al buscar o si es la sección de la vista de origen', () => {
  assert.equal(seccionAbierta(false, 'manual', true), false);
  assert.equal(seccionAbierta(false, 'audit', true), true);
  assert.equal(seccionAbierta(false, 'audit', false), false);
  assert.equal(seccionAbierta(true, 'manual', false), true);
});

test('el manual usa iconos de lucide y resalta la sección activa, sin el triángulo escrito', () => {
  const vista = leer('../src/components/ManualView.tsx');
  assert.ok(vista.includes("from 'lucide-react'") && vista.includes('ChevronRight') && vista.includes('Search'));
  assert.ok(!vista.includes('▶'));
  assert.ok(vista.includes('manual-seccion--activa'));
  assert.match(leer('../src/components/Vistas.css'), /\.manual-vista\s*\{[^}]*80ch/);
});

test('Actividad: filtros en una barra bajo la cabecera y estado vacío con icono', () => {
  const vista = leer('../src/components/ActivityView.tsx');
  const cab = vista.indexOf('<CabeceraVista');
  assert.ok(cab > 0 && vista.indexOf('actividad__filtros') > cab);
  assert.ok(vista.includes('<EstadoVacio icono='));
});

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
