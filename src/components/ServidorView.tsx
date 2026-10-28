import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { useActiveGuildId, useServerMap, useShowNotification } from '../store/useStore';
import { getGuildSettingsDiscord, updateGuildDiscord, auditLog } from '../api/tauri';
import type { AjustesServidor, EntradaAuditoria } from '../types';
import { Button, Card, Input, Select, Cargando, EstadoVacio, ErrorPanel } from './ui';
import { CabeceraVista } from './EstadoVista';
import './Vistas.css';

const NIVELES_VERIFICACION = [
  'Ninguno',
  'Bajo: correo verificado',
  'Medio: cuenta con más de 5 minutos',
  'Alto: miembro desde hace más de 10 minutos',
  'Muy alto: teléfono verificado',
];
const FILTROS_CONTENIDO = [
  'Desactivado',
  'Analizar miembros sin rol',
  'Analizar a todos los miembros',
];
const NOTIFICACIONES = ['Todos los mensajes', 'Solo menciones'];
const TIEMPOS_AFK: [number, string][] = [
  [60, '1 minuto'],
  [300, '5 minutos'],
  [900, '15 minutos'],
  [1800, '30 minutos'],
  [3600, '1 hora'],
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
      setError(mensajeError(e, 'No se pudieron leer los ajustes del servidor'));
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
      setErrorAud(mensajeError(e, 'No se pudo leer el registro de auditoría'));
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
    return [...m.entries()].sort((a, b) => a[1].localeCompare(b[1], 'es'));
  }, [entradas]);
  const visibles = useMemo(
    () => (entradas ?? []).filter((e) => filtro === '' || String(e.action_type) === filtro),
    [entradas, filtro],
  );

  if (!guildId) {
    return (
      <div className="vista">
        <EstadoVacio titulo="Ningún servidor seleccionado" descripcion="Elige un servidor en la columna de la izquierda para editar sus ajustes." />
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
      showNotification('Ajustes del servidor guardados', 'success');
      void cargarAuditoria(guildId);
    } catch (e) {
      setErrorGuardar(mensajeError(e, 'No se pudieron guardar los ajustes'));
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
      <option value="">Ninguno</option>
      {actual && !lista.some((c) => c.id === actual) && <option value={actual}>Canal desconocido ({actual})</option>}
      {lista.map((c) => (
        <option key={c.id} value={c.id}>{c.channel_type === 2 || c.channel_type === 13 ? '🔊 ' : '# '}{c.name}</option>
      ))}
    </>
  );

  return (
    <div className="vista servidor-view">
      <CabeceraVista titulo="Servidor" />

      <Card titulo="Ajustes generales">
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
                  etiqueta="Nombre del servidor"
                  value={borrador.name}
                  maxLength={100}
                  onChange={(e) => poner({ name: e.target.value })}
                  error={borrador.name.trim().length < 2 ? 'El nombre debe tener al menos 2 caracteres' : undefined}
                />
                <Select
                  etiqueta="Notificaciones por defecto"
                  value={borrador.default_message_notifications}
                  onChange={(e) => poner({ default_message_notifications: Number(e.target.value) })}
                >
                  {NOTIFICACIONES.map((t, i) => <option key={i} value={i}>{t}</option>)}
                </Select>
                <div className="ui-campo servidor__ancho">
                  <label htmlFor="servidor-descripcion">Descripción</label>
                  <textarea
                    id="servidor-descripcion"
                    className="ui-control"
                    rows={3}
                    maxLength={120}
                    value={borrador.description ?? ''}
                    onChange={(e) => poner({ description: e.target.value === '' ? null : e.target.value })}
                  />
                  <span className="ui-campo__ayuda">Discord solo la muestra en servidores de comunidad.</span>
                </div>
                <Select
                  etiqueta="Nivel de verificación"
                  value={borrador.verification_level}
                  onChange={(e) => poner({ verification_level: Number(e.target.value) })}
                >
                  {NIVELES_VERIFICACION.map((t, i) => <option key={i} value={i}>{t}</option>)}
                </Select>
                <Select
                  etiqueta="Filtro de contenido explícito"
                  value={borrador.explicit_content_filter}
                  onChange={(e) => poner({ explicit_content_filter: Number(e.target.value) })}
                >
                  {FILTROS_CONTENIDO.map((t, i) => <option key={i} value={i}>{t}</option>)}
                </Select>
                <Select
                  etiqueta="Canal del sistema"
                  ayuda="Recibe los mensajes de bienvenida y avisos."
                  value={borrador.system_channel_id ?? ''}
                  onChange={(e) => poner({ system_channel_id: e.target.value || null })}
                >
                  {opcionesCanal(canalesTexto, borrador.system_channel_id)}
                </Select>
                <Select
                  etiqueta="Canal AFK"
                  ayuda="Los miembros inactivos pasan a este canal de voz."
                  value={borrador.afk_channel_id ?? ''}
                  onChange={(e) => poner({ afk_channel_id: e.target.value || null })}
                >
                  {opcionesCanal(canalesVoz, borrador.afk_channel_id)}
                </Select>
                <Select
                  etiqueta="Tiempo de inactividad (AFK)"
                  value={borrador.afk_timeout}
                  onChange={(e) => poner({ afk_timeout: Number(e.target.value) })}
                >
                  {!TIEMPOS_AFK.some(([s]) => s === borrador.afk_timeout) && (
                    <option value={borrador.afk_timeout}>{borrador.afk_timeout} s</option>
                  )}
                  {TIEMPOS_AFK.map(([s, t]) => <option key={s} value={s}>{t}</option>)}
                </Select>
              </div>
            </fieldset>
            {errorGuardar && <p className="servidor__error" role="alert">{errorGuardar}</p>}
            <div className="servidor__acciones">
              <Button variante="secundario" onClick={descartar} disabled={!cambios || guardando}>Descartar</Button>
              <Button type="submit" cargando={guardando} disabled={!cambios || borrador.name.trim().length < 2}>Guardar</Button>
            </div>
          </form>
        )}
      </Card>

      <section className="auditoria" aria-labelledby="auditoria-titulo">
        <div className="auditoria__cab">
          <h3 id="auditoria-titulo">Registro de auditoría</h3>
          <div style={{ display: 'flex', gap: 'var(--sp-2)', alignItems: 'center' }}>
            <Select
              aria-label="Filtrar por tipo de acción"
              value={filtro}
              onChange={(e) => setFiltro(e.target.value)}
              disabled={!entradas || entradas.length === 0}
            >
              <option value="">Todas las acciones</option>
              {tiposAccion.map(([tipo, nombre]) => <option key={tipo} value={String(tipo)}>{nombre}</option>)}
            </Select>
            <Button variante="secundario" onClick={() => { void cargarAuditoria(guildId); }} disabled={cargandoAud}>Actualizar</Button>
          </div>
        </div>
        <Card>
          {cargandoAud && <Cargando lineas={4} />}
          {errorAud && !cargandoAud && <ErrorPanel mensaje={errorAud} onReintentar={() => { void cargarAuditoria(guildId); }} />}
          {entradas && !cargandoAud && entradas.length === 0 && (
            <EstadoVacio titulo="Sin entradas" descripcion="Discord no devolvió acciones recientes en este servidor." />
          )}
          {entradas && !cargandoAud && entradas.length > 0 && visibles.length === 0 && (
            <EstadoVacio titulo="Sin resultados" descripcion="Ninguna de las últimas 50 entradas coincide con ese tipo de acción." />
          )}
          {entradas && !cargandoAud && visibles.length > 0 && (
            <table className="auditoria__tabla">
              <thead>
                <tr><th>Fecha</th><th>Acción</th><th>Autor</th><th>Objetivo</th><th>Motivo</th></tr>
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
