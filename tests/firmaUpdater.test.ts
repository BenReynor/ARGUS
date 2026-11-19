import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { verifyArtifact } from '../scripts/verificar-firma-updater.mjs';
import { releaseVersion } from '../scripts/comprobar-version-release.mjs';

test('firma real de Tauri: acepta el artefacto y rechaza alteraciones, otra clave y otra versión', () => {
  const directory = mkdtempSync(join(tmpdir(), 'argus-signature-test-'));
  try {
    const key = join(directory, 'fixture.key');
    const artifact = join(directory, 'fixture.bin');
    const password = 'temporary-test-fixture';
    const env = { PATH: process.env.PATH, SystemRoot: process.env.SystemRoot, TMPDIR: directory, TEMP: directory };
    const cli = (...args: string[]) => {
      const result = spawnSync(process.execPath, [resolve('node_modules/@tauri-apps/cli/tauri.js'), 'signer', ...args], {
        env, encoding: 'utf8', timeout: 30_000,
      });
      // No se incluye stdout: generate imprime claves de prueba que no deben
      // acabar en logs ni confundirse con las claves de producción del autor.
      assert.equal(result.status, 0, `Tauri signer failed: ${result.stderr}`);
    };
    cli('generate', '--ci', '--password', password, '--write-keys', key);
    const data = Buffer.from('ARGUS updater artifact fixture');
    writeFileSync(artifact, data);
    cli('sign', '--private-key-path', key, '--password', password, '--app-version', '1.3.0', artifact);
    const publicKey = readFileSync(`${key}.pub`, 'utf8').trim();
    const signature = readFileSync(`${artifact}.sig`, 'utf8').trim();
    verifyArtifact(data, signature, publicKey, '1.3.0');
    assert.throws(() => verifyArtifact(Buffer.from('tampered'), signature, publicKey, '1.3.0'), /verification failed/);
    assert.throws(() => verifyArtifact(data, signature, publicKey, '2.0.0'), /version/);
    const tampered = Buffer.from(signature, 'base64').toString('utf8').replace('version:1.3.0', 'version:2.0.0');
    assert.throws(() => verifyArtifact(data, Buffer.from(tampered).toString('base64'), publicKey, '2.0.0'), /verification failed/);
    const other = join(directory, 'other.key');
    cli('generate', '--ci', '--password', password, '--write-keys', other);
    assert.throws(() => verifyArtifact(data, signature, readFileSync(`${other}.pub`, 'utf8').trim(), '1.3.0'), /public key/);
    assert.throws(() => verifyArtifact(data, 'invalid', publicKey, '1.3.0'), /encoding/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('publicar requiere las mismas versiones en tag, Cargo, frontend y Tauri', () => {
  assert.equal(releaseVersion('v1.3.0', ['1.3.0', '1.3.0']), '1.3.0');
  assert.throws(() => releaseVersion('v1.3.0', ['1.2.0', '1.3.0']), /must match/);
  assert.throws(() => releaseVersion('main', ['1.3.0']), /must match/);
});
