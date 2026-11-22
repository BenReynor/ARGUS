import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { chipsMiembro, colorRol, inicialesAvatar, urlAvatar } from '../src/miembros.ts';

const rol = (id: string, name: string, position: number, color = 0) => ({
  id, name, color, position, permissions: '', permissions_bitfield: null, member_count: 0, mentionable: false, hoist: false,
});
const roles = [rol('1', '@everyone', 0), rol('2', 'Mod', 5, 0xff0000), rol('3', 'Admin', 9, 0x00ff00), rol('4', 'VIP', 2), rol('5', 'Dev', 3, 0x0000ff)];

test('chips con nombre y color, ordenados por posición, sin IDs ni @everyone', () => {
  const c = chipsMiembro({ roles: ['1', '2', '3'] }, roles);
  assert.deepEqual(c.visibles.map((r) => r.name), ['Admin', 'Mod']);
  assert.equal(c.visibles[0].color, '#00ff00');
  assert.equal(c.visibles[1].color, '#ff0000');
  assert.equal(c.masEtiqueta, null);
});

test('«+N» y tooltip con el resto', () => {
  const c = chipsMiembro({ roles: ['2', '3', '4', '5'] }, roles, 2);
  assert.deepEqual(c.visibles.map((r) => r.name), ['Admin', 'Mod']);
  assert.equal(c.masEtiqueta, '+2');
  assert.equal(c.masTooltip, 'Dev, VIP');
});

test('rol sin color y rol desconocido', () => {
  assert.equal(colorRol(0), null);
  assert.equal(colorRol(255), '#0000ff');
  assert.deepEqual(chipsMiembro({ roles: ['4', '999'] }, roles).visibles.map((r) => r.name), ['VIP']);
});

test('avatar real si existe y respaldo con inicial si no', () => {
  assert.equal(urlAvatar('10', null), null);
  assert.equal(urlAvatar('10', 'abc', 32), 'https://cdn.discordapp.com/avatars/10/abc.png?size=32');
  assert.ok(urlAvatar('10', 'a_abc')!.includes('.gif'));
  assert.equal(inicialesAvatar('ana lopez'), 'AL');
  assert.equal(inicialesAvatar('   '), '?');
});
