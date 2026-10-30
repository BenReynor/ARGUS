import React, { useState, useEffect } from 'react';
import { useStore, useConfig, useUiSettings, useSetActiveTab, useShowNotification, useActiveGuildId, useGuildSettings } from '../store/useStore';
import { openUrl } from '@tauri-apps/plugin-opener';
import { getConfig, saveConfig, saveGuildSettings, addBot, deleteBot, startBot, getBotStatus, inviteUrl, rutaMcp } from '../api/tauri';
import { t, IDIOMAS } from '../i18n';
import { Button, Input, Select, Card, Cargando, EstadoVacio } from './ui';
import './Vistas.css';
import { Save, Trash2, Plus, Server, CheckCircle, AlertCircle, Eye, EyeOff, Copy } from 'lucide-react';
import { TEMAS, aplicarTema } from '../temas';

const Seccion: React.FC<{ titulo: string; descripcion: string; ancha?: boolean; children: React.ReactNode }> = ({ titulo, descripcion, ancha, children }) => (
  <Card className={`ajustes__seccion${ancha ? ' ajustes__seccion--ancha' : ''}`}>
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

const nombreTema = (id: string): string => {
  switch (id) {
    case 'minimal': return t('Minimal');
    case 'discord': return t('Discord');
    case 'cyberpunk': return t('Cyberpunk');
    case 'calido': return t('Warm');
    case 'datos': return t('Data');
    case 'anime': return t('Anime');
    case 'manga': return t('Manga');
    case 'glass': return t('Glass');
    case 'retro': return t('Retro');
    case 'brutalista': return t('Brutalist');
    default: return id;
  }
};

const nombreModo = (modo: string): string => {
  switch (modo) {
    case 'claro': return t('Light');
    case 'noche': return t('Night');
    case 'noir': return t('Noir');
    default: return modo;
  }
};

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
      showNotification(t('Server settings saved'), 'success');
    } catch (e) {
      showNotification(t('Error saving the server settings: {error}', { error: String(e) }), 'error');
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
  const setUiLanguage = useStore((s) => s.setUiLanguage);

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
      showNotification(typeof e === 'string' ? e : t('Could not save the theme'), 'error');
    }
  };

  const handleSave = async () => {
    const id = formData.server_id.trim();
    if (id && !/^\d+$/.test(id)) {
      setServerIdError(t('The server ID contains only digits.'));
      return;
    }
    setServerIdError('');
    setSaving(true);
    try {
      await persist();
      showNotification(t('Settings saved'), 'success');
    } catch (e) {
      showNotification(typeof e === 'string' ? e : t('Error while saving'), 'error');
    } finally {
      setSaving(false);
    }
  };

  const handleTestConnection = async () => {
    if (!token && !hasSavedBot) {
      showNotification(t('Enter a token first'), 'warning');
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
        showNotification(t('Connection successful'), 'success');
      } else {
        setTestResult('error');
        showNotification(t('Invalid token or connection error'), 'error');
      }
    } catch {
      setTestResult('error');
      showNotification(t('Connection error'), 'error');
    } finally {
      setTesting(false);
    }
  };

  const handleAddBot = async () => {
    if (!token.trim()) {
      setTokenError(t('Enter the bot token.'));
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
      showNotification(t('Bot added: {name}', { name: bot.name }), 'success');
      if (bot.warning) showNotification(bot.warning, 'warning');
    } catch (e) {
      setTokenError(typeof e === 'string' ? e : t('Could not add the bot. Check the token.'));
    } finally {
      setAdding(false);
    }
  };

  const handleRemoveBot = async (id: string) => {
    try {
      await deleteBot(id);
      setConfig(await getConfig());
      showNotification(t('Bot removed'), 'success');
    } catch (e) {
      showNotification(typeof e === 'string' ? e : t('Could not remove the bot'), 'error');
    } finally {
      setConfirmRemove(null);
    }
  };

  const handleInvite = async () => {
    try {
      await openUrl(await inviteUrl());
    } catch (e) {
      showNotification(typeof e === 'string' ? e : t('Could not open the invite link'), 'error');
    }
  };

  const copyToken = () => {
    navigator.clipboard.writeText(token);
    showNotification(t('Token copied'), 'info');
  };

  const [rutaMcpBin, setRutaMcpBin] = useState('');
  useEffect(() => {
    rutaMcp()
      .then(setRutaMcpBin)
      .catch((e) => showNotification(typeof e === 'string' ? e : t('Could not locate argus-mcp'), 'error'));
  }, []);
  const comandoMcp = `claude mcp add argus -- ${/\s/.test(rutaMcpBin) ? JSON.stringify(rutaMcpBin) : rutaMcpBin}`;
  const jsonMcp = JSON.stringify({ mcpServers: { argus: { command: rutaMcpBin, args: [] } } }, null, 2);
  const copiar = async (texto: string, aviso: string) => {
    try {
      await navigator.clipboard.writeText(texto);
      showNotification(aviso, 'success');
    } catch {
      showNotification(t('Could not copy to the clipboard'), 'error');
    }
  };

  return (
    <div className="vista ajustes">
      <div className="vista__cabecera">
        <h2 className="vista__solo-lector">{t('Settings')}</h2>
      </div>

      <Seccion titulo={t('Bots')} descripcion={t('Register the bots ARGUS can use and choose which one to work with.')}>
        <div className="ajustes__fila">
          <Input
            etiqueta={t('Bot token')}
            ayuda={t('Find it in Discord Developer Portal → Bot → Reset Token. The name and avatar are read from Discord.')}
            error={tokenError}
            type={showToken ? 'text' : 'password'}
            value={token}
            onChange={(e) => { setToken(e.target.value); setTokenError(''); }}
            placeholder={t('Token of the bot to add')}
            autoComplete="off"
          />
          <button className="icon-btn" aria-label={showToken ? t('Hide token') : t('Show token')} onClick={() => setShowToken(!showToken)}>
            {showToken ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
          <button className="icon-btn" onClick={copyToken} title={t('Copy token')} aria-label={t('Copy token')} disabled={!token}>
            <Copy size={16} />
          </button>
        </div>
        <div>
          <Button variante="secundario" onClick={handleAddBot} cargando={adding} disabled={!token.trim()}>
            {!adding && <Plus size={14} />} {t('Add bot')}
          </Button>
        </div>

        {config.bots.map((b) => (
          <div key={b.id} className="ajustes__fila">
            <strong>{b.name}{b.id === config.active_bot_id ? ' ' + t('(active)') : ''}</strong>
            {avisos[b.id] && (
              <span className="ajustes__aviso"><AlertCircle size={14} /> {avisos[b.id]}</span>
            )}
            {confirmRemove === b.id ? (
              <>
                <span>{t('Remove the bot “{name}”? Its saved token will be deleted.', { name: b.name })}</span>
                <Button variante="peligro" onClick={() => handleRemoveBot(b.id)}>{t('Confirm')}</Button>
                <Button variante="secundario" onClick={() => setConfirmRemove(null)}>{t('Cancel')}</Button>
              </>
            ) : (
              <Button variante="secundario" onClick={() => setConfirmRemove(b.id)}>
                <Trash2 size={14} /> {t('Remove bot')}
              </Button>
            )}
          </div>
        ))}

        <div className="ajustes__fila">
          <Button variante="secundario" onClick={handleInvite} disabled={!config.active_bot_id}>
            <Server size={14} /> {t('Invite to a server')}
          </Button>
          <Button variante="secundario" onClick={handleTestConnection} cargando={testing} disabled={!token && !hasSavedBot}>
            {!testing && <Server size={14} />} {testing ? t('Testing…') : t('Test connection')}
          </Button>
        </div>
        <p className="ajustes__ayuda">{t('Invite opens the Discord link in your browser to add the active bot to one of your servers.')}</p>
        {testResult === 'success' && (
          <div className="ajustes__ok"><CheckCircle size={16} /> {t('Connection successful: bot online on the gateway.')}</div>
        )}
        {testResult === 'error' && (
          <div className="ajustes__error"><AlertCircle size={16} /> {t('Invalid token or gateway error.')}</div>
        )}
      </Seccion>

      <Seccion titulo={t('This server')} descripcion={t('ARGUS-specific settings for the active server.')}>
        {!activeGuildId ? (
          <EstadoVacio titulo={t('No server selected')} descripcion={t('Pick a server in the left column to configure it.')} />
        ) : !guildSettings ? (
          <Cargando lineas={3} />
        ) : (
          <>
            <Input
              etiqueta={t('Alias')}
              ayuda={t('Your own name to recognize this server in ARGUS.')}
              error={(guildSettings.alias ?? '').length > 60 ? t('The alias cannot exceed 60 characters.') : undefined}
              type="text"
              value={guildSettings.alias ?? ''}
              onChange={(e) => handleGuildChange({ alias: e.target.value || null })}
            />
            <div className="ui-campo">
              <label htmlFor="ajustes-notas">{t('Notes')}</label>
              <textarea
                id="ajustes-notas"
                className="ui-control"
                value={guildSettings.notes}
                rows={3}
                onChange={(e) => handleGuildChange({ notes: e.target.value })}
              />
              <span className="ui-campo__ayuda">{t('Private notes; they are not sent to Discord.')}</span>
            </div>
            <Interruptor titulo={t('Surveillance')} ayuda={t('Record server events.')} checked={guildSettings.vigilance_enabled} onChange={(v) => handleGuildChange({ vigilance_enabled: v })} />
            <Interruptor titulo={t('Personality')} ayuda={t('Let the bot reply with its personality.')} checked={guildSettings.personality_enabled} onChange={(v) => handleGuildChange({ personality_enabled: v })} />
            <Interruptor titulo={t('Templates')} ayuda={t('Allow applying structure templates.')} checked={guildSettings.templates_enabled} onChange={(v) => handleGuildChange({ templates_enabled: v })} />
            <div>
              <Button onClick={handleSaveGuild} cargando={savingGuild} disabled={(guildSettings.alias ?? '').length > 60}>
                {!savingGuild && <Save size={16} />} {t('Save server settings')}
              </Button>
            </div>
          </>
        )}
      </Seccion>

      <Seccion ancha titulo="MCP" descripcion={t('Connect Claude Code or another MCP client to your bots with the argus-mcp binary. It works with ARGUS closed.')}>
        {!rutaMcpBin ? (
          <Cargando lineas={2} />
        ) : (
          <>
            <div className="ajustes__codigo">
              <span className="ajustes__codigo-etiqueta">{t('Binary path')}</span>
              <code>{rutaMcpBin}</code>
            </div>
            <div className="ajustes__codigo">
              <div className="ajustes__codigo-cabecera">
                <span className="ajustes__codigo-etiqueta">Claude Code</span>
                <Button variante="secundario" onClick={() => copiar(comandoMcp, t('Command copied'))}>
                  <Copy size={14} /> {t('Copy')}
                </Button>
              </div>
              <pre><code>{comandoMcp}</code></pre>
            </div>
            <div className="ajustes__codigo">
              <div className="ajustes__codigo-cabecera">
                <span className="ajustes__codigo-etiqueta">{t('Other clients (JSON)')}</span>
                <Button variante="secundario" onClick={() => copiar(jsonMcp, t('JSON copied'))}>
                  <Copy size={14} /> {t('Copy')}
                </Button>
              </div>
              <pre><code>{jsonMcp}</code></pre>
            </div>
            <p className="ajustes__aviso">
              <AlertCircle size={14} /> {t('MCP can do anything the active bot’s permissions allow: use it only with trusted clients.')}
            </p>
          </>
        )}
      </Seccion>

      <Seccion ancha titulo={t('Appearance')} descripcion={t('Choose the interface theme and mode. It applies instantly and is saved.')}>
        <div className="temas-rejilla" role="radiogroup" aria-label={t('Interface theme')}>
          {TEMAS.flatMap((tema) => {
            const variantes = [{ modo: '', nombre: nombreTema(tema.id) }];
            if (tema.modo) variantes.push({ modo: tema.modo, nombre: `${nombreTema(tema.id)} · ${nombreModo(tema.modo)}` });
            return variantes.map((v) => {
              const seleccionado = (config.ui_theme || 'discord') === tema.id && (config.ui_mode || '') === v.modo;
              return (
                <button
                  key={`${tema.id}-${v.modo}`}
                  type="button"
                  role="radio"
                  aria-checked={seleccionado}
                  className="tema-opcion"
                  onClick={() => handleTheme(tema.id, v.modo)}
                >
                  <span className="tema-miniatura" data-tema={tema.id} data-modo={v.modo || undefined} aria-hidden="true">
                    <span className="sidebar tema-miniatura__lateral">
                      <span className="nav-item active">{t('Dashboard')}</span>
                      <span className="nav-item">{t('Channels')}</span>
                    </span>
                    <span className="tema-miniatura__cuerpo">
                      <span className="ui-card tema-miniatura__tarjeta">
                        <span className="tema-miniatura__titulo">{t('Server')}</span>
                        <span className="ui-badge">{t('Online')}</span>
                      </span>
                      <span className="ui-btn ui-btn--primario ui-btn--sm">{t('Action')}</span>
                    </span>
                  </span>
                  <span className="tema-opcion__nombre">
                    {seleccionado && <CheckCircle size={14} aria-hidden="true" />} {v.nombre}{seleccionado ? ' ' + t('(active)') : ''}
                  </span>
                </button>
              );
            });
          })}
        </div>
        <Select
          etiqueta="Language / Idioma"
          ayuda={t('Choose the language of the interface. It applies instantly and is saved.')}
          value={config.ui_language || 'en'}
          onChange={async (e) => {
            try {
              await setUiLanguage(e.target.value);
            } catch (err) {
              showNotification(typeof err === 'string' ? err : t('Could not save the language'), 'error');
            }
          }}
        >
          {IDIOMAS.map((i) => (
            <option key={i.id} value={i.id}>{i.nombre}</option>
          ))}
        </Select>
      </Seccion>

      <Seccion ancha titulo={t('General')} descripcion={t('Application behavior and the server it starts with.')}>
        <Input
          etiqueta={t('Active server ID')}
          ayuda={t('Right-click the server → Copy ID (with Discord developer mode on).')}
          error={serverIdError}
          type="text"
          value={formData.server_id}
          onChange={(e) => { handleChange('server_id', e.target.value); setServerIdError(''); }}
          placeholder={t('Server ID')}
        />
        <Interruptor titulo={t('Auto-start the bot when ARGUS opens')} ayuda={t('Connects the active bot as soon as the app starts.')} checked={formData.auto_start} onChange={(v) => handleChange('auto_start', v)} />
        <Interruptor titulo={t('Minimize to the tray when closing the window')} ayuda={t('ARGUS keeps running in the background.')} checked={formData.minimize_to_tray} onChange={(v) => handleChange('minimize_to_tray', v)} />
        <div>
          <Button onClick={handleSave} cargando={saving}>
            {!saving && <Save size={14} />} {saving ? t('Saving…') : t('Save settings')}
          </Button>
        </div>
      </Seccion>
    </div>
  );
};

