// Importa únicamente la clave PÚBLICA creada por el autor con Tauri signer.
import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { assertProductionKey } from './crear-latest.mjs';

export function configurePublicKey(publicKeyPath, configPath = 'src-tauri/tauri.conf.json') {
  if (!publicKeyPath?.endsWith('.pub')) throw new Error('Provide the public .pub file, never the private key');
  const encoded = readFileSync(publicKeyPath, 'utf8').trim();
  assertProductionKey(encoded);
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  config.plugins.updater.pubkey = encoded;
  writeFileSync(configPath, JSON.stringify(config, null, 2) + '\n');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  configurePublicKey(process.argv[2]);
  console.log('Updater public key configured. Set both GitHub signing secrets and keep an external backup before release.');
}
