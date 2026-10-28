import React, { useEffect, useRef, useState } from 'react';
import { useServerMap, useBotStatus, useAddLog, useShowNotification, useSetActiveTab, useConfig, useActiveGuildId, useGuildPanel, useLoadGuildPanel, PERMISOS_REQUERIDOS } from '../store/useStore';
import { resyncMap, onSyncProgress } from '../api/tauri';
import type { SyncProgress } from '../api/tauri';
import { Avatar, Badge, Button, Card, Cargando, EstadoVacio, ErrorPanel } from './ui';
import './Dashboard.css';

interface Actividad {
  hora: string;
  texto: string;
}

const MAX_ACTIVIDAD = 8;

/** Por qué importa cada permiso del bot; se muestra solo cuando falta. */
const EXPLICACION_PERMISO: Record<string, string> = {
  'Ver canales': 'Sin él el bot no ve los canales ni puede sincronizar el mapa.',
  'Enviar mensajes': 'No podrá enviar mensajes ni respuestas.',
  'Leer el historial de mensajes': 'No podrá leer mensajes anteriores de los canales.',
  'Gestionar canales': 'No podrá crear, editar ni borrar canales.',
  'Gestionar roles': 'No podrá crear, editar ni asignar roles.',
  'Moderar miembros': 'No podrá aplicar timeouts a miembros.',
  'Expulsar miembros': 'No podrá expulsar miembros.',
  'Banear miembros': 'No podrá banear miembros.',
};

const ahora = () => new Date().toLocaleTimeString();

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
  const anterior = useRef<{ canales: number; roles: number; miembros: number } | null>(null);

  useEffect(() => {
    const desuscribir = onSyncProgress(setProgress);
    return () => {
      desuscribir.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const canales = serverMap?.channels.length ?? 0;
  const roles = serverMap?.roles.length ?? 0;
  const miembros = serverMap?.members.filter((m) => !m.bot).length ?? 0;

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
    registra('Canales', previo.canales, canales);
    registra('Roles', previo.roles, roles);
    registra('Miembros', previo.miembros, miembros);
    if (cambios.length === 0) cambios.push({ hora: ahora(), texto: 'Mapa actualizado sin cambios de estructura' });
    setActividad((a) => [...cambios, ...a].slice(0, MAX_ACTIVIDAD));
  }, [serverMap]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleResync = async () => {
    setResyncing(true);
    setProgress(null);
    try {
      await resyncMap();
      addLog('[SYSTEM] Mapa resincronizado');
      showNotification('Mapa resincronizado', 'success');
    } catch (e) {
      showNotification('Error al resincronizar: ' + String(e), 'error');
    } finally {
      setResyncing(false);
    }
  };

  // Estados sin contenido: sin bot, sin servidor, cargando, error.
  if (!config.active_bot_id) {
    return (
      <div className="dash">
        <EstadoVacio titulo="No hay ningún bot activo" descripcion="Registra un bot con su token y elígelo como activo para ver sus servidores.">
          <Button onClick={() => setActiveTab('settings')}>Ir a Ajustes</Button>
        </EstadoVacio>
      </div>
    );
  }
  if (!activeGuildId) {
    return (
      <div className="dash">
        <EstadoVacio titulo="Ningún servidor seleccionado" descripcion="Elige un servidor en la columna de la izquierda. Si no aparece ninguno, invita al bot desde Ajustes.">
          <Button variante="secundario" onClick={() => setActiveTab('settings')}>Invitar al bot</Button>
        </EstadoVacio>
      </div>
    );
  }
  if (guildPanel.loading && !guildPanel.guild) {
    return (
      <div className="dash">
        <Card><Cargando lineas={4} /></Card>
      </div>
    );
  }
  if (guildPanel.error) {
    return (
      <div className="dash">
        <ErrorPanel mensaje={guildPanel.error} onReintentar={() => { void loadGuildPanel(activeGuildId); }} />
      </div>
    );
  }

  const guild = guildPanel.guild;
  const faltan = guildPanel.permissions ? PERMISOS_REQUERIDOS.filter((p) => !guildPanel.permissions!.includes(p)) : [];
  const icono = guild?.icon ? `https://cdn.discordapp.com/icons/${guild.id}/${guild.icon}.png?size=64` : null;
  const enVivo = botStatus.connected;

  return (
    <div className="dash">
      <Card>
        <div className="dash__cabecera">
          <Avatar src={icono} nombre={guild?.name ?? 'Servidor'} tamano={56} />
          <div className="dash__nombre">
            <h2>{guild?.name ?? 'Servidor'}</h2>
            {guild?.member_count != null && (
              <span className="dash__kpi-etiqueta">~{guild.member_count.toLocaleString()} miembros (aproximado)</span>
            )}
          </div>
          <Badge tono={enVivo ? 'exito' : 'peligro'}>{enVivo ? '● Gateway en vivo' : '○ Gateway desconectado'}</Badge>
        </div>
      </Card>

      <div className="dash__kpis">
        {[
          { etiqueta: 'Miembros', valor: miembros },
          { etiqueta: 'Canales', valor: canales },
          { etiqueta: 'Roles', valor: roles },
        ].map((k) => (
          <Card key={k.etiqueta}>
            <div className="dash__kpi">
              <span className="dash__kpi-valor">{serverMap ? k.valor.toLocaleString() : '—'}</span>
              <span className="dash__kpi-etiqueta">{k.etiqueta}</span>
            </div>
          </Card>
        ))}
      </div>

      <Card titulo="Permisos del bot">
        {guildPanel.permissions === null ? (
          <Cargando lineas={3} />
        ) : (
          <ul className="dash__permisos">
            {PERMISOS_REQUERIDOS.map((permiso) => {
              const tiene = guildPanel.permissions!.includes(permiso);
              return (
                <li key={permiso} className="dash__permiso">
                  <Badge tono={tiene ? 'exito' : 'aviso'}>{tiene ? '✓' : '✗'}</Badge>
                  <span>{permiso}</span>
                  {!tiene && <span className="dash__permiso-nota">{EXPLICACION_PERMISO[permiso] ?? 'Algunas acciones estarán desactivadas.'}</span>}
                </li>
              );
            })}
          </ul>
        )}
        {faltan.length > 0 && (
          <p className="dash__permiso-nota">Faltan {faltan.length} permisos: concédelos al rol del bot en Discord para activar esas acciones.</p>
        )}
      </Card>

      <Card titulo="Sincronización">
        <div className="dash__sync">
          <Button variante="secundario" cargando={resyncing} onClick={handleResync}>
            {resyncing ? 'Resincronizando…' : 'Resincronizar'}
          </Button>
          <div className="dash__progreso" aria-live="polite">
            {resyncing && progress ? (
              <>
                <progress max={progress.total} value={progress.step} />
                <span>Paso {progress.step}/{progress.total}: {progress.message}</span>
              </>
            ) : (
              <span className="dash__kpi-etiqueta">Repite la sincronización completa del servidor.</span>
            )}
          </div>
        </div>
      </Card>

      <Card titulo="Actividad reciente">
        {botStatus.last_error && <Badge tono="peligro">{botStatus.last_error}</Badge>}
        {actividad.length === 0 ? (
          <span className="dash__kpi-etiqueta">Aún no hay cambios en el mapa.</span>
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
