export function extraerSugerencia(texto: string): string | null {
  return /```argus-template\s*\n([\s\S]*?)```/i.exec(texto)?.[1].trim() ?? null;
}

/** Review and the user's decision are required before importing a suggestion. */
export class SugerenciaPendiente {
  estado: 'pending' | 'reviewed' | 'discarded' | 'accepted' = 'pending';
  readonly data: string;
  constructor(data: string) { this.data = data; }

  async revisar(preview: (data: string) => Promise<void>): Promise<void> {
    if (this.estado !== 'pending') throw new Error('Suggestion is no longer pending');
    await preview(this.data);
    this.estado = 'reviewed';
  }

  descartar(): void {
    if (this.estado === 'pending' || this.estado === 'reviewed') this.estado = 'discarded';
  }

  async aceptar(guardar: (data: string) => Promise<void>): Promise<void> {
    if (this.estado !== 'reviewed') throw new Error('Suggestion must be reviewed first');
    await guardar(this.data);
    this.estado = 'accepted';
  }
}
