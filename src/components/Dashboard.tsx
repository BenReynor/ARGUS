import React, { useEffect, useState } from 'react';
import { useServerMap, useBotStatus, useAddLog, useShowNotification, useSetActiveTab, useConfig, useActiveGuildId, useGuildPanel, PERMISOS_REQUERIDOS } from '../store/useStore';
import { startBot, stopBot, resyncMap, onSyncProgress } from '../api/tauri';
import type { SyncProgress } from '../api/tauri';
import { Users, Hash, Shield, Activity, Zap, Circle, Server, RefreshCw } from 'lucide-react';

interface StatItem {
  label: string;
  value: number;
  icon: any;
  color: string;
}

interface ActionItem {
  label: string;
  desc: string;
  action: () => void;
}

export const Dashboard: React.FC = () => {
  const serverMap = useServerMap();
  const botStatus = useBotStatus();
  const addLog = useAddLog();
  const showNotification = useShowNotification();
  const setActiveTab = useSetActiveTab();
  const config = useConfig();
  const activeGuildId = useActiveGuildId();
  const guildPanel = useGuildPanel();

  const stats: StatItem[] = [
    { label: 'CANALES', value: serverMap?.channels.length || 0, icon: Hash, color: 'var(--accent-cyan)' },
    { label: 'MIEMBROS', value: serverMap?.members.filter(m => !m.bot).length || 0, icon: Users, color: 'var(--accent-green)' },
    { label: 'ROLES', value: serverMap?.roles.length || 0, icon: Shield, color: 'var(--accent-purple)' },
    { label: 'BOTS', value: serverMap?.members.filter(m => m.bot).length || 0, icon: Activity, color: 'var(--accent-orange)' },
  ];

  const quickActions: ActionItem[] = [
    { label: 'Ver miembros', desc: 'Gestionar miembros del servidor', action: () => setActiveTab('members') },
    { label: 'Ver logs', desc: 'Abrir terminal de logs', action: () => setActiveTab('logs') },
  ];

  const [resyncing, setResyncing] = useState(false);
  const [progress, setProgress] = useState<SyncProgress | null>(null);

  useEffect(() => {
    const desuscribir = onSyncProgress(setProgress);
    return () => {
      desuscribir.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const handleResync = async () => {
    setResyncing(true);
    setProgress(null);
    try {
      await resyncMap();
      addLog('[SYSTEM] Mapa resincronizado');
      showNotification('Mapa resincronizado', 'success');
    } catch (e) {
      showNotification('Error al resincronizar: ' + String(e), 'error');
    } finally {
      setResyncing(false);
    }
  };

  const handleBotToggle = async () => {
    try {
      if (botStatus.running) {
        await stopBot();
        addLog('[SYSTEM] Bot detenido manualmente');
        showNotification('Bot detenido', 'info');
      } else {
        if (config.bots.length === 0) {
          showNotification('Configura el token primero', 'warning');
          return;
        }
        await startBot();
        addLog('[SYSTEM] Bot iniciado manualmente');
        showNotification('Bot iniciando...', 'info');
      }
    } catch (e) {
      showNotification('Error: ' + (e as Error).message, 'error');
    }
  };

  return (
    <div className="dashboard">
      <section className="dashboard-section">
        <h2>SERVIDOR ACTIVO</h2>
        {!activeGuildId && <p>Ningún servidor seleccionado. Elige uno en la cabecera.</p>}
        {activeGuildId && guildPanel.loading && <p>Cargando datos del servidor...</p>}
        {activeGuildId && guildPanel.error && <p className="error">{guildPanel.error}</p>}
        {guildPanel.guild && (
          <>
            <div className="member-cell">
              {guildPanel.guild.icon ? (
                <img
                  src={`https://cdn.discordapp.com/icons/${guildPanel.guild.id}/${guildPanel.guild.icon}.png?size=64`}
                  alt=""
                  width={48}
                  height={48}
                  style={{ borderRadius: '50%' }}
                />
              ) : (
                <Server size={48} />
              )}
              <div>
                <span className="member-name">{guildPanel.guild.name}</span>
                <span className="member-username">
                  ~{guildPanel.guild.member_count} miembros (aproximado)
                </span>
              </div>
            </div>
            <h3>PERMISOS DEL BOT</h3>
            <ul>
              {PERMISOS_REQUERIDOS.map((permiso) => {
                const tiene = guildPanel.permissions?.includes(permiso) ?? false;
                return (
                  <li key={permiso} style={{ color: tiene ? 'var(--accent-green)' : 'var(--accent-orange)' }}>
                    {tiene ? '✓' : '✗ Falta:'} {permiso}
                  </li>
                );
              })}
            </ul>
          </>
        )}
      </section>

      <section className="dashboard-section">
        <div className="section-header">
          <h2>ESTADO DEL SISTEMA</h2>
          <div className={`system-status ${botStatus.connected ? 'online' : botStatus.running ? 'connecting' : 'offline'}`}>
            <Circle className={`status-dot ${botStatus.connected ? 'online' : botStatus.running ? 'connecting' : 'offline'}`} size={10} />
            <span>{botStatus.connected ? 'SISTEMA OPERATIVO' : botStatus.running ? 'INICIALIZANDO...' : 'SISTEMA APAGADO'}</span>
            <button className="btn-primary" onClick={handleBotToggle}>
              {botStatus.running ? (
                <>
                  <Circle className="status-online" size={12} />
                  APAGAR BOT
                </>
              ) : (
                <>
                  <Zap size={14} />
                  ENCENDER BOT
                </>
              )}
            </button>
          </div>
        </div>

        <div className="stats-grid">
          {stats.map((stat, i) => (
            <div key={i} className="stat-card" style={{ '--stat-color': stat.color } as React.CSSProperties}>
              <div className="stat-icon" style={{ background: stat.color }}>
                <stat.icon size={24} />
              </div>
              <div className="stat-info">
                <span className="stat-value">{stat.value}</span>
                <span className="stat-label">{stat.label}</span>
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="dashboard-section">
        <h2>ACCIONES RÁPIDAS</h2>
        <div className="actions-grid">
          <button className="action-card" onClick={handleResync} disabled={resyncing || !activeGuildId}>
            <div className="action-icon"><RefreshCw size={20} /></div>
            <div className="action-info">
              <span className="action-label">{resyncing ? 'Resincronizando...' : 'Resincronizar mapa'}</span>
              <span className="action-desc">
                {resyncing && progress
                  ? `Paso ${progress.step}/${progress.total}: ${progress.message}`
                  : 'Repite la sincronización completa del servidor'}
              </span>
            </div>
          </button>
          {quickActions.map((action, i) => (
            <button key={i} className="action-card" onClick={action.action}>
              <div className="action-icon"><Activity size={20} /></div>
              <div className="action-info">
                <span className="action-label">{action.label}</span>
                <span className="action-desc">{action.desc}</span>
              </div>
            </button>
          ))}
        </div>
      </section>

      {serverMap && (
        <section className="dashboard-section">
          <h2>MAPA DEL SERVIDOR - RESUMEN</h2>
          <div className="server-summary">
            <div className="summary-card">
              <h3>📁 CATEGORÍAS</h3>
              <ul>
                {Object.keys(serverMap.categories).map((cat, i) => (
                  <li key={i}>
                    <span className="cat-name">{cat}</span>
                    <span className="cat-count">{serverMap.categories[cat].length} canales</span>
                  </li>
                ))}
              </ul>
            </div>
            <div className="summary-card">
              <h3>👥 ROLES PRINCIPALES</h3>
              <ul>
                {serverMap.roles.slice(0, 6).map((role) => (
                  <li key={role.id}>
                    <span className="role-name" style={{ color: `#${role.color.toString(16).padStart(6, '0')}` }}>
                      {role.name}
                    </span>
                    <span className="role-count">{role.member_count}</span>
                  </li>
                ))}
              </ul>
            </div>
            <div className="summary-card">
              <h3>🤖 BOTS ACTIVOS</h3>
              <ul>
                {serverMap.members.filter(m => m.bot).map((bot) => (
                  <li key={bot.user_id}>
                    <span className="bot-name">{bot.username}</span>
                    <span className="bot-id">{bot.user_id.slice(-6)}</span>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </section>
      )}

      <section className="dashboard-section">
        <h2>ACTIVIDAD RECIENTE</h2>
        <div className="activity-log">
          <div className="log-entry">
            <span className="log-time">[INIT]</span>
            <span className="log-source">SYSTEM</span>
            <span className="log-message">ARGUS v1.0.0 inicializado</span>
          </div>
          <div className="log-entry">
            <span className="log-time">[CONFIG]</span>
            <span className="log-source">CONFIG</span>
            <span className="log-message">Server ID: {config.active_guild_id ?? '--'}</span>
          </div>
          {botStatus.running && (
            <div className="log-entry">
              <span className="log-time">[BOT]</span>
              <span className="log-source">DISCORD</span>
              <span className="log-message">Conexión a gateway establecida</span>
            </div>
          )}
          {botStatus.last_error && (
            <div className="log-entry error">
              <span className="log-time">[ERROR]</span>
              <span className="log-source">DISCORD</span>
              <span className="log-message">{botStatus.last_error}</span>
            </div>
          )}
        </div>
      </section>
    </div>
  );
};