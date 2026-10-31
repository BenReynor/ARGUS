import React, { useEffect, useRef, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { Download, Upload } from 'lucide-react';
import { useActiveGuildId, useShowNotification } from '../store/useStore';
import { leerContextoServidor, guardarContextoServidor, getGuildSettings, saveGuildSettings } from '../api/tauri';
import { Button, Cargando, EstadoVacio, ErrorPanel } from './ui';
import { t } from '../i18n';
import './Asistente.css';

const ESPERA_GUARDADO_MS = 800;
type Estado = 'idle' | 'pendiente' | 'guardando' | 'guardado' | 'error';
const mensajeError = (e: unknown, defecto: string) => (typeof e === 'string' ? e : e instanceof Error ? e.message : defecto);

const etiquetaEstado = (e: Estado) =>
  e === 'pendiente' ? t('Unsaved changes…') : e === 'guardando' ? t('Saving…') : e === 'guardado' ? t('Saved') : e === 'error' ? t('Not saved') : '';

/** Pestaña «Context & personality»: contexto del servidor y personalidad del bot, con guardado automático. */
export const ContextoPersonalidad: React.FC = () => {
  const guildId = useActiveGuildId();
  const showNotification = useShowNotification();

  const [cargando, setCargando] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [contexto, setContexto] = useState('');
  const [personalidad, setPersonalidad] = useState('');
  const [estadoCtx, setEstadoCtx] = useState<Estado>('idle');
  const [estadoPers, setEstadoPers] = useState<Estado>('idle');

  const tCtx = useRef<ReturnType<typeof setTimeout> | null>(null);
  const tPers = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendiente = useRef<{ guildId: string; contexto: string | null; personalidad: string | null } | null>(null);

  const volcarContexto = async (id: string, texto: string) => {
    setEstadoCtx('guardando');
    try {
      await guardarContextoServidor(id, texto);
      if (pendiente.current?.guildId === id) pendiente.current.contexto = null;
      setEstadoCtx('guardado');
    } catch (e) {
      setEstadoCtx('error');
      showNotification(mensajeError(e, t('Could not save the server context')), 'error');
    }
  };
  // Se lee el resto de ajustes justo antes de guardar para no pisar cambios hechos en «Discord replies».
  const volcarPersonalidad = async (id: string, texto: string) => {
    setEstadoPers('guardando');
    try {
      const actuales = await getGuildSettings(id);
      await saveGuildSettings(id, { ...actuales, chat_personalidad: texto });
      if (pendiente.current?.guildId === id) pendiente.current.personalidad = null;
      setEstadoPers('guardado');
    } catch (e) {
      setEstadoPers('error');
      showNotification(mensajeError(e, t('Could not save the personality')), 'error');
    }
  };

  // Guarda lo que quede pendiente al cambiar de servidor o salir de la pestaña.
  const vaciar = () => {
    if (tCtx.current) { clearTimeout(tCtx.current); tCtx.current = null; }
    if (tPers.current) { clearTimeout(tPers.current); tPers.current = null; }
    const p = pendiente.current;
    if (!p) return;
    if (p.contexto !== null) void guardarContextoServidor(p.guildId, p.contexto).catch(() => {});
    if (p.personalidad !== null) {
      const texto = p.personalidad;
      void getGuildSettings(p.guildId).then((s) => saveGuildSettings(p.guildId, { ...s, chat_personalidad: texto })).catch(() => {});
    }
    pendiente.current = null;
  };

  useEffect(() => {
    if (!guildId) return;
    let vigente = true;
    setCargando(true);
    setError(null);
    setEstadoCtx('idle');
    setEstadoPers('idle');
    pendiente.current = { guildId, contexto: null, personalidad: null };
    (async () => {
      try {
        const [c, s] = await Promise.all([leerContextoServidor(guildId), getGuildSettings(guildId)]);
        if (!vigente) return;
        setContexto(c);
        setPersonalidad(s.chat_personalidad ?? '');
      } catch (e) {
        if (vigente) setError(mensajeError(e, t('Could not load the context and personality')));
      } finally {
        if (vigente) setCargando(false);
      }
    })();
    return () => { vigente = false; vaciar(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [guildId]);

  const cambiarContexto = (texto: string) => {
    setContexto(texto);
    if (!guildId) return;
    pendiente.current = { guildId, contexto: texto, personalidad: pendiente.current?.personalidad ?? null };
    setEstadoCtx('pendiente');
    if (tCtx.current) clearTimeout(tCtx.current);
    tCtx.current = setTimeout(() => { void volcarContexto(guildId, texto); }, ESPERA_GUARDADO_MS);
  };
  const cambiarPersonalidad = (texto: string) => {
    setPersonalidad(texto);
    if (!guildId) return;
    pendiente.current = { guildId, contexto: pendiente.current?.contexto ?? null, personalidad: texto };
    setEstadoPers('pendiente');
    if (tPers.current) clearTimeout(tPers.current);
    tPers.current = setTimeout(() => { void volcarPersonalidad(guildId, texto); }, ESPERA_GUARDADO_MS);
  };

  const importar = async () => {
    try {
      const ruta = await open({ multiple: false, filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }] });
      if (typeof ruta !== 'string') return;
      if (contexto.trim() && !window.confirm(t('Replace the current context with the imported file?'))) return;
      cambiarContexto(await readTextFile(ruta));
    } catch (e) {
      showNotification(mensajeError(e, t('Could not import the file')), 'error');
    }
  };
  const exportar = async () => {
    try {
      const ruta = await save({ defaultPath: 'server-context.md', filters: [{ name: 'Markdown', extensions: ['md'] }] });
      if (!ruta) return;
      await writeTextFile(ruta, contexto);
      showNotification(t('Context exported'), 'success');
    } catch (e) {
      showNotification(mensajeError(e, t('Could not export the context')), 'error');
    }
  };

  if (!guildId) {
    return <EstadoVacio titulo={t('No server selected')} descripcion={t('Pick a server in the left column to edit its context and personality.')} />;
  }
  if (error) return <ErrorPanel mensaje={error} />;
  if (cargando) return <Cargando lineas={4} />;

  return (
    <div className="vista vista--llena asistente" style={{ overflow: 'auto' }}>
      <section className="asistente__panel" style={{ flex: '0 0 auto' }} aria-label={t('Server context')}>
        <header className="asistente__cabecera">
          <h3>{t('Server context')}</h3>
          <div className="asistente__acciones">
            <Button variante="secundario" onClick={() => { void importar(); }}><Upload size={14} /> {t('Import .md')}</Button>
            <Button variante="secundario" onClick={() => { void exportar(); }}><Download size={14} /> {t('Export .md')}</Button>
          </div>
        </header>
        <small>{t('Describe your server (rules, tone, channels, people). The bot reads this when it replies. Markdown is supported.')}</small>
        <textarea
          className="ui-control asistente__plan"
          rows={14}
          style={{ flex: '0 0 auto' }}
          value={contexto}
          aria-label={t('Server context')}
          placeholder={t('# About this server\nA friendly community about board games.\n\n## Rules\n- Be kind.\n- No spam.\n\n## Channels\n- #general: casual chat\n- #events: weekly game nights')}
          onChange={(e) => cambiarContexto(e.target.value)}
        />
        <div className="asistente__acciones">
          <span>{t('{count} characters', { count: contexto.length.toLocaleString() })}</span>
          <span role="status">{etiquetaEstado(estadoCtx)}</span>
        </div>
      </section>

      <section className="asistente__panel" style={{ flex: '0 0 auto' }} aria-label={t('Bot personality')}>
        <header className="asistente__cabecera"><h3>{t('Bot personality')}</h3></header>
        <small>{t('How the bot speaks and behaves when it replies on Discord.')}</small>
        <textarea
          className="ui-control"
          rows={6}
          value={personalidad}
          aria-label={t('Personality')}
          placeholder={t('Example: You are Luna, the friendly assistant of this server. Be brief and cheerful. Never give medical advice.')}
          onChange={(e) => cambiarPersonalidad(e.target.value)}
        />
        <div className="asistente__acciones">
          <span role="status">{etiquetaEstado(estadoPers)}</span>
        </div>
      </section>
    </div>
  );
};
