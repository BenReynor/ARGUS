export interface TemaInfo {
  id: string;
  nombre: string;
  /** Variante de modo del tema (valor de data-modo), si existe. */
  modo?: string;
  modoNombre?: string;
}

/** Los colores de muestra de cada tema viven en styles/temas/muestras.css (data-muestra). */
export const TEMAS: TemaInfo[] = [
  { id: 'minimal', nombre: 'Minimal', modo: 'claro', modoNombre: 'Claro' },
  { id: 'discord', nombre: 'Discord' },
  { id: 'cyberpunk', nombre: 'Cyberpunk' },
  { id: 'calido', nombre: 'Cálido' },
  { id: 'datos', nombre: 'Datos', modo: 'claro', modoNombre: 'Claro' },
  { id: 'anime', nombre: 'Anime', modo: 'noche', modoNombre: 'Noche' },
  { id: 'manga', nombre: 'Manga', modo: 'noir', modoNombre: 'Noir' },
  { id: 'glass', nombre: 'Glass', modo: 'claro', modoNombre: 'Claro' },
  { id: 'retro', nombre: 'Retro' },
  { id: 'brutalista', nombre: 'Brutalista' },
];

/** Aplica tema y modo al <html>; valores vacíos o desconocidos caen al tema por defecto. */
export const aplicarTema = (tema?: string, modo?: string) => {
  const root = document.documentElement;
  const info = TEMAS.find((t) => t.id === tema) ?? TEMAS.find((t) => t.id === 'discord')!;
  root.dataset.tema = info.id;
  if (modo && info.modo === modo) root.dataset.modo = modo;
  else root.dataset.modo = '';
};
