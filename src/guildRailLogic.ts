// Reglas del riel de servidores, separadas del componente para poder probarlas.

export interface EstadoCarga {
  /** Bot cuya petición de servidores está en curso o fue la última. */
  bot: string | null;
  enCurso: boolean;
  /** Momento (ms) de la última petición. */
  ultima: number;
}

/** Discord limita GET /users/@me/guilds: al volver a la ventana, una cada 10 s. */
export const ESPERA_AL_VOLVER_MS = 10_000;

/**
 * ¿Hay que pedir los servidores de `bot` ahora?
 * Con el mismo bot no se lanzan dos peticiones a la vez ni, sin `forzar`, más de
 * una cada 10 s. Si el bot cambió se pide siempre: una petición en curso del bot
 * anterior no debe impedir cargar la lista del nuevo (su respuesta se descarta).
 */
export function debeCargar(estado: EstadoCarga, bot: string, forzar: boolean, ahora: number): boolean {
  if (estado.bot !== bot) return true;
  if (estado.enCurso) return false;
  return forzar || ahora - estado.ultima >= ESPERA_AL_VOLVER_MS;
}

/** Una respuesta solo se aplica si sigue siendo del bot activo. */
export function respuestaVigente(estado: EstadoCarga, botDeLaRespuesta: string): boolean {
  return estado.bot === botDeLaRespuesta;
}

/**
 * Servidor que debe quedar activo con la lista del bot actual: el activo si está
 * en ella; si no (p. ej. se cambió a un bot que no está en ese servidor), el
 * primero de la lista; `null` si el bot no está en ningún servidor.
 */
export function servidorTrasCargar(ids: string[], activo: string | null): string | null {
  if (activo && ids.includes(activo)) return activo;
  return ids[0] ?? null;
}
