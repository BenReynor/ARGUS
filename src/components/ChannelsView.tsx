import React, { useMemo, useState } from 'react';
import { useServerMap, useSetSelectedChannel, useSelectedChannel, useAddLog, useShowNotification, useGuildPanel, motivoPermisoAusente } from '../store/useStore';
import { sendMessage } from '../api/tauri';
import { Button, Badge, EstadoVacio } from './ui';
import { EstadoVista, BuscadorVista, CabeceraVista } from './EstadoVista';
import { Hash, Volume2, Folder, Megaphone, Radio, MessagesSquare, Send, Copy, Search, ChevronRight } from 'lucide-react';
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
      </CabeceraVista>

      <EstadoVista>
        {total === 0 ? (
          <EstadoVacio titulo="Este servidor no tiene canales" descripcion="Crea un canal en Discord y pulsa Resincronizar en el Panel para verlo aquí." />
        ) : grupos.length === 0 ? (
          <EstadoVacio titulo="Sin resultados" descripcion={`Ningún canal coincide con «${search}».`} />
        ) : (
          <div className="vista__lista vista__tarjeta" role="tree" aria-label="Canales por categoría">
            {grupos.map((g) => {
              const abierta = !!search || !cerradas[g.id];
              return (
                <div key={g.id} className="arbol__categoria" role="treeitem" aria-expanded={abierta}>
                  <button
                    className={`arbol__cabecera${abierta ? ' arbol__cabecera--abierta' : ''}`}
                    onClick={() => setCerradas((p) => ({ ...p, [g.id]: abierta }))}
                  >
                    <ChevronRight size={14} />
                    <span>{g.nombre}</span>
                    <span className="vista__cuenta num">{g.canales.length}</span>
                  </button>
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
          </div>
        )}
      </EstadoVista>
    </div>
  );
};
