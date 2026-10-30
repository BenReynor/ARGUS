import React, { useEffect, useRef } from 'react';
import { useStore, useActiveGuildId, useGuildPanel, useLoadGuildPanel, useServerMap } from '../store/useStore';
import { Button, Cargando, EstadoVacio, ErrorPanel } from './ui';
import { t } from '../i18n';
import './Vistas.css';

/** Muestra el estado cargando/error/vacío común de Canales, Roles y Miembros; si hay datos, pinta los hijos. */
export const EstadoVista: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const activeGuildId = useActiveGuildId();
  const guildPanel = useGuildPanel();
  const loadGuildPanel = useLoadGuildPanel();
  const serverMap = useServerMap();
  const refreshServerMap = useStore((s) => s.refreshServerMap);

  if (!activeGuildId) {
    return <EstadoVacio titulo={t('No server selected')} descripcion={t('Pick a server in the left column to see its data.')} />;
  }
  if (guildPanel.error) {
    return <ErrorPanel mensaje={guildPanel.error} onReintentar={() => { void loadGuildPanel(activeGuildId); }} />;
  }
  if (guildPanel.loading) return <Cargando lineas={5} />;
  if (!serverMap) {
    return (
      <EstadoVacio titulo={t('Loading the server data')} descripcion={t('They are read from Discord when you pick the server. If they do not appear, press Load again (the reason is in Logs).')}>
        <Button variante="secundario" onClick={() => { void refreshServerMap(); }}>{t('Load again')}</Button>
      </EstadoVacio>
    );
  }
  return <>{children}</>;
};

/** Cabecera común: título y contador a la izquierda, acciones arriba a la derecha. */
export const CabeceraVista: React.FC<{ titulo: string; cuenta?: number; children?: React.ReactNode }> = ({ titulo, cuenta, children }) => (
  <header className="vista__cabecera">
    <h2>
      {titulo}
      {cuenta !== undefined && <span className="vista__cuenta"> {cuenta.toLocaleString()}</span>}
    </h2>
    {children && <div className="vista__acciones">{children}</div>}
  </header>
);

/** Buscador de la vista; la tecla `/` lo enfoca (si no se escribe en otro campo) y Escape lo limpia. */
export const BuscadorVista: React.FC<{
  valor: string;
  onCambio: (v: string) => void;
  placeholder: string;
  icono: React.ReactNode;
}> = ({ valor, onCambio, placeholder, icono }) => {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const alPulsar = (e: KeyboardEvent) => {
      if (e.key !== '/' || e.ctrlKey || e.metaKey || e.altKey) return;
      const el = e.target as HTMLElement | null;
      if (el && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName))) return;
      e.preventDefault();
      ref.current?.focus();
    };
    window.addEventListener('keydown', alPulsar);
    return () => window.removeEventListener('keydown', alPulsar);
  }, []);
  return (
    <label className="vista__buscar">
      {icono}
      <input
        ref={ref}
        className="ui-control"
        type="search"
        value={valor}
        placeholder={`${placeholder} ( / )`}
        aria-label={placeholder}
        aria-keyshortcuts="/"
        onChange={(e) => onCambio(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            if (valor) onCambio('');
            else ref.current?.blur();
          }
        }}
      />
    </label>
  );
};

export const colorHex = (color: number): string => `#${color.toString(16).padStart(6, '0')}`;
