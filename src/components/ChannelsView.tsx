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
import { CHANNEL_TYPE_LABELS } from '../types';
import type { Channel } from '../types';

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
  const objeto = datos.categoria ? 'categoría' : 'canal';
  const titulo = `${datos.editar ? 'Editar' : 'Crear'} ${objeto}`;

  const guardar = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!nombre.trim()) { setError(`El nombre de la ${objeto} no puede estar vacío`); return; }
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
      setError(typeof err === 'string' ? err : `No se pudo guardar la ${objeto}`);
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
          <Button variante="secundario" onClick={onCerrar}>Cancelar</Button>
          <Button type="submit" form="form-canal" cargando={guardando}>{datos.editar ? 'Guardar' : 'Crear'}</Button>
        </>
      }
    >
      <form id="form-canal" className="ui-dialogo__cuerpo" onSubmit={guardar}>
        <Input data-autofocus etiqueta="Nombre" value={nombre} onChange={(e) => setNombre(e.target.value)} maxLength={100} />
        {!datos.editar && !datos.categoria && (
          <Select etiqueta="Tipo" value={tipo} onChange={(e) => setTipo(e.target.value)}>
            <option value="0">Texto</option>
            <option value="2">Voz</option>
          </Select>
        )}
        {!datos.categoria && (
          <Select etiqueta="Categoría" value={padre} onChange={(e) => setPadre(e.target.value)}>
            <option value="">Sin categoría</option>
            {categorias.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
          </Select>
        )}
        {esTexto && (
          <>
            <Input
              etiqueta="Tema"
              value={tema}
              onChange={(e) => setTema(e.target.value)}
              maxLength={1024}
              ayuda={datos.editar ? 'Déjalo vacío para no cambiar el tema actual.' : undefined}
            />
            <Input
              etiqueta="Modo lento (segundos)"
              type="number"
              min={0}
              max={21600}
              value={lento}
              onChange={(e) => setLento(e.target.value)}
              ayuda={datos.editar ? 'Déjalo vacío para no cambiarlo; 0 lo desactiva.' : '0 o vacío: sin modo lento. Máximo 21600 (6 h).'}
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
    addLog('[CANALES] ' + mensaje);
  };

  const guardar = async (f: Formulario, d: DatosCanal) => {
    if (!guildId) throw 'No hay un servidor activo';
    if (f.editar) {
      await updateChannel(guildId, f.editar.id, d);
      if (selectedChannel?.id === f.editar.id) setSelectedChannel(null);
    } else {
      await createChannel(guildId, d);
    }
    setFormulario(null);
    const objeto = f.categoria ? 'Categoría' : 'Canal';
    await tras(objeto + ' «' + d.name + '» ' + (f.editar ? 'actualizad' : 'cread') + (f.categoria ? 'a' : 'o'));
  };

  const borrar = async () => {
    if (!aBorrar || !guildId) return;
    const c = aBorrar;
    try {
      await deleteChannel(guildId, c.id);
    } catch (e) {
      showNotification(typeof e === 'string' ? e : 'No se pudo borrar', 'error');
      return;
    }
    setABorrar(null);
    if (selectedChannel?.id === c.id) setSelectedChannel(null);
    await tras((c.channel_type === TIPO_CATEGORIA ? 'Categoría' : 'Canal') + ' «' + c.name + '» borrad' + (c.channel_type === TIPO_CATEGORIA ? 'a' : 'o'));
  };

  const guardarPermisos = async ({ fijar, quitar }: CambiosPermisos) => {
    const canal = selectedChannel;
    if (!guildId || !canal) throw 'No hay un servidor activo';
    try {
      for (const e of fijar) await setChannelPermission(guildId, canal.id, e.id, e.tipo, e.allow.toString(), e.deny.toString());
      for (const e of quitar) await deleteChannelPermission(guildId, canal.id, e.id, e.tipo);
    } finally {
      await useStore.getState().refreshServerMap();
    }
    const mensaje = 'Permisos de «' + canal.name + '» guardados';
    showNotification(mensaje, 'success');
    addLog('[CANALES] ' + mensaje);
  };

  const grupos = useMemo(() => {
    const mapa = serverMap;
    if (!mapa) return [];
    const q = search.trim().toLowerCase();
    const nombreCategoria = (id: string) =>
      id === SIN_CATEGORIA ? 'Sin categoría' : mapa.channels.find((c) => c.id === id)?.name ?? id;
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
      showNotification('Mensaje enviado', 'success');
      setNewMessage('');
    } catch (e) {
      showNotification(typeof e === 'string' ? e : 'Error al enviar el mensaje', 'error');
    }
  };

  const copyId = (id: string) => {
    void navigator.clipboard.writeText(id);
    showNotification('ID copiado', 'info');
  };

  const iconoDe = (c: Channel) => CHANNEL_TYPE_ICONS[c.channel_type] ?? Hash;
  const total = serverMap?.channels.filter((c) => c.channel_type !== 4).length ?? 0;

  return (
    <div className="vista channels-view">
      <CabeceraVista titulo="Canales" cuenta={total}>
        <BuscadorVista valor={search} onCambio={setSearch} placeholder="Buscar canales…" icono={<Search size={16} />} />
        <Button disabled={!!motivoGestionar} title={motivoGestionar ?? undefined} onClick={() => setFormulario({ categoria: true, editar: null, padre: '' })}>
          <Plus size={16} /> Categoría
        </Button>
        <Button disabled={!!motivoGestionar} title={motivoGestionar ?? undefined} onClick={() => setFormulario({ categoria: false, editar: null, padre: '' })}>
          <Plus size={16} /> Canal
        </Button>
      </CabeceraVista>

      <EstadoVista>
        {total === 0 ? (
          <EstadoVacio titulo="Este servidor no tiene canales" descripcion="Crea el primero con el botón «Canal»." />
        ) : grupos.length === 0 ? (
          <EstadoVacio titulo="Sin resultados" descripcion={`Ningún canal coincide con «${search}».`} />
        ) : (
          <div className="vista__lista vista__tarjeta" role="tree" aria-label="Canales por categoría">
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
                    <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? 'Crear canal en ' + g.nombre} aria-label={'Crear canal en ' + g.nombre} onClick={() => setFormulario({ categoria: false, editar: null, padre: g.id === SIN_CATEGORIA ? '' : g.id })}>
                      <Plus size={14} />
                    </Button>
                    {g.id !== SIN_CATEGORIA && (() => {
                      const cat = categorias.find((x) => x.id === g.id);
                      return cat ? (
                        <>
                          <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? 'Editar categoría'} aria-label={'Editar categoría ' + g.nombre} onClick={() => setFormulario({ categoria: true, editar: cat, padre: '' })}>
                            <Pencil size={12} />
                          </Button>
                          <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? 'Borrar categoría'} aria-label={'Borrar categoría ' + g.nombre} onClick={() => setABorrar(cat)}>
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
                              <Badge>{CHANNEL_TYPE_LABELS[c.channel_type] ?? `Tipo ${c.channel_type}`}</Badge>
                            </button>
                            <Button variante="ghost" tamano="sm" title="Copiar ID" aria-label={`Copiar ID de ${c.name}`} onClick={() => copyId(c.id)}>
                              <Copy size={12} />
                            </Button>
                            <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? 'Editar canal'} aria-label={'Editar canal ' + c.name} onClick={() => setFormulario({ categoria: false, editar: c, padre: '' })}>
                              <Pencil size={12} />
                            </Button>
                            <Button variante="ghost" tamano="sm" disabled={!!motivoGestionar} title={motivoGestionar ?? 'Borrar canal'} aria-label={'Borrar canal ' + c.name} onClick={() => setABorrar(c)}>
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
              <Button variante="ghost" tamano="sm" onClick={() => setSelectedChannel(null)}>Cerrar</Button>
            </div>
            {selectedChannel.channel_type === 0 || selectedChannel.channel_type === 5 ? (
              <>
                <textarea
                  className="ui-control"
                  value={newMessage}
                  onChange={(e) => setNewMessage(e.target.value)}
                  placeholder="Escribe un mensaje…"
                  rows={3}
                  onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void handleSend(selectedChannel.id); } }}
                />
                <div className="vista__etiquetas">
                  <Button disabled={!!motivoEnviar} title={motivoEnviar ?? undefined} onClick={() => handleSend(selectedChannel.id)}>
                    <Send size={16} /> Enviar
                  </Button>
                  {motivoEnviar && <span className="vista__nota">{motivoEnviar}</span>}
                </div>
              </>
            ) : (
              <span className="vista__nota">Solo se pueden enviar mensajes a canales de texto.</span>
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
        titulo={aBorrar?.channel_type === TIPO_CATEGORIA ? 'Borrar categoría' : 'Borrar canal'}
        descripcion={aBorrar?.channel_type === TIPO_CATEGORIA ? 'Los canales de la categoría no se borran: quedarán sin categoría.' : 'Se borrarán el canal y sus mensajes. No se puede deshacer.'}
        nombre={aBorrar?.name ?? ''}
        onConfirmar={borrar}
        onCerrar={() => setABorrar(null)}
      />
    </div>
  );
};
