import React, { useState, useEffect } from 'react';
import { useStore, useConfig, useUiSettings, useSetActiveTab, useShowNotification, useActiveGuildId, useGuildSettings } from '../store/useStore';
import { openUrl } from '@tauri-apps/plugin-opener';
import { getConfig, saveConfig, saveGuildSettings, addBot, deleteBot, startBot, getBotStatus, inviteUrl } from '../api/tauri';
import { Save, Key, Trash2, Plus, Server, ToggleLeft, ToggleRight, CheckCircle, AlertCircle, Loader2, Eye, EyeOff, Copy, Palette } from 'lucide-react';
import { TEMAS, aplicarTema } from '../temas';

export const SettingsView: React.FC = () => {
  const config = useConfig();
  const setActiveTab = useSetActiveTab();
  const showNotification = useShowNotification();
  const activeGuildId = useActiveGuildId();
  const guildSettings = useGuildSettings();
  const setGuildSettings = useStore((s) => s.setGuildSettings);
  const [savingGuild, setSavingGuild] = useState(false);

  const handleGuildChange = (patch: Partial<NonNullable<typeof guildSettings>>) => {
    if (guildSettings) setGuildSettings({ ...guildSettings, ...patch });
  };

  const handleSaveGuild = async () => {
    if (!activeGuildId || !guildSettings) return;
    setSavingGuild(true);
    try {
      await saveGuildSettings(activeGuildId, guildSettings);
      showNotification('Ajustes del servidor guardados', 'success');
    } catch (e) {
      showNotification(`Error guardando los ajustes del servidor: ${e}`, 'error');
    } finally {
      setSavingGuild(false);
    }
  };
  const [showToken, setShowToken] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);

  const uiSettings = useUiSettings();
  const setUiSettings = useStore((s) => s.setUiSettings);
  const setConfig = useStore((s) => s.setConfig);

  // El token solo vive en este estado local: nunca se persiste en la UI.
  // El backend lo cifra (AES-GCM) al recibirlo vía add_bot/update_bot.
  const [token, setToken] = useState('');
  const hasSavedBot = config.bots.length > 0;
  const [adding, setAdding] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);
  // Avisos (no errores) por bot, p. ej. falta el intent de miembros.
  const [avisos, setAvisos] = useState<Record<string, string>>({});
  const setAviso = (id: string, aviso: string | null) =>
    setAvisos((prev) => {
      const next = { ...prev };
      if (aviso) next[id] = aviso;
      else delete next[id];
      return next;
    });

  const [formData, setFormData] = useState({
    server_id: config.active_guild_id ?? '',
    auto_start: uiSettings.auto_start,
    minimize_to_tray: uiSettings.minimize_to_tray,
  });

  useEffect(() => {
    setFormData({
      server_id: config.active_guild_id ?? '',
      auto_start: uiSettings.auto_start,
      minimize_to_tray: uiSettings.minimize_to_tray,
    });
  }, [config, uiSettings]);

  const handleChange = (field: string, value: any) => {
    setFormData(prev => ({ ...prev, [field]: value }));
  };

  const persist = async () => {
    const next = { ...config, active_guild_id: formData.server_id || null };
    await saveConfig(next);
    if (token) {
      // El backend valida el token con Discord y guarda id, nombre y avatar oficiales.
      const added = await addBot(token);
      setAviso(added.id, added.warning);
      setToken('');
    }
    // El backend es la fuente de verdad (ids de bots, token ya cifrado)
    setConfig(await getConfig());
    setUiSettings({
      auto_start: formData.auto_start,
      minimize_to_tray: formData.minimize_to_tray,
    });
  };

  const handleTheme = async (ui_theme: string, ui_mode: string) => {
    const previo = { ui_theme: config.ui_theme, ui_mode: config.ui_mode };
    setConfig({ ui_theme, ui_mode });
    aplicarTema(ui_theme, ui_mode);
    try {
      await saveConfig({ ...config, ui_theme, ui_mode });
    } catch (e) {
      setConfig(previo);
      showNotification(typeof e === 'string' ? e : 'No se pudo guardar el tema', 'error');
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await persist();
      showNotification('Configuración guardada', 'success');
    } catch (e) {
      showNotification(typeof e === 'string' ? e : 'Error al guardar', 'error');
    } finally {
      setSaving(false);
    }
  };

  const handleTestConnection = async () => {
    if (!token && !hasSavedBot) {
      showNotification('Ingresa un token primero', 'warning');
      return;
    }
    setTesting(true);
    setTestResult(null);
    try {
      await persist();
      const started = await startBot();
      if (config.active_bot_id) setAviso(config.active_bot_id, started.warning);
      const status = await getBotStatus();
      if (status.connected) {
        setTestResult('success');
        showNotification('Conexión exitosa', 'success');
      } else {
        setTestResult('error');
        showNotification('Token inválido o error de conexión', 'error');
      }
    } catch {
      setTestResult('error');
      showNotification('Error de conexión', 'error');
    } finally {
      setTesting(false);
    }
  };

  const handleAddBot = async () => {
    if (!token.trim()) {
      showNotification('Ingresa un token primero', 'warning');
      return;
    }
    setAdding(true);
    try {
      // El backend valida el token con Discord y guarda nombre y avatar oficiales.
      const bot = await addBot(token);
      setToken('');
      setAviso(bot.id, bot.warning);
      setConfig(await getConfig());
      showNotification(`Bot añadido: ${bot.name}`, 'success');
      if (bot.warning) showNotification(bot.warning, 'warning');
    } catch (e) {
      showNotification(typeof e === 'string' ? e : 'No se pudo añadir el bot', 'error');
    } finally {
      setAdding(false);
    }
  };

  const handleRemoveBot = async (id: string) => {
    try {
      await deleteBot(id);
      setConfig(await getConfig());
      showNotification('Bot quitado', 'success');
    } catch (e) {
      showNotification(typeof e === 'string' ? e : 'No se pudo quitar el bot', 'error');
    } finally {
      setConfirmRemove(null);
    }
  };

  const handleInvite = async () => {
    try {
      await openUrl(await inviteUrl());
    } catch (e) {
      showNotification(typeof e === 'string' ? e : 'No se pudo abrir el enlace de invitación', 'error');
    }
  };

  const copyToken = () => {
    navigator.clipboard.writeText(token);
    showNotification('Token copiado', 'info');
  };

  return (
    <div className="settings-view">
      <div className="view-header">
        <h2>CONFIGURACIÓN</h2>
      </div>

      <div className="settings-grid">
        <section className="settings-section">
          <h3><Key size={18} /> AUTENTICACIÓN DISCORD</h3>
          <div className="form-group">
            <label>Bot Token</label>
            <div className="input-with-actions">
              <input
                type={showToken ? 'text' : 'password'}
                value={token}
                onChange={(e) => setToken(e.target.value)}
                placeholder="Token del bot a añadir"
              />
              <button className="icon-btn" onClick={() => setShowToken(!showToken)}>
                {showToken ? <EyeOff size={16} /> : <Eye size={16} />}
              </button>
              <button className="icon-btn" onClick={copyToken} title="Copiar token">
                <Copy size={16} />
              </button>
            </div>
            <p className="help-text">Token del bot desde Discord Developer Portal → Bot → Reset Token. El nombre y el avatar se leen de Discord.</p>
            <button className="btn-secondary" onClick={handleAddBot} disabled={adding || !token.trim()}>
              {adding ? <Loader2 size={14} className="spin" /> : <Plus size={14} />}
              AÑADIR BOT
            </button>
          </div>

          {config.bots.map((b) => (
            <div key={b.id} className="form-group">
              <label>{b.name}{b.id === config.active_bot_id ? ' (activo)' : ''}</label>
              {avisos[b.id] && (
                <p className="help-text" style={{ color: 'var(--warning, #f5a623)' }}>
                  <AlertCircle size={14} /> {avisos[b.id]}
                </p>
              )}
              {confirmRemove === b.id ? (
                <div className="input-with-actions">
                  <span>¿Quitar el bot «{b.name}»? Se borrará su token guardado.</span>
                  <button className="btn-secondary" onClick={() => handleRemoveBot(b.id)}>CONFIRMAR</button>
                  <button className="btn-secondary" onClick={() => setConfirmRemove(null)}>CANCELAR</button>
                </div>
              ) : (
                <button className="btn-secondary" onClick={() => setConfirmRemove(b.id)}>
                  <Trash2 size={14} /> QUITAR BOT
                </button>
              )}
            </div>
          ))}

          <div className="form-group">
            <button className="btn-secondary" onClick={handleInvite} disabled={!config.active_bot_id}>
              <Server size={14} /> Invitar a un servidor
            </button>
            <p className="help-text">Abre en el navegador el enlace de Discord para añadir el bot activo a uno de tus servidores.</p>
          </div>

          <div className="form-group">
            <label>Server ID (Guild ID)</label>
            <div className="input-with-actions">
              <input
                type="text"
                value={formData.server_id}
                onChange={(e) => handleChange('server_id', e.target.value)}
                placeholder="ID del servidor"
              />
              <button className="icon-btn" onClick={() => { navigator.clipboard.writeText(formData.server_id); showNotification('Server ID copiado', 'info'); }}>
                <Copy size={16} />
              </button>
            </div>
            <p className="help-text">ID del servidor donde opera el bot (click derecho → Copy ID en Discord)</p>
          </div>

          <div className="test-connection">
            <button className="btn-secondary" onClick={handleTestConnection} disabled={testing || (!token && !hasSavedBot)}>
              {testing ? (
                <>
                  <Loader2 size={16} className="spin" />
                  PROBANDO...
                </>
              ) : (
                <>
                  <Server size={14} />
                  PROBAR CONEXIÓN
                </>
              )}
            </button>
            {testResult === 'success' && (
              <div className="test-success">
                <CheckCircle size={16} /> Conexión exitosa - Bot online en gateway
              </div>
            )}
            {testResult === 'error' && (
              <div className="test-error">
                <AlertCircle size={16} /> Token inválido o error de gateway
              </div>
            )}
          </div>
        </section>

        <section className="settings-section">
          <h3><ToggleLeft size={18} /> COMPORTAMIENTO</h3>
          <div className="toggles">
            <label className="toggle-label">
              <input
                type="checkbox"
                checked={formData.auto_start}
                onChange={(e) => handleChange('auto_start', e.target.checked)}
              />
              <ToggleRight size={20} />
              <span>Auto-iniciar bot al abrir ARGUS</span>
            </label>
            <label className="toggle-label">
              <input
                type="checkbox"
                checked={formData.minimize_to_tray}
                onChange={(e) => handleChange('minimize_to_tray', e.target.checked)}
              />
              <ToggleRight size={20} />
              <span>Minimizar a bandeja del sistema al cerrar ventana</span>
            </label>
          </div>
        </section>

        <section className="settings-section">
          <h3><Palette size={18} /> APARIENCIA</h3>
          <div className="form-group">
            {TEMAS.map((t) => {
              const activo = (config.ui_theme || 'discord') === t.id;
              const variantes = [{ modo: '', nombre: 'Base', m: t.muestra }];
              if (t.modo && t.muestraModo) variantes.push({ modo: t.modo, nombre: t.modoNombre ?? t.modo, m: t.muestraModo });
              return (
                <div key={t.id} className="input-with-actions">
                  <strong style={{ minWidth: 100 }}>{t.nombre}{activo ? ' (activo)' : ''}</strong>
                  {variantes.map((v) => {
                    const seleccionado = activo && (config.ui_mode || '') === v.modo;
                    return (
                      <button
                        key={v.modo || 'base'}
                        className="btn-secondary"
                        aria-pressed={seleccionado}
                        title={`${t.nombre} · ${v.nombre}`}
                        onClick={() => handleTheme(t.id, v.modo)}
                        style={{ background: v.m.bg, color: v.m.text, border: `2px solid ${seleccionado ? v.m.accent : 'transparent'}` }}
                      >
                        <span style={{ display: 'inline-block', width: 12, height: 12, borderRadius: '50%', background: v.m.accent, marginRight: 6 }} />
                        {v.nombre}
                      </button>
                    );
                  })}
                </div>
              );
            })}
            <p className="help-text">El tema se aplica al instante y se guarda en la configuración.</p>
          </div>
        </section>

        <section className="settings-section">
          <h3><Server size={18} /> ESTE SERVIDOR</h3>
          {!activeGuildId ? (
            <p>Elige un servidor en la cabecera para configurarlo.</p>
          ) : !guildSettings ? (
            <p>Cargando ajustes del servidor…</p>
          ) : (
            <>
              <div className="form-group">
                <label>Alias</label>
                <input
                  type="text"
                  value={guildSettings.alias ?? ''}
                  placeholder="Nombre propio para este servidor"
                  onChange={(e) => handleGuildChange({ alias: e.target.value || null })}
                />
              </div>
              <div className="form-group">
                <label>Notas</label>
                <textarea
                  value={guildSettings.notes}
                  rows={3}
                  onChange={(e) => handleGuildChange({ notes: e.target.value })}
                />
              </div>
              <div className="toggles">
                <label className="toggle-label">
                  <input
                    type="checkbox"
                    checked={guildSettings.vigilance_enabled}
                    onChange={(e) => handleGuildChange({ vigilance_enabled: e.target.checked })}
                  />
                  <ToggleRight size={20} />
                  <span>Vigilancia</span>
                </label>
                <label className="toggle-label">
                  <input
                    type="checkbox"
                    checked={guildSettings.personality_enabled}
                    onChange={(e) => handleGuildChange({ personality_enabled: e.target.checked })}
                  />
                  <ToggleRight size={20} />
                  <span>Personalidad</span>
                </label>
                <label className="toggle-label">
                  <input
                    type="checkbox"
                    checked={guildSettings.templates_enabled}
                    onChange={(e) => handleGuildChange({ templates_enabled: e.target.checked })}
                  />
                  <ToggleRight size={20} />
                  <span>Plantillas</span>
                </label>
              </div>
              <button className="btn btn-primary" onClick={handleSaveGuild} disabled={savingGuild}>
                {savingGuild ? <Loader2 size={16} className="spin" /> : <Save size={16} />} Guardar ajustes del servidor
              </button>
            </>
          )}
        </section>

        <section className="settings-section">
          <h3><Save size={18} /> ACCIONES</h3>
          <div className="settings-actions">
            <button className="btn-primary" onClick={handleSave} disabled={saving}>
              {saving ? (
                <>
                  <Loader2 size={16} className="spin" />
                  GUARDANDO...
                </>
              ) : (
                <>
                  <Save size={14} />
                  GUARDAR CONFIGURACIÓN
                </>
              )}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
};