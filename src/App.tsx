import React, { useEffect } from 'react';
import { useStore } from './store/useStore';
import { Sidebar } from './components/Sidebar';
import { GuildRail } from './components/GuildRail';
import { Header } from './components/Header';
import { Dashboard } from './components/Dashboard';
import { ChannelsView } from './components/ChannelsView';
import { MembersView } from './components/MembersView';
import { RolesView } from './components/RolesView';
import { SettingsView } from './components/SettingsView';
import { LogsView } from './components/LogsView';
import { ServidorView } from './components/ServidorView';
import { AsistenteView } from './components/AsistenteView';
import { Notification } from './components/Notification';
import { LayoutDashboard, Hash, Users, Shield, Terminal, Settings, Server, Sparkles } from 'lucide-react';
import { aplicarTema } from './temas';
import { setIdioma, t } from './i18n';
import { pasoCarga, finCarga } from './carga';
import { useBotStatus, useShowNotification } from './store/useStore';
import { getConfig, getBotStatus, refreshServerMap, getServerMap, onMapUpdated, onSyncProgress, onBotStatusChange, onLog } from './api/tauri';

function App() {
  const setConfig = useStore((s) => s.setConfig);
  const { botStatus, setBotStatus, serverMap, setServerMap, isLoading, setIsLoading } = useStore();
  const showNotification = useShowNotification();

  const uiTheme = useStore((s) => s.config.ui_theme);
  const uiMode = useStore((s) => s.config.ui_mode);
  useEffect(() => {
    aplicarTema(uiTheme, uiMode);
  }, [uiTheme, uiMode]);

  // El idioma se fija durante el render para que los t() de los hijos ya lo vean;
  // la `key` del contenedor fuerza a repintar todo el árbol al cambiarlo.
  const uiLanguage = useStore((s) => s.config.ui_language) || 'en';
  setIdioma(uiLanguage);

  useEffect(() => {
    const init = async () => {
      try {
        pasoCarga('Reading your settings…', 50);
        // El backend es la fuente de verdad (incluye tema y modo guardados).
        setConfig(await getConfig());
      } catch (e) {
        console.error('Config load error:', e);
      }
      try {
        pasoCarga('Connecting to the bot…', 65);
        const status = await getBotStatus();
        setBotStatus(status);
        pasoCarga('Loading your servers…', 80);
        const map = await refreshServerMap();
        setServerMap(map);
      } catch (e) {
        console.error('Init error:', e);
      }
      finCarga();
    };
    init();

    // Sin polling: el backend avisa con eventos.
    const recargarMapa = async () => {
      try {
        const map = await getServerMap();
        if (map) setServerMap(map);
      } catch (e) {
        console.error('Map reload error:', e);
      }
    };
    const desuscribir = [
      onMapUpdated(recargarMapa),
      onSyncProgress((p) => {
        if (p.step >= p.total) recargarMapa();
      }),
      onBotStatusChange(setBotStatus),
      onLog((texto) => useStore.getState().addLog(texto)),
    ];

    return () => {
      desuscribir.forEach((u) => u.then((fn) => fn()).catch(() => {}));
    };
  }, [setBotStatus, setServerMap, setConfig]);

  const tabs: { id: string; label: string; icon: string }[] = [
    { id: 'dashboard', label: t('Dashboard'), icon: 'layout-dashboard' },
    { id: 'channels', label: t('Channels'), icon: 'hash' },
    { id: 'members', label: t('Members'), icon: 'users' },
    { id: 'roles', label: t('Roles'), icon: 'shield' },
    { id: 'servidor', label: t('Server'), icon: 'server' },
    { id: 'asistente', label: t('AI Assistant'), icon: 'sparkles' },
    { id: 'logs', label: t('Logs'), icon: 'terminal' },
    { id: 'settings', label: t('Settings'), icon: 'settings' },
  ];

  const activeTab = useStore((s) => s.activeTab);
  const seccion = tabs.find((tab) => tab.id === activeTab) ?? tabs[0];
  const iconosSeccion: Record<string, any> = {
    'layout-dashboard': LayoutDashboard, hash: Hash, users: Users, shield: Shield, server: Server, sparkles: Sparkles, terminal: Terminal, settings: Settings,
  };

  const renderView = () => {
    switch (activeTab) {
      case 'dashboard': return <Dashboard />;
      case 'channels': return <ChannelsView />;
      case 'members': return <MembersView />;
      case 'roles': return <RolesView />;
      case 'servidor': return <ServidorView />;
      case 'asistente': return <AsistenteView />;
      case 'logs': return <LogsView />;
      case 'settings': return <SettingsView />;
      default: return <Dashboard />;
    }
  };

  return (
    <div className="app" key={uiLanguage}>
      <GuildRail />
      <Sidebar tabs={tabs} />
      <div className="main-layout">
        <Header titulo={seccion.label} icono={iconosSeccion[seccion.icon]} />
        <main className="main-content">
          {renderView()}
        </main>
      </div>
      <Notification />
    </div>
  );
}

export default App;