import React, { useEffect } from 'react';
import { useStore } from './store/useStore';
import { Sidebar } from './components/Sidebar';
import { Header } from './components/Header';
import { Dashboard } from './components/Dashboard';
import { ChannelsView } from './components/ChannelsView';
import { MembersView } from './components/MembersView';
import { RolesView } from './components/RolesView';
import { SettingsView } from './components/SettingsView';
import { LogsView } from './components/LogsView';
import { Notification } from './components/Notification';
import { useBotStatus, useShowNotification } from './store/useStore';
import { getBotStatus, refreshServerMap } from './api/tauri';

function App() {
  const { botStatus, setBotStatus, serverMap, setServerMap, isLoading, setIsLoading } = useStore();
  const showNotification = useShowNotification();

  useEffect(() => {
    const init = async () => {
      try {
        const status = await getBotStatus();
        setBotStatus(status);
        const map = await refreshServerMap();
        setServerMap(map);
      } catch (e) {
        console.error('Init error:', e);
      }
    };
    init();

    // Poll status every 5 seconds
    const interval = setInterval(async () => {
      try {
        const status = await getBotStatus();
        setBotStatus(status);
      } catch (e) {
        console.error('Status poll error:', e);
      }
    }, 5000);

    return () => clearInterval(interval);
  }, [setBotStatus, setServerMap]);

  const tabs = [
    { id: 'dashboard', label: 'Panel', icon: 'layout-dashboard' },
    { id: 'channels', label: 'Canales', icon: 'hash' },
    { id: 'members', label: 'Miembros', icon: 'users' },
    { id: 'roles', label: 'Roles', icon: 'shield' },
    { id: 'logs', label: 'Logs', icon: 'terminal' },
    { id: 'settings', label: 'Config', icon: 'settings' },
  ] as const;

  const renderView = () => {
    switch (useStore.getState().activeTab) {
      case 'dashboard': return <Dashboard />;
      case 'channels': return <ChannelsView />;
      case 'members': return <MembersView />;
      case 'roles': return <RolesView />;
      case 'logs': return <LogsView />;
      case 'settings': return <SettingsView />;
      default: return <Dashboard />;
    }
  };

  return (
    <div className="app scanlines">
      <Header />
      <div className="main-layout">
        <Sidebar tabs={tabs} />
        <main className="main-content">
          {renderView()}
        </main>
      </div>
      <Notification />
    </div>
  );
}

export default App;