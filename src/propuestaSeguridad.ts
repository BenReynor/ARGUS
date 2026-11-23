import type { AccionPropuesta } from './types/index.ts';

// Mismas herramientas que ACCIONES_DESTRUCTIVAS en propuesta.rs.
const IRREVERSIBLES = new Set([
  'borrar_canal', 'borrar_rol', 'banear', 'expulsar', 'borrar_mensaje',
  'borrar_mensajes', 'borrar_invitacion', 'borrar_webhook', 'cancelar_evento', 'enviar_mensaje',
  'crear_encuesta', 'editar_mensaje', 'crear_post_foro', 'enviar_dm', 'avisar',
]);

/** Vista previa local: no consulta Discord ni modifica el orden de ejecución. */
export function seguridadPropuesta(acciones: AccionPropuesta[]) {
  const irreversibles = acciones.map((accion, indice) => ({ accion, indice }))
    .filter(({ accion }) => IRREVERSIBLES.has(accion.herramienta));
  const borrados = acciones.filter((a) => a.herramienta === 'borrar_canal' || a.herramienta === 'borrar_rol').length;
  const baneos = acciones.filter((a) => a.herramienta === 'banear').length;
  return {
    irreversibles,
    requiereNombre: borrados > 5 || baneos > 3,
    // Aproximación de una petición de escritura por acción; las lecturas de
    // permisos, resolución de nombres y los reintentos pueden alargarla.
    segundosEstimados: acciones.length > 30 ? acciones.length * 2 : null,
  };
}
