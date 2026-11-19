import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

test('comprueba literales y rechaza comandos sin registrar o retirados', () => {
  const dir = mkdtempSync(join(tmpdir(), 'argus-invoke-'));
  try {
    mkdirSync(join(dir, 'src'));
    mkdirSync(join(dir, 'src-tauri', 'src'), { recursive: true });
    const ui = join(dir, 'src', 'view.tsx');
    const rust = join(dir, 'src-tauri', 'src', 'lib.rs');
    const run = () => spawnSync(process.execPath, [resolve('scripts/comprobar-invoke.mjs')], { cwd: dir, encoding: 'utf8' });
    writeFileSync(rust, 'generate_handler![modulo::registrado, otro]');
    writeFileSync(ui, 'invoke<Array<string>>("registrado"); invoke(`otro`);');
    assert.equal(run().status, 0);
    writeFileSync(ui, 'invoke<Array<string>>("ausente")');
    assert.match(run().stderr, /ausente.*no registrado/);
    for (const nombre of ['apply_template', 'export_all_data', 'import_data', 'uninstall_wipe']) {
      writeFileSync(ui, `invoke('${nombre}')`);
      writeFileSync(rust, `generate_handler![modulo::${nombre}]`);
      const result = run();
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /comando retirado/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
