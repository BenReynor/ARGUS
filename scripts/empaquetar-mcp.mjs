// Compila `argus-mcp` y lo copia a src-tauri/binaries/argus-mcp-<triple>,
// que es donde Tauri lo busca (bundle.externalBin). Úsalo antes de `tauri build`.
import { execSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';

const triple = execSync('rustc -vV').toString().match(/host: (\S+)/)[1];
const ext = triple.includes('windows') ? '.exe' : '';
execSync('cargo build --release --bin argus-mcp --manifest-path src-tauri/Cargo.toml', { stdio: 'inherit' });
mkdirSync('src-tauri/binaries', { recursive: true });
copyFileSync(`src-tauri/target/release/argus-mcp${ext}`, `src-tauri/binaries/argus-mcp-${triple}${ext}`);
console.log(`argus-mcp listo para empaquetar (${triple})`);
