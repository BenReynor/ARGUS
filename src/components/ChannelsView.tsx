import React, { useState } from 'react';
import { useServerMap, useSetSelectedChannel, useSelectedChannel, useAddLog, useShowNotification, useGuildPanel, motivoPermisoAusente } from '../store/useStore';
import { sendMessage } from '../api/tauri';
import { Button, Input } from './ui';
import { Hash, MessageSquare, Volume2, Folder, Send, Copy, Search, ChevronRight } from 'lucide-react';

const CHANNEL_TYPE_ICONS: Record<number, any> = {
  0: MessageSquare,
  2: Volume2,
  4: Folder,
  15: Hash,
};

export const ChannelsView: React.FC = () => {
  const serverMap = useServerMap();
  const setSelectedChannel = useSetSelectedChannel();
  const selectedChannel = useSelectedChannel();
  const addLog = useAddLog();
  const showNotification = useShowNotification();
  const motivoEnviar = motivoPermisoAusente(useGuildPanel().permissions, 'Enviar mensajes');
  const [search, setSearch] = useState('');
  const [expandedCategories, setExpandedCategories] = useState<Record<string, boolean>>({});
  const [newMessage, setNewMessage] = useState('');

  const categories = serverMap?.categories || {};
  const categoryNames: Record<string, string> = {
    'none': '📁 SIN CATEGORÍA',
  };

  const handleSend = async (channelId: string) => {
    if (!newMessage.trim()) return;
    try {
      await sendMessage(channelId, newMessage);
      addLog(`[SENT] #${channelId}: ${newMessage.slice(0, 50)}...`);
      showNotification('Mensaje enviado', 'success');
      setNewMessage('');
    } catch {
      showNotification('Error al enviar', 'error');
    }
  };

  const copyId = (id: string) => {
    navigator.clipboard.writeText(id);
    showNotification('ID copiado', 'info');
  };

  return (
    <div className="channels-view">
      <div className="view-header">
        <h2>CANALES DEL SERVIDOR</h2>
        <div className="search-box">
          <Search size={16} />
          <Input
            type="text"
            placeholder="Filtrar canales..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      </div>

      <div className="channels-tree">
        {Object.entries(categories).map(([catId, channels]) => {
          const filtered = channels.filter(c => 
            c.name.toLowerCase().includes(search.toLowerCase())
          );
          if (filtered.length === 0 && search) return null;

          const isExpanded = expandedCategories[catId] !== false;
          return (
            <div key={catId} className="category-group">
              <button className="category-header" onClick={() => setExpandedCategories(prev => ({ ...prev, [catId]: !isExpanded }))}>
                <ChevronRight size={14} className={isExpanded ? 'expanded' : ''} />
                <Folder size={16} />
                <span>{categoryNames[catId] || catId}</span>
                <span className="channel-count">{filtered.length}</span>
              </button>
              {isExpanded && (
                <div className="channels-list">
                  {filtered.map((channel) => {
                    const Icon = CHANNEL_TYPE_ICONS[channel.channel_type] || Hash;
                    return (
                      <div key={channel.id} className="channel-item">
                        <Icon size={14} className={`type-icon type-${channel.channel_type}`} />
                        <span className="channel-name">#{channel.name}</span>
                        <div className="channel-actions">
                          <button className="icon-btn" onClick={() => copyId(channel.id)} title="Copiar ID">
                            <Copy size={12} />
                          </button>
                          <button className="icon-btn" onClick={() => setSelectedChannel(channel)} title="Seleccionar">
                            <ChevronRight size={12} />
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {selectedChannel && (
        <div className="channel-detail-panel">
          <div className="panel-header">
            <h3>#{selectedChannel.name}</h3>
            <button className="icon-btn" onClick={() => setSelectedChannel(null)}>
              <ChevronRight size={16} className="rotate-180" />
            </button>
          </div>
          <div className="message-send">
            <textarea
              value={newMessage}
              onChange={(e) => setNewMessage(e.target.value)}
              placeholder="Escribe un mensaje..."
              rows={3}
              onKeyDown={(e) => e.key === 'Enter' && !e.shiftKey && (e.preventDefault(), handleSend(selectedChannel.id))}
            />
            <Button disabled={!!motivoEnviar} title={motivoEnviar ?? undefined} onClick={() => handleSend(selectedChannel.id)}>
              <Send size={16} />
              ENVIAR
            </Button>
            {motivoEnviar && <span className="channel-count">{motivoEnviar}</span>}
          </div>
        </div>
      )}
    </div>
  );
};