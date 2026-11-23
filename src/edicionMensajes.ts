/** Límite de caracteres de un mensaje de Discord. */
export const LIMITE_MENSAJE = 2000;

/** «Editar» solo se ofrece en los mensajes del bot activo (Discord no permite otros). */
export const puedeEditarMensaje = (autorId: string, botId: string | null | undefined): boolean =>
  !!botId && autorId === botId;

/** Caracteres que quedan (negativo si se pasa); cuenta puntos de código como Discord. */
export const caracteresRestantes = (texto: string): number => LIMITE_MENSAJE - [...texto].length;

/** Motivo por el que no se puede guardar, o `null` si se puede guardar. */
export const errorEdicion = (texto: string, original: string): 'vacio' | 'largo' | 'igual' | null => {
  if (texto.trim() === '') return 'vacio';
  if (caracteresRestantes(texto) < 0) return 'largo';
  if (texto === original) return 'igual';
  return null;
};

/** Guarda la edición con el comando indicado; no llama si el texto no es válido. */
export const guardarEdicion = async (
  guardar: (canalId: string, mensajeId: string, contenido: string) => Promise<string>,
  canalId: string,
  mensajeId: string,
  texto: string,
  original: string,
): Promise<string> => {
  if (errorEdicion(texto, original)) throw new Error('invalid edit');
  return guardar(canalId, mensajeId, texto);
};
