// Recorrido automático de la interfaz con capturas (npm run e2e:capturas).
// Levanta Vite, inyecta un backend de Tauri simulado (scripts/e2e-mock.mjs) con datos
// demo, visita cada vista y pestaña y guarda un PNG por pantalla fuera del repo
// (E2E_CAPTURAS; por defecto en el tmp del sistema). Falla si hay errores de JS,
// comandos invoke sin simular, texto técnico visible o IDs de Discord crudos.
// No usa red ni tokens reales y no forma parte de `npm test`.
import { chromium } from 'playwright-core';
import { createServer } from 'vite';
import { existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { instalarMock } from './e2e-mock.mjs';

const carpeta = process.env.E2E_CAPTURAS || join(tmpdir(), 'argus-capturas');
const puerto = Number(process.env.E2E_PUERTO || 14210);

/** Localiza el Chromium ya instalado por Playwright en ~/.cache/ms-playwright. */
function buscarChromium() {
  if (process.env.E2E_CHROMIUM) return process.env.E2E_CHROMIUM;
  const base = process.env.PLAYWRIGHT_BROWSERS_PATH || join(homedir(), '.cache', 'ms-playwright');
  if (!existsSync(base)) throw new Error(`No existe ${base}: instala Chromium con Playwright.`);
  const candidatos = [];
  for (const d of readdirSync(base)) {
    if (!/^chromium(_headless_shell)?-\d+$/.test(d)) continue;
    for (const sub of readdirSync(join(base, d))) {
      for (const bin of ['chrome', 'headless_shell', 'chrome-headless-shell']) {
        const ruta = join(base, d, sub, bin);
        if (existsSync(ruta)) candidatos.push(ruta);
      }
    }
  }
  if (!candidatos.length) throw new Error(`No se encontró Chromium en ${base}.`);
  return candidatos.sort().reverse()[0];
}

const fallos = [];
const fallo = (pantalla, msg) => fallos.push(`[${pantalla}] ${msg}`);
let pantallaActual = 'arranque';

rmSync(carpeta, { recursive: true, force: true });
mkdirSync(carpeta, { recursive: true });

const vite = await createServer({ server: { port: puerto, strictPort: true, host: '127.0.0.1', hmr: false, watch: { ignored: ['**/src-tauri/**', '**/node_modules/**', '**/.git/**', '**/.dedalo/**', '**/docs/**', '**/dist/**'] } }, logLevel: 'error' });
await vite.listen();
const navegador = await chromium.launch({ executablePath: buscarChromium(), args: ['--no-sandbox'] });

try {
  const pagina = await navegador.newPage({ viewport: { width: 1440, height: 900 } });
  await instalarMock(pagina);
  pagina.on('pageerror', (e) => fallo(pantallaActual, `error de JavaScript: ${e.message}`));
  pagina.on('console', (m) => {
    if (m.type() === 'error') fallo(pantallaActual, `console.error: ${m.text()}`);
  });
  await pagina.goto(`http://127.0.0.1:${puerto}/`, { waitUntil: 'load' });
  await pagina.waitForSelector('.sidebar-nav .nav-item', { timeout: 20000 });

  let n = 0;
  const capturar = async (nombre) => {
    pantallaActual = nombre;
    await pagina.waitForTimeout(600);
    n += 1;
    const archivo = join(carpeta, `${String(n).padStart(2, '0')}-${nombre}.png`);
    await pagina.screenshot({ path: archivo, fullPage: false });
    console.log(`  ${archivo}`);
    // Texto técnico o IDs crudos visibles (innerText no incluye el valor de los inputs).
    const texto = await pagina.evaluate(() => document.body.innerText);
    if (!(await pagina.evaluate(() => document.querySelector('.main-content')?.innerText.trim().length > 0))) {
      fallo(nombre, 'la vista está vacía');
    }
    for (const malo of ['undefined', 'NaN', '[object Object]', 'null']) {
      if (new RegExp(`(^|[^\\w])${malo.replace(/[[\]]/g, '\\$&')}([^\\w]|$)`).test(texto)) {
        fallo(nombre, `texto técnico visible: ${malo}`);
      }
    }
    const ids = texto.match(/(?<!\d)\d{17,20}(?!\d)/g);
    if (ids) fallo(nombre, `ID de Discord crudo visible: ${[...new Set(ids)].join(', ')}`);
    const sinSimular = await pagina.evaluate(() => window.__e2eSinSimular.splice(0));
    for (const c of sinSimular) fallo(nombre, `comando invoke sin simular: ${c}`);
  };

  const vistas = await pagina.$$eval('.sidebar-nav .nav-item', (b) => b.map((x) => x.textContent.trim()));
  const nombreArchivo = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  const irA = async (etiqueta) => {
    await pagina.locator('.sidebar-nav .nav-item', { hasText: new RegExp(`^\\s*${etiqueta}\\s*$`) }).first().click();
    await pagina.waitForTimeout(300);
  };
  const pestanas = async (etiqueta, prefijo) => {
    const nombres = await pagina.$$eval('[role="tab"]', (b) => b.map((x) => x.textContent.trim()));
    for (const nombre of nombres) {
      await pagina.locator('[role="tab"]', { hasText: new RegExp(`^\\s*${nombre.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`) }).first().click();
      await capturar(`${prefijo}-${nombreArchivo(nombre)}`);
    }
  };

  await pagina.waitForTimeout(500);
  if (!(await pagina.evaluate(() => document.body.innerText.includes('Demo Server')))) {
    fallo('inicio', 'los datos demo no se cargaron (no aparece «Demo Server»)');
  }
  await capturar('inicio');
  for (const etiqueta of vistas) {
    pantallaActual = etiqueta;
    await irA(etiqueta);
    const slug = nombreArchivo(etiqueta);
    if (['AI Assistant', 'Members'].includes(etiqueta)) await pestanas(etiqueta, slug);
    else await capturar(slug);
    if (etiqueta === 'Settings') {
      // Config no tiene pestañas: se recorre por secciones desplazando el contenedor.
      const alto = await pagina.evaluate(() => {
        const c = document.querySelector('.main-content');
        return c ? c.scrollHeight - c.clientHeight : 0;
      });
      for (let y = 900, i = 2; y < alto + 900; y += 800, i++) {
        await pagina.evaluate((top) => document.querySelector('.main-content')?.scrollTo(0, top), y - 100);
        await capturar(`${slug}-parte-${i}`);
      }
    }
  }
} catch (e) {
  fallo(pantallaActual, `el recorrido se interrumpió: ${e.message}`);
} finally {
  await navegador.close();
  await vite.close();
}

if (fallos.length) {
  console.error(`e2e:capturas FALLÓ (${fallos.length}):\n- ${[...new Set(fallos)].join('\n- ')}`);
  console.error(`Capturas parciales en ${carpeta}`);
  process.exit(1);
}
console.log(`e2e:capturas OK — capturas en ${carpeta}`);
