import React, { useEffect, useState } from 'react';
import { TriangleAlert } from 'lucide-react';
import { useActiveGuildId, useShowNotification } from '../store/useStore';
import { listarAgentesIa, getGuildSettings, saveGuildSettings, listChannels } from '../api/tauri';
import type { AgenteIa, Channel, GuildSettings } from '../types';
import { Button, Cargando, EstadoVacio, ErrorPanel, Input, Select } from './ui';
import { t } from '../i18n';
import './Asistente.css';

const TIPOS_TEXTO = [0, 5];
const CONTEXTO_MAX = 30;
const mensajeError = (e: unknown, defecto: string) => (typeof e === 'string' ? e : e instanceof Error ? e.message : defecto);
/** Los modelos gratis de opencode: el proveedor `opencode/` o los que terminan en `-free`. */
const esGratis = (agente: string, modelo: string) => agente === 'opencode' && (modelo.startsWith('opencode/') || modelo.endsWith('-free'));

export const RespuestasDiscord: React.FC = () => {
  const guildId = useActiveGuildId();
  const showNotification = useShowNotification();

  const [agentes, setAgentes] = useState<AgenteIa[] | null>(null);
  const [canales, setCanales] = useState<Channel[]>([]);
  const [ajustes, setAjustes] = useState<GuildSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [guardando, setGuardando] = useState(false);
  const [sucio, setSucio] = useState(false);

  useEffect(() => {
    if (!guildId) return;
    let vigente = true;
    setAjustes(null);
    setError(null);
    setSucio(false);
    (async () => {
      try {
        const [a, s, c] = await Promise.all([listarAgentesIa(), getGuildSettings(guildId), listChannels(guildId)]);
        if (!vigente) return;
        setAgentes(a);
        setAjustes(s);
        setCanales(c.filter((x) => TIPOS_TEXTO.includes(x.channel_type)).sort((x, y) => x.position - y.position));
      } catch (e) {
        if (vigente) setError(mensajeError(e, t('Could not load the Discord reply settings')));
      }
    })();
    return () => { vigente = false; };
  }, [guildId]);

  if (!guildId) {
    return <EstadoVacio titulo={t('No server selected')} descripcion={t('Pick a server in the left column to set up its Discord replies.')} />;
  }
  if (error) return <ErrorPanel mensaje={error} />;
  if (!ajustes || !agentes) return <Cargando lineas={4} />;
  if (agentes.length === 0) {
    return <EstadoVacio titulo={t('No AI agent installed')} descripcion={t('The assistant needs the claude or opencode command line. Install one of them, sign in with it, and press Check again.')} />;
  }

  const cambiar = (parte: Partial<GuildSettings>) => { setAjustes({ ...ajustes, ...parte }); setSucio(true); };
  const agenteActual = agentes.find((a) => a.agente === ajustes.chat_agente) ?? null;
  const modelos = agenteActual?.modelos ?? [];
  const gratis = modelos.filter((m) => esGratis(ajustes.chat_agente, m));
  const resto = modelos.filter((m) => !esGratis(ajustes.chat_agente, m));

  const cambiarAgente = (nombre: string) => {
    const a = agentes.find((x) => x.agente === nombre);
    const lista = a?.modelos ?? [];
    cambiar({ chat_agente: nombre as GuildSettings['chat_agente'], chat_modelo: lista.find((m) => esGratis(nombre, m)) ?? lista[0] ?? '' });
  };
  const alternarCanal = (id: string) => cambiar({
    chat_canales: ajustes.chat_canales.includes(id) ? ajustes.chat_canales.filter((x) => x !== id) : [...ajustes.chat_canales, id],
  });
  const numero = (v: string, min: number, max: number, defecto: number) => {
    const n = Math.round(Number(v));
    return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : defecto;
  };

  const guardar = async () => {
    setGuardando(true);
    try {
      await saveGuildSettings(guildId, ajustes);
      setSucio(false);
      showNotification(t('Discord reply settings saved'), 'success');
    } catch (e) {
      showNotification(mensajeError(e, t('Could not save the Discord reply settings')), 'error');
    } finally {
      setGuardando(false);
    }
  };

  return (
    <div className="vista vista--llena asistente" style={{ overflow: 'auto' }}>
      <section className="asistente__panel" style={{ flex: '0 0 auto' }} aria-label={t('Discord replies')}>
        <header className="asistente__cabecera"><h3>{t('Discord replies')}</h3></header>

        <div className="asistente__aviso" role="note">
          <TriangleAlert size={16} />
          <span>
            {t('The bot only replies while ARGUS is open.')} {t('Conversations are sent to the model provider of the chosen agent.')}
          </span>
        </div>

        <label style={{ display: 'flex', alignItems: 'center', gap: 'var(--sp-2)' }}>
          <input type="checkbox" role="switch" checked={ajustes.personality_enabled} onChange={(e) => cambiar({ personality_enabled: e.target.checked })} />
          <span>{t('Reply on Discord when the bot is mentioned or replied to')}</span>
        </label>

        <div className="asistente__acciones">
          <Select etiqueta={t('Agent')} value={ajustes.chat_agente} onChange={(e) => cambiarAgente(e.target.value)}>
            {agentes.map((a) => <option key={a.agente} value={a.agente}>{a.agente}</option>)}
          </Select>
          {modelos.length > 0 ? (
            <Select etiqueta={t('Model')} value={ajustes.chat_modelo} onChange={(e) => cambiar({ chat_modelo: e.target.value })}>
              {ajustes.chat_modelo && !modelos.includes(ajustes.chat_modelo) && <option value={ajustes.chat_modelo}>{ajustes.chat_modelo}</option>}
              {gratis.length > 0 && (
                <optgroup label={t('Free models')}>
                  {gratis.map((m) => <option key={m} value={m}>{`★ ${m} (${t('free')})`}</option>)}
                </optgroup>
              )}
              {gratis.length > 0 ? <optgroup label={t('Other models')}>{resto.map((m) => <option key={m} value={m}>{m}</option>)}</optgroup> : resto.map((m) => <option key={m} value={m}>{m}</option>)}
            </Select>
          ) : (
            <Input etiqueta={t('Model')} value={ajustes.chat_modelo} placeholder={t('Model name')} onChange={(e) => cambiar({ chat_modelo: e.target.value })} />
          )}
        </div>
        {gratis.length > 0 && <small>{t('Tip: a free opencode model is enough to chat.')}</small>}

        <label>
          <span>{t('Personality')}</span>
          <textarea
            className="ui-control"
            rows={6}
            value={ajustes.chat_personalidad}
            aria-label={t('Personality')}
            placeholder={t('Example: You are Luna, the friendly assistant of this server. Be brief and cheerful. Never give medical advice.')}
            onChange={(e) => cambiar({ chat_personalidad: e.target.value })}
          />
        </label>

        <fieldset style={{ border: 0, padding: 0, margin: 0 }}>
          <legend>{t('Allowed channels')}</legend>
          <small>{t('None selected = the bot replies in all channels.')}</small>
          {canales.length === 0 ? (
            <p>{t('This server has no text channels.')}</p>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--sp-1)', maxHeight: 200, overflow: 'auto' }}>
              {canales.map((c) => (
                <label key={c.id} style={{ display: 'flex', alignItems: 'center', gap: 'var(--sp-2)' }}>
                  <input type="checkbox" checked={ajustes.chat_canales.includes(c.id)} onChange={() => alternarCanal(c.id)} />
                  <span>#{c.name}</span>
                </label>
              ))}
            </div>
          )}
        </fieldset>

        <div className="asistente__acciones">
          <Input
            etiqueta={t('Wait per user (seconds)')}
            type="number" min={0} value={ajustes.chat_espera_s}
            onChange={(e) => cambiar({ chat_espera_s: numero(e.target.value, 0, 3600, 10) })}
          />
          <Input
            etiqueta={t('Context messages')}
            ayuda={t('Previous channel messages passed to the agent (max {max}).', { max: CONTEXTO_MAX })}
            type="number" min={0} max={CONTEXTO_MAX} value={ajustes.chat_contexto}
            onChange={(e) => cambiar({ chat_contexto: numero(e.target.value, 0, CONTEXTO_MAX, 10) })}
          />
        </div>

        <div className="asistente__acciones">
          <Button onClick={() => { void guardar(); }} disabled={!sucio || guardando}>{guardando ? t('Saving…') : t('Save settings')}</Button>
        </div>
      </section>
    </div>
  );
};
