import React from 'react';
import { useActiveGuildId, useGuildPanel, useLoadGuildPanel, useServerMap } from '../store/useStore';
import { Cargando, EstadoVacio, ErrorPanel } from './ui';
import './Vistas.css';

/** Muestra el estado cargando/error/vacío común de Canales, Roles y Miembros; si hay datos, pinta los hijos. */
export const EstadoVista: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const activeGuildId = useActiveGuildId();
  const guildPanel = useGuildPanel();
  const loadGuildPanel = useLoadGuildPanel();
  const serverMap = useServerMap();

  if (!activeGuildId) {
    return <EstadoVacio titulo="Ningún servidor seleccionado" descripcion="Elige un servidor en la columna de la izquierda para ver sus datos." />;
  }
  if (guildPanel.error) {
    return <ErrorPanel mensaje={guildPanel.error} onReintentar={() => { void loadGuildPanel(activeGuildId); }} />;
  }
  if (guildPanel.loading) return <Cargando lineas={5} />;
  if (!serverMap) {
    return <EstadoVacio titulo="Sin datos del servidor" descripcion="El mapa del servidor aún no se ha sincronizado. Espera unos segundos o vuelve a abrir el servidor." />;
  }
  return <>{children}</>;
};

export const BuscadorVista: React.FC<{
  valor: string;
  onCambio: (v: string) => void;
  placeholder: string;
  icono: React.ReactNode;
}> = ({ valor, onCambio, placeholder, icono }) => (
  <label className="vista__buscar">
    {icono}
    <input
      className="ui-control"
      type="search"
      value={valor}
      placeholder={placeholder}
      aria-label={placeholder}
      onChange={(e) => onCambio(e.target.value)}
    />
  </label>
);

export const colorHex = (color: number): string => `#${color.toString(16).padStart(6, '0')}`;
