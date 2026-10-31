import React, { useEffect, useState } from 'react';
import { TriangleAlert } from 'lucide-react';
import { useActiveGuildId, useShowNotification } from '../store/useStore';
import { listarAgentesIa, getGuildSettings, saveGuildSettings, listChannels, listMembers } from '../api/tauri';
import type { AgenteIa, Channel, GuildSettings, Member } from '../types';
import { Avatar, Button, Cargando, EstadoVacio, ErrorPanel, Input, Select } from './ui';
import { t } from '../i18n';
import './Asistente.css';

const TIPOS_TEXTO = [0, 5];
const CONTEXTO_MAX = 30;
const mensajeError = (e: unknown, defecto: string) => (typeof e === 'string' ? e : e instanceof Error ? e.message : defecto);
/** Los modelos gratis de opencode: el proveedor `opencode/` o los que terminan en `-free`. */
const esGratis = (agente: string, modelo: string) => agente === 'opencode' && (modelo.startsWith('opencode/') || modelo.endsWith('-free') || modelo.endsWith(':free'));

export const RespuestasDiscord: React.FC<{ onAbrirPersonalidad?: () => void }> = ({ onAbrirPersonalidad }) => {
  const guildId = useActiveGuildId();
  const showNotification = useShowNotification();

  const [agentes, setAgentes] = useState<AgenteIa[] | null>(null);
  const [miembros, setMiembros] = useState<Member[]>([]);
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
        const [a, s, c, m] = await Promise.all([listarAgentesIa(), getGuildSettings(guildId), listChannels(guildId), listMembers(guildId).catch(() => [] as Member[])]);
        if (!vigente) return;
        setAgentes(a);
        // Sin modelo guardado, el más barato del agente: las respuestas en
        // Discord son para conversar sin gastar (el desplegable mostraba opus).
        const agente = a.find((x) => x.agente === s.chat_agente);
        setAjustes(s.chat_modelo ? s : { ...s, chat_modelo: modeloMasBarato(s.chat_agente, agente?.modelos ?? []) });
        setMiembros(m.filter((x) => !x.bot));
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
    cambiar({ chat_agente: nombre as GuildSettings['chat_agente'], chat_modelo: modeloMasBarato(nombre, lista) });
  };
  const hayAutorizados = (ajustes.chat_ordenes_usuarios ?? []).length > 0;
  const modelosOrdenes = agentes.find((a) => a.agente === ajustes.ordenes_agente)?.modelos ?? [];
  const cambiarAgenteOrdenes = (nombre: string) => {
    const lista = agentes.find((x) => x.agente === nombre)?.modelos ?? [];
    const defecto = nombre === 'claude' && lista.includes('sonnet') ? 'sonnet' : (lista[0] ?? '');
    cambiar({ ordenes_agente: nombre as GuildSettings['ordenes_agente'], ordenes_modelo: defecto });
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

        <div>
          <strong>{t('Personality')}</strong>
          <p style={{ margin: 'var(--sp-1) 0', whiteSpace: 'pre-wrap' }}>
            {ajustes.chat_personalidad.trim()
              ? (ajustes.chat_personalidad.length > 160 ? `${ajustes.chat_personalidad.slice(0, 160)}…` : ajustes.chat_personalidad)
              : <em>{t('No personality set yet.')}</em>}
          </p>
          <Button variante="secundario" onClick={onAbrirPersonalidad}>{t('Edit in Context & personality')}</Button>
        </div>

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

        <SelectorMiembros
          etiqueta={t('Reply only to')}
          ayuda={t('None selected = the bot replies to everyone.')}
          miembros={miembros}
          valor={ajustes.chat_usuarios ?? []}
          onChange={(ids) => cambiar({ chat_usuarios: ids })}
        />

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

        <fieldset style={{ border: 0, padding: 0, margin: 0, display: 'flex', flexDirection: 'column', gap: 'var(--sp-2)' }}>
          <legend>{t('Orders from Discord')}</legend>
          <div className="asistente__aviso" role="note">
            <TriangleAlert size={16} />
            <span>{t('Orders really change your server and are recorded in Logs.')}</span>
          </div>
          <label style={{ display: 'flex', alignItems: 'center', gap: 'var(--sp-2)' }}>
            <input
              type="checkbox" role="switch"
              checked={ajustes.chat_ordenes && hayAutorizados}
              disabled={!hayAutorizados}
              onChange={(e) => cambiar({ chat_ordenes: e.target.checked })}
            />
            <span>{t('Let authorized users give orders from Discord')}</span>
          </label>
          {!hayAutorizados && <small>{t('Choose at least one authorized user below to turn orders on. Nobody else can give orders.')}</small>}
          <SelectorMiembros
            etiqueta={t('Users who can give orders')}
            ayuda={t('Only these users can give orders. Empty = nobody.')}
            miembros={miembros}
            valor={ajustes.chat_ordenes_usuarios ?? []}
            onChange={(ids) => cambiar({ chat_ordenes_usuarios: ids, ...(ids.length === 0 ? { chat_ordenes: false } : {}) })}
          />
          <label style={{ display: 'flex', alignItems: 'center', gap: 'var(--sp-2)' }}>
            <input type="checkbox" role="switch" checked={ajustes.chat_ordenes_confirmar} onChange={(e) => cambiar({ chat_ordenes_confirmar: e.target.checked })} />
            <span>{t('Ask for confirmation before applying')}</span>
          </label>
          <div className="asistente__acciones">
            <Select etiqueta={t('Builder agent')} value={ajustes.ordenes_agente} onChange={(e) => cambiarAgenteOrdenes(e.target.value)}>
              {agentes.map((a) => <option key={a.agente} value={a.agente}>{a.agente}</option>)}
            </Select>
            {modelosOrdenes.length > 0 ? (
              <Select etiqueta={t('Builder model')} value={ajustes.ordenes_modelo} onChange={(e) => cambiar({ ordenes_modelo: e.target.value })}>
                {ajustes.ordenes_modelo && !modelosOrdenes.includes(ajustes.ordenes_modelo) && <option value={ajustes.ordenes_modelo}>{ajustes.ordenes_modelo}</option>}
                {modelosOrdenes.map((m) => <option key={m} value={m}>{m}</option>)}
              </Select>
            ) : (
              <Input etiqueta={t('Builder model')} value={ajustes.ordenes_modelo} placeholder={t('Model name')} onChange={(e) => cambiar({ ordenes_modelo: e.target.value })} />
            )}
          </div>
        </fieldset>

        <div className="asistente__acciones">
          <Button onClick={() => { void guardar(); }} disabled={!sucio || guardando}>{guardando ? t('Saving…') : t('Save settings')}</Button>
        </div>
      </section>
    </div>
  );
};

const nombreMiembro = (m: Member) => m.nickname || m.username;

/** Selector múltiple de miembros: buscador, lista filtrada y chips con avatar y nombre. */
const SelectorMiembros: React.FC<{
  etiqueta: string; ayuda: string; miembros: Member[]; valor: string[]; onChange: (ids: string[]) => void;
}> = ({ etiqueta, ayuda, miembros, valor, onChange }) => {
  const [busqueda, setBusqueda] = useState('');
  const q = busqueda.trim().toLowerCase();
  const candidatos = miembros
    .filter((m) => !valor.includes(m.user_id) && (nombreMiembro(m).toLowerCase().includes(q) || m.username.toLowerCase().includes(q)))
    .slice(0, 8);
  return (
    <fieldset style={{ border: 0, padding: 0, margin: 0 }}>
      <legend>{etiqueta}</legend>
      <small>{ayuda}</small>
      {valor.length > 0 && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 'var(--sp-1)', margin: 'var(--sp-1) 0' }}>
          {valor.map((id) => {
            const m = miembros.find((x) => x.user_id === id);
            const nombre = m ? nombreMiembro(m) : id;
            return (
              <span key={id} className="ui-badge" style={{ display: 'inline-flex', alignItems: 'center', gap: 'var(--sp-1)' }}>
                <Avatar src={m?.avatar ?? null} nombre={nombre} tamano={20} />
                <span>{nombre}</span>
                <button type="button" aria-label={t('Remove {name}', { name: nombre })} onClick={() => onChange(valor.filter((x) => x !== id))}>×</button>
              </span>
            );
          })}
        </div>
      )}
      <Input
        aria-label={t('Search members by name')}
        placeholder={t('Search members by name')}
        value={busqueda}
        onChange={(e) => setBusqueda(e.target.value)}
      />
      {q !== '' && (
        candidatos.length === 0 ? <small>{t('No members found.')}</small> : (
          <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
            {candidatos.map((m) => (
              <li key={m.user_id}>
                <button type="button" style={{ display: 'flex', alignItems: 'center', gap: 'var(--sp-2)' }} onClick={() => { onChange([...valor, m.user_id]); setBusqueda(''); }}>
                  <Avatar src={m.avatar} nombre={nombreMiembro(m)} tamano={20} />
                  <span>{nombreMiembro(m)}</span>
                </button>
              </li>
            ))}
          </ul>
        )
      )}
    </fieldset>
  );
};

/** Modelo por defecto para conversar: uno gratis si hay; si no, haiku en claude; si no, el primero. */
export function modeloMasBarato(agente: string, modelos: string[]): string {
  return modelos.find((m) => esGratis(agente, m))
    ?? (agente === 'claude' && modelos.includes('haiku') ? 'haiku' : undefined)
    ?? modelos[0]
    ?? '';
}
