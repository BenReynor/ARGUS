import { es } from './i18n/es';

export type Idioma = 'en' | 'es';

export const IDIOMAS: { id: Idioma; nombre: string }[] = [
  { id: 'en', nombre: 'English' },
  { id: 'es', nombre: 'Español' },
];

let actual: Idioma = 'en';

const normalizar = (lang: string | null | undefined): Idioma => (lang === 'es' ? 'es' : 'en');

export const idiomaActual = (): Idioma => actual;

/** Fija el idioma de la interfaz y el atributo `lang` del documento. */
export const setIdioma = (lang: string | null | undefined): void => {
  actual = normalizar(lang);
  if (typeof document !== 'undefined') document.documentElement.lang = actual;
};

/**
 * Traduce un texto fuente en inglés. Los placeholders `{nombre}` se sustituyen
 * con `vars`. Sin entrada en el catálogo se muestra el inglés.
 */
export const t = (texto: string, vars?: Record<string, string | number>): string => {
  const base = actual === 'es' ? (es[texto] ?? texto) : texto;
  if (!vars) return base;
  return base.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
};
