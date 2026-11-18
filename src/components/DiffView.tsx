import type { LineaDiff } from '../types';
import './Asistente.css';

/** Formato compartido del modo Plan y las comparaciones de snapshots. */
export const DiffView = ({ lineas, label }: { lineas: LineaDiff[]; label: string }) => (
  <div className="asistente__diff" role="group" aria-label={label}>
    {lineas.map((l, i) => (
      <div key={i} className={l.tipo === 'added' ? 'anadida' : l.tipo === 'removed' ? 'quitada' : ''}>
        {l.tipo === 'added' ? '+ ' : l.tipo === 'removed' ? '− ' : '  '}{l.texto}
      </div>
    ))}
  </div>
);
