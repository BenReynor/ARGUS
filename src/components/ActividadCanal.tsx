import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { useActiveGuildId, useGuildPanel, useShowNotification, useAddLog, motivoPermisoAusente } from '../store/useStore';
import {
  channelMessages, deleteMessage, bulkDeleteMessages, pinMessage, unpinMessage,
  createInvite, listInvites, deleteInvite, listWebhooks, createWebhook, deleteWebhook,
} from '../api/tauri';
import type { MensajeCanal, Invitacion, Webhook } from '../api/tauri';
import { Button, Badge, Cargando, ErrorPanel, EstadoVacio, Dialogo, Input, Select } from './ui';
import { Pin, PinOff, Trash2, Copy, Plus, RefreshCw, Paperclip } from 'lucide-react';

type Pestana = 'mensajes' | 'invitaciones' | 'webhooks';
const PESTANAS: { id: Pestana; texto: string }[] = [
  { id: 'mensajes', texto: 'Mensajes' },
  { id: 'invitaciones', texto: 'Invitaciones' },
  { id: 'webhooks', texto: 'Webhooks' },
];
const CATORCE_DIAS_MS = 14 * 24 * 60 * 60 * 1000;
const CADUCIDADES: [string, string][] = [
  ['1800', '30 minutos'], ['3600', '1 hora'], ['21600', '6 horas'], ['43200', '12 horas'],
  ['86400', '1 día'], ['604800', '7 días'], ['0', 'Nunca'],
];

const mensajeError = (e: unknown, defecto: string) => (typeof e === 'string' ? e : defecto);
const fecha = (iso: string) => {
  const d = new Date(iso);
  return isNaN(d.getTime()) ? iso : d.toLocaleString();
};

/** Carga una lista asíncrona con estado de carga/error y recarga manual. */
function useLista<T>(cargar: () => Promise<T[]>, clave: string) {
  const [datos, setDatos] = useState<T[] | null>(null);
  const [error, setError] = useState('');
  const [cargando, setCargando] = useState(true);
  const recargar = useCallback(async () => {
    setCargando(true);
    setError('');
    try {
      setDatos(await cargar());
    } catch (e) {
      setError(mensajeError(e, 'No se pudo cargar la lista'));
    } finally {
      setCargando(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [clave]);
  useEffect(() => { setDatos(null); void recargar(); }, [recargar]);
  return { datos, error, cargando, recargar };
}

const Estado: React.FC<{ cargando: boolean; error: string; vacio: boolean; textoVacio: string; onReintentar: () => void; children: React.ReactNode }> = (
  { cargando, error, vacio, textoVacio, onReintentar, children },
) => {
  if (error) return <ErrorPanel mensaje={error} onReintentar={onReintentar} />;
  if (cargando && vacio) return <Cargando lineas={3} />;
  if (vacio) return <EstadoVacio titulo={textoVacio} />;
  return <>{children}</>;
};

const Mensajes: React.FC<{ guildId: string; canalId: string }> = ({ guildId, canalId }) => {
  const permisos = useGuildPanel().permissions;
  const motivo = motivoPermisoAusente(permisos, 'Gestionar mensajes');
  const notificar = useShowNotification();
  const addLog = useAddLog();
  const { datos, error, cargando, recargar } = useLista<MensajeCanal>(() => channelMessages(guildId, canalId), guildId + canalId);
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [unico, setUnico] = useState<MensajeCanal | null>(null);
  const [masa, setMasa] = useState(false);
  const [trabajando, setTrabajando] = useState(false);

  useEffect(() => setSel(new Set()), [guildId, canalId]);

  const accion = async (hacer: () => Promise<string>, fallo: string) => {
    setTrabajando(true);
    try {
      const m = await hacer();
      notificar(m, 'success');
      addLog('[MENSAJES] ' + m);
      return true;
    } catch (e) {
      notificar(mensajeError(e, fallo), 'error');
      return false;
    } finally {
      setTrabajando(false);
    }
  };

  const alternar = (id: string) => setSel((p) => { const n = new Set(p); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const antiguos = useMemo(() => {
    const ahora = Date.now();
    return (datos ?? []).filter((m) => sel.has(m.id) && ahora - new Date(m.timestamp).getTime() >= CATORCE_DIAS_MS).length;
  }, [datos, sel]);

  const fijar = async (m: MensajeCanal) => {
    const f = m.pinned ? unpinMessage : pinMessage;
    if (await accion(() => f(guildId, canalId, m.id), 'No se pudo cambiar el mensaje fijado')) await recargar();
  };
  const borrar = async () => {
    if (!unico) return;
    const m = unico;
    if (await accion(() => deleteMessage(guildId, canalId, m.id), 'No se pudo borrar el mensaje')) {
      setUnico(null);
      await recargar();
    }
  };
  const borrarMasa = async () => {
    if (await accion(() => bulkDeleteMessages(guildId, canalId, [...sel]), 'No se pudieron borrar los mensajes')) {
      setMasa(false);
      setSel(new Set());
      await recargar();
    }
  };

  return (
    <div className="actividad__seccion">
      <div className="vista__etiquetas">
        <Button variante="secundario" tamano="sm" onClick={() => void recargar()} cargando={cargando}><RefreshCw size={14} /> Actualizar</Button>
        <Button variante="peligro" tamano="sm" disabled={!!motivo || sel.size < 2} title={motivo ?? (sel.size < 2 ? 'Selecciona al menos 2 mensajes' : undefined)} onClick={() => setMasa(true)}>
          <Trash2 size={14} /> Borrar selección ({sel.size})
        </Button>
        {motivo && <span className="vista__nota">{motivo}</span>}
      </div>
      <Estado cargando={cargando} error={error} vacio={!datos || datos.length === 0} textoVacio="Este canal no tiene mensajes" onReintentar={() => void recargar()}>
        <ul className="mensajes" aria-label="Últimos mensajes">
          {(datos ?? []).map((m) => (
            <li key={m.id} className={`mensaje${sel.has(m.id) ? ' mensaje--sel' : ''}`}>
              <input type="checkbox" aria-label={`Seleccionar mensaje de ${m.author_name}`} checked={sel.has(m.id)} disabled={!!motivo} onChange={() => alternar(m.id)} />
              <div className="mensaje__cuerpo">
                <div className="mensaje__meta">
                  <strong>{m.author_name}</strong>
                  {m.author_bot && <Badge tono="acento">BOT</Badge>}
                  {m.pinned && <Badge tono="aviso">Fijado</Badge>}
                  <time className="num" dateTime={m.timestamp}>{fecha(m.timestamp)}</time>
                </div>
                {m.content && <p className="mensaje__texto">{m.content}</p>}
                {m.attachments.map((a) => (
                  <a key={a.url} className="mensaje__adjunto" href={a.url} target="_blank" rel="noreferrer"><Paperclip size={12} /> {a.filename}</a>
                ))}
              </div>
              <div className="mensaje__acciones">
                <Button variante="ghost" tamano="sm" disabled={!!motivo || trabajando} title={motivo ?? (m.pinned ? 'Desfijar' : 'Fijar')} aria-label={(m.pinned ? 'Desfijar' : 'Fijar') + ` mensaje de ${m.author_name}`} onClick={() => void fijar(m)}>
                  {m.pinned ? <PinOff size={14} /> : <Pin size={14} />}
                </Button>
                <Button variante="ghost" tamano="sm" disabled={!!motivo || trabajando} title={motivo ?? 'Borrar'} aria-label={`Borrar mensaje de ${m.author_name}`} onClick={() => setUnico(m)}>
                  <Trash2 size={14} />
                </Button>
              </div>
            </li>
          ))}
        </ul>
      </Estado>

      <Dialogo
        abierto={!!unico}
        titulo="Borrar mensaje"
        descripcion="El mensaje se borrará de Discord. No se puede deshacer."
        onCerrar={() => setUnico(null)}
        pie={
          <>
            <Button variante="secundario" onClick={() => setUnico(null)}>Cancelar</Button>
            <Button variante="peligro" cargando={trabajando} onClick={() => void borrar()}>Borrar</Button>
          </>
        }
      >
        <p className="mensaje__texto">{unico?.author_name}: {unico?.content || '(sin texto)'}</p>
      </Dialogo>
      <Dialogo
        abierto={masa}
        titulo={`Borrar ${sel.size} mensajes`}
        descripcion="Discord solo permite el borrado en bloque de mensajes de menos de 14 días y entre 2 y 100 a la vez."
        onCerrar={() => setMasa(false)}
        pie={
          <>
            <Button variante="secundario" onClick={() => setMasa(false)}>Cancelar</Button>
            <Button variante="peligro" cargando={trabajando} onClick={() => void borrarMasa()}>Borrar {sel.size}</Button>
          </>
        }
      >
        {antiguos > 0 ? (
          <span className="ui-campo__error" role="alert">{antiguos} de los seleccionados tienen más de 14 días: Discord rechazará el borrado. Deselecciónalos o bórralos de uno en uno.</span>
        ) : (
          <p className="vista__nota">Esta acción no se puede deshacer.</p>
        )}
      </Dialogo>
    </div>
  );
};

const Invitaciones: React.FC<{ guildId: string; canalId: string }> = ({ guildId, canalId }) => {
  const motivoCrear = motivoPermisoAusente(useGuildPanel().permissions, 'Crear invitaciones');
  const motivoGestion = motivoPermisoAusente(useGuildPanel().permissions, 'Gestionar servidor');
  const notificar = useShowNotification();
  const addLog = useAddLog();
  const { datos, error, cargando, recargar } = useLista<Invitacion>(() => listInvites(guildId), guildId);
  const [edad, setEdad] = useState('86400');
  const [usos, setUsos] = useState('0');
  const [creando, setCreando] = useState(false);

  const del = (datos ?? []).filter((i) => i.channel_id === canalId);

  const crear = async (e: React.FormEvent) => {
    e.preventDefault();
    setCreando(true);
    try {
      const i = await createInvite(guildId, canalId, Number(edad), Math.max(0, Math.floor(Number(usos) || 0)));
      void navigator.clipboard.writeText(i.url);
      notificar('Invitación creada y enlace copiado', 'success');
      addLog('[INVITACIONES] Creada ' + i.code);
      await recargar();
    } catch (err) {
      notificar(mensajeError(err, 'No se pudo crear la invitación'), 'error');
    } finally {
      setCreando(false);
    }
  };
  const borrar = async (i: Invitacion) => {
    try {
      const m = await deleteInvite(guildId, i.code);
      notificar(m, 'success');
      addLog('[INVITACIONES] Borrada ' + i.code);
      await recargar();
    } catch (err) {
      notificar(mensajeError(err, 'No se pudo borrar la invitación'), 'error');
    }
  };
  const copiar = (i: Invitacion) => { void navigator.clipboard.writeText(i.url); notificar('Enlace copiado', 'info'); };

  return (
    <div className="actividad__seccion">
      <form className="actividad__form" onSubmit={crear}>
        <Select etiqueta="Caduca" value={edad} onChange={(e) => setEdad(e.target.value)}>
          {CADUCIDADES.map(([v, t]) => <option key={v} value={v}>{t}</option>)}
        </Select>
        <Input etiqueta="Usos máximos" type="number" min={0} max={100} value={usos} onChange={(e) => setUsos(e.target.value)} ayuda="0: sin límite" />
        <Button type="submit" cargando={creando} disabled={!!motivoCrear} title={motivoCrear ?? undefined}><Plus size={14} /> Crear</Button>
      </form>
      {motivoCrear && <span className="vista__nota">{motivoCrear}</span>}
      <Estado cargando={cargando} error={error} vacio={del.length === 0} textoVacio="Este canal no tiene invitaciones" onReintentar={() => void recargar()}>
        <ul className="actividad__lista">
          {del.map((i) => (
            <li key={i.code} className="actividad__fila">
              <div className="actividad__datos">
                <code>{i.url}</code>
                <span className="vista__nota num">
                  {i.uses}/{i.max_uses || '∞'} usos · {i.expires_at ? 'caduca ' + fecha(i.expires_at) : 'no caduca'}{i.inviter ? ' · ' + i.inviter : ''}
                </span>
              </div>
              <Button variante="ghost" tamano="sm" title="Copiar enlace" aria-label={`Copiar enlace de ${i.code}`} onClick={() => copiar(i)}><Copy size={14} /></Button>
              <Button variante="ghost" tamano="sm" disabled={!!motivoGestion} title={motivoGestion ?? 'Borrar invitación'} aria-label={`Borrar invitación ${i.code}`} onClick={() => void borrar(i)}><Trash2 size={14} /></Button>
            </li>
          ))}
        </ul>
      </Estado>
    </div>
  );
};

const Webhooks: React.FC<{ guildId: string; canalId: string }> = ({ guildId, canalId }) => {
  const motivo = motivoPermisoAusente(useGuildPanel().permissions, 'Gestionar webhooks');
  const notificar = useShowNotification();
  const addLog = useAddLog();
  const { datos, error, cargando, recargar } = useLista<Webhook>(() => listWebhooks(guildId), guildId);
  const [nombre, setNombre] = useState('');
  const [creando, setCreando] = useState(false);
  const [aviso, setAviso] = useState('');

  const del = (datos ?? []).filter((w) => w.channel_id === canalId);

  const crear = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!nombre.trim()) { setAviso('El nombre del webhook no puede estar vacío'); return; }
    setAviso('');
    setCreando(true);
    try {
      const w = await createWebhook(guildId, canalId, nombre.trim());
      if (w.url) void navigator.clipboard.writeText(w.url);
      notificar('Webhook «' + w.name + '» creado' + (w.url ? ' y URL copiada' : ''), 'success');
      addLog('[WEBHOOKS] Creado ' + w.name);
      setNombre('');
      await recargar();
    } catch (err) {
      notificar(mensajeError(err, 'No se pudo crear el webhook'), 'error');
    } finally {
      setCreando(false);
    }
  };
  const borrar = async (w: Webhook) => {
    try {
      const m = await deleteWebhook(guildId, w.id);
      notificar(m, 'success');
      addLog('[WEBHOOKS] Borrado ' + w.name);
      await recargar();
    } catch (err) {
      notificar(mensajeError(err, 'No se pudo borrar el webhook'), 'error');
    }
  };

  return (
    <div className="actividad__seccion">
      <form className="actividad__form" onSubmit={crear}>
        <Input etiqueta="Nombre" value={nombre} maxLength={80} onChange={(e) => setNombre(e.target.value)} error={aviso || undefined} />
        <Button type="submit" cargando={creando} disabled={!!motivo} title={motivo ?? undefined}><Plus size={14} /> Crear</Button>
      </form>
      {motivo && <span className="vista__nota">{motivo}</span>}
      <Estado cargando={cargando} error={error} vacio={del.length === 0} textoVacio="Este canal no tiene webhooks" onReintentar={() => void recargar()}>
        <ul className="actividad__lista">
          {del.map((w) => (
            <li key={w.id} className="actividad__fila">
              <div className="actividad__datos">
                <strong>{w.name}</strong>
                <span className="vista__nota">{w.url ? 'URL disponible' : 'Discord no entrega la URL de webhooks creados por otros'}</span>
              </div>
              <Button variante="ghost" tamano="sm" disabled={!w.url} title={w.url ? 'Copiar URL' : 'URL no disponible'} aria-label={`Copiar URL de ${w.name}`} onClick={() => { void navigator.clipboard.writeText(w.url ?? ''); notificar('URL copiada', 'info'); }}><Copy size={14} /></Button>
              <Button variante="ghost" tamano="sm" disabled={!!motivo} title={motivo ?? 'Borrar webhook'} aria-label={`Borrar webhook ${w.name}`} onClick={() => void borrar(w)}><Trash2 size={14} /></Button>
            </li>
          ))}
        </ul>
      </Estado>
    </div>
  );
};

/** Mensajes, invitaciones y webhooks de un canal de texto, en pestañas. */
export const ActividadCanal: React.FC<{ canalId: string }> = ({ canalId }) => {
  const guildId = useActiveGuildId();
  const [pestana, setPestana] = useState<Pestana>('mensajes');
  if (!guildId) return null;
  return (
    <div className="actividad">
      <div role="tablist" aria-label="Actividad del canal" className="actividad__pestanas">
        {PESTANAS.map((p) => (
          <button key={p.id} role="tab" id={`tab-${p.id}`} aria-selected={pestana === p.id} aria-controls="panel-actividad"
            className={`actividad__pestana${pestana === p.id ? ' actividad__pestana--activa' : ''}`} onClick={() => setPestana(p.id)}>
            {p.texto}
          </button>
        ))}
      </div>
      <div role="tabpanel" id="panel-actividad" aria-labelledby={`tab-${pestana}`}>
        {pestana === 'mensajes' && <Mensajes guildId={guildId} canalId={canalId} />}
        {pestana === 'invitaciones' && <Invitaciones guildId={guildId} canalId={canalId} />}
        {pestana === 'webhooks' && <Webhooks guildId={guildId} canalId={canalId} />}
      </div>
    </div>
  );
};
