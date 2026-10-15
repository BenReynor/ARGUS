import React from 'react';
import { useConfig, useBotStatus, useActiveTab, useSetActiveTab } from '../store/useStore';
import { startBot, stopBot } from '../api/tauri';
import {
  LayoutDashboard, Hash, Users, Shield, Terminal, Settings,
  Power, Circle, Bot, Server
} from 'lucide-react';

interface SidebarProps {
  tabs: { id: string; label: string; icon: string }[];
}

export const Sidebar: React.FC<SidebarProps> = ({ tabs }) => {
  const config = useConfig();
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
          <div className="bot-info">
            <Bot size={16} />
            <span>Agente Albert</span>
          </div>
          <button
            className={`btn-bot-toggle ${botStatus.running ? 'running' : ''}`}
            onClick={async () => {
              try {
                if (botStatus.running) {
                  await stopBot();
                } else {
                  if (!config.discord_token) {
                    alert('Configura el token en Ajustes');
                    return;
                  }
                  await startBot();
                }
              } catch (e) {
                console.error('Bot toggle error:', e);
              }
            }}
            disabled={!config.discord_token && !botStatus.running}
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