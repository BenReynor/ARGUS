import React, { useMemo, useState } from 'react';
import { useServerMap, useShowNotification } from '../store/useStore';
import { Copy, Search } from 'lucide-react';
import { Badge, Button, EstadoVacio } from './ui';
import { EstadoVista, BuscadorVista, colorHex } from './EstadoVista';

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
      <div className="vista__cabecera">
        <h2>Roles del servidor <span className="vista__cuenta">{total}</span></h2>
        <BuscadorVista valor={search} onCambio={setSearch} placeholder="Buscar roles…" icono={<Search size={16} />} />
      </div>

      <EstadoVista>
        {total === 0 ? (
          <EstadoVacio titulo="Este servidor no tiene roles" />
        ) : roles.length === 0 ? (
          <EstadoVacio titulo="Sin resultados" descripcion={`Ningún rol coincide con «${search}».`} />
        ) : (
          <ul className="vista__lista" aria-label="Roles">
            {roles.map((role) => {
              const perms = resumenPermisos(role.permissions);
              return (
                <li key={role.id} className="vista__fila">
                  <span className="rol__color" style={{ backgroundColor: colorHex(role.color) }} title={colorHex(role.color).toUpperCase()} aria-hidden="true" />
                  <div className="vista__fila-cuerpo">
                    <span className="vista__fila-nombre">{role.name}</span>
                    <span className="vista__fila-sub">{perms.texto}</span>
                  </div>
                  <span className="vista__mono" title="Posición en la jerarquía">Pos. {role.position}</span>
                  <span className="vista__mono">{colorHex(role.color).toUpperCase()}</span>
                  <Badge tono={perms.admin ? 'peligro' : 'neutro'}>{perms.admin ? 'Administrador' : 'Permisos'}</Badge>
                  <span className="vista__cuenta">{role.member_count} miembros</span>
                  <Button variante="ghost" tamano="sm" title="Copiar ID" aria-label={`Copiar ID de ${role.name}`} onClick={() => copyId(role.id)}>
                    <Copy size={12} />
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
      </EstadoVista>
    </div>
  );
};
