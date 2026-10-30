import React, { useEffect, useMemo, useState } from 'react';
import { Check, Minus, X, Plus, Trash2, Users, User } from 'lucide-react';
import { Button, Badge, Select } from './ui';
import { GRUPOS_PERMISOS } from './RolesView';
import { t } from '../i18n';
import { nombreOficialPermiso } from '../permisos';
import type { Channel, Member, Role } from '../types';

type Estado = 'permitir' | 'heredar' | 'denegar';

export interface Entrada { id: string; tipo: number; allow: bigint; deny: bigint }
export interface CambiosPermisos { fijar: Entrada[]; quitar: Entrada[] }

const clave = (e: { id: string; tipo: number }) => `${e.tipo}:${e.id}`;
const aBits = (s: string) => { try { return BigInt(s || '0'); } catch { return 0n; } };
const cuentaBits = (b: bigint) => { let n = 0; for (; b > 0n; b >>= 1n) n += Number(b & 1n); return n; };

const desdeCanal = (c: Channel): Entrada[] =>
  (c.overwrites ?? []).map((o) => ({ id: o.id, tipo: o.type_, allow: aBits(o.allow), deny: aBits(o.deny) }));

const estadoDe = (e: Entrada, bit: number): Estado => {
  const m = 1n << BigInt(bit);
  return e.allow & m ? 'permitir' : e.deny & m ? 'denegar' : 'heredar';
};

const ESTADOS: { valor: Estado; icono: any }[] = [
  { valor: 'permitir', icono: Check },
  { valor: 'heredar', icono: Minus },
  { valor: 'denegar', icono: X },
];

const etiquetaEstado = (e: Estado): string =>
  e === 'permitir' ? t('Allow') : e === 'heredar' ? t('Inherit') : t('Deny');

const etiquetaGrupo = (g: string): string => {
  switch (g) {
    case 'General': return t('General');
    case 'Miembros': return t('Members');
    case 'Texto': return t('Text');
    case 'Voz': return t('Voice');
    case 'Avanzado': return t('Advanced');
    default: return g;
  }
};

export const PermisosCanal: React.FC<{
  canal: Channel;
  roles: Role[];
  miembros: Member[];
  /** Motivo por el que no se puede editar (falta «Gestionar roles»), o null. */
  motivoBloqueo: string | null;
  onGuardar: (cambios: CambiosPermisos) => Promise<void>;
}> = ({ canal, roles, miembros, motivoBloqueo, onGuardar }) => {
  const original = useMemo(() => desdeCanal(canal), [canal]);
  const [borrador, setBorrador] = useState<Entrada[]>(original);
  const [sel, setSel] = useState<string | null>(null);
  const [guardando, setGuardando] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    setBorrador(original);
    setError('');
  }, [original]);

  useEffect(() => { setSel(null); }, [canal.id]);

  const nombreDe = (e: Entrada) => {
    if (e.tipo === 0) return roles.find((r) => r.id === e.id)?.name ?? t('Role {id}', { id: e.id });
    const m = miembros.find((x) => x.user_id === e.id);
    return m ? m.nickname || m.username : t('Member {id}', { id: e.id });
  };

  const cambios = useMemo<CambiosPermisos>(() => {
    const ant = new Map(original.map((e) => [clave(e), e]));
    const act = new Set(borrador.map(clave));
    const fijar = borrador.filter((e) => {
      const o = ant.get(clave(e));
      return !o || o.allow !== e.allow || o.deny !== e.deny;
    });
    const quitar = original.filter((e) => !act.has(clave(e)));
    return { fijar, quitar };
  }, [original, borrador]);
  const totalCambios = cambios.fijar.length + cambios.quitar.length;

  const actual = borrador.find((e) => clave(e) === sel) ?? null;

  const poner = (bit: number, estado: Estado) =>
    setBorrador((b) => b.map((e) => {
      if (clave(e) !== sel) return e;
      const m = 1n << BigInt(bit);
      return {
        ...e,
        allow: estado === 'permitir' ? e.allow | m : e.allow & ~m,
        deny: estado === 'denegar' ? e.deny | m : e.deny & ~m,
      };
    }));

  const anadir = (tipo: number, id: string) => {
    if (!id) return;
    const nueva: Entrada = { id, tipo, allow: 0n, deny: 0n };
    setBorrador((b) => [...b, nueva]);
    setSel(clave(nueva));
  };

  const quitar = (e: Entrada) => {
    setBorrador((b) => b.filter((x) => clave(x) !== clave(e)));
    if (sel === clave(e)) setSel(null);
  };

  const descartar = () => { setBorrador(original); setSel(null); setError(''); };

  const guardar = async () => {
    setGuardando(true);
    setError('');
    try {
      await onGuardar(cambios);
    } catch (err) {
      setError(typeof err === 'string' ? err : t('Could not save the permissions'));
    } finally {
      setGuardando(false);
    }
  };

  const usados = new Set(borrador.map(clave));
  const rolesLibres = roles.filter((r) => !usados.has(clave({ id: r.id, tipo: 0 })));
  const miembrosLibres = miembros.filter((m) => !usados.has(clave({ id: m.user_id, tipo: 1 })));
  const bloqueado = !!motivoBloqueo;

  const resumen = (e: Entrada) => {
    const p = cuentaBits(e.allow), d = cuentaBits(e.deny);
    return p + d === 0 ? t('All inherited') : t('{allowed} allowed · {denied} denied', { allowed: p, denied: d });
  };

  return (
    <section className="permisos" aria-label={t('Channel permissions')}>
      <h4 className="permisos__titulo">{t('Channel permissions')}</h4>
      {bloqueado && <span className="vista__nota">{motivoBloqueo}</span>}

      <div className="permisos__cuerpo">
        <div className="permisos__lista">
          <div role="listbox" aria-label={t('Channel overwrites')} className="permisos__items">
            {borrador.length === 0 && (
              <span className="vista__nota">{t('No overwrites: the channel inherits the permissions of its category and the server.')}</span>
            )}
            {borrador.map((e) => {
              const Icono = e.tipo === 0 ? Users : User;
              const k = clave(e);
              return (
                <div key={k} className={`permisos__item${sel === k ? ' permisos__item--activo' : ''}`}>
                  <button className="permisos__sel" role="option" aria-selected={sel === k} onClick={() => setSel(k)}>
                    <Icono size={14} aria-hidden="true" />
                    <span className="permisos__nombre">
                      <span>{nombreDe(e)}</span>
                      <small className="num">{resumen(e)}</small>
                    </span>
                    <Badge tono={e.tipo === 0 ? 'acento' : 'info'}>{e.tipo === 0 ? t('Role') : t('Member')}</Badge>
                  </button>
                  <Button variante="ghost" tamano="sm" disabled={bloqueado} title={motivoBloqueo ?? t('Remove overwrite')} aria-label={t('Remove the overwrite of {name}', { name: nombreDe(e) })} onClick={() => quitar(e)}>
                    <Trash2 size={12} />
                  </Button>
                </div>
              );
            })}
          </div>
          <Select etiqueta={t('Add role')} value="" disabled={bloqueado || rolesLibres.length === 0} onChange={(ev) => anadir(0, ev.target.value)}>
            <option value="">{rolesLibres.length ? t('Choose a role…') : t('No roles left')}</option>
            {rolesLibres.map((r) => <option key={r.id} value={r.id}>{r.name}</option>)}
          </Select>
          <Select etiqueta={t('Add member')} value="" disabled={bloqueado || miembrosLibres.length === 0} onChange={(ev) => anadir(1, ev.target.value)}>
            <option value="">{miembrosLibres.length ? t('Choose a member…') : t('No members left')}</option>
            {miembrosLibres.map((m) => <option key={m.user_id} value={m.user_id}>{m.nickname || m.username}</option>)}
          </Select>
        </div>

        <div className="permisos__detalle">
          {!actual ? (
            <span className="vista__nota"><Plus size={12} aria-hidden="true" /> {t('Pick a role or member from the list, or add one, to edit their permissions.')}</span>
          ) : (
            GRUPOS_PERMISOS.map(({ grupo, permisos }) => (
              <fieldset key={grupo} className="permisos__grupo" disabled={bloqueado}>
                <legend>{etiquetaGrupo(grupo)}</legend>
                {permisos.map(([bit, nombreEs]) => {
                  const estado = estadoDe(actual, bit);
                  const nombre = nombreOficialPermiso(nombreEs);
                  return (
                    <div key={bit} className="permisos__fila">
                      <span>{nombre}</span>
                      <div className="permisos__tres" role="radiogroup" aria-label={t('{permission} for {name}', { permission: nombre, name: nombreDe(actual) })}>
                        {ESTADOS.map(({ valor, icono: I }) => (
                          <button
                            key={valor}
                            type="button"
                            role="radio"
                            aria-checked={estado === valor}
                            aria-label={etiquetaEstado(valor)}
                            title={etiquetaEstado(valor)}
                            className={`permisos__op permisos__op--${valor}${estado === valor ? ' permisos__op--on' : ''}`}
                            onClick={() => poner(bit, valor)}
                          >
                            <I size={12} aria-hidden="true" />
                          </button>
                        ))}
                      </div>
                    </div>
                  );
                })}
              </fieldset>
            ))
          )}
        </div>
      </div>

      {error && <span className="ui-campo__error" role="alert">{error}</span>}
      <div className="vista__etiquetas">
        <Button disabled={bloqueado || totalCambios === 0} cargando={guardando} title={motivoBloqueo ?? undefined} onClick={guardar}>{t('Save changes')}</Button>
        <Button variante="secundario" disabled={totalCambios === 0 || guardando} onClick={descartar}>{t('Discard')}</Button>
        {totalCambios > 0 && <span className="vista__nota num">{totalCambios === 1 ? t('1 unsaved change') : t('{count} unsaved changes', { count: totalCambios })}</span>}
      </div>
    </section>
  );
};
