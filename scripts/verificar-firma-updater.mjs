// Verificación de artefactos de CI con la clave pública; nunca lee secretos.
import { createHash, createPublicKey, verify } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
export function assertProductionKey(encoded) {
  const canonical = typeof encoded === 'string' && encoded.length > 0 &&
    Buffer.from(encoded, 'base64').toString('base64') === encoded;
  const lines = Buffer.from(canonical ? encoded : '', 'base64').toString('utf8').trim().split(/\r?\n/);
  const packet = Buffer.from(lines[1] || '', 'base64');
  if (lines.length !== 2 || !lines[0].startsWith('untrusted comment:') ||
      lines[0].includes('ARGUS provisional public key') || packet.length !== 42 ||
      packet.toString('base64') !== lines[1] || packet.subarray(0, 2).toString('ascii') !== 'Ed' ||
      !packet.subarray(10).some((byte) => byte !== 0)) {
    throw new Error('Configure the author public key before release');
  }
}

function decode(value) {
  const bytes = Buffer.from(value, 'base64');
  if (bytes.toString('base64') !== value) throw new Error('Invalid signature encoding');
  return bytes;
}

export function verifyArtifact(data, encodedSignature, encodedKey, version) {
  assertProductionKey(encodedKey);
  const packet = decode(Buffer.from(encodedKey, 'base64').toString('utf8').trim().split(/\r?\n/)[1]);
  const lines = decode(encodedSignature.trim()).toString('utf8').trim().split(/\r?\n/);
  if (lines.length !== 4 || !lines[0].startsWith('untrusted comment: ') ||
      !lines[2].startsWith('trusted comment: ')) throw new Error('Invalid minisign signature');
  const signature = decode(lines[1]);
  const global = decode(lines[3]);
  if (signature.length !== 74 || global.length !== 64 ||
      !signature.subarray(2, 10).equals(packet.subarray(2, 10))) {
    throw new Error('Signature does not match the configured public key');
  }
  const algorithm = signature.subarray(0, 2).toString('ascii');
  if (algorithm !== 'Ed' && algorithm !== 'ED') throw new Error('Unsupported signature algorithm');
  const publicKey = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), packet.subarray(10)]),
    format: 'der', type: 'spki',
  });
  const digest = algorithm === 'ED' ? createHash('blake2b512').update(data).digest() : data;
  const trusted = lines[2].slice('trusted comment: '.length);
  if (!verify(null, digest, publicKey, signature.subarray(10)) ||
      !verify(null, Buffer.concat([signature.subarray(10), Buffer.from(trusted)]), publicKey, global)) {
    throw new Error('Artifact signature verification failed');
  }
  if (version && !trusted.split('\t').includes(`version:${version.replace(/^v/, '')}`)) {
    throw new Error('Signed artifact version does not match the release');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [artifact, version] = process.argv.slice(2);
  const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
  verifyArtifact(readFileSync(artifact), readFileSync(`${artifact}.sig`, 'utf8'), config.plugins.updater.pubkey, version);
  console.log('Updater artifact signature verified against the configured public key.');
}
