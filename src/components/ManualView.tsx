import React, { useMemo, useState } from 'react';
import { ChevronRight, Search } from 'lucide-react';
import { useStore } from '../store/useStore';
import { t } from '../i18n';
import { buscarManual, seccionAbierta, seccionesDeVista, seccionesManual } from '../manual';
import { CabeceraVista } from './EstadoVista';
import './Vistas.css';

/** Manual de la app: todas las funciones por áreas, con buscador; el botón «?» abre la sección de la vista. */
export const ManualView: React.FC = () => {
  const vista = useStore((s) => s.manualVista);
  const [consulta, setConsulta] = useState('');
  const todas = seccionesManual(t);
  const resultado = useMemo(() => buscarManual(todas, consulta), [todas, consulta]);
  const destacadas = new Set(seccionesDeVista(todas, vista).map((s) => s.id));
  const origen = vista !== 'manual';
  // Las de la vista de origen primero y abiertas; con búsqueda se abren todas las coincidencias.
  const ordenadas = [...resultado].sort((a, b) => Number(destacadas.has(b.id)) - Number(destacadas.has(a.id)));
  const filtrando = consulta.trim() !== '';

  return (
    <div className="vista manual-vista">
      <CabeceraVista titulo={t('Manual')} />
      <label className="vista__buscar manual-vista__buscar">
        <Search size={16} aria-hidden="true" />
        <input
          className="ui-control"
          type="search"
          value={consulta}
          onChange={(e) => setConsulta(e.target.value)}
          placeholder={t('Search the manual')}
          aria-label={t('Search the manual')}
        />
      </label>
      {ordenadas.length === 0 && <p>{t('No manual section matches your search')}</p>}
      {ordenadas.map((s) => {
        const activa = origen && destacadas.has(s.id);
        return (
          <details
            key={`${s.id}-${vista}-${filtrando}`}
            className={`manual-seccion${activa ? ' manual-seccion--activa' : ''}`}
            open={seccionAbierta(filtrando, vista, destacadas.has(s.id))}
          >
            <summary>
              <ChevronRight size={16} className="manual-seccion__chevron" aria-hidden="true" />
              {s.title}
            </summary>
            <h3>{t('What it does')}</h3>
            <p>{s.que}</p>
            <h3>{t('How to use it')}</h3>
            <p>{s.como}</p>
            <h3>{t('Limits')}</h3>
            <p>{s.limites}</p>
          </details>
        );
      })}
    </div>
  );
};
