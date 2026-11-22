import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { controlesServicio, estadoInicial, resumenServicio } from '../src/servicio.ts';

test('el servicio y el arranque con el sistema vienen desactivados por defecto', () => {
  const e = estadoInicial();
  assert.equal(e.enabled, false);
  assert.equal(e.autostart, false);
  assert.equal(resumenServicio(e), 'stopped');
  assert.deepEqual(controlesServicio(e), { canStart: false, canStop: false });
});

test('iniciar exige habilitar; detener solo con el servicio en marcha', () => {
  const habilitado = { ...estadoInicial(), enabled: true };
  assert.deepEqual(controlesServicio(habilitado), { canStart: true, canStop: false });
  const enMarcha = { ...habilitado, running: true, gateway_connected: true };
  assert.deepEqual(controlesServicio(enMarcha), { canStart: false, canStop: true });
});

test('el resumen distingue detenido, Gateway conectado, caído y sin informar', () => {
  const base = { ...estadoInicial(), enabled: true };
  assert.equal(resumenServicio(base), 'stopped');
  assert.equal(resumenServicio({ ...base, running: true, gateway_connected: true }), 'running_connected');
  assert.equal(resumenServicio({ ...base, running: true, gateway_connected: false }), 'running_gateway_down');
  assert.equal(resumenServicio({ ...base, running: true, gateway_connected: null }), 'running_unknown');
});
