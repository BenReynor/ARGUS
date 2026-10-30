import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { useActiveGuildId, useGuildPanel, useShowNotification, useAddLog, motivoPermisoAusente } from '../store/useStore';
import {
  channelMessages, deleteMessage, bulkDeleteMessages, pinMessage, unpinMessage,
  createInvite, listInvites, deleteInvite, listWebhooks, createWebhook, deleteWebhook,
} from '../api/tauri';
import type { MensajeCanal, Invitacion, Webhook } from '../api/tauri';
import { Button, Badge, Cargando, ErrorPanel, EstadoVacio, Dialogo, Input, Select } from './ui';
import { Pin, PinOff, Trash2, Copy, Plus, RefreshCw, Paperclip } from 'lucide-react';
import { t } from '../i18n';

type Pestana = 'mensajes' | 'invitaciones' | 'webhooks';
const PESTANAS: Pestana[] = ['mensajes', 'invitaciones', 'webhooks'];
const textoPestana = (p: Pestana): string =>
  p === 'mensajes' ? t('Messages') : p === 'invitaciones' ? t('Invites') : t('Webhooks');
const CATORCE_DIAS_MS = 14 * 24 * 60 * 60 * 1000;
const caducidades = (): [string, string][] => [
  ['1800', t('30 minutes')], ['3600', t('1 hour')], ['21600', t('6 hours')], ['43200', t('12 hours')],
  ['86400', t('1 day')], ['604800', t('7 days')], ['0', t('Never')],
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
      setError(mensajeError(e, t('Could not load the list')));
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
      addLog(t('[MESSAGES] {message}', { message: m }));
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
    if (await accion(() => f(guildId, canalId, m.id), t('Could not change the pinned message'))) await recargar();
  };
  const borrar = async () => {
    if (!unico) return;
    const m = unico;
    if (await accion(() => deleteMessage(guildId, canalId, m.id), t('Could not delete the message'))) {
      setUnico(null);
      await recargar();
    }
  };
  const borrarMasa = async () => {
    if (await accion(() => bulkDeleteMessages(guildId, canalId, [...sel]), t('Could not delete the messages'))) {
      setMasa(false);
      setSel(new Set());
      await recargar();
    }
  };

  return (
    <div className="actividad__seccion">
      <div className="vista__etiquetas">
        <Button variante="secundario" tamano="sm" onClick={() => void recargar()} cargando={cargando}><RefreshCw size={14} /> {t('Refresh')}</Button>
        <Button variante="peligro" tamano="sm" disabled={!!motivo || sel.size < 2} title={motivo ?? (sel.size < 2 ? t('Select at least 2 messages') : undefined)} onClick={() => setMasa(true)}>
          <Trash2 size={14} /> {t('Delete selection ({count})', { count: sel.size })}
        </Button>
        {motivo && <span className="vista__nota">{motivo}</span>}
      </div>
      <Estado cargando={cargando} error={error} vacio={!datos || datos.length === 0} textoVacio={t('This channel has no messages')} onReintentar={() => void recargar()}>
        <ul className="mensajes" aria-label={t('Latest messages')}>
          {(datos ?? []).map((m) => (
            <li key={m.id} className={`mensaje${sel.has(m.id) ? ' mensaje--sel' : ''}`}>
              <input type="checkbox" aria-label={t('Select the message from {name}', { name: m.author_name })} checked={sel.has(m.id)} disabled={!!motivo} onChange={() => alternar(m.id)} />
              <div className="mensaje__cuerpo">
                <div className="mensaje__meta">
                  <strong>{m.author_name}</strong>
                  {m.author_bot && <Badge tono="acento">BOT</Badge>}
                  {m.pinned && <Badge tono="aviso">{t('Pinned')}</Badge>}
                  <time className="num" dateTime={m.timestamp}>{fecha(m.timestamp)}</time>
                </div>
                {m.content && <p className="mensaje__texto">{m.content}</p>}
                {m.attachments.map((a) => (
                  <a key={a.url} className="mensaje__adjunto" href={a.url} target="_blank" rel="noreferrer"><Paperclip size={12} /> {a.filename}</a>
                ))}
              </div>
              <div className="mensaje__acciones">
                <Button variante="ghost" tamano="sm" disabled={!!motivo || trabajando} title={motivo ?? (m.pinned ? t('Unpin') : t('Pin'))} aria-label={m.pinned ? t('Unpin the message from {name}', { name: m.author_name }) : t('Pin the message from {name}', { name: m.author_name })} onClick={() => void fijar(m)}>
                  {m.pinned ? <PinOff size={14} /> : <Pin size={14} />}
                </Button>
                <Button variante="ghost" tamano="sm" disabled={!!motivo || trabajando} title={motivo ?? t('Delete')} aria-label={t('Delete the message from {name}', { name: m.author_name })} onClick={() => setUnico(m)}>
                  <Trash2 size={14} />
                </Button>
              </div>
            </li>
          ))}
        </ul>
      </Estado>

      <Dialogo
        abierto={!!unico}
        titulo={t('Delete message')}
        descripcion={t('The message will be deleted from Discord. This cannot be undone.')}
        onCerrar={() => setUnico(null)}
        pie={
          <>
            <Button variante="secundario" onClick={() => setUnico(null)}>{t('Cancel')}</Button>
            <Button variante="peligro" cargando={trabajando} onClick={() => void borrar()}>{t('Delete')}</Button>
          </>
        }
      >
        <p className="mensaje__texto">{unico?.author_name}: {unico?.content || t('(no text)')}</p>
      </Dialogo>
      <Dialogo
        abierto={masa}
        titulo={t('Delete {count} messages', { count: sel.size })}
        descripcion={t('Discord only allows bulk deletion of messages less than 14 days old, between 2 and 100 at a time.')}
        onCerrar={() => setMasa(false)}
        pie={
          <>
            <Button variante="secundario" onClick={() => setMasa(false)}>{t('Cancel')}</Button>
            <Button variante="peligro" cargando={trabajando} onClick={() => void borrarMasa()}>{t('Delete {count}', { count: sel.size })}</Button>
          </>
        }
      >
        {antiguos > 0 ? (
          <span className="ui-campo__error" role="alert">{t('{count} of the selected messages are older than 14 days: Discord will reject the deletion. Deselect them or delete them one by one.', { count: antiguos })}</span>
        ) : (
          <p className="vista__nota">{t('This action cannot be undone.')}</p>
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
      notificar(t('Invite created and link copied'), 'success');
      addLog(t('[INVITES] Created {code}', { code: i.code }));
      await recargar();
    } catch (err) {
      notificar(mensajeError(err, t('Could not create the invite')), 'error');
    } finally {
      setCreando(false);
    }
  };
  const borrar = async (i: Invitacion) => {
    try {
      const m = await deleteInvite(guildId, i.code);
      notificar(m, 'success');
      addLog(t('[INVITES] Deleted {code}', { code: i.code }));
      await recargar();
    } catch (err) {
      notificar(mensajeError(err, t('Could not delete the invite')), 'error');
    }
  };
  const copiar = (i: Invitacion) => { void navigator.clipboard.writeText(i.url); notificar(t('Link copied'), 'info'); };

  return (
    <div className="actividad__seccion">
      <form className="actividad__form" onSubmit={crear}>
        <Select etiqueta={t('Expires')} value={edad} onChange={(e) => setEdad(e.target.value)}>
          {caducidades().map(([v, texto]) => <option key={v} value={v}>{texto}</option>)}
        </Select>
        <Input etiqueta={t('Max uses')} type="number" min={0} max={100} value={usos} onChange={(e) => setUsos(e.target.value)} ayuda={t('0: unlimited')} />
        <Button type="submit" cargando={creando} disabled={!!motivoCrear} title={motivoCrear ?? undefined}><Plus size={14} /> {t('Create')}</Button>
      </form>
      {motivoCrear && <span className="vista__nota">{motivoCrear}</span>}
      <Estado cargando={cargando} error={error} vacio={del.length === 0} textoVacio={t('This channel has no invites')} onReintentar={() => void recargar()}>
        <ul className="actividad__lista">
          {del.map((i) => (
            <li key={i.code} className="actividad__fila">
              <div className="actividad__datos">
                <code>{i.url}</code>
                <span className="vista__nota num">
                  {t('{uses}/{max} uses', { uses: i.uses, max: i.max_uses || '∞' })} · {i.expires_at ? t('expires {date}', { date: fecha(i.expires_at) }) : t('never expires')}{i.inviter ? ' · ' + i.inviter : ''}
                </span>
              </div>
              <Button variante="ghost" tamano="sm" title={t('Copy link')} aria-label={t('Copy the link of {code}', { code: i.code })} onClick={() => copiar(i)}><Copy size={14} /></Button>
              <Button variante="ghost" tamano="sm" disabled={!!motivoGestion} title={motivoGestion ?? t('Delete invite')} aria-label={t('Delete invite {code}', { code: i.code })} onClick={() => void borrar(i)}><Trash2 size={14} /></Button>
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
    if (!nombre.trim()) { setAviso(t('The webhook name cannot be empty')); return; }
    setAviso('');
    setCreando(true);
    try {
      const w = await createWebhook(guildId, canalId, nombre.trim());
      if (w.url) void navigator.clipboard.writeText(w.url);
      notificar(w.url ? t('Webhook “{name}” created and URL copied', { name: w.name }) : t('Webhook “{name}” created', { name: w.name }), 'success');
      addLog(t('[WEBHOOKS] Created {name}', { name: w.name }));
      setNombre('');
      await recargar();
    } catch (err) {
      notificar(mensajeError(err, t('Could not create the webhook')), 'error');
    } finally {
      setCreando(false);
    }
  };
  const borrar = async (w: Webhook) => {
    try {
      const m = await deleteWebhook(guildId, w.id);
      notificar(m, 'success');
      addLog(t('[WEBHOOKS] Deleted {name}', { name: w.name }));
      await recargar();
    } catch (err) {
      notificar(mensajeError(err, t('Could not delete the webhook')), 'error');
    }
  };

  return (
    <div className="actividad__seccion">
      <form className="actividad__form" onSubmit={crear}>
        <Input etiqueta={t('Name')} value={nombre} maxLength={80} onChange={(e) => setNombre(e.target.value)} error={aviso || undefined} />
        <Button type="submit" cargando={creando} disabled={!!motivo} title={motivo ?? undefined}><Plus size={14} /> {t('Create')}</Button>
      </form>
      {motivo && <span className="vista__nota">{motivo}</span>}
      <Estado cargando={cargando} error={error} vacio={del.length === 0} textoVacio={t('This channel has no webhooks')} onReintentar={() => void recargar()}>
        <ul className="actividad__lista">
          {del.map((w) => (
            <li key={w.id} className="actividad__fila">
              <div className="actividad__datos">
                <strong>{w.name}</strong>
                <span className="vista__nota">{w.url ? t('URL available') : t('Discord does not provide the URL of webhooks created by others')}</span>
              </div>
              <Button variante="ghost" tamano="sm" disabled={!w.url} title={w.url ? t('Copy URL') : t('URL not available')} aria-label={t('Copy the URL of {name}', { name: w.name })} onClick={() => { void navigator.clipboard.writeText(w.url ?? ''); notificar(t('URL copied'), 'info'); }}><Copy size={14} /></Button>
              <Button variante="ghost" tamano="sm" disabled={!!motivo} title={motivo ?? t('Delete webhook')} aria-label={t('Delete webhook {name}', { name: w.name })} onClick={() => void borrar(w)}><Trash2 size={14} /></Button>
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
      <div role="tablist" aria-label={t('Channel activity')} className="actividad__pestanas">
        {PESTANAS.map((p) => (
          <button key={p} role="tab" id={`tab-${p}`} aria-selected={pestana === p} aria-controls="panel-actividad"
            className={`actividad__pestana${pestana === p ? ' actividad__pestana--activa' : ''}`} onClick={() => setPestana(p)}>
            {textoPestana(p)}
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
