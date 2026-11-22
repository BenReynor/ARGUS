/** Estado del servicio sin ventana tal como lo devuelve `servicio_estado`. */
export interface EstadoServicio {
  enabled: boolean;
  running: boolean;
  gateway_connected: boolean | null;
  autostart: boolean;
  last_error: string | null;
}

/** Valor inicial: todo desactivado (el servicio es opcional y viene apagado). */
export const estadoInicial = (): EstadoServicio => ({
  enabled: false, running: false, gateway_connected: null, autostart: false, last_error: null,
});

export type ResumenServicio = 'stopped' | 'running_connected' | 'running_gateway_down' | 'running_unknown';

/** Resume el estado: detenido, en marcha con Gateway conectado, caído o sin informar. */
export function resumenServicio(e: EstadoServicio): ResumenServicio {
  if (!e.running) return 'stopped';
  if (e.gateway_connected === true) return 'running_connected';
  if (e.gateway_connected === false) return 'running_gateway_down';
  return 'running_unknown';
}

/** Qué botones están disponibles: iniciar solo si está habilitado y detenido; detener solo en marcha. */
export const controlesServicio = (e: EstadoServicio) => ({
  canStart: e.enabled && !e.running,
  canStop: e.running,
});
