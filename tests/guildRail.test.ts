import { test } from 'node:test';
import assert from 'node:assert/strict';
import { debeCargar, respuestaVigente, servidorTrasCargar, ESPERA_AL_VOLVER_MS } from '../src/guildRailLogic.ts';

test('cambiar de bot carga su lista aunque haya una petición del bot anterior en curso', () => {
  const enCursoBotA = { bot: 'A', enCurso: true, ultima: 1_000 };
  assert.equal(debeCargar(enCursoBotA, 'B', true, 1_100), true);
  assert.equal(debeCargar(enCursoBotA, 'B', false, 1_100), true);
});

test('con el mismo bot no se piden dos a la vez ni más de una cada 10 s al volver', () => {
  assert.equal(debeCargar({ bot: 'A', enCurso: true, ultima: 0 }, 'A', true, 50_000), false);
  const quieto = { bot: 'A', enCurso: false, ultima: 1_000 };
  assert.equal(debeCargar(quieto, 'A', false, 1_000 + ESPERA_AL_VOLVER_MS - 1), false);
  assert.equal(debeCargar(quieto, 'A', false, 1_000 + ESPERA_AL_VOLVER_MS), true);
  assert.equal(debeCargar(quieto, 'A', true, 1_001), true);
});

test('la respuesta tardía del bot anterior se descarta', () => {
  const ahoraB = { bot: 'B', enCurso: true, ultima: 0 };
  assert.equal(respuestaVigente(ahoraB, 'A'), false);
  assert.equal(respuestaVigente(ahoraB, 'B'), true);
});

test('el servidor activo pasa a uno del bot nuevo si el anterior no está en su lista', () => {
  assert.equal(servidorTrasCargar(['1', '2'], '2'), '2');
  assert.equal(servidorTrasCargar(['3', '4'], '2'), '3');
  assert.equal(servidorTrasCargar([], '2'), null);
  assert.equal(servidorTrasCargar(['5'], null), '5');
});
