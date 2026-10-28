import React, { useEffect, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useStore, useConfig, useActiveGuildId, useSetActiveGuildId, useLoadGuildPanel, useShowNotification } from '../store/useStore';
import { listGuilds, setActiveGuild, inviteUrl } from '../api/tauri';
import type { Guild } from '../api/tauri';
import { Avatar } from './ui';
import { Plus } from 'lucide-react';

const guildIconUrl = (g: Guild) =>
  g.icon ? `https://cdn.discordapp.com/icons/${g.id}/${g.icon}.png?size=64` : null;

/** Riel de servidores: equivale al antiguo selector del Header. */
export const GuildRail: React.FC = () => {
  const config = useConfig();
  const activeGuildId = useActiveGuildId();
  const setActiveGuildId = useSetActiveGuildId();
  const loadGuildPanel = useLoadGuildPanel();
  const loadGuildSettings = useStore((s) => s.loadGuildSettings);
  const showNotification = useShowNotification();
  const [guilds, setGuilds] = useState<Guild[]>([]);

  useEffect(() => {
    listGuilds()
      .then(setGuilds)
      .catch(() => setGuilds([]));
  }, [config.active_bot_id]);

  // El panel y los ajustes del servidor se recargan al cambiar de servidor o de bot.
  useEffect(() => {
    loadGuildPanel(activeGuildId);
    loadGuildSettings(activeGuildId);
  }, [activeGuildId, config.active_bot_id]);

  const elegir = async (id: string) => {
    if (id === activeGuildId) return;
    try {
      await setActiveGuild(id);
      setActiveGuildId(id);
    } catch {
      // se mantiene el servidor anterior si falla
    }
  };

  const invitar = async () => {
    try {
      await openUrl(await inviteUrl());
    } catch (e) {
      showNotification(typeof e === 'string' ? e : 'No se pudo abrir el enlace de invitación', 'error');
    }
  };

  return (
    <nav className="guild-rail" aria-label="Servidores">
      {guilds.map((g) => (
        <button
          key={g.id}
          className={`rail-item ${g.id === activeGuildId ? 'active' : ''}`}
          title={g.name}
          aria-label={g.name}
          aria-pressed={g.id === activeGuildId}
          onClick={() => elegir(g.id)}
        >
          <span className="rail-pill" />
          <Avatar src={guildIconUrl(g)} nombre={g.name} tamano={48} />
        </button>
      ))}
      <span className="rail-sep" />
      <button className="rail-item rail-add" title="Añadir el bot a un servidor" aria-label="Añadir el bot a un servidor" onClick={invitar}>
        <span className="rail-add-icon"><Plus size={22} /></span>
      </button>
    </nav>
  );
};
