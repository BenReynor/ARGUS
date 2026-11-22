import React, { useEffect, useRef, useState } from 'react';
import { useServerMap, useBotStatus, useAddLog, useShowNotification, useSetActiveTab, useConfig, useActiveGuildId, useGuildPanel, useLoadGuildPanel, PERMISOS_REQUERIDOS, useRecuentoMiembros } from '../store/useStore';
import { resyncMap, onSyncProgress, startBot, serverSnapshots, listOperations } from '../api/tauri';
import type { ServerSnapshotItem } from '../api/tauri';
import type { Operation } from '../activity';
import { latestSnapshot, readAuditSummary, recentOperations } from '../panel';
import type { SyncProgress } from '../api/tauri';
import { Avatar, Badge, Button, Card, Cargando, EstadoVacio, ErrorPanel } from './ui';
import { CabeceraVista } from './EstadoVista';
import { t } from '../i18n';
import './Dashboard.css';

interface Actividad {
  hora: string;
  texto: string;
}

const MAX_ACTIVIDAD = 8;

/** Nombre visible de cada permiso (la clave es el nombre que devuelve el backend). */
const nombrePermiso = (permiso: string): string => {
  switch (permiso) {
    case 'Ver canales': return t('View channels');
    case 'Enviar mensajes': return t('Send messages');
    case 'Leer el historial de mensajes': return t('Read message history');
    case 'Gestionar canales': return t('Manage channels');
    case 'Gestionar roles': return t('Manage roles');
    case 'Moderar miembros': return t('Moderate members');
    case 'Expulsar miembros': return t('Kick members');
    case 'Banear miembros': return t('Ban members');
    default: return permiso;
  }
};

/** Por qué importa cada permiso del bot; se muestra solo cuando falta. */
const explicacionPermiso = (permiso: string): string => {
  switch (permiso) {
    case 'Ver canales': return t('Without it the bot cannot see the channels or sync the map.');
    case 'Enviar mensajes': return t('It will not be able to send messages or replies.');
    case 'Leer el historial de mensajes': return t('It will not be able to read earlier messages in channels.');
    case 'Gestionar canales': return t('It will not be able to create, edit or delete channels.');
    case 'Gestionar roles': return t('It will not be able to create, edit or assign roles.');
    case 'Moderar miembros': return t('It will not be able to time out members.');
    case 'Expulsar miembros': return t('It will not be able to kick members.');
    case 'Banear miembros': return t('It will not be able to ban members.');
    default: return t('Some actions will be disabled.');
  }
};

const estadoOperacion = (status: string): string => {
  switch (status) {
    case 'pending': return t('Pending');
    case 'in_progress': return t('In progress');
    case 'verifying': return t('Verifying');
    case 'correct': return t('Verified');
    case 'unverified': return t('Unverified');
    case 'partial': return t('Partial');
    case 'failed': return t('Failed');
    case 'skipped': return t('Skipped');
    case 'cancelled': return t('Cancelled');
    case 'interrupted': return t('Interrupted');
    default: return t('Unknown status');
  }
};

const ahora =() => new Date().toLocaleTimeString();

export const Dashboard: React.FC = () => {
  const serverMap = useServerMap();
  const botStatus = useBotStatus();
  const addLog = useAddLog();
  const showNotification = useShowNotification();
  const setActiveTab = useSetActiveTab();
  const config = useConfig();
  const activeGuildId = useActiveGuildId();
  const guildPanel = useGuildPanel();
  const loadGuildPanel = useLoadGuildPanel();

  const [resyncing, setResyncing] = useState(false);
  const [progress, setProgress] = useState<SyncProgress | null>(null);
  const [actividad, setActividad] = useState<Actividad[]>([]);
  const [snapshots, setSnapshots] = useState<ServerSnapshotItem[] | null>(null);
  const [autoSnap, setAutoSnap] = useState<boolean | null>(null);
  const [operaciones, setOperaciones] = useState<Operation[] | null>(null);
  const [encendiendo, setEncendiendo] = useState(false);
  const anterior = useRef<{ canales: number; roles: number; miembros: number } | null>(null);

  useEffect(() => {
    const desuscribir = onSyncProgress(setProgress);
    return () => {
      desuscribir.then((fn) => fn()).catch(() => {});
    };
  }, []);

  // Solo lectura local (SQLite): ni snapshots ni operaciones llaman a Discord.
  const botId = config.active_bot_id;
  useEffect(() => {
    setSnapshots(null); setAutoSnap(null); setOperaciones(null);
    if (!botId || !activeGuildId) return;
    let activo = true;
    serverSnapshots(botId, activeGuildId, { op: 'list' })
      .then((l) => { if (activo) { setSnapshots(l.snapshots); setAutoSnap(l.schedule.enabled); } })
      .catch(() => { if (activo) setSnapshots([]); });
    listOperations().then((o) => { if (activo) setOperaciones(o); }).catch(() => { if (activo) setOperaciones([]); });
    return () => { activo = false; };
  }, [botId, activeGuildId]);

  const encender = async () => {
    setEncendiendo(true);
    try { await startBot(); } catch (e) { showNotification(String(e), 'error'); } finally { setEncendiendo(false); }
  };

  // Sin categorías (tipo 4), igual que la vista Canales.
  const canales = serverMap?.channels.filter((c) => c.channel_type !== 4).length ?? 0;
  const roles = serverMap?.roles.length ?? 0;
  const recuento = useRecuentoMiembros();
  const miembros = recuento?.valor ?? 0;

  // Actividad reciente: diferencias entre dos versiones consecutivas del mapa.
  useEffect(() => {
    if (!serverMap) {
      anterior.current = null;
      return;
    }
    const previo = anterior.current;
    anterior.current = { canales, roles, miembros };
    if (!previo) return;
    const cambios: Actividad[] = [];
    const registra = (nombre: string, antes: number, despues: number) => {
      if (antes !== despues) cambios.push({ hora: ahora(), texto: `${nombre}: ${antes} → ${despues}` });
    };
    registra(t('Channels'), previo.canales, canales);
    registra(t('Roles'), previo.roles, roles);
    registra(t('Members'), previo.miembros, miembros);
    if (cambios.length === 0) cambios.push({ hora: ahora(), texto: t('Map updated with no structural changes') });
    setActividad((a) => [...cambios, ...a].slice(0, MAX_ACTIVIDAD));
  }, [serverMap]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleResync = async () => {
    setResyncing(true);
    setProgress(null);
    try {
      await resyncMap();
      addLog('[SYSTEM] ' + t('Map resynced'));
      showNotification(t('Map resynced'), 'success');
    } catch (e) {
      showNotification(t('Resync failed: {error}', { error: String(e) }), 'error');
    } finally {
      setResyncing(false);
    }
  };

  // Estados sin contenido: sin bot, sin servidor, cargando, error.
  if (!config.active_bot_id) {
    return (
      <div className="vista dash">
        <EstadoVacio titulo={t('No active bot')} descripcion={t('Register a bot with its token and set it as active to see its servers.')}>
          <Button onClick={() => setActiveTab('settings')}>{t('Go to Settings')}</Button>
        </EstadoVacio>
      </div>
    );
  }
  if (!activeGuildId) {
    return (
      <div className="vista dash">
        <EstadoVacio titulo={t('No server selected')} descripcion={t('Pick a server in the left column. If none appears, invite the bot from Settings.')}>
          <Button variante="secundario" onClick={() => setActiveTab('settings')}>{t('Invite the bot')}</Button>
        </EstadoVacio>
      </div>
    );
  }
  if (guildPanel.loading && !guildPanel.guild) {
    return (
      <div className="vista dash">
        <Card><Cargando lineas={4} /></Card>
      </div>
    );
  }
  if (guildPanel.error) {
    return (
      <div className="vista dash">
        <ErrorPanel mensaje={guildPanel.error} onReintentar={() => { void loadGuildPanel(activeGuildId); }} />
      </div>
    );
  }

  const guild = guildPanel.guild;
  const faltan = guildPanel.permissions ? PERMISOS_REQUERIDOS.filter((p) => !guildPanel.permissions!.includes(p)) : [];
  const icono = guild?.icon ? `https://cdn.discordapp.com/icons/${guild.id}/${guild.icon}.png?size=64` : null;
  const enVivo = botStatus.connected;

  return (
    <div className="vista dash">
      <CabeceraVista titulo={t('Server dashboard')}>
        <Badge tono={enVivo ? 'exito' : 'peligro'}>{enVivo ? t('● Gateway live') : t('○ Gateway disconnected')}</Badge>
        <Button variante="secundario" cargando={resyncing} onClick={handleResync}>
          {resyncing ? t('Resyncing…') : t('Resync')}
        </Button>
      </CabeceraVista>

      <Card>
        <div className="dash__cabecera">
          <Avatar src={icono} nombre={guild?.name ?? t('Server')} tamano={56} />
          <div className="dash__nombre">
            <h3>{guild?.name ?? t('Server')}</h3>
            {recuento && (
              <span className="dash__kpi-etiqueta">
                {recuento.aproximado
                  ? t('~{count} members (approximate)', { count: recuento.valor.toLocaleString() })
                  : t('{count} members', { count: recuento.valor.toLocaleString() })}
              </span>
            )}
          </div>
        </div>
      </Card>

      <div className="dash__kpis">
        {[
          { etiqueta: t('Members'), valor: miembros, aprox: recuento?.aproximado ?? false, disponible: !!recuento },
          { etiqueta: t('Channels'), valor: canales, disponible: !!serverMap },
          { etiqueta: t('Roles'), valor: roles, disponible: !!serverMap },
        ].map((k) => (
          <Card key={k.etiqueta}>
            <div className="dash__kpi">
              <span className="dash__kpi-valor">{k.disponible ? `${'aprox' in k && k.aprox ? '~' : ''}${k.valor.toLocaleString()}` : '—'}</span>
              <span className="dash__kpi-etiqueta">{k.etiqueta}</span>
            </div>
          </Card>
        ))}
      </div>

      <Card titulo={t('Bot permissions')}>
        {guildPanel.permissions === null ? (
          <Cargando lineas={3} />
        ) : (
          <ul className="dash__permisos">
            {PERMISOS_REQUERIDOS.map((permiso) => {
              const tiene = guildPanel.permissions!.includes(permiso);
              return (
                <li key={permiso} className="dash__permiso">
                  <Badge tono={tiene ? 'exito' : 'aviso'}>{tiene ? '✓' : '✗'}</Badge>
                  <span>{nombrePermiso(permiso)}</span>
                  {!tiene && <span className="dash__permiso-nota">{explicacionPermiso(permiso)}</span>}
                </li>
              );
            })}
          </ul>
        )}
        {faltan.length > 0 && (
          <p className="dash__permiso-nota">{t('{count} permissions missing: grant them to the bot role in Discord to enable those actions.', { count: faltan.length })}</p>
        )}
      </Card>

      <Card titulo={t('Bot and Gateway')}>
        <div className="dash__fila">
          <Badge tono={botStatus.running ? 'exito' : 'neutro'}>{botStatus.running ? t('Bot on') : t('Bot off')}</Badge>
          <Badge tono={enVivo ? 'exito' : 'peligro'}>{enVivo ? t('● Gateway live') : t('○ Gateway disconnected')}</Badge>
          {!botStatus.running && <Button variante="secundario" cargando={encendiendo} onClick={encender}>{t('Turn on')}</Button>}
          <Button variante="secundario" onClick={() => setActiveTab('settings')}>{t('Go to Settings')}</Button>
        </div>
      </Card>

      <Card titulo={t('Latest snapshot')}>
        {snapshots === null ? <Cargando lineas={1} /> : (
          <div className="dash__fila">
            <span>{(() => { const u = latestSnapshot(snapshots); return u ? `${u.name} · ${new Date(u.created_at).toLocaleString()}` : t('No snapshots yet.'); })()}</span>
            {autoSnap !== null && <Badge tono={autoSnap ? 'exito' : 'neutro'}>{autoSnap ? t('Automatic snapshots on') : t('Automatic snapshots off')}</Badge>}
            <Button variante="secundario" onClick={() => setActiveTab('backups')}>{t('Open snapshots')}</Button>
          </div>
        )}
      </Card>

      <Card titulo={t('Last audit')}>
        {(() => {
          const a = botId ? readAuditSummary(localStorage, botId, activeGuildId) : null;
          return (
            <div className="dash__fila">
              <span>{a
                ? t('{total} findings ({high} high) · {date}', { total: a.summary.total, high: a.summary.high, date: new Date(a.checked_at).toLocaleString() })
                : t('The permission audit has not been run yet.')}</span>
              <Button variante="secundario" onClick={() => setActiveTab('permission-audit')}>{a ? t('Open audit') : t('Run audit')}</Button>
            </div>
          );
        })()}
      </Card>

      <Card titulo={t('Latest operations')}>
        {operaciones === null ? <Cargando lineas={2} /> : (() => {
          const ult = recentOperations(operaciones, activeGuildId);
          return ult.length === 0 ? (
            <span className="dash__kpi-etiqueta">{t('No operations yet.')}</span>
          ) : (
            <ul className="dash__actividad">
              {ult.map((o) => (
                <li key={o.id}>
                  <span className="dash__hora">{new Date(o.started_at).toLocaleString()}</span>
                  <span>{estadoOperacion(o.status)}</span>
                </li>
              ))}
            </ul>
          );
        })()}
        <Button variante="secundario" onClick={() => setActiveTab('activity')}>{t('Open activity')}</Button>
      </Card>

      <Card titulo={t('Synchronization')}>
        <div className="dash__progreso" aria-live="polite">
          {resyncing && progress ? (
            <>
              <progress max={progress.total} value={progress.step} />
              <span className="num">{t('Step {step}/{total}: {message}', { step: progress.step, total: progress.total, message: progress.message })}</span>
            </>
          ) : (
            <span className="dash__kpi-etiqueta">{t('Press “Resync” (top right) to repeat the full server synchronization.')}</span>
          )}
        </div>
      </Card>

      <Card titulo={t('Recent activity')}>
        {botStatus.last_error && <Badge tono="peligro">{botStatus.last_error}</Badge>}
        {actividad.length === 0 ? (
          <span className="dash__kpi-etiqueta">{t('No map changes yet. Press “Resync” to check whether the server has changed.')}</span>
        ) : (
          <ul className="dash__actividad">
            {actividad.map((a, i) => (
              <li key={i}>
                <span className="dash__hora">{a.hora}</span>
                <span>{a.texto}</span>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
};
