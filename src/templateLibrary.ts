// Lógica pura de la biblioteca de plantillas (sin React) para poder probarla.
export type TemplateItem = { id: string; name: string; description: string; tags: string; created_at: string };

export const MAX_TEMPLATE_FILE = 1048576;

export function filterTemplates<T extends TemplateItem>(items: T[], query: string): T[] {
  const q = query.toLocaleLowerCase().trim();
  return items.filter((item) => [item.name, item.tags].some((v) => v.toLocaleLowerCase().includes(q)));
}

export function splitTags(tags: string): string[] {
  return tags.split(',').map((s) => s.trim()).filter(Boolean);
}

export type FileLike = { size: number; arrayBuffer(): Promise<ArrayBuffer> };
export type ImportRead<P> = { ok: true; data: string; preview: P } | { ok: false };

/** Lee un archivo de plantilla (UTF-8 estricto, ≤1 MiB) y pide su vista previa al backend. */
export async function readTemplateFile<P>(file: FileLike, preview: (data: string) => Promise<P>): Promise<ImportRead<P>> {
  if (file.size > MAX_TEMPLATE_FILE) return { ok: false };
  try {
    const data = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
    return { ok: true, data, preview: await preview(data) };
  } catch {
    return { ok: false };
  }
}
