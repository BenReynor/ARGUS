// Tauri 2 pasa los argumentos de los comandos en camelCase: `guild_id` en Rust se
// llama `guildId` desde JS. Una clave en snake_case en invoke() compila, pero el
// comando falla al usarlo («missing required key guildId»). Esto lo detecta.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const archivos = [];
const recorrer = (dir) => {
  for (const n of readdirSync(dir)) {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) recorrer(p);
    else if (/\.(ts|tsx)$/.test(n)) archivos.push(p);
  }
};
recorrer('src');

// invoke<...>('comando', { clave_snake: ... }): solo las claves de primer nivel.
const llamada = /invoke(?:<[^>]*>)?\(\s*'([a-z_]+)'\s*,\s*\{([^}]*)\}/g;
const errores = [];
for (const f of archivos) {
  const texto = readFileSync(f, 'utf8');
  for (const m of texto.matchAll(llamada)) {
    for (const clave of m[2].matchAll(/(?:^|[\s,{])([a-z]+(?:_[a-z0-9]+)+)\s*:/g)) {
      const linea = texto.slice(0, m.index).split('\n').length;
      errores.push(`${f}:${linea}: '${m[1]}' recibe '${clave[1]}'; en Tauri 2 debe ir en camelCase`);
    }
  }
}
if (errores.length) {
  console.error(errores.join('\n'));
  process.exit(1);
}
console.log(`invoke: OK (${archivos.length} archivos)`);
