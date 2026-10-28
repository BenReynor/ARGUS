import React, { useMemo, useState } from 'react';
import { useServerMap, useShowNotification } from '../store/useStore';
import { Copy, Search } from 'lucide-react';
import { Badge, Button, EstadoVacio } from './ui';
import { EstadoVista, BuscadorVista, CabeceraVista, colorHex } from './EstadoVista';

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
    return { admin: false, texto: 'Permisos desconocidos' };
  }
  const tiene = (bit: number) => (bits >> BigInt(bit)) & 1n;
  if (tiene(BIT_ADMIN)) return { admin: true, texto: 'Todos los permisos (Administrador)' };
  const nombres = PERMISOS.filter(([bit]) => tiene(bit)).map(([, nombre]) => nombre);
  let total = 0;
  for (let b = bits; b > 0n; b >>= 1n) total += Number(b & 1n);
  if (total === 0) return { admin: false, texto: 'Sin permisos' };
  const mostrados = nombres.slice(0, 3).join(', ') || `${total} permisos`;
  const resto = total - Math.min(3, nombres.length);
  return { admin: false, texto: nombres.length && resto > 0 ? `${mostrados} y ${resto} más` : mostrados };
};

export const RolesView: React.FC = () => {
  const serverMap = useServerMap();
  const showNotification = useShowNotification();
  const [search, setSearch] = useState('');

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
    showNotification('ID copiado', 'info');
  };

  const total = serverMap?.roles.length ?? 0;

  return (
    <div className="vista roles-view">
      <CabeceraVista titulo="Roles" cuenta={total}>
        <BuscadorVista valor={search} onCambio={setSearch} placeholder="Buscar roles…" icono={<Search size={16} />} />
      </CabeceraVista>

      <EstadoVista>
        {total === 0 ? (
          <EstadoVacio titulo="Este servidor no tiene roles" descripcion="Crea un rol en Discord y pulsa Resincronizar en el Panel para verlo aquí." />
        ) : roles.length === 0 ? (
          <EstadoVacio titulo="Sin resultados" descripcion={`Ningún rol coincide con «${search}». Borra la búsqueda para ver todos.`} />
        ) : (
          <div className="vista__tarjeta vista__tabla-caja">
            <table className="vista__tabla" aria-label="Roles">
              <thead>
                <tr>
                  <th scope="col">Rol</th>
                  <th scope="col" className="num" title="Posición en la jerarquía">Pos.</th>
                  <th scope="col">Tipo</th>
                  <th scope="col" className="num">Miembros</th>
                  <th scope="col"><span className="vista__solo-lector">Acciones</span></th>
                </tr>
              </thead>
              <tbody>
                {roles.map((role) => {
                  const perms = resumenPermisos(role.permissions);
                  return (
                    <tr key={role.id}>
                      <td>
                        <div className="vista__celda-rol">
                          <span className="rol__color" style={{ backgroundColor: role.color ? colorHex(role.color) : 'var(--text-3)' }} aria-hidden="true" />
                          <div className="vista__fila-cuerpo">
                            <span className="vista__fila-nombre vista__recorte">{role.name}</span>
                            <span className="vista__fila-sub vista__recorte">
                              {role.color ? <span className="vista__mono">{colorHex(role.color).toUpperCase()}</span> : 'Sin color'} · {perms.texto}
                            </span>
                          </div>
                        </div>
                      </td>
                      <td className="num">{role.position}</td>
                      <td><Badge tono={perms.admin ? 'peligro' : 'neutro'}>{perms.admin ? 'Administrador' : 'Permisos'}</Badge></td>
                      <td className="num">{contarMiembros(role.id).toLocaleString()}</td>
                      <td>
                        <Button variante="ghost" tamano="sm" title="Copiar ID" aria-label={`Copiar ID de ${role.name}`} onClick={() => copyId(role.id)}>
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
    </div>
  );
};
