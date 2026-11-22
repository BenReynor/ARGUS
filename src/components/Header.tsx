import React from 'react';
import { useStore, useBotStatus, useServerMap, useGuildPanel, useRecuentoMiembros } from '../store/useStore';
import { t } from '../i18n';
import { Server, Gauge, Users, Wifi, WifiOff, HelpCircle } from 'lucide-react';

/** Cabecera de la sección activa (48px), con el estado del Gateway. */
export const Header: React.FC<{ titulo: string; icono?: any; vista?: string }> = ({ titulo, icono: Icono, vista }) => {
  const openManual = useStore((s) => s.openManual);
  const botStatus = useBotStatus();
  const serverMap = useServerMap();
  const guildPanel = useGuildPanel();
  // Estos datos ya están cargados; el estado del bot no los rellenaba y se veía «--».
  const recuento = useRecuentoMiembros();
  const servidor = guildPanel.guild?.name ?? serverMap?.guild.name ?? botStatus.guild_name ?? null;

  return (
    <header className="header">
      <div className="header-left">
        {Icono && <Icono className="header-icono" size={20} aria-hidden="true" />}
        <h1 className="page-title">{titulo}</h1>
        {vista && vista !== 'manual' && (
          <button
            type="button"
            className="btn-ayuda-manual"
            title={t('Open the manual for this view')}
            aria-label={t('Open the manual for this view')}
            onClick={() => openManual(vista)}
          >
            <HelpCircle size={16} aria-hidden="true" />
          </button>
        )}
      </div>
      <div className="header-right">
        <div className={`connection-status ${botStatus.connected ? 'connected' : 'disconnected'}`}>
          <Wifi className="wifi-icon" size={14} aria-hidden="true" />
          <WifiOff className="wifi-off-icon" size={14} aria-hidden="true" />
          <span>{botStatus.connected ? t('Gateway live') : t('Gateway disconnected')}</span>
        </div>
        <div className="status-grid">
          {botStatus.latency_ms ? (
            <span className="status-item" title={t('Gateway latency')}>
              <Gauge size={14} aria-hidden="true" />
              <span className="num">{botStatus.latency_ms} ms</span>
            </span>
          ) : null}
          {recuento && recuento.valor > 0 && (
            <span className="status-item" title={recuento.aproximado ? t('Server members (approximate)') : t('Server members')}>
              <Users size={14} aria-hidden="true" />
              <span className="num">{recuento.aproximado ? '~' : ''}{recuento.valor.toLocaleString()}</span>
            </span>
          )}
          <span className="status-item" title={servidor ?? t('No server selected')}>
            <Server size={14} aria-hidden="true" />
            <span className="status-guild">{servidor ?? t('No server')}</span>
          </span>
        </div>
      </div>
    </header>
  );
};
