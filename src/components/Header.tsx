import React, { useEffect, useState } from 'react';
import { useConfig, useBotStatus, useActiveGuildId, useSetActiveGuildId } from '../store/useStore';
import { listGuilds, setActiveGuild } from '../api/tauri';
import type { Guild } from '../api/tauri';
import { Server, Cpu, MemoryStick, Wifi, WifiOff, Zap } from 'lucide-react';

export const Header: React.FC = () => {
  const config = useConfig();
  const botStatus = useBotStatus();
  const activeGuildId = useActiveGuildId();
  const setActiveGuildId = useSetActiveGuildId();
  const [guilds, setGuilds] = useState<Guild[]>([]);

  useEffect(() => {
    listGuilds()
      .then(setGuilds)
      .catch(() => setGuilds([]));
  }, []);

  const handleGuildChange = async (e: React.ChangeEvent<HTMLSelectElement>) => {
    const id = e.target.value;
    try {
      await setActiveGuild(id);
      setActiveGuildId(id);
    } catch {
      // se mantiene la guild anterior si falla
    }
  };

  return (
    <header className="header">
      <div className="header-left">
        <h1 className="page-title">PANEL DE CONTROL</h1>
        <span className="divider">/</span>
        <select
          className="server-name"
          aria-label="Servidor activo"
          value={activeGuildId ?? ''}
          onChange={handleGuildChange}
        >
          {!activeGuildId && <option value="" disabled>Selecciona servidor</option>}
          {guilds.map((g) => (
            <option key={g.id} value={g.id}>{g.name}</option>
          ))}
        </select>
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
            <span>{botStatus.guild_name ?? '--'}</span>
            <label>SERVER</label>
          </div>
        </div>
      </div>
    </header>
  );
};