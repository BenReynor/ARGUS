import React, { useMemo, useState } from 'react';
import { useStore } from '../store/useStore';
import { t } from '../i18n';
import { buscarManual, seccionesDeVista, seccionesManual } from '../manual';

/** Manual de la app: todas las funciones por áreas, con buscador; el botón «?» abre la sección de la vista. */
export const ManualView: React.FC = () => {
  const vista = useStore((s) => s.manualVista);
  const [consulta, setConsulta] = useState('');
  const todas = seccionesManual(t);
  const resultado = useMemo(() => buscarManual(todas, consulta), [todas, consulta]);
  const destacadas = new Set(seccionesDeVista(todas, vista).map((s) => s.id));
  // Las de la vista de origen primero y abiertas; con búsqueda se abren todas las coincidencias.
  const ordenadas = [...resultado].sort((a, b) => Number(destacadas.has(b.id)) - Number(destacadas.has(a.id)));
  const filtrando = consulta.trim() !== '';

  return (
    <div className="manual-vista">
      <input
        type="search"
        value={consulta}
        onChange={(e) => setConsulta(e.target.value)}
        placeholder={t('Search the manual')}
        aria-label={t('Search the manual')}
      />
      {ordenadas.length === 0 && <p>{t('No manual section matches your search')}</p>}
      {ordenadas.map((s) => (
        <details key={`${s.id}-${vista}-${filtrando}`} open={filtrando || (vista !== 'manual' && destacadas.has(s.id))}>
          <summary>{s.title}</summary>
          <h3>{t('What it does')}</h3>
          <p>{s.que}</p>
          <h3>{t('How to use it')}</h3>
          <p>{s.como}</p>
          <h3>{t('Limits')}</h3>
          <p>{s.limites}</p>
        </details>
      ))}
    </div>
  );
};
