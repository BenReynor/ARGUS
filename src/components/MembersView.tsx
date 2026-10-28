import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useServerMap, useAddLog, useShowNotification, useConfig, useGuildPanel, motivoPermisoAusente } from '../store/useStore';
import { sendDM, addRole, removeRole, timeoutMember } from '../api/tauri';
import { Avatar, Badge, Button, EstadoVacio } from './ui';
import { EstadoVista, BuscadorVista, CabeceraVista } from './EstadoVista';
import { MoreVertical, Search } from 'lucide-react';
import type { Member, Role } from '../types';

interface AccionMenu {
  clave: string;
  etiqueta: string;
  motivo: string | null;
  ejecutar: () => void;
}

/** Menú de acciones; una acción sin permiso queda desactivada y muestra por qué. */
const MenuAcciones: React.FC<{ nombre: string; acciones: AccionMenu[] }> = ({ nombre, acciones }) => {
  const [abierto, setAbierto] = useState(false);
  const raiz = useRef<HTMLDivElement>(null);

  const cerrar = (devolverFoco: boolean) => {
    setAbierto(false);
    if (devolverFoco) raiz.current?.querySelector<HTMLElement>('[aria-haspopup="menu"]')?.focus();
  };

  // Al abrir, el foco pasa al primer ítem activo; un clic fuera cierra el menú.
  useEffect(() => {
    if (!abierto) return;
    raiz.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
    const fuera = (e: MouseEvent) => {
      if (raiz.current && !raiz.current.contains(e.target as Node)) setAbierto(false);
    };
    document.addEventListener('mousedown', fuera);
    return () => document.removeEventListener('mousedown', fuera);
  }, [abierto]);

  const teclas = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape' && abierto) {
      e.stopPropagation();
      cerrar(true);
    } else if (abierto && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      const items = Array.from(raiz.current?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? []);
      if (items.length === 0) return;
      const i = items.indexOf(document.activeElement as HTMLElement);
      const paso = e.key === 'ArrowDown' ? 1 : -1;
      items[(i + paso + items.length) % items.length].focus();
    } else if (abierto && e.key === 'Tab') {
      setAbierto(false);
    }
  };

  return (
    <div className="menu" ref={raiz} onKeyDown={teclas}>
      <Button variante="ghost" tamano="sm" aria-haspopup="menu" aria-expanded={abierto} aria-label={`Acciones de ${nombre}`} onClick={() => setAbierto((v) => !v)}>
        <MoreVertical size={14} aria-hidden="true" />
      </Button>
      {abierto && (
        <div className="menu__lista" role="menu">
          {acciones.map((a) => (
            <button
              key={a.clave}
              role="menuitem"
              className="menu__item"
              disabled={!!a.motivo}
              onClick={() => { cerrar(true); a.ejecutar(); }}
            >
              {a.etiqueta}
              {a.motivo && <span className="menu__motivo">{a.motivo}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
};

export const MembersView: React.FC = () => {
  const serverMap = useServerMap();
  const addLog = useAddLog();
  const guildId = useConfig().active_guild_id;
  const showNotification = useShowNotification();
  const permisos = useGuildPanel().permissions;
  const motivoTimeout = motivoPermisoAusente(permisos, 'Moderar miembros');
  const motivoRoles = motivoPermisoAusente(permisos, 'Gestionar roles');
  const [search, setSearch] = useState('');

  const roles: Role[] = serverMap?.roles ?? [];

  const miembros = useMemo(() => {
    const q = search.trim().toLowerCase();
    return [...(serverMap?.members ?? [])]
      .filter((m) => !q || m.username.toLowerCase().includes(q) || (m.nickname ?? '').toLowerCase().includes(q))
      .sort((a, b) => (a.nickname || a.username).localeCompare(b.nickname || b.username));
  }, [serverMap, search]);

  const errorTexto = (e: unknown, defecto: string) => (typeof e === 'string' ? e : defecto);

  const handleDM = async (m: Member) => {
    const content = prompt(`DM a ${m.username}:`);
    if (!content) return;
    try {
      await sendDM(m.user_id, content);
      addLog(`[DM] ${m.username}`);
      showNotification('DM enviado', 'success');
    } catch (e) { showNotification(errorTexto(e, 'No se pudo enviar el DM'), 'error'); }
  };

  const handleRole = async (m: Member, role: Role, add: boolean) => {
    try {
      if (!guildId) throw new Error('No hay servidor seleccionado');
      if (add) await addRole(guildId, m.user_id, role.id);
      else await removeRole(guildId, m.user_id, role.id);
      showNotification(add ? 'Rol añadido' : 'Rol quitado', 'success');
    } catch (e) { showNotification(errorTexto(e, 'Error al cambiar el rol'), 'error'); }
  };

  const handleTimeout = async (m: Member) => {
    const mins = prompt(`Minutos de timeout para ${m.username}:`, '10');
    if (!mins) return;
    try {
      if (!guildId) { showNotification('No hay servidor seleccionado', 'error'); return; }
      await timeoutMember(guildId, m.user_id, parseInt(mins, 10));
      showNotification(`Timeout de ${mins} min aplicado`, 'success');
    } catch (e) { showNotification(errorTexto(e, 'No se pudo aplicar el timeout'), 'error'); }
  };

  const accionesDe = (m: Member): AccionMenu[] => {
    const acciones: AccionMenu[] = [
      { clave: 'dm', etiqueta: 'Enviar DM', motivo: m.bot ? 'No se puede enviar DM a un bot' : null, ejecutar: () => { void handleDM(m); } },
      { clave: 'timeout', etiqueta: 'Aplicar timeout', motivo: m.bot ? 'No se puede aplicar timeout a un bot' : motivoTimeout, ejecutar: () => { void handleTimeout(m); } },
    ];
    for (const r of roles.filter((x) => x.name !== '@everyone').slice(0, 10)) {
      const tiene = m.roles.includes(r.name);
      acciones.push({
        clave: `rol-${r.id}`,
        etiqueta: `${tiene ? 'Quitar' : 'Añadir'} rol ${r.name}`,
        motivo: motivoRoles,
        ejecutar: () => { void handleRole(m, r, !tiene); },
      });
    }
    return acciones;
  };

  const total = serverMap?.members.length ?? 0;

  return (
    <div className="vista members-view">
      <CabeceraVista titulo="Miembros" cuenta={total}>
        <BuscadorVista valor={search} onCambio={setSearch} placeholder="Buscar miembros…" icono={<Search size={16} />} />
      </CabeceraVista>

      <EstadoVista>
        {total === 0 ? (
          <EstadoVacio titulo="No hay miembros cargados" descripcion="Activa el intent de miembros del bot en el portal de desarrolladores de Discord y pulsa Resincronizar en el Panel." />
        ) : miembros.length === 0 ? (
          <EstadoVacio titulo="Sin resultados" descripcion={`Ningún miembro coincide con «${search}». Prueba con otro nombre o borra la búsqueda.`} />
        ) : (
          <ul className="vista__lista vista__tarjeta" aria-label="Miembros">
            {miembros.map((m) => {
              const nombre = m.nickname || m.username;
              return (
                <li key={m.user_id} className="vista__fila">
                  <Avatar src={m.avatar} nombre={nombre} tamano={32} />
                  <div className="vista__fila-cuerpo">
                    <span className="vista__fila-nombre vista__recorte">{nombre}</span>
                    {m.nickname && <span className="vista__fila-sub vista__recorte">@{m.username}</span>}
                  </div>
                  <div className="vista__etiquetas">
                    {m.bot && <Badge tono="acento">BOT</Badge>}
                    {m.roles.slice(0, 3).map((r) => <Badge key={r}>{r}</Badge>)}
                    {m.roles.length > 3 && <Badge>+{m.roles.length - 3}</Badge>}
                  </div>
                  <MenuAcciones nombre={nombre} acciones={accionesDe(m)} />
                </li>
              );
            })}
          </ul>
        )}
      </EstadoVista>
    </div>
  );
};
