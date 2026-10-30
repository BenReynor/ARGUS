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
import { Notification } from './components/Notification';
import { LayoutDashboard, Hash, Users, Shield, Terminal, Settings, Server } from 'lucide-react';
import { aplicarTema } from './temas';
import { setIdioma } from './i18n';
import { pasoCarga, finCarga } from './carga';
import { useBotStatus, useShowNotification } from './store/useStore';
import { getConfig, getBotStatus, refreshServerMap, getServerMap, onMapUpdated, onSyncProgress, onBotStatusChange } from './api/tauri';

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
        pasoCarga('Leyendo tu configuración…', 50);
        // El backend es la fuente de verdad (incluye tema y modo guardados).
        setConfig(await getConfig());
      } catch (e) {
        console.error('Config load error:', e);
      }
      try {
        pasoCarga('Conectando con el bot…', 65);
        const status = await getBotStatus();
        setBotStatus(status);
        pasoCarga('Cargando tus servidores…', 80);
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
    ];

    return () => {
      desuscribir.forEach((u) => u.then((fn) => fn()).catch(() => {}));
    };
  }, [setBotStatus, setServerMap, setConfig]);

  const tabs: { id: string; label: string; icon: string }[] = [
    { id: 'dashboard', label: 'Panel', icon: 'layout-dashboard' },
    { id: 'channels', label: 'Canales', icon: 'hash' },
    { id: 'members', label: 'Miembros', icon: 'users' },
    { id: 'roles', label: 'Roles', icon: 'shield' },
    { id: 'servidor', label: 'Servidor', icon: 'server' },
    { id: 'logs', label: 'Logs', icon: 'terminal' },
    { id: 'settings', label: 'Config', icon: 'settings' },
  ];

  const activeTab = useStore((s) => s.activeTab);
  const seccion = tabs.find((t) => t.id === activeTab) ?? tabs[0];
  const iconosSeccion: Record<string, any> = {
    'layout-dashboard': LayoutDashboard, hash: Hash, users: Users, shield: Shield, server: Server, terminal: Terminal, settings: Settings,
  };

  const renderView = () => {
    switch (activeTab) {
      case 'dashboard': return <Dashboard />;
      case 'channels': return <ChannelsView />;
      case 'members': return <MembersView />;
      case 'roles': return <RolesView />;
      case 'servidor': return <ServidorView />;
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