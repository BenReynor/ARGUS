// Manifiesto estático del updater: solo artefactos con su firma producida por Tauri.
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { join, basename } from 'node:path';
import { verifyArtifact, assertProductionKey } from './verificar-firma-updater.mjs';
export { assertProductionKey } from './verificar-firma-updater.mjs';

export function manifest(files, { tag, repository, notes, date }) {
  const version = tag.replace(/^v/, '');
  if (!/^\d+\.\d+\.\d+(?:-[\da-zA-Z.-]+)?$/.test(version)) throw new Error('Invalid release version');
  const platforms = {};
  for (const [target, pattern] of [
    ['linux-x86_64', /\.AppImage$/],
    ['windows-x86_64', /-setup\.exe$/],
    ['windows-x86_64-msi', /\.msi$/],
  ]) {
    const names = Object.keys(files).filter((name) => pattern.test(name));
    if (names.length !== 1) throw new Error(`Expected exactly one installer for ${target}`);
    const name = names[0];
    const signature = files[name + '.sig']?.trim();
    if (!signature) throw new Error(`Missing signature: ${name}`);
    platforms[target] = {
      signature,
      url: `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(basename(name))}`,
    };
  }
  return { version, notes, pub_date: date, platforms };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [directory, tag, repository, notesFile] = process.argv.slice(2);
  const files = Object.fromEntries(readdirSync(directory).map((name) => [name,
    name.endsWith('.sig') ? readFileSync(join(directory, name), 'utf8') : '',
  ]));
  const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
  const frontend = JSON.parse(readFileSync('package.json', 'utf8'));
  if ([config.version, frontend.version].some((version) => version !== tag.replace(/^v/, ''))) throw new Error('Tag, frontend and app versions must match');
  assertProductionKey(config.plugins.updater.pubkey);
  for (const name of Object.keys(files).filter((name) => /\.(AppImage|exe|msi)$/.test(name))) {
    verifyArtifact(readFileSync(join(directory, name)), files[name + '.sig'] || '', config.plugins.updater.pubkey, config.version);
  }
  writeFileSync(join(directory, 'latest.json'), JSON.stringify(manifest(files, {
    tag, repository, notes: readFileSync(notesFile, 'utf8'), date: new Date().toISOString(),
  }), null, 2) + '\n');
}
