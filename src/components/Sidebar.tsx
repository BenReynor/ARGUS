import React from 'react';
import { useStore, useConfig, useBotStatus, useActiveTab, useSetActiveTab, useShowNotification } from '../store/useStore';
import { startBot, stopBot, setActiveBot, botAvatarUrl } from '../api/tauri';
import {
  LayoutDashboard, Hash, Users, Shield, Terminal, Settings,
  Power, Circle, Bot, Server
} from 'lucide-react';

interface SidebarProps {
  tabs: { id: string; label: string; icon: string }[];
}

export const Sidebar: React.FC<SidebarProps> = ({ tabs }) => {
  const config = useConfig();
  const setConfig = useStore((s) => s.setConfig);
  const showNotification = useShowNotification();
  const hasToken = config.bots.length > 0;
  const botStatus = useBotStatus();
  const activeTab = useActiveTab();
  const setActiveTab = useSetActiveTab();

  const iconMap: Record<string, any> = {
    'layout-dashboard': LayoutDashboard,
    'hash': Hash,
    'users': Users,
    'shield': Shield,
    'terminal': Terminal,
    'settings': Settings,
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <div className="logo">
          <div className="logo-icon">
            <Server className="h-6 w-6" />
          </div>
          <span className="logo-text">ARGUS</span>
        </div>
        <div className="bot-status-mini">
          <Circle 
            className={botStatus.connected ? 'status-online' : botStatus.running ? 'status-connecting' : 'status-offline'} 
            size={8} 
          />
          <span className="status-text">
            {botStatus.connected ? 'ONLINE' : botStatus.running ? 'CONECTANDO...' : 'OFFLINE'}
          </span>
        </div>
      </div>

      <nav className="sidebar-nav">
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
          <div className="bot-list">
            {config.bots.length === 0 && (
              <div className="bot-info"><Bot size={16} /><span>Sin bot activo</span></div>
            )}
            {config.bots.map((b) => {
              const url = botAvatarUrl(b);
              return (
                <button
                  key={b.id}
                  className={`nav-item ${b.id === config.active_bot_id ? 'active' : ''}`}
                  title="Usar como bot activo"
                  onClick={async () => {
                    try {
                      await setActiveBot(b.id);
                      setConfig({ active_bot_id: b.id });
                    } catch (e) {
                      showNotification(typeof e === 'string' ? e : 'No se pudo cambiar de bot', 'error');
                    }
                  }}
                >
                  {url ? (
                    <img src={url} alt="" width={18} height={18} style={{ borderRadius: '50%' }} />
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
                    alert('Configura el token en Ajustes');
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
                Apagar
              </>
            ) : (
              <>
                <Power size={14} />
                Encender
              </>
            )}
          </button>
        </div>
        <div className="version">v1.0.0</div>
      </div>
    </aside>
  );
};