import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useServerMap, useAddLog, useShowNotification, useConfig, useGuildPanel, motivoPermisoAusente } from '../store/useStore';
import { sendDM, addRole, removeRole, timeoutMember, moderationContext, kickMember, banMember, unbanMember, listBans, setNickname } from '../api/tauri';
import type { ContextoModeracion, Baneo } from '../api/tauri';
import { Avatar, Badge, Button, EstadoVacio, Dialogo, Input, Select, Cargando, ErrorPanel } from './ui';
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

type Moderacion = { tipo: 'apodo' | 'expulsar' | 'banear'; miembro: Member };

/** Diálogo de moderación: motivo (va al registro de auditoría) y, al banear, días de mensajes a borrar. */
const DialogoModeracion: React.FC<{
  accion: Moderacion | null;
  onCerrar: () => void;
  onConfirmar: (a: Moderacion, texto: string, dias: number) => Promise<void>;
}> = ({ accion, onCerrar, onConfirmar }) => {
  const [texto, setTexto] = useState('');
  const [dias, setDias] = useState(0);
  const [trabajando, setTrabajando] = useState(false);
  useEffect(() => {
    if (accion) {
      setTexto(accion.tipo === 'apodo' ? accion.miembro.nickname ?? '' : '');
      setDias(0);
      setTrabajando(false);
    }
  }, [accion]);
  if (!accion) return <Dialogo abierto={false} titulo="" onCerrar={onCerrar} />;
  const { tipo, miembro } = accion;
  const nombre = miembro.nickname || miembro.username;
  const textos = {
    apodo: { titulo: `Cambiar apodo de ${nombre}`, boton: 'Guardar apodo', etiqueta: 'Apodo nuevo', ayuda: 'Déjalo vacío para quitar el apodo.' },
    expulsar: { titulo: `Expulsar a ${nombre}`, boton: 'Confirmar expulsión', etiqueta: 'Motivo (opcional)', ayuda: 'Podrá volver a entrar con una invitación. El motivo queda en el registro de auditoría.' },
    banear: { titulo: `Banear a ${nombre}`, boton: 'Confirmar baneo', etiqueta: 'Motivo (opcional)', ayuda: 'No podrá volver a entrar hasta que lo desbanees. El motivo queda en el registro de auditoría.' },
  }[tipo];
  const enviar = async () => {
    if (trabajando) return;
    setTrabajando(true);
    try { await onConfirmar(accion, texto.trim(), dias); } finally { setTrabajando(false); }
  };
  return (
    <Dialogo
      abierto
      titulo={textos.titulo}
      descripcion={tipo === 'apodo' ? undefined : 'Esta acción es seria: revisa los datos y confirma.'}
      onCerrar={onCerrar}
      pie={
        <>
          <Button variante="secundario" onClick={onCerrar}>Cancelar</Button>
          <Button variante={tipo === 'apodo' ? 'primario' : 'peligro'} cargando={trabajando} onClick={() => { void enviar(); }}>{textos.boton}</Button>
        </>
      }
    >
      <form className="mod__form" onSubmit={(e) => { e.preventDefault(); void enviar(); }}>
        <Input data-autofocus etiqueta={textos.etiqueta} ayuda={textos.ayuda} value={texto} maxLength={tipo === 'apodo' ? 32 : 400} onChange={(e) => setTexto(e.target.value)} autoComplete="off" />
        {tipo === 'banear' && (
          <Select etiqueta="Mensajes a borrar" ayuda="Elimina los mensajes recientes de esta persona en todo el servidor." value={dias} onChange={(e) => setDias(Number(e.target.value))}>
            <option value={0}>No borrar ninguno</option>
            <option value={1}>Último día</option>
            {[2, 3, 4, 5, 6].map((d) => <option key={d} value={d}>Últimos {d} días</option>)}
            <option value={7}>Últimos 7 días</option>
          </Select>
        )}
      </form>
    </Dialogo>
  );
};

/** Lista de baneos con motivo y botón para desbanear. */
const PestanaBaneados: React.FC<{ guildId: string | null; puede: string | null }> = ({ guildId, puede }) => {
  const showNotification = useShowNotification();
  const [baneos, setBaneos] = useState<Baneo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [trabajando, setTrabajando] = useState<string | null>(null);

  const cargar = async () => {
    if (!guildId) return;
    setError(null);
    setBaneos(null);
    try { setBaneos(await listBans(guildId)); }
    catch (e) { setError(typeof e === 'string' ? e : 'No se pudo leer la lista de baneos'); }
  };
  useEffect(() => { void cargar(); }, [guildId]); // eslint-disable-line react-hooks/exhaustive-deps

  const desbanear = async (b: Baneo) => {
    if (!guildId) return;
    setTrabajando(b.user_id);
    try {
      await unbanMember(guildId, b.user_id, null);
      showNotification(`${b.username} desbaneado`, 'success');
      setBaneos((l) => (l ?? []).filter((x) => x.user_id !== b.user_id));
    } catch (e) { showNotification(typeof e === 'string' ? e : 'No se pudo desbanear', 'error'); }
    finally { setTrabajando(null); }
  };

  if (error) return <ErrorPanel mensaje={error} onReintentar={() => { void cargar(); }} />;
  if (!baneos) return <Cargando lineas={3} />;
  if (baneos.length === 0) return <EstadoVacio titulo="No hay baneos" descripcion="Nadie está baneado en este servidor." />;
  return (
    <ul className="vista__lista vista__tarjeta" aria-label="Baneados">
      {baneos.map((b) => (
        <li key={b.user_id} className="vista__fila">
          <Avatar nombre={b.username} tamano={32} />
          <div className="vista__fila-cuerpo">
            <span className="vista__fila-nombre vista__recorte">{b.username}</span>
            <span className="vista__fila-sub vista__recorte">{b.reason ? `Motivo: ${b.reason}` : 'Sin motivo indicado'}</span>
          </div>
          <span title={puede ?? undefined}>
            <Button variante="secundario" tamano="sm" disabled={!!puede} cargando={trabajando === b.user_id} onClick={() => { void desbanear(b); }}>Desbanear</Button>
          </span>
        </li>
      ))}
    </ul>
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
  const motivoExpulsar = motivoPermisoAusente(permisos, 'Expulsar miembros');
  const motivoBanear = motivoPermisoAusente(permisos, 'Banear miembros');
  const motivoApodos = motivoPermisoAusente(permisos, 'Gestionar apodos');
  const [search, setSearch] = useState('');
  const [pestana, setPestana] = useState<'miembros' | 'baneados'>('miembros');
  const [ctx, setCtx] = useState<ContextoModeracion | null>(null);
  const [moderacion, setModeracion] = useState<Moderacion | null>(null);

  // Dueño y rol más alto del bot: sin ellos no se sabe qué miembros se pueden moderar.
  useEffect(() => {
    setCtx(null);
    if (!guildId) return;
    let vigente = true;
    moderationContext(guildId).then((c) => { if (vigente) setCtx(c); }).catch(() => {});
    return () => { vigente = false; };
  }, [guildId]);

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

  const confirmarModeracion = async (a: Moderacion, texto: string, dias: number) => {
    const nombre = a.miembro.nickname || a.miembro.username;
    try {
      if (!guildId) throw new Error('No hay servidor seleccionado');
      const motivo = texto || null;
      if (a.tipo === 'apodo') await setNickname(guildId, a.miembro.user_id, motivo, null);
      else if (a.tipo === 'expulsar') await kickMember(guildId, a.miembro.user_id, motivo);
      else await banMember(guildId, a.miembro.user_id, motivo, dias);
      addLog(`[${a.tipo}] ${nombre}`);
      showNotification(a.tipo === 'apodo' ? 'Apodo actualizado' : a.tipo === 'expulsar' ? `${nombre} expulsado` : `${nombre} baneado`, 'success');
      setModeracion(null);
    } catch (e) { showNotification(errorTexto(e, 'No se pudo completar la acción'), 'error'); }
  };

  /** Por qué no se puede moderar a este miembro (dueño, bot o rol igual/superior), o null. */
  const motivoJerarquia = (m: Member): string | null => {
    if (!ctx) return 'Comprobando la jerarquía de roles…';
    if (ctx.owner_id === m.user_id) return 'No se puede moderar al dueño del servidor';
    if (ctx.bot_id === m.user_id) return 'El bot no puede moderarse a sí mismo';
    const top = Math.max(0, ...roles.filter((r) => m.roles.includes(r.name)).map((r) => r.position));
    return top >= ctx.bot_top_position ? 'Su rol más alto es igual o superior al del bot: sube el rol del bot' : null;
  };

  const accionesDe = (m: Member): AccionMenu[] => {
    const jerarquia = motivoJerarquia(m);
    const acciones: AccionMenu[] = [
      { clave: 'dm', etiqueta: 'Enviar DM', motivo: m.bot ? 'No se puede enviar DM a un bot' : null, ejecutar: () => { void handleDM(m); } },
      { clave: 'timeout', etiqueta: 'Aplicar timeout', motivo: m.bot ? 'No se puede aplicar timeout a un bot' : motivoTimeout, ejecutar: () => { void handleTimeout(m); } },
    ];
    acciones.push(
      { clave: 'apodo', etiqueta: 'Cambiar apodo', motivo: motivoApodos ?? jerarquia, ejecutar: () => setModeracion({ tipo: 'apodo', miembro: m }) },
      { clave: 'expulsar', etiqueta: 'Expulsar', motivo: motivoExpulsar ?? jerarquia, ejecutar: () => setModeracion({ tipo: 'expulsar', miembro: m }) },
      { clave: 'banear', etiqueta: 'Banear', motivo: motivoBanear ?? jerarquia, ejecutar: () => setModeracion({ tipo: 'banear', miembro: m }) },
    );
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
        {pestana === 'miembros' && <BuscadorVista valor={search} onCambio={setSearch} placeholder="Buscar miembros…" icono={<Search size={16} />} />}
      </CabeceraVista>

      <div className="pestanas" role="tablist" aria-label="Secciones de miembros">
        {([['miembros', 'Miembros'], ['baneados', 'Baneados']] as [string, string][]).map(([id, etiqueta]) => (
          <button key={id} role="tab" aria-selected={pestana === id} className="pestanas__tab" onClick={() => setPestana(id as 'miembros' | 'baneados')}>{etiqueta}</button>
        ))}
      </div>

      <EstadoVista>
        {pestana === 'baneados' ? (
          <PestanaBaneados guildId={guildId} puede={motivoBanear} />
        ) : total === 0 ? (
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
      <DialogoModeracion accion={moderacion} onCerrar={() => setModeracion(null)} onConfirmar={confirmarModeracion} />
    </div>
  );
};
