import React from 'react';
import { useBotStatus, useServerMap, useGuildPanel } from '../store/useStore';
import { Server, Gauge, Users, Wifi, WifiOff } from 'lucide-react';

/** Cabecera de la sección activa (48px), con el estado del Gateway. */
export const Header: React.FC<{ titulo: string; icono?: any }> = ({ titulo, icono: Icono }) => {
  const botStatus = useBotStatus();
  const serverMap = useServerMap();
  const guildPanel = useGuildPanel();
  // Estos datos ya están cargados; el estado del bot no los rellenaba y se veía «--».
  const miembros = serverMap?.members.length ?? botStatus.member_count ?? 0;
  const servidor = guildPanel.guild?.name ?? serverMap?.guild.name ?? botStatus.guild_name ?? null;

  return (
    <header className="header">
      <div className="header-left">
        {Icono && <Icono className="header-icono" size={20} aria-hidden="true" />}
        <h1 className="page-title">{titulo}</h1>
      </div>
      <div className="header-right">
        <div className={`connection-status ${botStatus.connected ? 'connected' : 'disconnected'}`}>
          <Wifi className="wifi-icon" size={14} aria-hidden="true" />
          <WifiOff className="wifi-off-icon" size={14} aria-hidden="true" />
          <span>{botStatus.connected ? 'Gateway en vivo' : 'Gateway desconectado'}</span>
        </div>
        <div className="status-grid">
          {botStatus.latency_ms ? (
            <span className="status-item" title="Latencia del Gateway">
              <Gauge size={14} aria-hidden="true" />
              <span className="num">{botStatus.latency_ms} ms</span>
            </span>
          ) : null}
          {miembros > 0 && (
            <span className="status-item" title="Miembros del servidor">
              <Users size={14} aria-hidden="true" />
              <span className="num">{miembros.toLocaleString()}</span>
            </span>
          )}
          <span className="status-item" title={servidor ?? 'Ningún servidor seleccionado'}>
            <Server size={14} aria-hidden="true" />
            <span className="status-guild">{servidor ?? 'Sin servidor'}</span>
          </span>
        </div>
      </div>
    </header>
  );
};
