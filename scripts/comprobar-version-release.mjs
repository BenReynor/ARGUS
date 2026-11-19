import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function releaseVersion(tag, versions) {
  const version = tag?.replace(/^v/, '');
  if (!/^v\d+\.\d+\.\d+(?:-[\da-zA-Z.-]+)?$/.test(tag || '') ||
      versions.some((value) => value !== version)) {
    throw new Error('Release tag, Cargo, frontend, lockfile and Tauri versions must match');
  }
  return version;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const readJson = (file) => JSON.parse(readFileSync(file, 'utf8'));
  releaseVersion(process.argv[2], [
    readJson('package.json').version,
    readJson('package-lock.json').version,
    readJson('src-tauri/tauri.conf.json').version,
    /^version\s*=\s*"([^"]+)"/m.exec(readFileSync('src-tauri/Cargo.toml', 'utf8'))?.[1],
  ]);
  console.log('Release versions match.');
}
