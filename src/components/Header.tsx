import React from 'react';
import { useBotStatus } from '../store/useStore';
import { Server, Gauge, Users, Wifi, WifiOff } from 'lucide-react';

/** Cabecera de la sección activa (48px), con el estado del Gateway. */
export const Header: React.FC<{ titulo: string; icono?: any }> = ({ titulo, icono: Icono }) => {
  const botStatus = useBotStatus();

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
          <span className="status-item" title="Latencia">
            <Gauge size={14} aria-hidden="true" />
            <span className="num">{botStatus.latency_ms ? `${botStatus.latency_ms} ms` : '--'}</span>
          </span>
          <span className="status-item" title="Miembros">
            <Users size={14} aria-hidden="true" />
            <span className="num">{botStatus.member_count ? botStatus.member_count.toString() : '--'}</span>
          </span>
          <span className="status-item" title={botStatus.guild_name ?? 'Sin servidor'}>
            <Server size={14} aria-hidden="true" />
            <span className="status-guild">{botStatus.guild_name ?? '--'}</span>
          </span>
        </div>
      </div>
    </header>
  );
};
