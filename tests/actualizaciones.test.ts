import assert from 'node:assert/strict';
import { test } from 'node:test';
import { generateKeyPairSync } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createUpdater, newerVersion, type UpdateState } from '../src/actualizaciones.ts';
import { manifest, assertProductionKey } from '../scripts/crear-latest.mjs';
import { configurePublicKey } from '../scripts/configurar-clave-updater.mjs';

test('precedencia SemVer: números, prereleases, metadatos y versiones inválidas', () => {
  for (const [next, current, expected] of [
    ['1.10.0', '1.9.9', true], ['2.0.0', '1.99.0', true],
    ['1.2.0', '1.2.0', false], ['1.1.9', '1.2.0', false],
    ['1.2.0-beta.2', '1.2.0-beta.1', true], ['1.2.0-beta.10', '1.2.0-beta.2', true],
    ['1.2.0', '1.2.0-rc.1', true], ['1.2.0-beta', '1.2.0', false],
    ['1.2.0-beta', '1.2.0-alpha', true], ['1.2.0-beta.1', '1.2.0-beta', true],
    ['1.2.0+build2', '1.2.0+build1', false], ['v1.3.0', '1.2.0', true],
    ['invalid', '1.2.0', false], ['1.3.0-01', '1.2.0', false], ['1.3.0+build..a', '1.2.0', false],
  ] as const) assert.equal(newerVersion(next, current), expected, `${next} vs ${current}`);
});

function fixture(manual = false, offline = false, invalidSignature = false) {
  let state!: UpdateState;
  let downloads = 0, restarts = 0;
  const updater = createUpdater({
    current: '1.2.0',
    check: async () => {
      if (offline) throw new Error('offline');
      return { version: '1.3.0', body: 'Release notes <script>untrusted</script>', close: async () => {},
        downloadAndInstall: async () => { downloads++; if (invalidSignature) throw new Error('invalid signature'); } };
    },
    supported: async () => !manual,
    restart: async () => { restarts++; },
    changed: (value) => { state = value; },
  });
  return { updater, state: () => state, downloads: () => downloads, restarts: () => restarts };
}

test('aviso con versión y notas; comprobar no descarga; solo Update instala y reinicia', async () => {
  const f = fixture();
  await f.updater.update();
  await f.updater.check();
  assert.deepEqual(f.state().notice, { version: '1.3.0', notes: 'Release notes <script>untrusted</script>', manual: false });
  assert.equal(f.downloads(), 0);
  assert.equal(f.restarts(), 0);
  await f.updater.update();
  assert.equal(f.downloads(), 1);
  assert.equal(f.restarts(), 1);
});

test('Later cancela el aviso y no permite instalar; deb ofrece solo descarga manual', async () => {
  for (const manual of [false, true]) {
    const f = fixture(manual);
    await f.updater.check();
    assert.equal(f.state().notice?.manual, manual);
    if (!manual) f.updater.dismiss();
    await f.updater.update();
    assert.equal(f.downloads(), 0);
    assert.equal(f.restarts(), 0);
  }
});

test('sin red el arranque es silencioso y la comprobación manual informa sin diálogo', async () => {
  const f = fixture(false, true);
  await f.updater.check();
  assert.equal(f.state().status, 'idle');
  assert.equal(f.state().notice, null);
  await f.updater.check(true);
  assert.equal(f.state().status, 'unavailable');
});

test('firma rechazada impide reinicio y muestra fallo; no reintenta instalar sola', async () => {
  const f = fixture(false, false, true);
  await f.updater.check();
  await f.updater.update();
  assert.equal(f.state().status, 'failed');
  assert.equal(f.state().installing, false);
  assert.equal(f.downloads(), 1);
  assert.equal(f.restarts(), 0);
});

test('latest.json contiene las firmas y URL de cada plataforma, notas y versión', () => {
  const files = { 'ARGUS.AppImage': '', 'ARGUS.AppImage.sig': 'linux-signature\n', 'ARGUS-setup.exe': '', 'ARGUS-setup.exe.sig': 'win-signature\n', 'ARGUS.msi': '', 'ARGUS.msi.sig': 'msi-signature\n' };
  const meta = { tag: 'v1.3.0', repository: 'owner/repo', notes: 'Release notes', date: '2026-11-18T00:00:00Z' };
  const result = manifest(files, meta);
  assert.equal(result.version, '1.3.0');
  assert.equal(result.notes, meta.notes);
  assert.equal(result.platforms['linux-x86_64'].signature, 'linux-signature');
  assert.equal(result.platforms['windows-x86_64'].url, 'https://github.com/owner/repo/releases/download/v1.3.0/ARGUS-setup.exe');
  assert.throws(() => manifest({ ...files, 'ARGUS.AppImage.sig': '' }, meta), /Missing signature/);
  assert.throws(() => manifest({}, meta), /Expected exactly one/);
});


test('sin versión nueva no hay aviso; una comprobación manual informa', async () => {
  for (const candidate of [null, '1.1.0', '1.2.0']) {
    let state!: UpdateState;
    const updater = createUpdater({
      current: '1.2.0',
      check: async () => candidate && ({ version: candidate, close: async () => {}, downloadAndInstall: async () => { assert.fail('unexpected install'); } }),
      supported: async () => true, restart: async () => { assert.fail('unexpected restart'); },
      changed: (value) => { state = value; },
    });
    await updater.check();
    assert.equal(state.notice, null);
    assert.equal(state.status, 'idle');
    await updater.check(true);
    assert.equal(state.status, 'current');
  }
});

test('doble clic no duplica instalaciones ni reinicios', async () => {
  let finish!: () => void;
  let downloads = 0, restarts = 0;
  const updater = createUpdater({
    current: '1.2.0', supported: async () => true,
    check: async () => ({ version: '1.3.0', close: async () => {}, downloadAndInstall: async () => {
      downloads++;
      await new Promise<void>((resolve) => { finish = resolve; });
    } }),
    restart: async () => { restarts++; }, changed: () => {},
  });
  await updater.check();
  const first = updater.update();
  await updater.update();
  assert.equal(downloads, 1);
  assert.equal(restarts, 0);
  finish();
  await first;
  assert.equal(restarts, 1);
});


test('release rechaza clave pública provisional, vacía o mal formada', () => {
  const provisional = Buffer.from('untrusted comment: ARGUS provisional public key - replace before release\n' + Buffer.alloc(42).toString('base64')).toString('base64');
  for (const key of [provisional, '', 'invalid']) assert.throws(() => assertProductionKey(key), /Configure the author public key/);
});

test('importa solo una clave pública válida y conserva el endpoint y el resto de la configuración', () => {
  const directory = mkdtempSync(join(tmpdir(), 'argus-updater-'));
  try {
    // Clave efímera solo para este test: nunca se configura en la aplicación.
    const { publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32);
    const packet = Buffer.concat([Buffer.from('Ed'), Buffer.alloc(8), raw]);
    const encoded = Buffer.from('untrusted comment: minisign public key\n' + packet.toString('base64') + '\n').toString('base64');
    const configPath = join(directory, 'tauri.conf.json');
    const publicPath = join(directory, 'author.key.pub');
    const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
    writeFileSync(configPath, JSON.stringify(config));
    writeFileSync(publicPath, encoded + '\n');
    configurePublicKey(publicPath, configPath);
    assert.deepEqual(JSON.parse(readFileSync(configPath, 'utf8')), {
      ...config, plugins: { ...config.plugins, updater: { ...config.plugins.updater, pubkey: encoded } },
    });
    const configured = readFileSync(configPath, 'utf8');
    assert.throws(() => configurePublicKey(join(directory, 'author.key'), configPath), /public .pub file/);
    const provisional = Buffer.from('untrusted comment: ARGUS provisional public key - replace before release\n' + packet.toString('base64')).toString('base64');
    for (const invalid of [provisional, '', 'invalid',
      Buffer.from('untrusted comment: public key\n' + Buffer.alloc(42).toString('base64')).toString('base64'),
      Buffer.from('untrusted comment: public key\n' + Buffer.concat([Buffer.from('Ed'), Buffer.alloc(40)]).toString('base64')).toString('base64'),
      encoded + '!']) {
      writeFileSync(publicPath, invalid);
      assert.throws(() => configurePublicKey(publicPath, configPath), /Configure the author public key/);
      assert.equal(readFileSync(configPath, 'utf8'), configured);
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
