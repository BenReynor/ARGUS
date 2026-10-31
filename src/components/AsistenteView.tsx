import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { Check, CircleDashed, Download, Loader2, Square, TriangleAlert, Upload, X } from 'lucide-react';
import { useActiveGuildId, useShowNotification } from '../store/useStore';
import {
  listarAgentesIa, enviarAlAsistente, leerPlanServidor, guardarPlanServidor, leerConversacion,
  guardarConversacion, nuevaConversacion, aplicarPropuesta, cancelarAsistente,
  onAsistenteEvento, onAsistentePropuesta, onAsistenteAccion, onAsistenteResumen,
} from '../api/tauri';
import type { AccionPropuesta, AgenteIa, AvanceAccion, EstadoAccion, EventoPropuesta, ResumenPropuesta } from '../types';
import { Badge, Button, Cargando, EstadoVacio, ErrorPanel, Input, Select } from './ui';
import { t } from '../i18n';
import { RespuestasDiscord } from './RespuestasDiscord';
import './Asistente.css';

type Modo = 'read' | 'execute';
interface Herramienta { id: string; nombre: string; ok: boolean | null }
interface Mensaje { id: string; rol: 'user' | 'assistant'; texto: string; herramientas: Herramienta[]; error?: string }

const ESPERA_GUARDADO_MS = 800;
const PANEL_MIN = 25;
const PANEL_MAX = 75;

const mensajeError = (e: unknown, defecto: string) => (typeof e === 'string' ? e : e instanceof Error ? e.message : defecto);
let contador = 0;
const nuevoId = () => `m${Date.now()}-${contador++}`;

/** Lo guardado en BD es JSON libre: se queda solo con lo que tiene forma de mensaje. */
const comoMensajes = (v: unknown): Mensaje[] =>
  Array.isArray(v)
    ? v.filter((m) => m && (m.rol === 'user' || m.rol === 'assistant') && typeof m.texto === 'string')
      .map((m) => ({ id: String(m.id ?? nuevoId()), rol: m.rol, texto: m.texto, herramientas: Array.isArray(m.herramientas) ? m.herramientas : [], error: m.error }))
    : [];

// Mismo orden seguro que `ordenar_propuesta` (propuesta.rs): roles → categorías → canales → permisos → roles de miembros → resto.
const GRUPOS = () => [t('Roles'), t('Categories'), t('Channels'), t('Permissions'), t('Member roles'), t('Other')];
const esCategoria = (a: AccionPropuesta) => {
  const tipo = (a.args as { tipo?: unknown } | null)?.tipo;
  if (typeof tipo !== 'string') return false;
  const clave = tipo.trim().toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '');
  return clave === 'categoria' || clave === 'category';
};
const grupoDe = (a: AccionPropuesta): number => {
  switch (a.herramienta) {
    case 'crear_rol': case 'editar_rol': return 0;
    case 'crear_canal': return esCategoria(a) ? 1 : 2;
    case 'editar_canal': return 2;
    case 'permiso_canal': case 'quitar_permiso_canal': return 3;
    case 'dar_rol': case 'quitar_rol': return 4;
    default: return 5;
  }
};
const ordenar = (acciones: AccionPropuesta[]) =>
  acciones.map((a, i) => ({ a, g: grupoDe(a), i })).sort((x, y) => x.g - y.g || x.i - y.i).map((x) => x.a);

const esErrorSinSesion = (m: string) => m.startsWith(t('The AI agent is not signed in. Sign in with its command line and try again'));

const ChatAsistente: React.FC = () => {
  const guildId = useActiveGuildId();
  const showNotification = useShowNotification();

  const [agentes, setAgentes] = useState<AgenteIa[] | null>(null);
  const [errorAgentes, setErrorAgentes] = useState<string | null>(null);
  const [agente, setAgente] = useState('');
  const [modelo, setModelo] = useState('');
  const [modo, setModo] = useState<Modo>('read');

  const [cargandoServidor, setCargandoServidor] = useState(false);
  const [errorServidor, setErrorServidor] = useState<string | null>(null);
  const [plan, setPlan] = useState('');
  const [estadoGuardado, setEstadoGuardado] = useState<'idle' | 'pendiente' | 'guardando' | 'guardado' | 'error'>('idle');

  const [mensajes, setMensajes] = useState<Mensaje[]>([]);
  const [texto, setTexto] = useState('');
  const [enviando, setEnviando] = useState(false);
  const [sinSesion, setSinSesion] = useState(false);

  const [propuesta, setPropuesta] = useState<EventoPropuesta | null>(null);
  const [aplicando, setAplicando] = useState(false);
  const [avances, setAvances] = useState<Record<number, AvanceAccion>>({});
  const [resumen, setResumen] = useState<ResumenPropuesta | null>(null);
  const [errorAplicar, setErrorAplicar] = useState<string | null>(null);

  const [ancho, setAncho] = useState(50);
  const contenedor = useRef<HTMLDivElement>(null);
  const finMensajes = useRef<HTMLDivElement>(null);

  // Referencias al último valor para los manejadores de eventos y los guardados diferidos.
  const refs = useRef({ guildId, plan, mensajes, agente, modelo });
  refs.current = { guildId, plan, mensajes, agente, modelo };
  const planCargado = useRef<{ guildId: string; contenido: string } | null>(null);
  const temporizador = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cargarAgentes = useCallback(async () => {
    setErrorAgentes(null);
    try {
      setAgentes(await listarAgentesIa());
    } catch (e) {
      setErrorAgentes(mensajeError(e, t('Could not detect AI agents')));
    }
  }, []);
  useEffect(() => { void cargarAgentes(); }, [cargarAgentes]);

  // Agente y modelo siempre válidos respecto a lo instalado.
  const agenteActual = agentes?.find((a) => a.agente === agente) ?? null;
  useEffect(() => {
    if (!agentes || agentes.length === 0) return;
    const a = agentes.find((x) => x.agente === agente) ?? agentes[0];
    if (a.agente !== agente) {
      setAgente(a.agente);
      setModelo(a.modelos[0] ?? '');
    } else if (!modelo && a.modelos.length > 0) {
      setModelo(a.modelos[0]);
    }
  }, [agentes, agente, modelo]);

  // Plan y conversación del servidor activo.
  useEffect(() => {
    if (!guildId) return;
    let vigente = true;
    setCargandoServidor(true);
    setErrorServidor(null);
    setMensajes([]);
    setPropuesta(null);
    setResumen(null);
    setAvances({});
    setEstadoGuardado('idle');
    (async () => {
      try {
        const [p, c] = await Promise.all([leerPlanServidor(guildId), leerConversacion(guildId)]);
        if (!vigente) return;
        planCargado.current = { guildId, contenido: p };
        setPlan(p);
        if (c) {
          setMensajes(comoMensajes(c.mensajes));
          setAgente(c.agente);
          setModelo(c.modelo);
        }
      } catch (e) {
        if (vigente) setErrorServidor(mensajeError(e, t('Could not load the plan and the conversation')));
      } finally {
        if (vigente) setCargandoServidor(false);
      }
    })();
    return () => { vigente = false; };
  }, [guildId]);

  const volcarPlan = useCallback(async (id: string, contenido: string) => {
    setEstadoGuardado('guardando');
    try {
      await guardarPlanServidor(id, contenido);
      planCargado.current = { guildId: id, contenido };
      setEstadoGuardado('guardado');
    } catch (e) {
      setEstadoGuardado('error');
      showNotification(mensajeError(e, t('Could not save the plan')), 'error');
    }
  }, [showNotification]);

  // Guardado automático tras una pausa al escribir; al cambiar de servidor o salir se guarda lo pendiente.
  const cambiarPlan = (contenido: string) => {
    setPlan(contenido);
    if (!guildId) return;
    setEstadoGuardado('pendiente');
    if (temporizador.current) clearTimeout(temporizador.current);
    temporizador.current = setTimeout(() => { void volcarPlan(guildId, contenido); }, ESPERA_GUARDADO_MS);
  };
  useEffect(() => () => {
    if (!temporizador.current) return;
    clearTimeout(temporizador.current);
    temporizador.current = null;
    const { guildId: id, plan: contenido } = refs.current;
    const base = planCargado.current;
    if (id && base && base.guildId === id && base.contenido !== contenido) {
      void guardarPlanServidor(id, contenido).catch(() => {});
    }
  }, [guildId]);

  // Eventos del agente y de la aplicación de la propuesta.
  useEffect(() => {
    const editarUltimo = (f: (m: Mensaje) => Mensaje) =>
      setMensajes((ms) => (ms.length && ms[ms.length - 1].rol === 'assistant' ? [...ms.slice(0, -1), f(ms[ms.length - 1])] : ms));
    const bajas = [
      onAsistenteEvento((e) => {
        if (e.tipo === 'texto') {
          editarUltimo((m) => ({ ...m, texto: m.texto ? `${m.texto}\n${e.texto}` : e.texto }));
        } else if (e.tipo === 'herramienta') {
          editarUltimo((m) => ({
            ...m,
            herramientas: m.herramientas.some((h) => h.id === e.id)
              ? m.herramientas.map((h) => (h.id === e.id ? { ...h, ok: e.ok } : h))
              : [...m.herramientas, { id: e.id, nombre: e.nombre, ok: e.ok }],
          }));
        } else if (e.tipo === 'error') {
          if (esErrorSinSesion(e.mensaje)) setSinSesion(true);
          editarUltimo((m) => ({ ...m, error: e.mensaje }));
        }
      }),
      onAsistentePropuesta((p) => { setPropuesta(p); setResumen(null); setAvances({}); setErrorAplicar(null); }),
      onAsistenteAccion((a) => setAvances((v) => ({ ...v, [a.indice]: a }))),
      onAsistenteResumen((r) => { setResumen(r); setAplicando(false); }),
    ];
    return () => { bajas.forEach((b) => b.then((fn) => fn()).catch(() => {})); };
  }, []);

  useEffect(() => { finMensajes.current?.scrollIntoView({ block: 'end' }); }, [mensajes, propuesta, avances]);

  const enviar = async () => {
    const contenido = texto.trim();
    if (!guildId || !contenido || enviando || !agente || !modelo) return;
    setTexto('');
    setSinSesion(false);
    setPropuesta(null);
    setResumen(null);
    setAvances({});
    setEnviando(true);
    setMensajes((ms) => [
      ...ms,
      { id: nuevoId(), rol: 'user', texto: contenido, herramientas: [] },
      { id: nuevoId(), rol: 'assistant', texto: '', herramientas: [] },
    ]);
    try {
      await enviarAlAsistente(contenido, agente, modelo, modo, plan);
    } catch (e) {
      const msg = mensajeError(e, t('The assistant could not answer'));
      if (esErrorSinSesion(msg)) setSinSesion(true);
      setMensajes((ms) => (ms.length && ms[ms.length - 1].rol === 'assistant' && !ms[ms.length - 1].error
        ? [...ms.slice(0, -1), { ...ms[ms.length - 1], error: msg }]
        : ms));
    } finally {
      setEnviando(false);
    }
  };

  // Al terminar un turno se guarda la conversación (ya con la respuesta completa).
  const turnoPrevio = useRef(false);
  useEffect(() => {
    if (turnoPrevio.current && !enviando) {
      const r = refs.current;
      if (r.guildId && r.agente) void guardarConversacion(r.guildId, r.agente, r.modelo, r.mensajes).catch(() => {});
    }
    turnoPrevio.current = enviando;
  }, [enviando]);

  const detener = () => { void cancelarAsistente().catch(() => {}); };

  const reiniciar = async (id: string) => {
    try {
      await nuevaConversacion(id);
      setMensajes([]);
      setPropuesta(null);
      setResumen(null);
      setAvances({});
      setSinSesion(false);
    } catch (e) {
      showNotification(mensajeError(e, t('Could not start a new conversation')), 'error');
    }
  };

  const cambiarAgente = (nombre: string) => {
    if (nombre === agente) return;
    const a = agentes?.find((x) => x.agente === nombre);
    setAgente(nombre);
    setModelo(a?.modelos[0] ?? '');
    // La sesión de un agente no vale para otro: la conversación empieza de cero.
    if (guildId && mensajes.length > 0) void reiniciar(guildId);
  };

  const aplicar = async () => {
    if (!propuesta || aplicando) return;
    setAplicando(true);
    setAvances({});
    setResumen(null);
    setErrorAplicar(null);
    try {
      const r = await aplicarPropuesta(propuesta.propuesta);
      setResumen(r);
      setAvances(Object.fromEntries(r.resultados.map((x) => [x.indice, { ...x, total: r.total }])));
    } catch (e) {
      setErrorAplicar(mensajeError(e, t('Could not apply the proposal')));
    } finally {
      setAplicando(false);
    }
  };

  const importar = async () => {
    if (!guildId) return;
    try {
      const ruta = await open({ multiple: false, filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }] });
      if (typeof ruta !== 'string') return;
      if (plan.trim() && !window.confirm(t('Replace the current plan with the imported file?'))) return;
      cambiarPlan(await readTextFile(ruta));
    } catch (e) {
      showNotification(mensajeError(e, t('Could not import the file')), 'error');
    }
  };

  const exportar = async () => {
    try {
      const ruta = await save({ defaultPath: 'server-plan.md', filters: [{ name: 'Markdown', extensions: ['md'] }] });
      if (!ruta) return;
      await writeTextFile(ruta, plan);
      showNotification(t('Plan exported'), 'success');
    } catch (e) {
      showNotification(mensajeError(e, t('Could not export the plan')), 'error');
    }
  };

  const arrastrar = (e: React.PointerEvent<HTMLDivElement>) => {
    const caja = contenedor.current;
    if (!caja) return;
    e.preventDefault();
    const mover = (ev: PointerEvent) => {
      const r = caja.getBoundingClientRect();
      setAncho(Math.min(PANEL_MAX, Math.max(PANEL_MIN, ((ev.clientX - r.left) / r.width) * 100)));
    };
    const soltar = () => {
      window.removeEventListener('pointermove', mover);
      window.removeEventListener('pointerup', soltar);
    };
    window.addEventListener('pointermove', mover);
    window.addEventListener('pointerup', soltar);
  };

  const accionesOrdenadas = useMemo(() => (propuesta ? ordenar(propuesta.propuesta.acciones) : []), [propuesta]);

  if (!guildId) {
    return <EstadoVacio titulo={t('No server selected')} descripcion={t('Pick a server in the left column to write its plan and talk to the assistant.')} />;
  }
  if (errorAgentes) return <ErrorPanel mensaje={errorAgentes} onReintentar={() => { void cargarAgentes(); }} />;
  if (!agentes) return <Cargando lineas={4} />;
  if (agentes.length === 0) {
    return (
      <EstadoVacio titulo={t('No AI agent installed')} descripcion={t('The assistant needs the claude or opencode command line. Install one of them, sign in with it, and press Check again.')}>
        <ul className="asistente__instalar">
          <li><strong>claude</strong>: <code>npm install -g @anthropic-ai/claude-code</code>, {t('then run')} <code>claude</code> {t('and sign in')}</li>
          <li><strong>opencode</strong>: <code>npm install -g opencode-ai</code>, {t('then run')} <code>opencode auth login</code></li>
        </ul>
        <Button variante="secundario" onClick={() => { setAgentes(null); void cargarAgentes(); }}>{t('Check again')}</Button>
      </EstadoVacio>
    );
  }
  if (errorServidor) return <ErrorPanel mensaje={errorServidor} />;
  if (cargandoServidor) return <Cargando lineas={4} />;

  const hayMensajes = mensajes.length > 0;
  const total = accionesOrdenadas.length;
  const hechas = Object.keys(avances).length;
  const textoGuardado = {
    idle: '', pendiente: t('Unsaved changes…'), guardando: t('Saving…'), guardado: t('Saved'), error: t('Not saved'),
  }[estadoGuardado];

  return (
    <div className="vista vista--llena asistente">
      <div className="asistente__paneles" ref={contenedor}>
        <section className="asistente__panel" style={{ flexBasis: `${ancho}%` }} aria-label={t('Server plan')}>
          <header className="asistente__cabecera">
            <h3>{t('Server plan')}</h3>
            <div className="asistente__acciones">
              <Button variante="secundario" tamano="sm" onClick={() => { void importar(); }}><Upload size={14} /> {t('Import .md')}</Button>
              <Button variante="secundario" tamano="sm" onClick={() => { void exportar(); }}><Download size={14} /> {t('Export .md')}</Button>
            </div>
          </header>
          <textarea
            className="ui-control asistente__plan"
            value={plan}
            spellCheck={false}
            aria-label={t('Server plan')}
            placeholder={t('Describe your server in Markdown: categories, channels with their description, roles with permissions, and the permissions of each role in each channel.')}
            onChange={(e) => cambiarPlan(e.target.value)}
          />
          <footer className="asistente__pie">
            <span>{t('{count} characters', { count: plan.length.toLocaleString() })}</span>
            <span role="status">{textoGuardado}</span>
          </footer>
        </section>

        <div
          className="asistente__divisor"
          role="separator"
          aria-orientation="vertical"
          aria-label={t('Resize panels')}
          aria-valuenow={Math.round(ancho)}
          aria-valuemin={PANEL_MIN}
          aria-valuemax={PANEL_MAX}
          tabIndex={0}
          onPointerDown={arrastrar}
          onKeyDown={(e) => {
            if (e.key === 'ArrowLeft') setAncho((a) => Math.max(PANEL_MIN, a - 2));
            if (e.key === 'ArrowRight') setAncho((a) => Math.min(PANEL_MAX, a + 2));
          }}
        />

        <section className="asistente__panel asistente__panel--chat" aria-label={t('Chat')}>
          <header className="asistente__cabecera asistente__cabecera--chat">
            <Select aria-label={t('Agent')} value={agente} onChange={(e) => cambiarAgente(e.target.value)} disabled={enviando}>
              {agentes.map((a) => <option key={a.agente} value={a.agente}>{a.agente}</option>)}
            </Select>
            {agenteActual && agenteActual.modelos.length > 0 ? (
              <Select aria-label={t('Model')} value={modelo} onChange={(e) => setModelo(e.target.value)} disabled={enviando}>
                {!agenteActual.modelos.includes(modelo) && modelo && <option value={modelo}>{modelo}</option>}
                {agenteActual.modelos.map((m) => <option key={m} value={m}>{m}</option>)}
              </Select>
            ) : (
              <Input aria-label={t('Model')} value={modelo} placeholder={t('Model name')} onChange={(e) => setModelo(e.target.value)} disabled={enviando} />
            )}
            <div className="asistente__modo" role="group" aria-label={t('Mode')}>
              <button type="button" className={modo === 'read' ? 'activo' : ''} aria-pressed={modo === 'read'} disabled={enviando} onClick={() => setModo('read')}>{t('Read')}</button>
              <button type="button" className={modo === 'execute' ? 'activo' : ''} aria-pressed={modo === 'execute'} disabled={enviando} onClick={() => setModo('execute')}>{t('Execute')}</button>
            </div>
            <Button variante="ghost" tamano="sm" disabled={enviando || aplicando || !hayMensajes} onClick={() => { void reiniciar(guildId); }}>{t('New conversation')}</Button>
          </header>

          <div className="asistente__mensajes" aria-live="polite">
            {!hayMensajes && (
              <EstadoVacio
                titulo={t('Talk to the assistant')}
                descripcion={modo === 'read'
                  ? t('Read mode: ask about your server or your plan. The assistant cannot change anything.')
                  : t('Execute mode: ask it to build your plan. It proposes the changes and you approve them.')}
              />
            )}
            {sinSesion && (
              <div className="asistente__aviso" role="alert">
                <TriangleAlert size={16} />
                <span>{t('The AI agent is not signed in. Sign in with its command line and try again')}: <code>{agente === 'opencode' ? 'opencode auth login' : 'claude'}</code></span>
              </div>
            )}
            {mensajes.map((m) => (
              <article key={m.id} className={`asistente__mensaje asistente__mensaje--${m.rol}`}>
                {m.herramientas.length > 0 && (
                  <div className="asistente__chips">
                    {m.herramientas.map((h) => (
                      <Badge key={h.id} tono={h.ok === null ? 'info' : h.ok ? 'exito' : 'peligro'}>{h.nombre}</Badge>
                    ))}
                  </div>
                )}
                {m.texto && <p className="asistente__texto">{m.texto}</p>}
                {!m.texto && !m.error && m.rol === 'assistant' && enviando && <span className="ui-skeleton" style={{ width: '40%' }} />}
                {m.error && <p className="asistente__error" role="alert">{m.error}</p>}
              </article>
            ))}

            {propuesta && (
              <section className="asistente__propuesta" aria-label={t('Proposal')}>
                <h4>{t('Proposal')}</h4>
                {propuesta.propuesta.resumen && <p>{propuesta.propuesta.resumen}</p>}
                <div className="asistente__chips">
                  {propuesta.validacion.cambios.map((c) => <Badge key={c} tono="acento">{c}</Badge>)}
                </div>
                {(aplicando || hechas > 0) && (
                  <div className="asistente__progreso">
                    <div role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={hechas} aria-label={t('Progress')}>
                      <div style={{ width: `${total ? (hechas / total) * 100 : 0}%` }} />
                    </div>
                    <span>{t('{done} of {total}', { done: hechas, total })}</span>
                  </div>
                )}
                {GRUPOS().map((nombre, g) => {
                  const del = accionesOrdenadas.map((a, i) => ({ a, i })).filter(({ a }) => grupoDe(a) === g);
                  if (del.length === 0) return null;
                  return (
                    <div key={nombre} className="asistente__grupo">
                      <h5>{nombre} <span>{del.length}</span></h5>
                      <ul>
                        {del.map(({ a, i }) => (
                          <FilaAccion key={i} texto={a.descripcion || a.herramienta} avance={avances[i]} activa={aplicando && i === hechas} />
                        ))}
                      </ul>
                    </div>
                  );
                })}
                {resumen && (
                  <p className="asistente__resumen" role="status">
                    {t('Done: {done}, failed: {failed}, skipped: {skipped}', { done: resumen.hechas, failed: resumen.fallidas, skipped: resumen.omitidas + resumen.canceladas })}
                  </p>
                )}
                {errorAplicar && <p className="asistente__error" role="alert">{errorAplicar}</p>}
                <div className="asistente__acciones">
                  {aplicando ? (
                    <Button variante="peligro" onClick={detener}><Square size={14} /> {t('Stop')}</Button>
                  ) : resumen ? (
                    <Button variante="secundario" onClick={() => { setPropuesta(null); setResumen(null); setAvances({}); }}>{t('Close')}</Button>
                  ) : (
                    <>
                      <Button onClick={() => { void aplicar(); }} disabled={total === 0}>{t('Apply')}</Button>
                      <Button variante="secundario" onClick={() => setPropuesta(null)}>{t('Cancel')}</Button>
                    </>
                  )}
                </div>
              </section>
            )}
            <div ref={finMensajes} />
          </div>

          <form className="asistente__entrada" onSubmit={(e) => { e.preventDefault(); void enviar(); }}>
            <textarea
              className="ui-control"
              rows={2}
              value={texto}
              aria-label={t('Message')}
              placeholder={t('Write a message (Enter to send, Shift+Enter for a new line)')}
              onChange={(e) => setTexto(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
                  e.preventDefault();
                  void enviar();
                }
              }}
            />
            {enviando ? (
              <Button variante="peligro" onClick={detener}><Square size={14} /> {t('Stop')}</Button>
            ) : (
              <Button type="submit" disabled={!texto.trim() || !modelo || aplicando}>{t('Send')}</Button>
            )}
          </form>
        </section>
      </div>
    </div>
  );
};

const FilaAccion: React.FC<{ texto: string; avance?: AvanceAccion; activa: boolean }> = ({ texto, avance, activa }) => {
  const estado: EstadoAccion | 'pendiente' = avance ? avance.estado : 'pendiente';
  const etiqueta = { pendiente: t('Pending'), ok: t('Done'), error: t('Error'), omitida: t('Skipped') }[estado];
  return (
    <li className={`asistente__accion asistente__accion--${estado}`}>
      <span className="asistente__icono" role="img" aria-label={etiqueta}>
        {estado === 'ok' ? <Check size={14} /> : estado === 'pendiente' ? (activa ? <Loader2 size={14} className="asistente__giro" /> : <CircleDashed size={14} />) : <X size={14} />}
      </span>
      <span>
        {texto}
        {avance && estado !== 'ok' && avance.mensaje && <small>{avance.mensaje}</small>}
      </span>
    </li>
  );
};

export const AsistenteView: React.FC = () => {
  const [pestana, setPestana] = useState<'chat' | 'discord'>('chat');
  return (
    <div className="vista vista--llena" style={{ padding: 0 }}>
      <div className="pestanas" role="tablist" aria-label={t('Assistant sections')} style={{ margin: 0, padding: '0 var(--sp-3)' }}>
        {([['chat', t('Chat')], ['discord', t('Discord replies')]] as ['chat' | 'discord', string][]).map(([id, etiqueta]) => (
          <button key={id} role="tab" aria-selected={pestana === id} className="pestanas__tab" onClick={() => setPestana(id)}>{etiqueta}</button>
        ))}
      </div>
      {pestana === 'chat' ? <ChatAsistente /> : <RespuestasDiscord />}
    </div>
  );
};
