import React, { useState, useRef, useEffect, useMemo } from 'react';
import { useLogs, useClearLogs, useShowNotification } from '../store/useStore';
import type { LogEntry } from '../store/useStore';
import { Trash2, Copy, Download, Search } from 'lucide-react';
import { Button, Input, Select, Badge, EstadoVacio } from './ui';
import { t } from '../i18n';
import './Vistas.css';

type Nivel = 'error' | 'aviso' | 'info' | 'sistema';
type Tono = 'peligro' | 'aviso' | 'info' | 'neutro';

const NIVELES: Record<Nivel, { etiqueta: () => string; tono: Tono }> = {
  error: { etiqueta: () => t('ERROR'), tono: 'peligro' },
  aviso: { etiqueta: () => t('WARNING'), tono: 'aviso' },
  info: { etiqueta: () => t('INFO'), tono: 'info' },
  sistema: { etiqueta: () => t('SYSTEM'), tono: 'neutro' },
};

const nivelDe = (texto: string): Nivel => {
  if (texto.includes('[ERROR]') || /error/i.test(texto)) return 'error';
  if (texto.includes('[WARN]') || /warning|aviso/i.test(texto)) return 'aviso';
  if (texto.includes('[SYSTEM]') || /inicializ|Bot iniciad/.test(texto)) return 'sistema';
  return 'info';
};

const sinEtiqueta = (texto: string) => texto.replace(/^\[(ERROR|WARN|SYSTEM|SENT|DM|INFO)\]\s*/, '');

const linea = (l: LogEntry) => `${l.ts} ${l.texto}`;

export const LogsView: React.FC = () => {
  const logs = useLogs();
  const clearLogs = useClearLogs();
  const showNotification = useShowNotification();
  const [busqueda, setBusqueda] = useState('');
  const [nivel, setNivel] = useState<'todos' | Nivel>('todos');
  const [autoScroll, setAutoScroll] = useState(true);
  const contenedor = useRef<HTMLDivElement>(null);

  const visibles = useMemo(
    () =>
      logs.filter(
        (l) =>
          (nivel === 'todos' || nivelDe(l.texto) === nivel) &&
          l.texto.toLowerCase().includes(busqueda.trim().toLowerCase()),
      ),
    [logs, nivel, busqueda],
  );

  useEffect(() => {
    if (autoScroll && contenedor.current) contenedor.current.scrollTop = contenedor.current.scrollHeight;
  }, [visibles, autoScroll]);

  const copyAll = () => {
    navigator.clipboard.writeText(visibles.map(linea).join('\n'));
    showNotification(t('Logs copied'), 'info');
  };

  const downloadLogs = () => {
    const blob = new Blob([visibles.map(linea).join('\n')], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `argus-logs-${new Date().toISOString().slice(0, 10)}.txt`;
    a.click();
    URL.revokeObjectURL(url);
    showNotification(t('Logs downloaded'), 'info');
  };

  return (
    <div className="vista vista--llena">
      <div className="vista__cabecera">
        <h2 className="vista__solo-lector">{t('Logs')}</h2>
        <div className="logs__filtros">
          <Select
            aria-label={t('Filter by level')}
            value={nivel}
            onChange={(e) => setNivel(e.target.value as 'todos' | Nivel)}
          >
            <option value="todos">{t('All levels')}</option>
            {(Object.keys(NIVELES) as Nivel[]).map((n) => (
              <option key={n} value={n}>{NIVELES[n].etiqueta()}</option>
            ))}
          </Select>
          <label className="vista__buscar">
            <Search size={16} />
            <Input
              type="search"
              aria-label={t('Search the logs')}
              placeholder={t('Search the logs…')}
              value={busqueda}
              onChange={(e) => setBusqueda(e.target.value)}
            />
          </label>
          <label className="ajustes__interruptor">
            <input type="checkbox" checked={autoScroll} onChange={(e) => setAutoScroll(e.target.checked)} />
            <span>{t('Auto-scroll')}</span>
          </label>
          <Button variante="ghost" className="logs__accion" title={t('Copy')} aria-label={t('Copy')} onClick={copyAll} disabled={visibles.length === 0}>
            <Copy size={14} aria-hidden="true" /> <span className="logs__accion-texto">{t('Copy')}</span>
          </Button>
          <Button variante="ghost" className="logs__accion" title={t('Download')} aria-label={t('Download')} onClick={downloadLogs} disabled={visibles.length === 0}>
            <Download size={14} aria-hidden="true" /> <span className="logs__accion-texto">{t('Download')}</span>
          </Button>
          <Button variante="peligro" className="logs__accion" title={t('Clear')} aria-label={t('Clear')} onClick={clearLogs} disabled={logs.length === 0}>
            <Trash2 size={14} aria-hidden="true" /> <span className="logs__accion-texto">{t('Clear')}</span>
          </Button>
        </div>
      </div>

      {visibles.length === 0 ? (
        <EstadoVacio
          titulo={logs.length === 0 ? t('No logs yet') : t('No logs match')}
          descripcion={logs.length === 0 ? t('ARGUS actions will show up here.') : t('Try another level or another search.')}
        />
      ) : (
        <div className="logs__lista" ref={contenedor}>
          {visibles.map((l, i) => {
            const n = NIVELES[nivelDe(l.texto)];
            return (
              <div key={`${l.ts}-${i}`} className="logs__fila">
                <time className="logs__hora" dateTime={l.ts} style={{ color: 'var(--text-3)' }}>
                  {new Date(l.ts).toLocaleTimeString()}
                </time>
                <Badge tono={n.tono}>{n.etiqueta()}</Badge>
                <span className="logs__texto">{sinEtiqueta(l.texto)}</span>
              </div>
            );
          })}
        </div>
      )}

      <span className="vista__cuenta">{t('{shown} of {total} entries', { shown: visibles.length, total: logs.length })}</span>
    </div>
  );
};
