import React, { useState, useRef, useEffect } from 'react';
import { useLogs, useAddLog, useClearLogs, useShowNotification } from '../store/useStore';
import { Trash2, Copy, Download, Filter, X } from 'lucide-react';
import { Button, Input, EstadoVacio } from './ui';

export const LogsView: React.FC = () => {
  const logs = useLogs();
  const addLog = useAddLog();
  const clearLogs = useClearLogs();
  const showNotification = useShowNotification();
  const [filter, setFilter] = useState('');
  const [autoScroll, setAutoScroll] = useState(true);
  const logContainerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (autoScroll && logContainerRef.current) {
      logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight;
    }
  }, [logs, autoScroll]);

  const filteredLogs = logs.filter(log => 
    log.toLowerCase().includes(filter.toLowerCase())
  );

  const copyAll = () => {
    navigator.clipboard.writeText(logs.join('\n'));
    showNotification('Logs copiados', 'info');
  };

  const downloadLogs = () => {
    const blob = new Blob([logs.join('\n')], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `argus-logs-${new Date().toISOString().slice(0, 10)}.txt`;
    a.click();
    URL.revokeObjectURL(url);
    showNotification('Logs descargados', 'info');
  };

  const getLogClass = (log: string) => {
    if (log.includes('[ERROR]') || log.includes('error')) return 'error';
    if (log.includes('[WARN]') || log.includes('warning')) return 'warn';
    if (log.includes('[SENT]') || log.includes('enviado')) return 'sent';
    if (log.includes('[SYSTEM]') || log.includes('inicializ') || log.includes('Bot iniciad')) return 'system';
    return '';
  };

  return (
    <div className="logs-view">
      <div className="view-header">
        <h2>LOGS DEL SISTEMA</h2>
        <div className="header-controls">
          <div className="search-box">
            <Filter size={16} />
            <Input
              type="text"
              placeholder="Filtrar logs..."
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            {filter && <button className="icon-btn" onClick={() => setFilter('')}>
              <X size={14} />
            </button>}
          </div>
          <label className="toggle-label">
            <input type="checkbox" checked={autoScroll} onChange={(e) => setAutoScroll(e.target.checked)} />
            <span>Auto-scroll</span>
          </label>
          <Button variante="ghost" onClick={copyAll}>
            <Copy size={14} /> COPIAR
          </Button>
          <Button variante="ghost" onClick={downloadLogs}>
            <Download size={14} /> DESCARGAR
          </Button>
          <Button variante="peligro" onClick={clearLogs}>
            <Trash2 size={14} /> LIMPIAR
          </Button>
        </div>
      </div>

      <div className="log-container" ref={logContainerRef}>
        {filteredLogs.length === 0 ? (
          <EstadoVacio titulo={`No hay logs${filter ? ' que coincidan' : ''}`} />
        ) : (
          filteredLogs.map((log, i) => (
            <div key={i} className={`log-entry ${getLogClass(log)}`}>
              <span className="log-line">{log}</span>
            </div>
          ))
        )}
      </div>

      <div className="log-status">
        <span>{filteredLogs.length} / {logs.length} entradas</span>
        <span className="mono">Última actualización: {new Date().toLocaleTimeString()}</span>
      </div>
    </div>
  );
};