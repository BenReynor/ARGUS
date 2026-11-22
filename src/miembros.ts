import type { Member, Role } from './types';

export interface RolChip {
  id: string;
  name: string;
  /** Color CSS (#rrggbb) o null si el rol no tiene color. */
  color: string | null;
  position: number;
}

export interface ChipsMiembro {
  visibles: RolChip[];
  /** Roles que no caben, para el «+N» y su tooltip. */
  resto: RolChip[];
  /** Texto del «+N», o null si no hay resto. */
  masEtiqueta: string | null;
  /** Nombres del resto separados por coma, para el tooltip. */
  masTooltip: string;
}

export function colorRol(color: number): string | null {
  return color > 0 ? `#${(color & 0xffffff).toString(16).padStart(6, '0')}` : null;
}

/** Roles del miembro como chips con nombre y color, de mayor a menor posición. IDs desconocidos se omiten. */
export function chipsMiembro(m: Pick<Member, 'roles'>, roles: Role[], max = 3): ChipsMiembro {
  const porId = new Map(roles.map((r) => [r.id, r]));
  const chips: RolChip[] = m.roles
    .map((id) => porId.get(id))
    .filter((r): r is Role => !!r && r.name !== '@everyone')
    .sort((a, b) => b.position - a.position || a.name.localeCompare(b.name))
    .map((r) => ({ id: r.id, name: r.name, color: colorRol(r.color), position: r.position }));
  const visibles = chips.slice(0, max);
  const resto = chips.slice(max);
  return {
    visibles,
    resto,
    masEtiqueta: resto.length ? `+${resto.length}` : null,
    masTooltip: resto.map((r) => r.name).join(', '),
  };
}

/** URL del avatar de Discord, o null si no hay (entonces se usa la inicial). Acepta hash o URL completa. */
export function urlAvatar(userId: string, avatar: string | null | undefined, tamano = 64): string | null {
  if (!avatar) return null;
  if (/^https?:\/\//.test(avatar)) return avatar;
  const ext = avatar.startsWith('a_') ? 'gif' : 'png';
  return `https://cdn.discordapp.com/avatars/${userId}/${avatar}.${ext}?size=${tamano}`;
}

/** Iniciales de respaldo cuando no hay avatar. */
export function inicialesAvatar(nombre: string): string {
  return nombre.trim().split(/\s+/).slice(0, 2).map((p) => Array.from(p)[0]?.toUpperCase() ?? '').join('') || '?';
}

export interface RecuentoMiembros {
  valor: number;
  /** true si la cifra puede no ser exacta (sin mapa cargado o con miembros sin descargar). */
  aproximado: boolean;
}

/**
 * Recuento de miembros único para el Panel y la cabecera. Con el mapa cargado
 * cuenta sus miembros (bots incluidos); es aproximado si Discord informa de más
 * miembros de los descargados. Sin mapa usa la cifra aproximada de Discord.
 */
export function recuentoMiembros(descargados: number | null, total: number | null): RecuentoMiembros | null {
  if (descargados !== null) return { valor: descargados, aproximado: total !== null && descargados < total };
  if (total !== null) return { valor: total, aproximado: true };
  return null;
}
