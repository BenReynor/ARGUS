import assert from 'node:assert/strict';
import { test } from 'node:test';
import { confirmarDescripciones } from '../src/costeDescripciones.ts';

test('la app no pide diálogo bajo el umbral', async () => {
  assert.equal(await confirmarDescripciones({ confirmar: false }, async () => { throw new Error('unexpected dialog'); }), true);
});

test('la app espera el diálogo y respeta tanto cancelar como confirmar', async () => {
  for (const respuesta of [false, true]) {
    let resolver!: (respuesta: boolean) => void;
    let continuado = false;
    const resultado = confirmarDescripciones({ confirmar: true }, () => new Promise<boolean>((resolve) => { resolver = resolve; }))
      .then((aceptado) => { continuado = true; return aceptado; });
    await Promise.resolve();
    assert.equal(continuado, false);
    resolver(respuesta);
    assert.equal(await resultado, respuesta);
  }
});
