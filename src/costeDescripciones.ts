interface Coste { confirmar: boolean }

/** Mantiene el paso a la generación bloqueado hasta resolver el diálogo. */
export async function confirmarDescripciones(coste: Coste, dialogo: () => Promise<boolean>): Promise<boolean> {
  return !coste.confirmar || await dialogo();
}
