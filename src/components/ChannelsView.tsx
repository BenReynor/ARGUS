import React, { useEffect, useMemo, useState } from 'react';
import { useServerMap, useSetSelectedChannel, useSelectedChannel, useAddLog, useShowNotification, useGuildPanel, motivoPermisoAusente, useActiveGuildId, useStore } from '../store/useStore';
import { sendMessage, createChannel, updateChannel, deleteChannel, setChannelPermission, deleteChannelPermission } from '../api/tauri';
import { PermisosCanal } from './PermisosCanal';
import { ActividadCanal } from './ActividadCanal';
import type { CambiosPermisos } from './PermisosCanal';
import type { DatosCanal } from '../api/tauri';
import { Button, Badge, EstadoVacio, Dialogo, Input, Select, ConfirmarPeligroso } from './ui';
import { EstadoVista, BuscadorVista, CabeceraVista } from './EstadoVista';
import { Hash, Volume2, Folder, Megaphone, Radio, MessagesSquare, Send, Copy, Search, ChevronRight, Plus, Pencil, Trash2 } from 'lucide-react';
import { t } from '../i18n';
import type { Channel } from '../types';

const tipoCanal = (tipo: number): string => {
  switch (tipo) {
    case 0: return t('Text');
    case 2: return t('Voice');
    case 4: return t('Category');
    case 5: return t('Announcement');
    case 10: return t('Announcement thread');
    case 11: return t('Public thread');
    case 12: return t('Private thread');
    case 13: return t('Stage');
    case 15: return t('Forum');
    case 16: return t('Media');
    default: return t('Type {type}', { type: tipo });
  }
};

const CHANNEL_TYPE_ICONS: Record<number, any> = {
  0: Hash,
  2: Volume2,
  4: Folder,
  5: Megaphone,
  13: Radio,
  15: MessagesSquare,
};

const SIN_CATEGORIA = 'none';
const TIPO_CATEGORIA = 4;

/** Qué diálogo de formulario está abierto: crear/editar un canal o una categoría. */
interface Formulario {
  categoria: boolean;
  editar: Channel | null;
  /** Categoría preseleccionada al crear un canal. */
  padre: string;
}

const FormularioCanal: React.FC<{
  datos: Formulario | null;
  categorias: Channel[];
  onCerrar: () => void;
  onGuardar: (f: Formulario, d: DatosCanal) => Promise<void>;
}> = ({ datos, categorias, onCerrar, onGuardar }) => {
  const [nombre, setNombre] = useState('');
  const [tema, setTema] = useState('');
  const [padre, setPadre] = useState('');
  const [lento, setLento] = useState('');
  const [tipo, setTipo] = useState('0');
  const [error, setError] = useState('');
  const [guardando, setGuardando] = useState(false);

  useEffect(() => {
    if (!datos) return;
    setNombre(datos.editar?.name ?? '');
    setTema('');
    setLento('');
    setTipo('0');
    setPadre(datos.editar ? datos.editar.parent_id ?? '' : datos.padre);
    setError('');
    setGuardando(false);
  }, [datos]);

  if (!datos) return <Dialogo abierto={false} titulo="" onCerrar={onCerrar} />;
  const esTexto = !datos.categoria && (datos.editar ? [0, 5].includes(datos.editar.channel_type) : tipo === '0');
  const titulo = datos.categoria ? (datos.editar ? t('Edit category') : t('Create category')) : (datos.editar ? t('Edit channel') : t('Create channel'));

  const guardar = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!nombre.trim()) { setError(datos.categoria ? t('The category name cannot be empty') : t('The channel name cannot be empty')); return; }
    const d: DatosCanal = { name: nombre.trim() };
    if (!datos.editar) d.channel_type = datos.categoria ? TIPO_CATEGORIA : Number(tipo);
    if (!datos.categoria) {
      if (datos.editar || padre) d.parent_id = padre;
      if (esTexto && tema.trim()) d.topic = tema.trim();
      if (esTexto && lento !== '') d.rate_limit_per_user = Math.max(0, Math.floor(Number(lento) || 0));
    }
    setGuardando(true);
    try {
      await onGuardar(datos, d);
    } catch (err) {
      setError(typeof err === 'string' ? err : datos.categoria ? t('Could not save the category') : t('Could not save the channel'));
      setGuardando(false);
    }
  };

  return (
    <Dialogo
      abierto
      titulo={titulo}
      onCerrar={onCerrar}
      pie={
        <>
          <Button variante="secundario" onClick={onCerrar}>{t('Cancel')}</Button>
          <Button type="submit" form="form-canal" cargando={guardando}>{datos.editar ? t('Save') : t('Create')}</Button>
        </>
      }
    >
      <form id="form-canal" className="ui-dialogo__cuerpo" onSubmit={guardar}>
        <Input data-autofocus etiqueta={t('Name')} value={nombre} onChange={(e) => setNombre(e.target.value)} maxLength={100} />
        {!datos.editar && !datos.categoria && (
          <Select etiqueta={t('Type')} value={tipo} onChange={(e) => setTipo(e.target.value)}>
            <option value="0">{t('Text')}</option>
            <option value="2">{t('Voice')}</option>
          </Select>
        )}
        {!datos.categoria && (
          <Select etiqueta={t('Category')} value={padre} onChange={(e) => setPadre(e.target.value)}>
            <option value="">{t('No category')}</option>
            {categorias.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </Select>
        )}
        {esTexto && (
          <>
            <Input
              etiqueta={t('Topic')}
              value={tema}
              onChange={(e) => setTema(e.target.value)}
              maxLength={1024}
              ayuda={datos.editar ? t('Leave it empty to keep the current topic.') : undefined}
            />
            <Input
              etiqueta={t('Slowmode (seconds)')}
              type="number"
              min={0}
              max={21600}
              value={lento}
              onChange={(e) => setLento(e.target.value)}
              ayuda={datos.editar ? t('Leave it empty to keep it; 0 turns it off.') : t('0 or empty: no slowmode. Maximum 21600 (6 h).')}
            />
          </>
        )}
        {error && <span className="ui-campo__error" role="alert">{error}</span>}
      </form>
    </Dialogo>
  );
};

export const ChannelsView: React.FC = () => {
  const serverMap = useServerMap();
  const setSelectedChannel = useSetSelectedChannel();
  const selectedChannel = useSelectedChannel();
  const addLog = useAddLog();
  const showNotification = useShowNotification();
  const motivoEnviar = motivoPermisoAusente(useGuildPanel().permissions, 'Enviar mensajes');
  const [search, setSearch] = useState('');
  const [cerradas, setCerradas] = useState<Record<string, boolean>>({});
  const [newMessage, setNewMessage] = useState('');
  const guildId = useActiveGuildId();
  const motivoGestionar = motivoPermisoAusente(useGuildPanel().permissions, 'Gestionar canales');
  const motivoRoles = motivoPermisoAusente(useGuildPanel().permissions, 'Gestionar roles');
  const [formulario, setFormulario] = useState<Formulario | null>(null);
  const [aBorrar, setABorrar] = useState<Channel | null>(null);
  const categorias = useMemo(
    () => (serverMap?.channels ?? []).filter((c) => c.channel_type === TIPO_CATEGORIA).sort((a, b) => a.position - b.position),
    [serverMap],
  );

  const tras = async (mensaje: string) => {
    await useStore.getState().refreshServerMap();
    showNotification(mensaje, 'success');
    addLog(t('[CHANNELS] {message}', { message: mensaje }));
  };

  const guardar = async (f: Formulario, d: DatosCanal) => {
    if (!guildId) throw t('There is no active server');
    if (f.editar) {
      await updateChannel(guildId, f.editar.id, d);
      if (selectedChannel?.id === f.editar.id) setSelectedChannel(null);
    } else {
      await createChannel(guildId, d);
    }
    setFormulario(null);
    await tras(
      f.categoria
        ? (f.editar ? t('Category “{name}” updated', { name: d.name }) : t('Category “{name}” created', { name: d.name }))
        : (f.editar ? t('Channel “{name}” updated', { name: d.name }) : t('Channel “{name}” created', { name: d.name })),
    );
  };

  const borrar = async () => {
    if (!aBorrar || !guildId) return;
    const c = aBorrar;
    try {
      await deleteChannel(guildId, c.id);
    } catch (e) {
      showNotification(typeof e === 'string' ? e : t('Could not delete'), 'error');
      return;
    }
    setABorrar(null);
    if (selectedChannel?.id === c.id) setSelectedChannel(null);
    await tras(c.channel_type === TIPO_CATEGORIA ? t('Category “{name}” deleted', { name: c.name }) : t('Channel “{name}” deleted', { name: c.name }));
  };

  const guardarPermisos = async ({ fijar, quitar }: CambiosPermisos) => {
    const canal = selectedChannel;
    if (!guildId || !canal) throw t('There is no active server');
    try {
      for (const e of fijar) await setChannelPermission(guildId, canal.id, e.id, e.tipo, e.allow.toString(), e.deny.toString());
      for (const e of quitar) await deleteChannelPermission(guildId, canal.id, e.id, e.tipo);
    } finally {
      await useStore.getState().refreshServerMap();
    }
    const mensaje = t('Permissions of “{name}” saved', { name: canal.name });
    showNotification(mensaje, 'success');
    addLog(t('[CHANNELS] {message}', { message: mensaje }));
  };

  const grupos = useMemo(() => {
    const mapa = serverMap;
    if (!mapa) return [];
    const q = search.trim().toLowerCase();
    const nombreCategoria = (id: string) =>
      id === SIN_CATEGORIA ? t('No category') : mapa.channels.find((c) => c.id === id)?.name ?? id;
    return Object.entries(mapa.categories)
      .map(([id, canales]) => ({
        id,
        nombre: nombreCategoria(id),
        canales: [...canales]
          .filter((c) => c.channel_type !== 4 && (!q || c.name.toLowerCase().includes(q) || nombreCategoria(id).toLowerCase().includes(q)))
          .sort((a, b) => a.position - b.position),
      }))
      .filter((g) => g.canales.length > 0 || (!q && mapa.channels.some((c) => c.id === g.id)))
      .sort((a, b) => (a.id === SIN_CATEGORIA ? -1 : b.id === SIN_CATEGORIA ? 1 : a.nombre.localeCompare(b.nombre)));
  }, [serverMap, search]);

  const handleSend = async (channelId: string) => {
    if (!newMessage.trim()) return;
    try {
      await sendMessage(channelId, newMessage);
      addLog(`[SENT] #${channelId}: ${newMessage.slice(0, 50)}...`);
      showNotification(t('Message sent'), 'success');
      setNewMessage('');
    } catch (e) {
      showNotification(typeof e === 'string' ? e : t('Could not send the message'), 'error');
    }
  };

  const copyId = (id: string) => {
    void navigator.clipboard.writeText(id);
    showNotification(t('ID copied'), 'info');
  };

  const iconoDe = (c: Channel) => CHANNEL_TYPE_ICONS[c.channel_type] ?? Hash;
  const total = serverMap?.channels.filter((c) => c.channel_type !== 4).length ?? 0;

  return (
    <div className="vista channels-view">
      <CabeceraVista titulo={t('Channels')} cuenta={total}>
        <BuscadorVista valor={search} onCambio={setSearch} placeholder={t('Search channels…')} icono={<Search size={16} />} />
        <Button disabled={!!motivoGestionar} title={motivoGestionar ?? undefined} onClick={() => setFormulario({ categoria: true, editar: null, padre: '' })}>
          <Plus size={16} /> {t('Category')}
        </Button>
        <Button disabled={!!motivoGestionar} title={motivoGestionar ?? undefined} onClick={() => setFormulario({ categoria: false, editar: null, padre: '' })}>
          <Plus size={16} /> {t('Channel')}
        </Button>
      </CabeceraVista>

      <EstadoVista>
        {total === 0 ? (
          <EstadoVacio titulo={t('This server has no channels')} descripcion={t('Create the first one with the “Channel” button.')} />
        ) : grupos.length === 0 ? (
          <EstadoVacio titulo={t('No results')} descripcion={t('No channel matches “{search}”.', { search })} />
        ) : (
          <div className="vista__lista vista__tarjeta" role="tree" aria-label={t('Channels by category')}>
            {grupos.map((g) => {
              const abierta = !!search || !cerradas[g.id];
              return (
                <div key={g.id} className="arbol__categoria" role="treeitem" aria-expanded={abierta}>
                  <div className="arbol__fila">
                    <button
                      className={`arbol__cabecera${abierta ? ' arbol__cabecera--abierta' : ''}`}
                      onClick={() => setCerradas((p) => ({ ...p, [g.id]: abierta }))}
                    >
                      <ChevronRight size={14} />
                      <span>{g.nombre}</span>
                      <span className="vista__cuenta num">{g.canales.length}</span>
                    </button>
                    <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? t('Create a channel in {name}', { name: g.nombre })} aria-label={t('Create a channel in {name}', { name: g.nombre })} onClick={() => setFormulario({ categoria: false, editar: null, padre: g.id === SIN_CATEGORIA ? '' : g.id })}>
                      <Plus size={14} />
                    </Button>
                    {g.id !== SIN_CATEGORIA && (() => {
                      const cat = categorias.find((x) => x.id === g.id);
                      return cat ? (
                        <>
                          <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? t('Edit category')} aria-label={t('Edit category {name}', { name: g.nombre })} onClick={() => setFormulario({ categoria: true, editar: cat, padre: '' })}>
                            <Pencil size={12} />
                          </Button>
                          <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? t('Delete category')} aria-label={t('Delete category {name}', { name: g.nombre })} onClick={() => setABorrar(cat)}>
                            <Trash2 size={12} />
                          </Button>
                        </>
                      ) : null;
                    })()}
                  </div>
                  {abierta && (
                    <div className="arbol__canales" role="group">
                      {g.canales.map((c) => {
                        const Icon = iconoDe(c);
                        return (
                          <div key={c.id} className={`arbol__canal${selectedChannel?.id === c.id ? ' arbol__canal--activo' : ''}`}>
                            <button className="arbol__canal-sel" aria-pressed={selectedChannel?.id === c.id} onClick={() => setSelectedChannel(c)}>
                              <Icon size={14} aria-hidden="true" />
                              <span className="arbol__canal-nombre">{c.name}</span>
                              <Badge>{tipoCanal(c.channel_type)}</Badge>
                            </button>
                            <Button variante="ghost" tamano="sm" title={t('Copy ID')} aria-label={t('Copy the ID of {name}', { name: c.name })} onClick={() => copyId(c.id)}>
                              <Copy size={12} />
                            </Button>
                            <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? t('Edit channel')} aria-label={t('Edit channel {name}', { name: c.name })} onClick={() => setFormulario({ categoria: false, editar: c, padre: '' })}>
                              <Pencil size={12} />
                            </Button>
                            <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? t('Delete channel')} aria-label={t('Delete channel {name}', { name: c.name })} onClick={() => setABorrar(c)}>
                              <Trash2 size={12} />
                            </Button>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {selectedChannel && (
          <div className="vista__panel vista__tarjeta channel-detail-panel">
            <div className="vista__cabecera">
              <h3 className="vista__titulo-panel">#{selectedChannel.name}</h3>
              <Button variante="ghost" tamano="sm" onClick={() => setSelectedChannel(null)}>{t('Close')}</Button>
            </div>
            {selectedChannel.channel_type === 0 || selectedChannel.channel_type === 5 ? (
              <>
                <textarea
                  className="ui-control"
                  value={newMessage}
                  onChange={(e) => setNewMessage(e.target.value)}
                  placeholder={t('Write a message…')}
                  rows={3}
                  onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void handleSend(selectedChannel.id); } }}
                />
                <div className="vista__etiquetas">
                  <Button disabled={!!motivoEnviar} title={motivoEnviar ?? undefined} onClick={() => handleSend(selectedChannel.id)}>
                    <Send size={16} /> {t('Send')}
                  </Button>
                  {motivoEnviar && <span className="vista__nota">{motivoEnviar}</span>}
                </div>
              </>
            ) : (
              <span className="vista__nota">{t('Messages can only be sent to text channels.')}</span>
            )}
            {(selectedChannel.channel_type === 0 || selectedChannel.channel_type === 5) && (
              <ActividadCanal canalId={selectedChannel.id} />
            )}
            <PermisosCanal
              canal={serverMap?.channels.find((c) => c.id === selectedChannel.id) ?? selectedChannel}
              roles={serverMap?.roles ?? []}
              miembros={serverMap?.members ?? []}
              motivoBloqueo={motivoRoles}
              onGuardar={guardarPermisos}
            />
          </div>
        )}
      </EstadoVista>

      <FormularioCanal datos={formulario} categorias={categorias} onCerrar={() => setFormulario(null)} onGuardar={guardar} />
      <ConfirmarPeligroso
        abierto={!!aBorrar}
        titulo={aBorrar?.channel_type === TIPO_CATEGORIA ? t('Delete category') : t('Delete channel')}
        descripcion={aBorrar?.channel_type === TIPO_CATEGORIA ? t('The channels in the category are not deleted: they will have no category.') : t('The channel and its messages will be deleted. This cannot be undone.')}
        nombre={aBorrar?.name ?? ''}
        onConfirmar={borrar}
        onCerrar={() => setABorrar(null)}
      />
    </div>
  );
};
