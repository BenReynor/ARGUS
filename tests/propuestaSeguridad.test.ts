import assert from 'node:assert/strict';
import { test } from 'node:test';
import { seguridadPropuesta } from '../src/propuestaSeguridad.ts';

const acciones = (herramienta: string, cantidad: number) => Array.from({ length: cantidad }, (_, i) => ({
  herramienta, descripcion: `Action ${i + 1}`, args: { nombre: `item-${i + 1}` },
}));

test('confirmación por nombre: más de cinco borrados de canales y roles, sumados', () => {
  assert.equal(seguridadPropuesta([]).requiereNombre, false);
  for (const herramienta of ['borrar_canal', 'borrar_rol']) {
    assert.equal(seguridadPropuesta(acciones(herramienta, 5)).requiereNombre, false);
    assert.equal(seguridadPropuesta(acciones(herramienta, 6)).requiereNombre, true);
  }
  assert.equal(seguridadPropuesta([...acciones('borrar_canal', 3), ...acciones('borrar_rol', 2)]).requiereNombre, false);
  assert.equal(seguridadPropuesta([...acciones('borrar_canal', 3), ...acciones('borrar_rol', 3)]).requiereNombre, true);
});

test('confirmación por nombre: más de tres baneos; otras acciones no inflan los umbrales', () => {
  assert.equal(seguridadPropuesta(acciones('banear', 3)).requiereNombre, false);
  assert.equal(seguridadPropuesta(acciones('banear', 4)).requiereNombre, true);
  assert.equal(seguridadPropuesta([
    ...acciones('borrar_canal', 5), ...acciones('banear', 3),
    ...acciones('expulsar', 10), ...acciones('borrar_mensajes', 10), ...acciones('crear_rol', 10),
  ]).requiereNombre, false);
});

test('la estimación aparece solo por encima de treinta acciones, a dos segundos por petición', () => {
  for (const cantidad of [0, 1, 29, 30]) {
    assert.equal(seguridadPropuesta(acciones('crear_canal', cantidad)).segundosEstimados, null);
  }
  assert.equal(seguridadPropuesta(acciones('crear_canal', 31)).segundosEstimados, 62);
  assert.equal(seguridadPropuesta(acciones('crear_canal', 60)).segundosEstimados, 120);
  assert.equal(seguridadPropuesta(acciones('borrar_rol', 100)).segundosEstimados, 200);
});

test('la advertencia conserva cada acción irreversible, sus argumentos y su posición original', () => {
  const herramientas = ['borrar_canal', 'borrar_rol', 'banear', 'expulsar', 'borrar_mensaje', 'borrar_mensajes'];
  const propuesta = [
    ...acciones('crear_rol', 1),
    ...herramientas.flatMap((h) => acciones(h, 1)),
    ...acciones('borrar_canal', 1), ...acciones('editar_canal', 1),
  ];
  const original = structuredClone(propuesta);
  const seguridad = seguridadPropuesta(propuesta);
  assert.deepEqual(seguridad.irreversibles.map(({ indice }) => indice), [1, 2, 3, 4, 5, 6, 7]);
  assert.deepEqual(seguridad.irreversibles.map(({ accion }) => accion), propuesta.slice(1, 8));
  assert.deepEqual(propuesta, original);
  assert.equal(seguridadPropuesta(acciones('crear_canal', 31)).irreversibles.length, 0);
});
