import React, { useEffect, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useStore, useConfig, useActiveGuildId, useSetActiveGuildId, useLoadGuildPanel, useShowNotification } from '../store/useStore';
import { listGuilds, setActiveGuild, inviteUrl } from '../api/tauri';
import type { Guild } from '../api/tauri';
import { Avatar } from './ui';
import { t } from '../i18n';
import { debeCargar, respuestaVigente, servidorTrasCargar } from '../guildRailLogic';
import type { EstadoCarga } from '../guildRailLogic';
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

  // Discord limita GET /users/@me/guilds a 1 por segundo: con el mismo bot nunca
  // dos a la vez y, al volver a la ventana, como mucho una cada 10 s. Al cambiar
  // de bot se pide siempre y la respuesta tardía del bot anterior se descarta
  // (antes una petición en curso del bot anterior dejaba su lista en pantalla).
  const estado = React.useRef<EstadoCarga>({ bot: null, enCurso: false, ultima: 0 });
  const activoRef = React.useRef(activeGuildId);
  activoRef.current = activeGuildId;
  const cargar = React.useCallback((forzar: boolean = true) => {
    const bot = config.active_bot_id;
    if (!bot) {
      estado.current = { bot: null, enCurso: false, ultima: 0 };
      setGuilds([]);
      setError(null);
      return;
    }
    if (!debeCargar(estado.current, bot, forzar, Date.now())) return;
    if (estado.current.bot !== bot) setGuilds([]);
    estado.current = { bot, enCurso: true, ultima: Date.now() };
    setCargando(true);
    listGuilds()
      .then((lista) => {
        if (!respuestaVigente(estado.current, bot)) return;
        setGuilds(lista);
        setError(null);
        // El servidor activo debe ser uno de este bot.
        const destino = servidorTrasCargar(lista.map((g) => g.id), activoRef.current);
        if (destino && destino !== activoRef.current) {
          setActiveGuild(destino).then(() => setActiveGuildId(destino)).catch(() => {});
        }
      })
      .catch((e) => {
        if (!respuestaVigente(estado.current, bot)) return;
        setError(typeof e === 'string' ? e : t("Could not read the bot's servers"));
      })
      .finally(() => {
        if (!respuestaVigente(estado.current, bot)) return;
        estado.current = { ...estado.current, enCurso: false };
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
      showNotification(typeof e === 'string' ? e : t('Could not open the invite link'), 'error');
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
    <nav className="guild-rail" aria-label={t('Servers')}>
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
          aria-label={t('Error reading servers: {error}. Press to retry.', { error })}
          onClick={() => cargar()}
          {...evTip(t('{error} — press to retry', { error }))}
        >
          <span className="rail-add-icon"><AlertTriangle size={22} /></span>
        </button>
      )}
      <span className="rail-sep" />
      <button
        className="rail-item rail-add"
        aria-label={t('Refresh the server list')}
        onClick={() => cargar()}
        disabled={cargando || !config.active_bot_id}
        {...evTip(cargando ? t('Refreshing servers…') : t('Refresh the server list'))}
      >
        <span className="rail-add-icon"><RefreshCw size={20} className={cargando ? 'rail-girando' : undefined} /></span>
      </button>
      <button
        className="rail-item rail-add"
        aria-label={t('Add the bot to a server')}
        onClick={invitar}
        {...evTip(t('Add the bot to a server'))}
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
