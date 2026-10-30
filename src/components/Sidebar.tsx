import React from 'react';
import { useStore, useConfig, useBotStatus, useActiveTab, useSetActiveTab, useShowNotification } from '../store/useStore';
import { startBot, stopBot, setActiveBot, botAvatarUrl } from '../api/tauri';
import {
  LayoutDashboard, Hash, Users, Shield, Terminal, Settings,
  Power, Circle, Bot, Server
} from 'lucide-react';
import { Avatar } from './ui';
import { t } from '../i18n';

interface SidebarProps {
  tabs: { id: string; label: string; icon: string }[];
}

export const Sidebar: React.FC<SidebarProps> = ({ tabs }) => {
  const config = useConfig();
  const setConfig = useStore((s) => s.setConfig);
  const showNotification = useShowNotification();
  const hasToken = config.bots.length > 0;
  const botStatus = useBotStatus();
  const botActivo = config.bots.find((b) => b.id === config.active_bot_id) ?? null;
  const activeTab = useActiveTab();
  const setActiveTab = useSetActiveTab();

  const iconMap: Record<string, any> = {
    'layout-dashboard': LayoutDashboard,
    'hash': Hash,
    'users': Users,
    'shield': Shield,
    'server': Server,
    'terminal': Terminal,
    'settings': Settings,
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span className="avatar-estado">
          <Avatar src={botActivo ? botAvatarUrl(botActivo) : null} nombre={botActivo?.name ?? 'ARGUS'} tamano={32} />
          <span
            className={`punto-estado ${botStatus.connected ? 'status-online' : botStatus.running ? 'status-connecting' : 'status-offline'}`}
            role="img"
            aria-label={botStatus.connected ? t('Gateway online') : botStatus.running ? t('Gateway connecting') : t('Gateway offline')}
          />
        </span>
        <div className="bot-activo">
          <span className="bot-activo-nombre">{botActivo?.name ?? t('No active bot')}</span>
          <span className="bot-status-mini">
            <span className="status-text">
              {botStatus.connected ? t('Online') : botStatus.running ? t('Connecting...') : t('Disconnected')}
            </span>
          </span>
        </div>
      </div>

      <nav className="sidebar-nav" aria-label={t('Sections')}>
        <span className="nav-categoria">{t('Sections')}</span>
        {tabs.map((tab) => {
          const Icon = iconMap[tab.icon];
          const isActive = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              className={`nav-item ${isActive ? 'active' : ''}`}
              onClick={() => setActiveTab(tab.id as any)}
            >
              <Icon size={18} />
              <span>{tab.label}</span>
            </button>
          );
        })}
      </nav>

      <div className="sidebar-footer">
        <div className="bot-control">
          <span className="nav-categoria">{t('Bots')}</span>
          <div className="bot-list">
            {config.bots.length === 0 && (
              <div className="bot-info"><Bot size={16} /><span>{t('No active bot')}</span></div>
            )}
            {config.bots.map((b) => {
              const url = botAvatarUrl(b);
              return (
                <button
                  key={b.id}
                  className={`nav-item ${b.id === config.active_bot_id ? 'active' : ''}`}
                  title={t('Use as the active bot')}
                  onClick={async () => {
                    try {
                      await setActiveBot(b.id);
                      setConfig({ active_bot_id: b.id });
                    } catch (e) {
                      showNotification(typeof e === 'string' ? e : t('Could not switch bot'), 'error');
                    }
                  }}
                >
                  {url ? (
                    <img src={url} alt="" width={18} height={18} className="nav-avatar" />
                  ) : (
                    <Bot size={18} />
                  )}
                  <span>{b.name}</span>
                </button>
              );
            })}
          </div>
          <button
            className={`btn-bot-toggle ${botStatus.running ? 'running' : ''}`}
            onClick={async () => {
              try {
                if (botStatus.running) {
                  await stopBot();
                } else {
                  if (!hasToken) {
                    showNotification(t('Add a bot in Settings to turn it on'), 'warning');
                    return;
                  }
                  await startBot();
                }
              } catch (e) {
                console.error('Bot toggle error:', e);
              }
            }}
            disabled={!hasToken && !botStatus.running}
          >
            {botStatus.running ? (
              <>
                <Circle className="status-online" size={8} />
                {t('Turn off')}
              </>
            ) : (
              <>
                <Power size={14} />
                {t('Turn on')}
              </>
            )}
          </button>
        </div>
        <div className="version">v{__APP_VERSION__}</div>
      </div>
    </aside>
  );
};