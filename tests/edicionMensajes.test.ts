import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { LIMITE_MENSAJE, caracteresRestantes, errorEdicion, guardarEdicion, puedeEditarMensaje } from '../src/edicionMensajes.ts';

test('«Editar» solo aparece en mensajes del bot activo', () => {
  assert.equal(puedeEditarMensaje('10', '10'), true);
  assert.equal(puedeEditarMensaje('11', '10'), false);
  assert.equal(puedeEditarMensaje('10', null), false);
  assert.equal(puedeEditarMensaje('10', ''), false);
});

test('contador de 2000 caracteres', () => {
  assert.equal(LIMITE_MENSAJE, 2000);
  assert.equal(caracteresRestantes('abc'), 1997);
  assert.equal(caracteresRestantes('😀'.repeat(2000)), 0);
  assert.equal(caracteresRestantes('a'.repeat(2001)), -1);
});

test('validación del texto editado', () => {
  assert.equal(errorEdicion('  ', 'x'), 'vacio');
  assert.equal(errorEdicion('a'.repeat(2001), 'x'), 'largo');
  assert.equal(errorEdicion('x', 'x'), 'igual');
  assert.equal(errorEdicion('y', 'x'), null);
});

test('guardar usa el comando con canal, mensaje y texto; rechaza lo inválido', async () => {
  const llamadas: string[][] = [];
  const guardar = async (c: string, m: string, t: string) => { llamadas.push([c, m, t]); return 'ok'; };
  assert.equal(await guardarEdicion(guardar, '1', '2', 'nuevo', 'viejo'), 'ok');
  assert.deepEqual(llamadas, [['1', '2', 'nuevo']]);
  await assert.rejects(guardarEdicion(guardar, '1', '2', '', 'viejo'));
  assert.equal(llamadas.length, 1);
  const falla = async (): Promise<string> => { throw 'The bot can only edit its own messages'; };
  await assert.rejects(guardarEdicion(falla, '1', '2', 'nuevo', 'viejo'), (e) => e === 'The bot can only edit its own messages');
});
