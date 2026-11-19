// Tauri 2 pasa los argumentos de los comandos en camelCase: `guild_id` en Rust se
// llama `guildId` desde JS. Una clave en snake_case en invoke() compila, pero el
// comando falla al usarlo («missing required key guildId»). Esto lo detecta.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import ts from 'typescript';

const retirados = ['apply_template', 'export_all_data', 'import_data', 'uninstall_wipe'];
const registro = readFileSync('src-tauri/src/lib.rs', 'utf8');
const bloque = registro.match(/generate_handler!\s*\[([\s\S]*?)\]/);
if (!bloque) throw new Error('No se encontró generate_handler! en src-tauri/src/lib.rs');
const handlers = new Set(bloque[1].replace(/\/\/[^\n]*/g, '').split(',')
  .map(s => s.trim().split('::').pop()).filter(Boolean));

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
  const fuente = ts.createSourceFile(f, texto, ts.ScriptTarget.Latest, true, f.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  function visitar(nodo) {
    if (ts.isCallExpression(nodo) && ts.isIdentifier(nodo.expression) && nodo.expression.text === 'invoke' && nodo.arguments.length) {
      const arg = nodo.arguments[0];
      if (ts.isStringLiteral(arg) || ts.isNoSubstitutionTemplateLiteral(arg)) {
        const nombre = arg.text;
        const linea = fuente.getLineAndCharacterOfPosition(nodo.getStart()).line + 1;
        if (!handlers.has(nombre)) errores.push(`${f}:${linea}: comando '${nombre}' no registrado`);
        if (retirados.includes(nombre)) errores.push(`${f}:${linea}: comando retirado '${nombre}'`);
      }
    }
    ts.forEachChild(nodo, visitar);
  }
  visitar(fuente);
  for (const m of texto.matchAll(llamada)) {
    for (const clave of m[2].matchAll(/(?:^|[\s,{])([a-z]+(?:_[a-z0-9]+)+)\s*:/g)) {
      const linea = texto.slice(0, m.index).split('\n').length;
      errores.push(`${f}:${linea}: '${m[1]}' recibe '${clave[1]}'; en Tauri 2 debe ir en camelCase`);
    }
  }
}
for (const nombre of retirados) {
  if (handlers.has(nombre)) errores.push(`generate_handler!: comando retirado '${nombre}' registrado`);
}
if (errores.length) {
  console.error(errores.join('\n'));
  process.exit(1);
}
console.log(`invoke: OK (${archivos.length} archivos)`);
