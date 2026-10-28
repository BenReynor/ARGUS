import React, { useEffect, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useStore, useConfig, useActiveGuildId, useSetActiveGuildId, useLoadGuildPanel, useShowNotification } from '../store/useStore';
import { listGuilds, setActiveGuild, inviteUrl } from '../api/tauri';
import type { Guild } from '../api/tauri';
import { Avatar } from './ui';
import { Plus, RefreshCw, AlertTriangle } from 'lucide-react';

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
  const [tip, setTip] = useState<{ texto: string; top: number; left: number } | null>(null);

  const [error, setError] = useState<string | null>(null);
  const [cargando, setCargando] = useState(false);

  // Discord limita GET /users/@me/guilds a 1 por segundo: nunca dos a la vez y,
  // al volver a la ventana, como mucho una cada 10 s (el botón no espera).
  const enCurso = React.useRef(false);
  const ultima = React.useRef(0);
  const cargar = React.useCallback((forzar: boolean = true) => {
    if (!config.active_bot_id) {
      setGuilds([]);
      setError(null);
      return;
    }
    if (enCurso.current || (!forzar && Date.now() - ultima.current < 10_000)) return;
    enCurso.current = true;
    ultima.current = Date.now();
    setCargando(true);
    listGuilds()
      .then((lista) => {
        setGuilds(lista);
        setError(null);
      })
      .catch((e) => setError(typeof e === 'string' ? e : 'No se pudieron leer los servidores del bot'))
      .finally(() => {
        enCurso.current = false;
        setCargando(false);
      });
  }, [config.active_bot_id]);

  // Al arrancar, al cambiar de bot y al volver a la ventana (p. ej. después de
  // invitar el bot desde el navegador): antes solo se pedía una vez y un
  // servidor nuevo no aparecía hasta reiniciar la app.
  useEffect(() => {
    cargar();
    const alVolver = () => cargar(false);
    window.addEventListener('focus', alVolver);
    return () => window.removeEventListener('focus', alVolver);
  }, [cargar]);

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

  // El riel hace scroll y recortaría un tooltip absoluto: se posiciona fixed junto al elemento.
  const mostrar = (e: React.SyntheticEvent<HTMLElement>, texto: string) => {
    const r = e.currentTarget.getBoundingClientRect();
    setTip({ texto, top: r.top + r.height / 2, left: r.right });
  };
  const ocultar = () => setTip(null);
  const evTip = (texto: string) => ({
    onMouseEnter: (e: React.MouseEvent<HTMLElement>) => mostrar(e, texto),
    onFocus: (e: React.FocusEvent<HTMLElement>) => mostrar(e, texto),
    onMouseLeave: ocultar,
    onBlur: ocultar,
  });

  return (
    <nav className="guild-rail" aria-label="Servidores">
      {guilds.map((g) => (
        <button
          key={g.id}
          className={`rail-item ${g.id === activeGuildId ? 'active' : ''}`}
          aria-label={g.name}
          aria-pressed={g.id === activeGuildId}
          onClick={() => elegir(g.id)}
          {...evTip(g.name)}
        >
          <span className="rail-pill" />
          <Avatar src={guildIconUrl(g)} nombre={g.name} tamano={48} />
        </button>
      ))}
      {error && (
        <button
          className="rail-item rail-error"
          aria-label={`Error al leer los servidores: ${error}. Pulsa para reintentar.`}
          onClick={() => cargar()}
          {...evTip(`${error} — pulsa para reintentar`)}
        >
          <span className="rail-add-icon"><AlertTriangle size={22} /></span>
        </button>
      )}
      <span className="rail-sep" />
      <button
        className="rail-item rail-add"
        aria-label="Actualizar la lista de servidores"
        onClick={() => cargar()}
        disabled={cargando || !config.active_bot_id}
        {...evTip(cargando ? 'Actualizando servidores…' : 'Actualizar la lista de servidores')}
      >
        <span className="rail-add-icon"><RefreshCw size={20} className={cargando ? 'rail-girando' : undefined} /></span>
      </button>
      <button
        className="rail-item rail-add"
        aria-label="Añadir el bot a un servidor"
        onClick={invitar}
        {...evTip('Añadir el bot a un servidor')}
      >
        <span className="rail-add-icon"><Plus size={22} /></span>
      </button>
      {tip && (
        <span
          role="tooltip"
          className="rail-tooltip"
          style={{ top: tip.top, left: tip.left } as React.CSSProperties}
        >
          {tip.texto}
        </span>
      )}
    </nav>
  );
};
