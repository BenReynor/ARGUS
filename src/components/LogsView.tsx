import React, { useState, useRef, useEffect, useMemo } from 'react';
import { useLogs, useClearLogs, useShowNotification } from '../store/useStore';
import type { LogEntry } from '../store/useStore';
import { Trash2, Copy, Download, Search } from 'lucide-react';
import { Button, Input, Select, Badge, EstadoVacio } from './ui';
import './Vistas.css';

type Nivel = 'error' | 'aviso' | 'info' | 'sistema';
type Tono = 'peligro' | 'aviso' | 'info' | 'neutro';

const NIVELES: Record<Nivel, { etiqueta: string; tono: Tono }> = {
  error: { etiqueta: 'ERROR', tono: 'peligro' },
  aviso: { etiqueta: 'AVISO', tono: 'aviso' },
  info: { etiqueta: 'INFO', tono: 'info' },
  sistema: { etiqueta: 'SISTEMA', tono: 'neutro' },
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
    showNotification('Logs copiados', 'info');
  };

  const downloadLogs = () => {
    const blob = new Blob([visibles.map(linea).join('\n')], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `argus-logs-${new Date().toISOString().slice(0, 10)}.txt`;
    a.click();
    URL.revokeObjectURL(url);
    showNotification('Logs descargados', 'info');
  };

  return (
    <div className="vista vista--llena">
      <div className="vista__cabecera">
        <h2>Logs</h2>
        <div className="logs__filtros">
          <Select
            aria-label="Filtrar por nivel"
            value={nivel}
            onChange={(e) => setNivel(e.target.value as 'todos' | Nivel)}
          >
            <option value="todos">Todos los niveles</option>
            {(Object.keys(NIVELES) as Nivel[]).map((n) => (
              <option key={n} value={n}>{NIVELES[n].etiqueta}</option>
            ))}
          </Select>
          <label className="vista__buscar">
            <Search size={16} />
            <Input
              type="search"
              aria-label="Buscar en los logs"
              placeholder="Buscar en los logs…"
              value={busqueda}
              onChange={(e) => setBusqueda(e.target.value)}
            />
          </label>
          <label className="ajustes__interruptor">
            <input type="checkbox" checked={autoScroll} onChange={(e) => setAutoScroll(e.target.checked)} />
            <span>Auto-scroll</span>
          </label>
          <Button variante="ghost" onClick={copyAll} disabled={visibles.length === 0}>
            <Copy size={14} /> Copiar
          </Button>
          <Button variante="ghost" onClick={downloadLogs} disabled={visibles.length === 0}>
            <Download size={14} /> Descargar
          </Button>
          <Button variante="peligro" onClick={clearLogs} disabled={logs.length === 0}>
            <Trash2 size={14} /> Limpiar
          </Button>
        </div>
      </div>

      {visibles.length === 0 ? (
        <EstadoVacio
          titulo={logs.length === 0 ? 'Aún no hay logs' : 'Ningún log coincide'}
          descripcion={logs.length === 0 ? 'Las acciones de ARGUS irán apareciendo aquí.' : 'Prueba con otro nivel o con otra búsqueda.'}
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
                <Badge tono={n.tono}>{n.etiqueta}</Badge>
                <span className="logs__texto">{sinEtiqueta(l.texto)}</span>
              </div>
            );
          })}
        </div>
      )}

      <span className="vista__cuenta">{visibles.length} de {logs.length} entradas</span>
    </div>
  );
};
