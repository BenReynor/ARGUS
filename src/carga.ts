// Pantalla de carga de index.html: avanza la barra con cada paso del arranque
// y se quita cuando la app tiene datos que mostrar.

const TIEMPO_MAXIMO_MS = 8000; // si Discord tarda, se muestra la app igual

let terminado = false;
let valorActual = 0;

export function pasoCarga(texto: string, porcentaje: number): void {
  if (terminado) return;
  valorActual = Math.max(valorActual, Math.min(100, porcentaje));
  const barra = document.getElementById('carga-barra');
  const paso = document.getElementById('carga-paso');
  const caja = document.getElementById('carga');
  if (barra) barra.style.width = `${valorActual}%`;
  if (paso) paso.textContent = texto;
  caja?.setAttribute('aria-valuenow', String(Math.round(valorActual)));
}

export function finCarga(): void {
  if (terminado) return;
  pasoCarga('Listo', 100);
  terminado = true;
  const caja = document.getElementById('carga');
  if (!caja) return;
  caja.classList.add('fuera');
  setTimeout(() => caja.remove(), 400);
}

setTimeout(finCarga, TIEMPO_MAXIMO_MS);
