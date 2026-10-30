import React, { useMemo, useState } from 'react';
import { useServerMap, useShowNotification, useConfig, useGuildPanel, motivoPermisoAusente } from '../store/useStore';
import { ArrowDown, ArrowUp, Copy, Lock, Pencil, Plus, Search, Trash2 } from 'lucide-react';
import { Badge, Button, ConfirmarPeligroso, Dialogo, EstadoVacio, Input } from './ui';
import { EstadoVista, BuscadorVista, CabeceraVista, colorHex } from './EstadoVista';
import { t } from '../i18n';
import { nombreOficialPermiso, GRUPOS_PERMISOS } from '../i18n/permisos';
import { createRole, updateRole, deleteRole, reorderRoles } from '../api/tauri';
import type { Role } from '../types';

export { GRUPOS_PERMISOS };
const COLOR_POR_DEFECTO = 0x99aab5;

const explicacionJerarquia = () =>
  t('Discord does not let you touch a role equal to or above the bot’s highest role. To edit it, raise the bot’s role in Server Settings → Roles.');

/** Título traducido de un grupo de permisos (los permisos mantienen su nombre oficial). */
export const etiquetaGrupo = (g: string): string => {
  switch (g) {
    case 'General': return t('General');
    case 'Miembros': return t('Members');
    case 'Texto': return t('Text');
    case 'Voz': return t('Voice');
    case 'Avanzado': return t('Advanced');
    default: return g;
  }
};

const Interruptor: React.FC<{
  etiqueta: string; ayuda?: string; valor: boolean; desactivado?: boolean; onCambio: (v: boolean) => void;
}> = ({ etiqueta, ayuda, valor, desactivado, onCambio }) => (
  <label className="interruptor">
    <span className="interruptor__texto">
      <span>{etiqueta}</span>
      {ayuda && <span className="interruptor__ayuda">{ayuda}</span>}
    </span>
    <input type="checkbox" role="switch" checked={valor} disabled={desactivado} onChange={(e) => onCambio(e.target.checked)} />
  </label>
);

interface FormRol { nombre: string; color: number; hoist: boolean; mentionable: boolean; bits: bigint }

const DialogoRol: React.FC<{
  rol: Role | null; esEveryone: boolean; guildId: string; onCerrar: () => void;
}> = ({ rol, esEveryone, guildId, onCerrar }) => {
  const showNotification = useShowNotification();
  const [form, setForm] = useState<FormRol>(() => {
    let bits = 0n;
    try { bits = BigInt(rol?.permissions || '0'); } catch { bits = 0n; }
    return { nombre: rol?.name ?? '', color: rol?.color ?? 0, hoist: rol?.hoist ?? false, mentionable: rol?.mentionable ?? false, bits };
  });
  const [guardando, setGuardando] = useState(false);
  const [error, setError] = useState('');
  const admin = ((form.bits >> 3n) & 1n) === 1n;
  const tiene = (bit: number) => ((form.bits >> BigInt(bit)) & 1n) === 1n;
  const poner = (bit: number, valor: boolean) =>
    setForm((f) => ({ ...f, bits: valor ? f.bits | (1n << BigInt(bit)) : f.bits & ~(1n << BigInt(bit)) }));

  const guardar = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!esEveryone && !form.nombre.trim()) { setError(t('Enter a name for the role')); return; }
    setGuardando(true);
    setError('');
    const datos = {
      name: esEveryone ? null : form.nombre.trim(),
      color: form.color,
      hoist: form.hoist,
      mentionable: form.mentionable,
      permissions: form.bits.toString(),
    };
    try {
      if (rol) await updateRole(guildId, rol.id, datos);
      else await createRole(guildId, datos);
      showNotification(rol ? t('Role updated') : t('Role created'), 'success');
      onCerrar();
    } catch (err) {
      setError(typeof err === 'string' ? err : t('Could not save the role'));
    } finally {
      setGuardando(false);
    }
  };

  return (
    <Dialogo
      abierto
      titulo={rol ? t('Edit role “{name}”', { name: rol.name }) : t('Create role')}
      onCerrar={onCerrar}
      pie={
        <>
          <Button variante="secundario" onClick={onCerrar}>{t('Cancel')}</Button>
          <Button type="submit" form="form-rol" cargando={guardando}>{rol ? t('Save changes') : t('Create role')}</Button>
        </>
      }
    >
      <form id="form-rol" className="rol-form" onSubmit={(e) => { void guardar(e); }}>
        <Input data-autofocus etiqueta={t('Name')} value={esEveryone ? '@everyone' : form.nombre} disabled={esEveryone}
          maxLength={100} onChange={(e) => setForm({ ...form, nombre: e.target.value })} error={error} autoComplete="off" />
        <div className="rol-form__fila">
          <label className="rol-form__color">
            <span>{t('Color')}</span>
            <input type="color" value={colorHex(form.color || COLOR_POR_DEFECTO)}
              onChange={(e) => setForm({ ...form, color: parseInt(e.target.value.slice(1), 16) })} />
          </label>
          <Button variante="ghost" tamano="sm" disabled={form.color === 0} onClick={() => setForm({ ...form, color: 0 })}>
            {t('Remove color')}
          </Button>
        </div>
        <Interruptor etiqueta={t('Display separately')} ayuda={t('Lists the members with this role in their own group.')} valor={form.hoist} onCambio={(v) => setForm({ ...form, hoist: v })} />
        <Interruptor etiqueta={t('Mentionable')} ayuda={t('Anyone can mention the role with @.')} valor={form.mentionable} onCambio={(v) => setForm({ ...form, mentionable: v })} />
        <div className="rol-form__admin">
          <Interruptor etiqueta="Administrator" ayuda={t('Shortcut: grants every permission and bypasses channel overwrites. Use with care.')} valor={admin} onCambio={(v) => poner(BIT_ADMIN, v)} />
        </div>
        <div className="rol-form__permisos">
          {GRUPOS_PERMISOS.map(({ grupo, permisos }) => (
            <fieldset key={grupo} className="rol-form__grupo">
              <legend>{etiquetaGrupo(grupo)}</legend>
              {permisos.map(([bit, nombre]) => (
                <Interruptor key={bit} etiqueta={nombreOficialPermiso(nombre)} valor={admin || tiene(bit)} desactivado={admin} onCambio={(v) => poner(bit, v)} />
              ))}
            </fieldset>
          ))}
        </div>
      </form>
    </Dialogo>
  );
};

/** Resumen corto de la lista de permisos que Rust envía separada por comas. */
// Discord envía los permisos como un campo de bits en texto (puede pasar de
// 2^53, por eso BigInt). Nombres de los más relevantes, en orden de importancia.
const PERMISOS: [number, string][] = [
  [5, 'Gestionar servidor'], [28, 'Gestionar roles'], [4, 'Gestionar canales'],
  [2, 'Banear'], [1, 'Expulsar'], [40, 'Moderar miembros'], [13, 'Gestionar mensajes'],
  [29, 'Gestionar webhooks'], [17, 'Mencionar @everyone'], [11, 'Enviar mensajes'],
  [10, 'Ver canales'], [16, 'Leer historial'], [20, 'Conectar a voz'], [0, 'Crear invitaciones'],
];
const BIT_ADMIN = 3;

const resumenPermisos = (permisos: string): { admin: boolean; texto: string } => {
  let bits: bigint;
  try {
    bits = BigInt(permisos || '0');
  } catch {
    return { admin: false, texto: t('Unknown permissions') };
  }
  const tiene = (bit: number) => (bits >> BigInt(bit)) & 1n;
  if (tiene(BIT_ADMIN)) return { admin: true, texto: t('All permissions (Administrator)') };
  const nombres = PERMISOS.filter(([bit]) => tiene(bit)).map(([, nombre]) => nombreOficialPermiso(nombre));
  let total = 0;
  for (let b = bits; b > 0n; b >>= 1n) total += Number(b & 1n);
  if (total === 0) return { admin: false, texto: t('No permissions') };
  const mostrados = nombres.slice(0, 3).join(', ') || t('{count} permissions', { count: total });
  const resto = total - Math.min(3, nombres.length);
  return { admin: false, texto: nombres.length && resto > 0 ? t('{shown} and {count} more', { shown: mostrados, count: resto }) : mostrados };
};

export const RolesView: React.FC = () => {
  const serverMap = useServerMap();
  const showNotification = useShowNotification();
  const [search, setSearch] = useState('');
  const config = useConfig();
  const guildId = config.active_guild_id;
  const botId = config.active_bot_id;
  const motivoGestion = motivoPermisoAusente(useGuildPanel().permissions, 'Gestionar roles');
  const jerarquia = explicacionJerarquia();
  const [editando, setEditando] = useState<Role | 'nuevo' | null>(null);
  const [borrando, setBorrando] = useState<Role | null>(null);
  const [moviendo, setMoviendo] = useState(false);

  // Posición del rol más alto del bot: desde ahí hacia arriba, los roles quedan bloqueados.
  const topeBot = useMemo(() => {
    const bot = serverMap?.members.find((m) => m.user_id === botId);
    if (!bot) return null;
    const posiciones = (serverMap?.roles ?? []).filter((r) => bot.roles.includes(r.id)).map((r) => r.position);
    return posiciones.length ? Math.max(...posiciones) : 0;
  }, [serverMap, botId]);
  const bloqueado = (r: Role) => topeBot !== null && r.position >= topeBot;
  const esEveryone = (r: Role) => r.id === serverMap?.guild.id;

  // Discord no envía cuántos miembros tiene cada rol: se cuenta con los roles
  // de cada miembro (@everyone, cuyo id es el del servidor, los tiene todos).
  const miembrosPorRol = useMemo(() => {
    const cuenta = new Map<string, number>();
    for (const m of serverMap?.members ?? []) {
      for (const r of m.roles ?? []) cuenta.set(r, (cuenta.get(r) ?? 0) + 1);
    }
    return cuenta;
  }, [serverMap]);
  const contarMiembros = (id: string) =>
    id === serverMap?.guild.id ? serverMap?.members.length ?? 0 : miembrosPorRol.get(id) ?? 0;

  const roles = useMemo(() => {
    const q = search.trim().toLowerCase();
    return [...(serverMap?.roles ?? [])]
      .filter((r) => !q || r.name.toLowerCase().includes(q))
      .sort((a, b) => b.position - a.position);
  }, [serverMap, search]);

  const copyId = (id: string) => {
    void navigator.clipboard.writeText(id);
    showNotification(t('ID copied'), 'info');
  };

  const total = serverMap?.roles.length ?? 0;

  const mover = async (rol: Role, delta: 1 | -1) => {
    if (!guildId) return;
    // Intercambia la posición con el vecino en la lista completa (sin filtrar).
    const todos = [...(serverMap?.roles ?? [])].sort((a, b) => b.position - a.position);
    const vecino = todos[todos.findIndex((r) => r.id === rol.id) - delta];
    if (!vecino) return;
    setMoviendo(true);
    try {
      await reorderRoles(guildId, [[rol.id, vecino.position], [vecino.id, rol.position]]);
    } catch (e) {
      showNotification(typeof e === 'string' ? e : t('Could not move the role'), 'error');
    } finally {
      setMoviendo(false);
    }
  };

  const borrar = async () => {
    if (!guildId || !borrando) return;
    try {
      await deleteRole(guildId, borrando.id);
      showNotification(t('Role “{name}” deleted', { name: borrando.name }), 'success');
      setBorrando(null);
    } catch (e) {
      showNotification(typeof e === 'string' ? e : t('Could not delete the role'), 'error');
    }
  };

  return (
    <div className="vista roles-view">
      <CabeceraVista titulo={t('Roles')} cuenta={total}>
        <BuscadorVista valor={search} onCambio={setSearch} placeholder={t('Search roles…')} icono={<Search size={16} />} />
        <Button disabled={!!motivoGestion || !guildId} title={motivoGestion ?? undefined} onClick={() => setEditando('nuevo')}>
          <Plus size={14} /> {t('Create role')}
        </Button>
      </CabeceraVista>
      {motivoGestion && <p className="rol-aviso" role="status">{t('{reason}: you will not be able to create or edit roles.', { reason: motivoGestion })}</p>}
      {roles.some(bloqueado) && (
        <p className="rol-aviso" role="note"><Lock size={14} aria-hidden="true" /> <span>{t('Roles with a lock are at or above the bot’s highest role.')} {jerarquia}</span></p>
      )}

      <EstadoVista>
        {total === 0 ? (
          <EstadoVacio titulo={t('This server has no roles')} descripcion={t('Create a role in Discord and press Resync in the Dashboard to see it here.')} />
        ) : roles.length === 0 ? (
          <EstadoVacio titulo={t('No results')} descripcion={t('No role matches “{search}”. Clear the search to see them all.', { search })} />
        ) : (
          <div className="vista__tarjeta vista__tabla-caja">
            <table className="vista__tabla" aria-label={t('Roles')}>
              <thead>
                <tr>
                  <th scope="col">{t('Role')}</th>
                  <th scope="col" className="num" title={t('Position in the hierarchy')}>{t('Pos.')}</th>
                  <th scope="col">{t('Type')}</th>
                  <th scope="col" className="num">{t('Members')}</th>
                  <th scope="col"><span className="vista__solo-lector">{t('Actions')}</span></th>
                </tr>
              </thead>
              <tbody>
                {roles.map((role) => {
                  const perms = resumenPermisos(role.permissions);
                  const cerrado = bloqueado(role);
                  const motivo = motivoGestion ?? (cerrado ? jerarquia : null);
                  const ordenable = !esEveryone(role) && !motivo && !search && !moviendo;
                  const sobre = roles[roles.indexOf(role) - 1];
                  const bajo = roles[roles.indexOf(role) + 1];
                  return (
                    <tr key={role.id}>
                      <td>
                        <div className="vista__celda-rol">
                          <span className="rol__color" style={{ backgroundColor: role.color ? colorHex(role.color) : 'var(--text-3)' }} aria-hidden="true" />
                          <div className="vista__fila-cuerpo">
                            <span className="vista__fila-nombre vista__recorte">
                              {cerrado && <Lock size={12} aria-label={t('Locked by the hierarchy')} />} {role.name}
                            </span>
                            <span className="vista__fila-sub vista__recorte">
                              {role.color ? <span className="vista__mono">{colorHex(role.color).toUpperCase()}</span> : t('No color')} · {perms.texto}
                            </span>
                          </div>
                        </div>
                      </td>
                      <td className="num">{role.position}</td>
                      <td><Badge tono={perms.admin ? 'peligro' : 'neutro'}>{perms.admin ? 'Administrator' : t('Permissions')}</Badge></td>
                      <td className="num">{contarMiembros(role.id).toLocaleString()}</td>
                      <td className="rol-acciones">
                        <Button variante="ghost" tamano="sm" title={motivo ?? (search ? t('Clear the search to reorder') : t('Move up'))} aria-label={t('Move up {name}', { name: role.name })}
                          disabled={!ordenable || !sobre || bloqueado(sobre)} onClick={() => { void mover(role, 1); }}><ArrowUp size={12} /></Button>
                        <Button variante="ghost" tamano="sm" title={motivo ?? (search ? t('Clear the search to reorder') : t('Move down'))} aria-label={t('Move down {name}', { name: role.name })}
                          disabled={!ordenable || !bajo || esEveryone(bajo)} onClick={() => { void mover(role, -1); }}><ArrowDown size={12} /></Button>
                        <Button variante="ghost" tamano="sm" title={motivo ?? t('Edit role')} aria-label={t('Edit {name}', { name: role.name })}
                          disabled={!!motivo} onClick={() => setEditando(role)}><Pencil size={12} /></Button>
                        <Button variante="ghost" tamano="sm" title={motivo ?? (esEveryone(role) ? t('@everyone cannot be deleted') : t('Delete role'))} aria-label={t('Delete {name}', { name: role.name })}
                          disabled={!!motivo || esEveryone(role)} onClick={() => setBorrando(role)}><Trash2 size={12} /></Button>
                        <Button variante="ghost" tamano="sm" title={t('Copy ID')} aria-label={t('Copy ID of {name}', { name: role.name })} onClick={() => copyId(role.id)}>
                          <Copy size={12} />
                        </Button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </EstadoVista>

      {editando && guildId && (
        <DialogoRol
          key={editando === 'nuevo' ? 'nuevo' : editando.id}
          rol={editando === 'nuevo' ? null : editando}
          esEveryone={editando !== 'nuevo' && esEveryone(editando)}
          guildId={guildId}
          onCerrar={() => setEditando(null)}
        />
      )}
      <ConfirmarPeligroso
        abierto={!!borrando}
        titulo={t('Delete role')}
        descripcion={t('The role is removed from every member who has it. This cannot be undone.')}
        nombre={borrando?.name ?? ''}
        onConfirmar={borrar}
        onCerrar={() => setBorrando(null)}
      />
    </div>
  );
};
