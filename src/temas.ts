export interface TemaInfo {
  id: string;
  nombre: string;
  /** Variante de modo del tema (valor de data-modo), si existe. */
  modo?: string;
  modoNombre?: string;
  /** Muestras (fondo, acento, texto) del tema base y de su modo. */
  muestra: { bg: string; accent: string; text: string };
  muestraModo?: { bg: string; accent: string; text: string };
}

export const TEMAS: TemaInfo[] = [
  { id: 'minimal', nombre: 'Minimal', modo: 'claro', modoNombre: 'Claro', muestra: { bg: '#0b0b0c', accent: '#5b57e8', text: '#ededef' }, muestraModo: { bg: '#fafafa', accent: '#4f46e5', text: '#18181b' } },
  { id: 'discord', nombre: 'Discord', muestra: { bg: '#313338', accent: '#5a66f2', text: '#f2f3f5' } },
  { id: 'cyberpunk', nombre: 'Cyberpunk', muestra: { bg: '#05070d', accent: '#00e5ff', text: '#e6f1ff' } },
  { id: 'calido', nombre: 'Cálido', muestra: { bg: '#faf7f2', accent: '#c4472a', text: '#2b2420' } },
  { id: 'datos', nombre: 'Datos', modo: 'claro', modoNombre: 'Claro', muestra: { bg: '#0d1117', accent: '#1f6feb', text: '#e6edf3' }, muestraModo: { bg: '#f6f8fa', accent: '#0969da', text: '#1f2328' } },
  { id: 'anime', nombre: 'Anime', modo: 'noche', modoNombre: 'Noche', muestra: { bg: '#fff7fb', accent: '#c81e6e', text: '#2a1b3d' }, muestraModo: { bg: '#170f24', accent: '#ff5fa2', text: '#fdf2ff' } },
  { id: 'manga', nombre: 'Manga', modo: 'noir', modoNombre: 'Noir', muestra: { bg: '#fbfaf7', accent: '#111111', text: '#111111' }, muestraModo: { bg: '#0c0c0c', accent: '#f5f3ee', text: '#f5f3ee' } },
  { id: 'glass', nombre: 'Glass', modo: 'claro', modoNombre: 'Claro', muestra: { bg: '#0f172a', accent: '#7aa2ff', text: '#f1f5ff' }, muestraModo: { bg: '#e9eef8', accent: '#3355e0', text: '#0f172a' } },
  { id: 'retro', nombre: 'Retro', muestra: { bg: '#1a1c2c', accent: '#73eff7', text: '#f4f4f4' } },
  { id: 'brutalista', nombre: 'Brutalista', muestra: { bg: '#f2f0e9', accent: '#ff4f00', text: '#000000' } },
];

/** Aplica tema y modo al <html>; valores vacíos o desconocidos caen al tema por defecto. */
export const aplicarTema = (tema?: string, modo?: string) => {
  const root = document.documentElement;
  const info = TEMAS.find((t) => t.id === tema) ?? TEMAS.find((t) => t.id === 'discord')!;
  root.dataset.tema = info.id;
  if (modo && info.modo === modo) root.dataset.modo = modo;
  else root.dataset.modo = '';
};
