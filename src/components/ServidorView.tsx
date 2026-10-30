import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { useActiveGuildId, useServerMap, useShowNotification } from '../store/useStore';
import { getGuildSettingsDiscord, updateGuildDiscord, auditLog } from '../api/tauri';
import type { AjustesServidor, EntradaAuditoria } from '../types';
import { Button, Card, Input, Select, Cargando, EstadoVacio, ErrorPanel } from './ui';
import { CabeceraVista } from './EstadoVista';
import { t, idiomaActual } from '../i18n';
import './Vistas.css';

const nivelesVerificacion = () => [
  t('None'),
  t('Low: verified email'),
  t('Medium: account older than 5 minutes'),
  t('High: member for more than 10 minutes'),
  t('Very high: verified phone'),
];
const filtrosContenido = () => [
  t('Disabled'),
  t('Scan members without a role'),
  t('Scan all members'),
];
const notificaciones = () => [t('All messages'), t('Only mentions')];
const tiemposAfk = (): [number, string][] => [
  [60, t('1 minute')],
  [300, t('5 minutes')],
  [900, t('15 minutes')],
  [1800, t('30 minutes')],
  [3600, t('1 hour')],
];
const TIPOS_TEXTO = [0, 5];
const TIPOS_VOZ = [2, 13];

const mensajeError = (e: unknown, defecto: string) => (typeof e === 'string' ? e : e instanceof Error ? e.message : defecto);

const igual = (a: AjustesServidor, b: AjustesServidor) => JSON.stringify(a) === JSON.stringify(b);

export const ServidorView: React.FC = () => {
  const guildId = useActiveGuildId();
  const serverMap = useServerMap();
  const showNotification = useShowNotification();

  const [original, setOriginal] = useState<AjustesServidor | null>(null);
  const [borrador, setBorrador] = useState<AjustesServidor | null>(null);
  const [cargando, setCargando] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [guardando, setGuardando] = useState(false);
  const [errorGuardar, setErrorGuardar] = useState<string | null>(null);

  const [entradas, setEntradas] = useState<EntradaAuditoria[] | null>(null);
  const [cargandoAud, setCargandoAud] = useState(false);
  const [errorAud, setErrorAud] = useState<string | null>(null);
  const [filtro, setFiltro] = useState('');

  const cargarAjustes = useCallback(async (id: string) => {
    setCargando(true);
    setError(null);
    try {
      const a = await getGuildSettingsDiscord(id);
      setOriginal(a);
      setBorrador(a);
    } catch (e) {
      setOriginal(null);
      setBorrador(null);
      setError(mensajeError(e, t('Could not read the server settings')));
    } finally {
      setCargando(false);
    }
  }, []);

  const cargarAuditoria = useCallback(async (id: string) => {
    setCargandoAud(true);
    setErrorAud(null);
    try {
      setEntradas(await auditLog(id));
    } catch (e) {
      setEntradas(null);
      setErrorAud(mensajeError(e, t('Could not read the audit log')));
    } finally {
      setCargandoAud(false);
    }
  }, []);

  useEffect(() => {
    setFiltro('');
    setErrorGuardar(null);
    if (!guildId) return;
    void cargarAjustes(guildId);
    void cargarAuditoria(guildId);
  }, [guildId, cargarAjustes, cargarAuditoria]);

  const canales = serverMap?.channels ?? [];
  const canalesTexto = useMemo(() => canales.filter((c) => TIPOS_TEXTO.includes(c.channel_type)).sort((a, b) => a.position - b.position), [canales]);
  const canalesVoz = useMemo(() => canales.filter((c) => TIPOS_VOZ.includes(c.channel_type)).sort((a, b) => a.position - b.position), [canales]);

  const tiposAccion = useMemo(() => {
    const m = new Map<number, string>();
    (entradas ?? []).forEach((e) => m.set(e.action_type, e.accion));
    return [...m.entries()].sort((a, b) => a[1].localeCompare(b[1], idiomaActual()));
  }, [entradas]);
  const visibles = useMemo(
    () => (entradas ?? []).filter((e) => filtro === '' || String(e.action_type) === filtro),
    [entradas, filtro],
  );

  if (!guildId) {
    return (
      <div className="vista">
        <EstadoVacio titulo={t('No server selected')} descripcion={t('Pick a server in the left column to edit its settings.')} />
      </div>
    );
  }

  const cambios = !!original && !!borrador && !igual(original, borrador);
  const poner = (parcial: Partial<AjustesServidor>) => setBorrador((b) => (b ? { ...b, ...parcial } : b));

  const guardar = async () => {
    if (!borrador) return;
    setGuardando(true);
    setErrorGuardar(null);
    try {
      const nuevo = await updateGuildDiscord(guildId, borrador);
      setOriginal(nuevo);
      setBorrador(nuevo);
      showNotification(t('Server settings saved'), 'success');
      void cargarAuditoria(guildId);
    } catch (e) {
      setErrorGuardar(mensajeError(e, t('Could not save the settings')));
    } finally {
      setGuardando(false);
    }
  };

  const descartar = () => {
    setBorrador(original);
    setErrorGuardar(null);
  };

  // Si el canal guardado ya no existe, se conserva como opción para no perder el valor sin avisar.
  const opcionesCanal = (lista: typeof canales, actual: string | null) => (
    <>
      <option value="">{t('None')}</option>
      {actual && !lista.some((c) => c.id === actual) && <option value={actual}>{t('Unknown channel ({id})', { id: actual })}</option>}
      {lista.map((c) => (
        <option key={c.id} value={c.id}>{c.channel_type === 2 || c.channel_type === 13 ? '🔊 ' : '# '}{c.name}</option>
      ))}
    </>
  );

  return (
    <div className="vista servidor-view">
      <CabeceraVista titulo={t('Server')} />

      <Card titulo={t('General settings')}>
        {cargando && <Cargando lineas={4} />}
        {error && <ErrorPanel mensaje={error} onReintentar={() => { void cargarAjustes(guildId); }} />}
        {borrador && !cargando && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (cambios && !guardando) void guardar();
            }}
          >
            <fieldset disabled={guardando} style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
              <div className="servidor__form">
                <Input
                  etiqueta={t('Server name')}
                  value={borrador.name}
                  maxLength={100}
                  onChange={(e) => poner({ name: e.target.value })}
                  error={borrador.name.trim().length < 2 ? t('The name must be at least 2 characters long') : undefined}
                />
                <Select
                  etiqueta={t('Default notifications')}
                  value={borrador.default_message_notifications}
                  onChange={(e) => poner({ default_message_notifications: Number(e.target.value) })}
                >
                  {notificaciones().map((n, i) => <option key={i} value={i}>{n}</option>)}
                </Select>
                <div className="ui-campo servidor__ancho">
                  <label htmlFor="servidor-descripcion">{t('Description')}</label>
                  <textarea
                    id="servidor-descripcion"
                    className="ui-control"
                    rows={3}
                    maxLength={120}
                    value={borrador.description ?? ''}
                    onChange={(e) => poner({ description: e.target.value === '' ? null : e.target.value })}
                  />
                  <span className="ui-campo__ayuda">{t('Discord only shows it on Community servers.')}</span>
                </div>
                <Select
                  etiqueta={t('Verification level')}
                  value={borrador.verification_level}
                  onChange={(e) => poner({ verification_level: Number(e.target.value) })}
                >
                  {nivelesVerificacion().map((n, i) => <option key={i} value={i}>{n}</option>)}
                </Select>
                <Select
                  etiqueta={t('Explicit content filter')}
                  value={borrador.explicit_content_filter}
                  onChange={(e) => poner({ explicit_content_filter: Number(e.target.value) })}
                >
                  {filtrosContenido().map((n, i) => <option key={i} value={i}>{n}</option>)}
                </Select>
                <Select
                  etiqueta={t('System channel')}
                  ayuda={t('Receives welcome messages and notices.')}
                  value={borrador.system_channel_id ?? ''}
                  onChange={(e) => poner({ system_channel_id: e.target.value || null })}
                >
                  {opcionesCanal(canalesTexto, borrador.system_channel_id)}
                </Select>
                <Select
                  etiqueta={t('AFK channel')}
                  ayuda={t('Inactive members are moved to this voice channel.')}
                  value={borrador.afk_channel_id ?? ''}
                  onChange={(e) => poner({ afk_channel_id: e.target.value || null })}
                >
                  {opcionesCanal(canalesVoz, borrador.afk_channel_id)}
                </Select>
                <Select
                  etiqueta={t('AFK timeout')}
                  value={borrador.afk_timeout}
                  onChange={(e) => poner({ afk_timeout: Number(e.target.value) })}
                >
                  {!tiemposAfk().some(([s]) => s === borrador.afk_timeout) && (
                    <option value={borrador.afk_timeout}>{borrador.afk_timeout} s</option>
                  )}
                  {tiemposAfk().map(([s, n]) => <option key={s} value={s}>{n}</option>)}
                </Select>
              </div>
            </fieldset>
            {errorGuardar && <p className="servidor__error" role="alert">{errorGuardar}</p>}
            <div className="servidor__acciones">
              <Button variante="secundario" onClick={descartar} disabled={!cambios || guardando}>{t('Discard')}</Button>
              <Button type="submit" cargando={guardando} disabled={!cambios || borrador.name.trim().length < 2}>{t('Save')}</Button>
            </div>
          </form>
        )}
      </Card>

      <section className="auditoria" aria-labelledby="auditoria-titulo">
        <div className="auditoria__cab">
          <h3 id="auditoria-titulo">{t('Audit log')}</h3>
          <div style={{ display: 'flex', gap: 'var(--sp-2)', alignItems: 'center' }}>
            <Select
              aria-label={t('Filter by action type')}
              value={filtro}
              onChange={(e) => setFiltro(e.target.value)}
              disabled={!entradas || entradas.length === 0}
            >
              <option value="">{t('All actions')}</option>
              {tiposAccion.map(([tipo, nombre]) => <option key={tipo} value={String(tipo)}>{nombre}</option>)}
            </Select>
            <Button variante="secundario" onClick={() => { void cargarAuditoria(guildId); }} disabled={cargandoAud}>{t('Refresh')}</Button>
          </div>
        </div>
        <Card>
          {cargandoAud && <Cargando lineas={4} />}
          {errorAud && !cargandoAud && <ErrorPanel mensaje={errorAud} onReintentar={() => { void cargarAuditoria(guildId); }} />}
          {entradas && !cargandoAud && entradas.length === 0 && (
            <EstadoVacio titulo={t('No entries')} descripcion={t('Discord returned no recent actions on this server.')} />
          )}
          {entradas && !cargandoAud && entradas.length > 0 && visibles.length === 0 && (
            <EstadoVacio titulo={t('No results')} descripcion={t('None of the last 50 entries match that action type.')} />
          )}
          {entradas && !cargandoAud && visibles.length > 0 && (
            <table className="auditoria__tabla">
              <thead>
                <tr><th>{t('Date')}</th><th>{t('Action')}</th><th>{t('Author')}</th><th>{t('Target')}</th><th>{t('Reason')}</th></tr>
              </thead>
              <tbody>
                {visibles.map((e) => (
                  <tr key={e.id}>
                    <td className="auditoria__fecha">{e.fecha}</td>
                    <td>{e.accion}</td>
                    <td>{e.autor}</td>
                    <td>{e.objetivo}</td>
                    <td>{e.reason ?? '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      </section>
    </div>
  );
};
