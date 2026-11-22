// Recorrido automático de la interfaz con capturas (npm run e2e:capturas).
// Levanta Vite, inyecta un backend de Tauri simulado (scripts/e2e-mock.mjs) con datos
// demo, elige el servidor demo en el riel, visita cada vista y pestaña y guarda un PNG
// por pantalla fuera del repo (E2E_CAPTURAS; por defecto en el tmp del sistema), en una
// subcarpeta por idioma (E2E_IDIOMAS, por defecto «en,es»). Falla si hay errores de JS,
// comandos invoke sin simular, texto técnico visible, IDs de Discord crudos o alguna vista
// con estado de «sin servidor».
// No usa red ni tokens reales y no forma parte de `npm test`.
import { chromium } from 'playwright-core';
import { createServer } from 'vite';
import { existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { instalarMock } from './e2e-mock.mjs';

const carpeta = process.env.E2E_CAPTURAS || join(tmpdir(), 'argus-capturas');
const puerto = Number(process.env.E2E_PUERTO || 14210);
const IDIOMAS = (process.env.E2E_IDIOMAS || 'en,es').split(',').map((x) => x.trim()).filter(Boolean);
// Vistas de la barra lateral, por posición (independiente del idioma).
const SLUGS = ['dashboard', 'channels', 'members', 'roles', 'backups', 'permission-audit', 'server', 'ai-assistant', 'activity', 'logs', 'manual', 'settings'];
const CON_PESTANAS = ['members', 'ai-assistant'];
const SIN_SERVIDOR = ['No server selected', 'Select a bot and server', 'Ningún servidor seleccionado', 'Selecciona un bot y un servidor'];

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

const nombreArchivo = (s) => s.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');

async function recorrer(idioma) {
  const dir = join(carpeta, idioma);
  mkdirSync(dir, { recursive: true });
  const pagina = await navegador.newPage({ viewport: { width: 1440, height: 900 } });
  try {
    await instalarMock(pagina, idioma);
    pagina.on('pageerror', (e) => fallo(pantallaActual, `error de JavaScript: ${e.message}`));
    pagina.on('console', (m) => {
      if (m.type() === 'error') fallo(pantallaActual, `console.error: ${m.text()}`);
    });
    await pagina.goto(`http://127.0.0.1:${puerto}/`, { waitUntil: 'load' });
    await pagina.waitForSelector('.sidebar-nav .nav-item', { timeout: 20000 });

    let n = 0;
    const capturar = async (nombre) => {
      pantallaActual = `${idioma}/${nombre}`;
      await pagina.waitForTimeout(600);
      n += 1;
      const archivo = join(dir, `${String(n).padStart(2, '0')}-${nombre}.png`);
      await pagina.screenshot({ path: archivo, fullPage: false });
      console.log(`  ${archivo}`);
      // Texto técnico o IDs crudos visibles (innerText no incluye el valor de los inputs).
      const texto = await pagina.evaluate(() => document.body.innerText);
      if (!(await pagina.evaluate(() => document.querySelector('.main-content')?.innerText.trim().length > 0))) {
        fallo(pantallaActual, 'la vista está vacía');
      }
      for (const malo of ['undefined', 'NaN', '[object Object]', 'null']) {
        if (new RegExp(`(^|[^\\w])${malo.replace(/[[\]]/g, '\\$&')}([^\\w]|$)`).test(texto)) {
          fallo(pantallaActual, `texto técnico visible: ${malo}`);
        }
      }
      const ids = texto.match(/(?<!\d)\d{17,20}(?!\d)/g);
      if (ids) fallo(pantallaActual, `ID de Discord crudo visible: ${[...new Set(ids)].join(', ')}`);
      for (const sin of SIN_SERVIDOR) {
        if (texto.includes(sin)) fallo(pantallaActual, `la vista muestra un estado de «sin servidor»: ${sin}`);
      }
      const sinSimular = await pagina.evaluate(() => window.__e2eSinSimular.splice(0));
      for (const c of sinSimular) fallo(pantallaActual, `comando invoke sin simular: ${c}`);
    };

    const vistas = await pagina.$$eval('.sidebar-nav .nav-item', (b) => b.map((x) => x.textContent.trim()));
    if (vistas.length !== SLUGS.length) fallo(`${idioma}/arranque`, `se esperaban ${SLUGS.length} vistas y hay ${vistas.length}`);
    const irA = async (i) => {
      await pagina.locator('.sidebar-nav .nav-item').nth(i).click();
      await pagina.waitForTimeout(300);
    };
    const pestanas = async (prefijo) => {
      const nombres = await pagina.$$eval('[role="tab"]', (b) => b.map((x) => x.textContent.trim()));
      for (let i = 0; i < nombres.length; i++) {
        await pagina.locator('[role="tab"]').nth(i).click();
        await capturar(`${prefijo}-${nombreArchivo(nombres[i]) || i + 1}`);
      }
    };

    await pagina.waitForTimeout(500);
    // Elige el servidor demo en el riel, como haría el usuario.
    await pagina.locator('.guild-rail button[aria-label="Demo Server"]').first().click();
    await pagina.waitForTimeout(500);
    if (!(await pagina.evaluate(() => document.body.innerText.includes('Demo Server')))) {
      fallo(`${idioma}/inicio`, 'los datos demo no se cargaron (no aparece «Demo Server»)');
    }
    await capturar('inicio');
    for (let i = 0; i < vistas.length; i++) {
      const slug = SLUGS[i] ?? nombreArchivo(vistas[i]);
      pantallaActual = `${idioma}/${slug}`;
      await irA(i);
      if (CON_PESTANAS.includes(slug)) await pestanas(slug);
      else await capturar(slug);
      if (slug === 'settings') {
        // Config no tiene pestañas: se recorre por secciones desplazando el contenedor.
        const alto = await pagina.evaluate(() => {
          const c = document.querySelector('.main-content');
          return c ? c.scrollHeight - c.clientHeight : 0;
        });
        for (let y = 900, k = 2; y < alto + 900; y += 800, k++) {
          await pagina.evaluate((top) => document.querySelector('.main-content')?.scrollTo(0, top), y - 100);
          await capturar(`${slug}-parte-${k}`);
        }
      }
    }
  } catch (e) {
    fallo(pantallaActual, `el recorrido se interrumpió: ${e.message}`);
  } finally {
    await pagina.close();
  }
}

try {
  for (const idioma of IDIOMAS) {
    pantallaActual = `${idioma}/arranque`;
    await recorrer(idioma);
  }
} finally {
  await navegador.close();
  await vite.close();
}

if (fallos.length) {
  console.error(`e2e:capturas FALLÓ (${fallos.length}):\n- ${[...new Set(fallos)].join('\n- ')}`);
  console.error(`Capturas parciales en ${carpeta}`);
  process.exit(1);
}
console.log(`e2e:capturas OK — capturas en ${carpeta} (${IDIOMAS.join(', ')})`);
