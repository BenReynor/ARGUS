import React, { useState, useEffect } from 'react';
import { useConfig, useSetActiveTab, useShowNotification } from '../store/useStore';
import { getConfig, saveConfig, startBot, getBotStatus } from '../api/tauri';
import { Save, Key, Server, ToggleLeft, ToggleRight, CheckCircle, AlertCircle, Loader2, Eye, EyeOff, Copy } from 'lucide-react';

export const SettingsView: React.FC = () => {
  const config = useConfig();
  const setActiveTab = useSetActiveTab();
  const showNotification = useShowNotification();
  const [showToken, setShowToken] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);

  const [formData, setFormData] = useState({
    discord_token: config.discord_token,
    server_id: config.server_id,
    auto_start: config.auto_start,
    minimize_to_tray: config.minimize_to_tray,
  });

  useEffect(() => {
    setFormData({
      discord_token: config.discord_token,
      server_id: config.server_id,
      auto_start: config.auto_start,
      minimize_to_tray: config.minimize_to_tray,
    });
  }, [config]);

  const handleChange = (field: string, value: any) => {
    setFormData(prev => ({ ...prev, [field]: value }));
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await saveConfig(formData);
      showNotification('Configuración guardada', 'success');
    } catch {
      showNotification('Error al guardar', 'error');
    } finally {
      setSaving(false);
    }
  };

  const handleTestConnection = async () => {
    if (!formData.discord_token) {
      showNotification('Ingresa un token primero', 'warning');
      return;
    }
    setTesting(true);
    setTestResult(null);
    try {
      await startBot();
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

  const copyToken = () => {
    navigator.clipboard.writeText(formData.discord_token);
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
                value={formData.discord_token}
                onChange={(e) => handleChange('discord_token', e.target.value)}
                placeholder="MTU2MDE0MTU0NDE4MTMzNDEzOA.GJ58RC..."
              />
              <button className="icon-btn" onClick={() => setShowToken(!showToken)}>
                {showToken ? <EyeOff size={16} /> : <Eye size={16} />}
              </button>
              <button className="icon-btn" onClick={copyToken} title="Copiar token">
                <Copy size={16} />
              </button>
            </div>
            <p className="help-text">Token del bot desde Discord Developer Portal → Bot → Reset Token</p>
          </div>

          <div className="form-group">
            <label>Server ID (Guild ID)</label>
            <div className="input-with-actions">
              <input
                type="text"
                value={formData.server_id}
                onChange={(e) => handleChange('server_id', e.target.value)}
                placeholder="1541513296375717921"
              />
              <button className="icon-btn" onClick={() => { navigator.clipboard.writeText(formData.server_id); showNotification('Server ID copiado', 'info'); }}>
                <Copy size={16} />
              </button>
            </div>
            <p className="help-text">ID del servidor donde opera el bot (click derecho → Copy ID en Discord)</p>
          </div>

          <div className="test-connection">
            <button className="btn-secondary" onClick={handleTestConnection} disabled={testing || !formData.discord_token}>
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