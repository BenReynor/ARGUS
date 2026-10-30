// Falla si algún literal pasado a t('…') / t("…") / t(`…`) en src/ no está en el
// catálogo español (src/i18n/es.ts).
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const raiz = join(fileURLToPath(new URL('.', import.meta.url)), '..');
const src = join(raiz, 'src');
const archivoCatalogo = join(src, 'i18n', 'es.ts');

const desescapar = (s, comilla) =>
  s.replace(/\\(.)/g, (_, c) => (c === 'n' ? '\n' : c === 't' ? '\t' : c === comilla || c === '\\' ? c : '\\' + c));

// Claves del catálogo: literales entre comillas que van seguidos de «:» (a veces tras salto de línea).
const catalogo = new Set();
const textoCatalogo = readFileSync(archivoCatalogo, 'utf8');
const reClave = /(['"`])((?:\\.|(?!\1)[^\\])*)\1\s*:/g;
for (const m of textoCatalogo.matchAll(reClave)) catalogo.add(desescapar(m[2], m[1]));

function* archivos(dir) {
  for (const nombre of readdirSync(dir)) {
    const ruta = join(dir, nombre);
    if (statSync(ruta).isDirectory()) yield* archivos(ruta);
    else if (/\.(ts|tsx)$/.test(nombre) && ruta !== archivoCatalogo) yield ruta;
  }
}

// t( seguido de un literal; (?<![\w$.]) evita useT(, split( , x.t( etc.
const reLlamada = /(?<![\w$.])t\(\s*(['"`])((?:\\.|(?!\1)[^\\])*)\1/g;
const faltan = [];
let total = 0;
for (const ruta of archivos(src)) {
  const codigo = readFileSync(ruta, 'utf8');
  for (const m of codigo.matchAll(reLlamada)) {
    if (m[1] === '`' && m[2].includes('${')) continue; // plantilla dinámica: no es un literal
    total++;
    const texto = desescapar(m[2], m[1]);
    if (!catalogo.has(texto)) {
      const linea = codigo.slice(0, m.index).split('\n').length;
      faltan.push(`${relative(raiz, ruta)}:${linea}  ${JSON.stringify(texto)}`);
    }
  }
}

if (faltan.length) {
  console.error(`i18n: ${faltan.length} texto(s) sin traducción en src/i18n/es.ts:`);
  for (const f of faltan) console.error('  ' + f);
  process.exit(1);
}
console.log(`i18n: OK (${total} textos comprobados, ${catalogo.size} en el catálogo)`);
