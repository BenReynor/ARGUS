// Solo CI de release: nunca imprime ni persiste secretos.
import { readFileSync } from 'node:fs';
import { assertProductionKey } from './crear-latest.mjs';
const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
assertProductionKey(config.plugins.updater.pubkey);
if (!process.env.TAURI_SIGNING_PRIVATE_KEY || !process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
  throw new Error('Configure both signing secrets (a password is required)');
}
