import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useServerMap, useAddLog, useShowNotification, useConfig, useGuildPanel, motivoPermisoAusente } from '../store/useStore';
import { t } from '../i18n';
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
      <Button variante="ghost" tamano="sm" aria-haspopup="menu" aria-expanded={abierto} aria-label={t('Actions for {name}', { name: nombre })} onClick={() => setAbierto((v) => !v)}>
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
    apodo: { titulo: t('Change nickname of {name}', { name: nombre }), boton: t('Save nickname'), etiqueta: t('New nickname'), ayuda: t('Leave it empty to remove the nickname.') },
    expulsar: { titulo: t('Kick {name}', { name: nombre }), boton: t('Confirm kick'), etiqueta: t('Reason (optional)'), ayuda: t('They will be able to rejoin with an invite. The reason is saved in the audit log.') },
    banear: { titulo: t('Ban {name}', { name: nombre }), boton: t('Confirm ban'), etiqueta: t('Reason (optional)'), ayuda: t('They will not be able to rejoin until you unban them. The reason is saved in the audit log.') },
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
      descripcion={tipo === 'apodo' ? undefined : t('This is a serious action: review the details and confirm.')}
      onCerrar={onCerrar}
      pie={
        <>
          <Button variante="secundario" onClick={onCerrar}>{t('Cancel')}</Button>
          <Button variante={tipo === 'apodo' ? 'primario' : 'peligro'} cargando={trabajando} onClick={() => { void enviar(); }}>{textos.boton}</Button>
        </>
      }
    >
      <form className="mod__form" onSubmit={(e) => { e.preventDefault(); void enviar(); }}>
        <Input data-autofocus etiqueta={textos.etiqueta} ayuda={textos.ayuda} value={texto} maxLength={tipo === 'apodo' ? 32 : 400} onChange={(e) => setTexto(e.target.value)} autoComplete="off" />
        {tipo === 'banear' && (
          <Select etiqueta={t('Messages to delete')} ayuda={t('Removes this person’s recent messages across the whole server.')} value={dias} onChange={(e) => setDias(Number(e.target.value))}>
            <option value={0}>{t('Delete none')}</option>
            <option value={1}>{t('Last day')}</option>
            {[2, 3, 4, 5, 6].map((d) => <option key={d} value={d}>{t('Last {count} days', { count: d })}</option>)}
            <option value={7}>{t('Last {count} days', { count: 7 })}</option>
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
    catch (e) { setError(typeof e === 'string' ? e : t('Could not read the ban list')); }
  };
  useEffect(() => { void cargar(); }, [guildId]); // eslint-disable-line react-hooks/exhaustive-deps

  const desbanear = async (b: Baneo) => {
    if (!guildId) return;
    setTrabajando(b.user_id);
    try {
      await unbanMember(guildId, b.user_id, null);
      showNotification(t('{name} unbanned', { name: b.username }), 'success');
      setBaneos((l) => (l ?? []).filter((x) => x.user_id !== b.user_id));
    } catch (e) { showNotification(typeof e === 'string' ? e : t('Could not unban'), 'error'); }
    finally { setTrabajando(null); }
  };

  if (error) return <ErrorPanel mensaje={error} onReintentar={() => { void cargar(); }} />;
  if (!baneos) return <Cargando lineas={3} />;
  if (baneos.length === 0) return <EstadoVacio titulo={t('No bans')} descripcion={t('Nobody is banned on this server.')} />;
  return (
    <ul className="vista__lista vista__tarjeta" aria-label={t('Banned')}>
      {baneos.map((b) => (
        <li key={b.user_id} className="vista__fila">
          <Avatar nombre={b.username} tamano={32} />
          <div className="vista__fila-cuerpo">
            <span className="vista__fila-nombre vista__recorte">{b.username}</span>
            <span className="vista__fila-sub vista__recorte">{b.reason ? t('Reason: {reason}', { reason: b.reason }) : t('No reason given')}</span>
          </div>
          <span title={puede ?? undefined}>
            <Button variante="secundario" tamano="sm" disabled={!!puede} cargando={trabajando === b.user_id} onClick={() => { void desbanear(b); }}>{t('Unban')}</Button>
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
    const content = prompt(t('DM to {name}:', { name: m.username }));
    if (!content) return;
    try {
      await sendDM(m.user_id, content);
      addLog(`[DM] ${m.username}`);
      showNotification(t('DM sent'), 'success');
    } catch (e) { showNotification(errorTexto(e, t('Could not send the DM')), 'error'); }
  };

  const handleRole = async (m: Member, role: Role, add: boolean) => {
    try {
      if (!guildId) throw new Error(t('No server selected'));
      if (add) await addRole(guildId, m.user_id, role.id);
      else await removeRole(guildId, m.user_id, role.id);
      showNotification(add ? t('Role added') : t('Role removed'), 'success');
    } catch (e) { showNotification(errorTexto(e, t('Error changing the role')), 'error'); }
  };

  const handleTimeout = async (m: Member) => {
    const mins = prompt(t('Timeout minutes for {name}:', { name: m.username }), '10');
    if (!mins) return;
    try {
      if (!guildId) { showNotification(t('No server selected'), 'error'); return; }
      await timeoutMember(guildId, m.user_id, parseInt(mins, 10));
      showNotification(t('{minutes}-minute timeout applied', { minutes: mins }), 'success');
    } catch (e) { showNotification(errorTexto(e, t('Could not apply the timeout')), 'error'); }
  };

  const confirmarModeracion = async (a: Moderacion, texto: string, dias: number) => {
    const nombre = a.miembro.nickname || a.miembro.username;
    try {
      if (!guildId) throw new Error(t('No server selected'));
      const motivo = texto || null;
      if (a.tipo === 'apodo') await setNickname(guildId, a.miembro.user_id, motivo, null);
      else if (a.tipo === 'expulsar') await kickMember(guildId, a.miembro.user_id, motivo);
      else await banMember(guildId, a.miembro.user_id, motivo, dias);
      addLog(`[${a.tipo === 'apodo' ? 'NICKNAME' : a.tipo === 'expulsar' ? 'KICK' : 'BAN'}] ${nombre}`);
      showNotification(a.tipo === 'apodo' ? t('Nickname updated') : a.tipo === 'expulsar' ? t('{name} kicked', { name: nombre }) : t('{name} banned', { name: nombre }), 'success');
      setModeracion(null);
    } catch (e) { showNotification(errorTexto(e, t('Could not complete the action')), 'error'); }
  };

  /** Por qué no se puede moderar a este miembro (dueño, bot o rol igual/superior), o null. */
  const motivoJerarquia = (m: Member): string | null => {
    if (!ctx) return t('Checking the role hierarchy…');
    if (ctx.owner_id === m.user_id) return t('The server owner cannot be moderated');
    if (ctx.bot_id === m.user_id) return t('The bot cannot moderate itself');
    const top = Math.max(0, ...roles.filter((r) => m.roles.includes(r.name)).map((r) => r.position));
    return top >= ctx.bot_top_position ? t('Their highest role is equal to or above the bot’s: raise the bot’s role') : null;
  };

  const accionesDe = (m: Member): AccionMenu[] => {
    const jerarquia = motivoJerarquia(m);
    const acciones: AccionMenu[] = [
      { clave: 'dm', etiqueta: t('Send DM'), motivo: m.bot ? t('You cannot send a DM to a bot') : null, ejecutar: () => { void handleDM(m); } },
      { clave: 'timeout', etiqueta: t('Apply timeout'), motivo: m.bot ? t('You cannot apply a timeout to a bot') : motivoTimeout, ejecutar: () => { void handleTimeout(m); } },
    ];
    acciones.push(
      { clave: 'apodo', etiqueta: t('Change nickname'), motivo: motivoApodos ?? jerarquia, ejecutar: () => setModeracion({ tipo: 'apodo', miembro: m }) },
      { clave: 'expulsar', etiqueta: t('Kick'), motivo: motivoExpulsar ?? jerarquia, ejecutar: () => setModeracion({ tipo: 'expulsar', miembro: m }) },
      { clave: 'banear', etiqueta: t('Ban'), motivo: motivoBanear ?? jerarquia, ejecutar: () => setModeracion({ tipo: 'banear', miembro: m }) },
    );
    for (const r of roles.filter((x) => x.name !== '@everyone').slice(0, 10)) {
      const tiene = m.roles.includes(r.name);
      acciones.push({
        clave: `rol-${r.id}`,
        etiqueta: tiene ? t('Remove role {name}', { name: r.name }) : t('Add role {name}', { name: r.name }),
        motivo: motivoRoles,
        ejecutar: () => { void handleRole(m, r, !tiene); },
      });
    }
    return acciones;
  };

  const total = serverMap?.members.length ?? 0;

  return (
    <div className="vista members-view">
      <CabeceraVista titulo={t('Members')} cuenta={total}>
        {pestana === 'miembros' && <BuscadorVista valor={search} onCambio={setSearch} placeholder={t('Search members…')} icono={<Search size={16} />} />}
      </CabeceraVista>

      <div className="pestanas" role="tablist" aria-label={t('Member sections')}>
        {([['miembros', t('Members')], ['baneados', t('Banned')]] as [string, string][]).map(([id, etiqueta]) => (
          <button key={id} role="tab" aria-selected={pestana === id} className="pestanas__tab" onClick={() => setPestana(id as 'miembros' | 'baneados')}>{etiqueta}</button>
        ))}
      </div>

      <EstadoVista>
        {pestana === 'baneados' ? (
          <PestanaBaneados guildId={guildId} puede={motivoBanear} />
        ) : total === 0 ? (
          <EstadoVacio titulo={t('No members loaded')} descripcion={t('Enable the bot’s members intent in the Discord developer portal and press Resync in the Dashboard.')} />
        ) : miembros.length === 0 ? (
          <EstadoVacio titulo={t('No results')} descripcion={t('No member matches “{search}”. Try another name or clear the search.', { search })} />
        ) : (
          <ul className="vista__lista vista__tarjeta" aria-label={t('Members')}>
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
