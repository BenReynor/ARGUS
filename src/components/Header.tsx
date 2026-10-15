import React from 'react';
import { useConfig, useBotStatus } from '../store/useStore';
import { Server, Cpu, MemoryStick, Wifi, WifiOff, Zap } from 'lucide-react';

export const Header: React.FC = () => {
  const config = useConfig();
  const botStatus = useBotStatus();

  return (
    <header className="header">
      <div className="header-left">
        <h1 className="page-title">PANEL DE CONTROL</h1>
        <span className="divider">/</span>
        <span className="server-name">ISD Internal Security Department</span>
      </div>
      <div className="header-center">
        <div className={`connection-status ${botStatus.connected ? 'connected' : 'disconnected'}`}>
          <Wifi className="wifi-icon" size={14} />
          <WifiOff className="wifi-off-icon" size={14} />
          <span>{botStatus.connected ? 'CONECTADO A DISCORD GATEWAY' : 'DESCONECTADO'}</span>
        </div>
      </div>
      <div className="header-right">
        <div className="status-grid">
          <div className="status-item">
            <Cpu size={12} />
            <span>{botStatus.latency_ms ? `${botStatus.latency_ms}ms` : '--'}</span>
            <label>LATENCIA</label>
          </div>
          <div className="status-item">
            <Zap size={12} />
            <span>{botStatus.member_count ? botStatus.member_count.toString() : '--'}</span>
            <label>MIEMBROS</label>
          </div>
          <div className="status-item">
            <Server size={12} />
            <span>{botStatus.guild_name ? 'ISD' : '--'}</span>
            <label>SERVER</label>
          </div>
        </div>
      </div>
    </header>
  );
};