import React, { useState, useEffect } from 'react';
import { useStore, useConfig, useUiSettings, useSetActiveTab, useShowNotification, useActiveGuildId, useGuildSettings } from '../store/useStore';
import { openUrl } from '@tauri-apps/plugin-opener';
import { getConfig, saveConfig, saveGuildSettings, addBot, deleteBot, startBot, getBotStatus, inviteUrl } from '../api/tauri';
import { Button, Input, Card } from './ui';
import './Vistas.css';
import { Save, Trash2, Plus, Server, CheckCircle, AlertCircle, Eye, EyeOff, Copy } from 'lucide-react';
import { TEMAS, aplicarTema } from '../temas';

const Seccion: React.FC<{ titulo: string; descripcion: string; children: React.ReactNode }> = ({ titulo, descripcion, children }) => (
  <Card className="ajustes__seccion">
    <header>
      <h3>{titulo}</h3>
      <p>{descripcion}</p>
    </header>
    {children}
  </Card>
);

const Interruptor: React.FC<{ checked: boolean; onChange: (v: boolean) => void; titulo: string; ayuda: string }> = ({ checked, onChange, titulo, ayuda }) => (
  <label className="ajustes__interruptor">
    <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    <span>
      {titulo}
      <small>{ayuda}</small>
    </span>
  </label>
);

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
  const [tokenError, setTokenError] = useState('');
  const [serverIdError, setServerIdError] = useState('');
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
    const next = { ...config, active_guild_id: formData.server_id.trim() || null };
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
    const id = formData.server_id.trim();
    if (id && !/^\d+$/.test(id)) {
      setServerIdError('El ID del servidor solo contiene números.');
      return;
    }
    setServerIdError('');
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
      setTokenError('Ingresa el token del bot.');
      return;
    }
    setTokenError('');
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
      setTokenError(typeof e === 'string' ? e : 'No se pudo añadir el bot. Revisa el token.');
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
    <div className="vista ajustes">
      <div className="vista__cabecera">
        <h2>Ajustes</h2>
      </div>

      <Seccion titulo="Bots" descripcion="Registra los bots propios que ARGUS puede usar y elige con cuál trabajar.">
        <div className="ajustes__fila">
          <Input
            etiqueta="Token del bot"
            ayuda="Lo encuentras en Discord Developer Portal → Bot → Reset Token. El nombre y el avatar se leen de Discord."
            error={tokenError}
            type={showToken ? 'text' : 'password'}
            value={token}
            onChange={(e) => { setToken(e.target.value); setTokenError(''); }}
            placeholder="Token del bot a añadir"
            autoComplete="off"
          />
          <button className="icon-btn" aria-label={showToken ? 'Ocultar token' : 'Mostrar token'} onClick={() => setShowToken(!showToken)}>
            {showToken ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
          <button className="icon-btn" onClick={copyToken} title="Copiar token" aria-label="Copiar token" disabled={!token}>
            <Copy size={16} />
          </button>
        </div>
        <div>
          <Button variante="secundario" onClick={handleAddBot} cargando={adding} disabled={!token.trim()}>
            {!adding && <Plus size={14} />} Añadir bot
          </Button>
        </div>

        {config.bots.map((b) => (
          <div key={b.id} className="ajustes__fila">
            <strong>{b.name}{b.id === config.active_bot_id ? ' (activo)' : ''}</strong>
            {avisos[b.id] && (
              <span className="ajustes__aviso"><AlertCircle size={14} /> {avisos[b.id]}</span>
            )}
            {confirmRemove === b.id ? (
              <>
                <span>¿Quitar el bot «{b.name}»? Se borrará su token guardado.</span>
                <Button variante="peligro" onClick={() => handleRemoveBot(b.id)}>Confirmar</Button>
                <Button variante="secundario" onClick={() => setConfirmRemove(null)}>Cancelar</Button>
              </>
            ) : (
              <Button variante="secundario" onClick={() => setConfirmRemove(b.id)}>
                <Trash2 size={14} /> Quitar bot
              </Button>
            )}
          </div>
        ))}

        <div className="ajustes__fila">
          <Button variante="secundario" onClick={handleInvite} disabled={!config.active_bot_id}>
            <Server size={14} /> Invitar a un servidor
          </Button>
          <Button variante="secundario" onClick={handleTestConnection} cargando={testing} disabled={!token && !hasSavedBot}>
            {!testing && <Server size={14} />} {testing ? 'Probando…' : 'Probar conexión'}
          </Button>
        </div>
        <p className="ajustes__ayuda">Invitar abre en el navegador el enlace de Discord para añadir el bot activo a uno de tus servidores.</p>
        {testResult === 'success' && (
          <div className="ajustes__ok"><CheckCircle size={16} /> Conexión exitosa: bot online en el gateway.</div>
        )}
        {testResult === 'error' && (
          <div className="ajustes__error"><AlertCircle size={16} /> Token inválido o error de gateway.</div>
        )}
      </Seccion>

      <Seccion titulo="Este servidor" descripcion="Ajustes propios de ARGUS para el servidor activo.">
        {!activeGuildId ? (
          <p className="ajustes__ayuda">Elige un servidor en la cabecera para configurarlo.</p>
        ) : !guildSettings ? (
          <p className="ajustes__ayuda">Cargando ajustes del servidor…</p>
        ) : (
          <>
            <Input
              etiqueta="Alias"
              ayuda="Nombre propio para reconocer este servidor en ARGUS."
              error={(guildSettings.alias ?? '').length > 60 ? 'El alias no puede superar 60 caracteres.' : undefined}
              type="text"
              value={guildSettings.alias ?? ''}
              onChange={(e) => handleGuildChange({ alias: e.target.value || null })}
            />
            <div className="ui-campo">
              <label htmlFor="ajustes-notas">Notas</label>
              <textarea
                id="ajustes-notas"
                className="ui-control"
                value={guildSettings.notes}
                rows={3}
                onChange={(e) => handleGuildChange({ notes: e.target.value })}
              />
              <span className="ui-campo__ayuda">Apuntes privados; no se envían a Discord.</span>
            </div>
            <Interruptor titulo="Vigilancia" ayuda="Registrar eventos del servidor." checked={guildSettings.vigilance_enabled} onChange={(v) => handleGuildChange({ vigilance_enabled: v })} />
            <Interruptor titulo="Personalidad" ayuda="Permitir que el bot responda con su personalidad." checked={guildSettings.personality_enabled} onChange={(v) => handleGuildChange({ personality_enabled: v })} />
            <Interruptor titulo="Plantillas" ayuda="Permitir aplicar plantillas de estructura." checked={guildSettings.templates_enabled} onChange={(v) => handleGuildChange({ templates_enabled: v })} />
            <div>
              <Button onClick={handleSaveGuild} cargando={savingGuild} disabled={(guildSettings.alias ?? '').length > 60}>
                {!savingGuild && <Save size={16} />} Guardar ajustes del servidor
              </Button>
            </div>
          </>
        )}
      </Seccion>

      <Seccion titulo="Apariencia" descripcion="Elige el tema y el modo de la interfaz. Se aplica al instante y se guarda.">
        {TEMAS.map((t) => {
          const activo = (config.ui_theme || 'discord') === t.id;
          const variantes = [{ modo: '', nombre: 'Base', muestra: t.id }];
          if (t.modo) variantes.push({ modo: t.modo, nombre: t.modoNombre ?? t.modo, muestra: `${t.id}-${t.modo}` });
          return (
            <div key={t.id} className="ajustes__fila">
              <strong style={{ minWidth: 100 }}>{t.nombre}{activo ? ' (activo)' : ''}</strong>
              {variantes.map((v) => {
                const seleccionado = activo && (config.ui_mode || '') === v.modo;
                return (
                  <button
                    key={v.modo || 'base'}
                    className="btn-secondary muestra-tema"
                    data-muestra={v.muestra}
                    aria-pressed={seleccionado}
                    title={`${t.nombre} · ${v.nombre}`}
                    onClick={() => handleTheme(t.id, v.modo)}
                  >
                    <span className="muestra-punto" />
                    {v.nombre}
                  </button>
                );
              })}
            </div>
          );
        })}
      </Seccion>

      <Seccion titulo="General" descripcion="Comportamiento de la aplicación y servidor con el que arranca.">
        <Input
          etiqueta="ID del servidor activo"
          ayuda="Clic derecho en el servidor → Copiar ID (con el modo desarrollador de Discord activo)."
          error={serverIdError}
          type="text"
          value={formData.server_id}
          onChange={(e) => { handleChange('server_id', e.target.value); setServerIdError(''); }}
          placeholder="ID del servidor"
        />
        <Interruptor titulo="Auto-iniciar el bot al abrir ARGUS" ayuda="Conecta el bot activo en cuanto arranca la aplicación." checked={formData.auto_start} onChange={(v) => handleChange('auto_start', v)} />
        <Interruptor titulo="Minimizar a la bandeja al cerrar la ventana" ayuda="ARGUS sigue funcionando en segundo plano." checked={formData.minimize_to_tray} onChange={(v) => handleChange('minimize_to_tray', v)} />
        <div>
          <Button onClick={handleSave} cargando={saving}>
            {!saving && <Save size={14} />} {saving ? 'Guardando…' : 'Guardar configuración'}
          </Button>
        </div>
      </Seccion>
    </div>
  );
};

