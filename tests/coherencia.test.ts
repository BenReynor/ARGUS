import { test } from 'node:test';
import { strictEqual, deepStrictEqual } from 'node:assert';
import { readFileSync } from 'node:fs';
import { nombrePermiso, nombrePermisoApi } from '../src/i18n/permisos.ts';
import { recuentoMiembros } from '../src/miembros.ts';

test('permission names are Spanish in Spanish and official English in English', () => {
  strictEqual(nombrePermiso('Gestionar roles', 'es'), 'Gestionar roles');
  strictEqual(nombrePermiso('Gestionar roles', 'en'), 'Manage Roles');
  strictEqual(nombrePermiso('Moderar miembros (timeout)', 'en'), 'Timeout Members');
  strictEqual(nombrePermiso('Permiso desconocido', 'en'), 'Permiso desconocido');
  strictEqual(nombrePermisoApi('Administrator', 3, 'es'), 'Administrador');
  strictEqual(nombrePermisoApi('Administrator', 3, 'en'), 'Administrator');
});

test('Roles, Server and Audit views go through the shared permission translator', () => {
  for (const f of ['RolesView', 'PermisosCanal']) {
    const src = readFileSync(`src/components/${f}.tsx`, 'utf8');
    strictEqual(src.includes('nombreOficialPermiso'), false, f);
    strictEqual(src.includes('nombrePermiso('), true, f);
  }
  const audit = readFileSync('src/components/AuditFindingsView.tsx', 'utf8');
  strictEqual(audit.includes('nombrePermisoApi('), true);
});

test('member count: loaded map is exact unless Discord reports more members', () => {
  deepStrictEqual(recuentoMiembros(120, 120), { valor: 120, aproximado: false });
  deepStrictEqual(recuentoMiembros(100, 120), { valor: 100, aproximado: true });
  deepStrictEqual(recuentoMiembros(5, null), { valor: 5, aproximado: false });
});

test('member count without a map is approximate; nothing known gives null', () => {
  deepStrictEqual(recuentoMiembros(null, 300), { valor: 300, aproximado: true });
  strictEqual(recuentoMiembros(null, null), null);
});

test('Panel and header use the same member count hook', () => {
  for (const f of ['Header', 'Dashboard']) {
    strictEqual(readFileSync(`src/components/${f}.tsx`, 'utf8').includes('useRecuentoMiembros()'), true, f);
  }
});
