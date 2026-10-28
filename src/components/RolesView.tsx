import React, { useMemo, useState } from 'react';
import { useServerMap, useShowNotification } from '../store/useStore';
import { Copy, Search } from 'lucide-react';
import { Badge, Button, EstadoVacio } from './ui';
import { EstadoVista, BuscadorVista, CabeceraVista, colorHex } from './EstadoVista';

/** Resumen corto de la lista de permisos que Rust envía separada por comas. */
const resumenPermisos = (permisos: string): { admin: boolean; texto: string } => {
  const lista = permisos.split(',').map((p) => p.trim()).filter(Boolean);
  if (lista.some((p) => p === 'Administrator' || p === 'Administrador')) return { admin: true, texto: 'Todos los permisos' };
  if (lista.length === 0) return { admin: false, texto: 'Sin permisos especiales' };
  const mostrados = lista.slice(0, 3).join(', ');
  return { admin: false, texto: lista.length > 3 ? `${mostrados} y ${lista.length - 3} más` : mostrados };
};

export const RolesView: React.FC = () => {
  const serverMap = useServerMap();
  const showNotification = useShowNotification();
  const [search, setSearch] = useState('');

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
                          <span className="rol__color" style={{ backgroundColor: colorHex(role.color) }} aria-hidden="true" />
                          <div className="vista__fila-cuerpo">
                            <span className="vista__fila-nombre vista__recorte">{role.name}</span>
                            <span className="vista__fila-sub vista__recorte">
                              <span className="vista__mono">{colorHex(role.color).toUpperCase()}</span> · {perms.texto}
                            </span>
                          </div>
                        </div>
                      </td>
                      <td className="num">{role.position}</td>
                      <td><Badge tono={perms.admin ? 'peligro' : 'neutro'}>{perms.admin ? 'Administrador' : 'Permisos'}</Badge></td>
                      <td className="num">{role.member_count.toLocaleString()}</td>
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
