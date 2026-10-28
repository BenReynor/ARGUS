import React from 'react';
import './ui.css';

type Variante = 'primario' | 'secundario' | 'peligro' | 'ghost';
type Tamano = 'sm' | 'md' | 'lg';

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variante?: Variante;
  tamano?: Tamano;
  cargando?: boolean;
}

export const Button: React.FC<ButtonProps> = ({
  variante = 'primario', tamano = 'md', cargando = false, disabled, className, children, type = 'button', ...rest
}) => (
  <button
    type={type}
    {...rest}
    disabled={disabled || cargando}
    aria-busy={cargando}
    className={`ui-btn ui-btn--${variante}${tamano !== 'md' ? ` ui-btn--${tamano}` : ''}${className ? ` ${className}` : ''}`}
  >
    {cargando && <span className="ui-spinner" aria-hidden="true" />}
    {children}
  </button>
);

interface CampoProps {
  etiqueta?: string;
  ayuda?: string;
  error?: string;
}

const Campo: React.FC<CampoProps & { id: string; children: React.ReactNode }> = ({ etiqueta, ayuda, error, id, children }) => {
  if (!etiqueta && !error && !ayuda) return <>{children}</>;
  return (
    <div className="ui-campo">
      {etiqueta && <label htmlFor={id}>{etiqueta}</label>}
      {children}
      {ayuda && <span className="ui-campo__ayuda" id={`${id}-ayuda`}>{ayuda}</span>}
      {error && <span className="ui-campo__error" role="alert">{error}</span>}
    </div>
  );
};

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement> & CampoProps>(
  ({ etiqueta, ayuda, error, className, id, ...rest }, ref) => {
    const auto = React.useId();
    const uid = id ?? auto;
    return (
      <Campo etiqueta={etiqueta} ayuda={ayuda} error={error} id={uid}>
        <input ref={ref} id={uid} aria-invalid={!!error} aria-describedby={ayuda ? `${uid}-ayuda` : undefined} className={`ui-control${className ? ` ${className}` : ''}`} {...rest} />
      </Campo>
    );
  },
);
Input.displayName = 'Input';

export const Select = React.forwardRef<HTMLSelectElement, React.SelectHTMLAttributes<HTMLSelectElement> & CampoProps>(
  ({ etiqueta, ayuda, error, className, id, children, ...rest }, ref) => {
    const auto = React.useId();
    const uid = id ?? auto;
    return (
      <Campo etiqueta={etiqueta} ayuda={ayuda} error={error} id={uid}>
        <select ref={ref} id={uid} aria-invalid={!!error} aria-describedby={ayuda ? `${uid}-ayuda` : undefined} className={`ui-control${className ? ` ${className}` : ''}`} {...rest}>
          {children}
        </select>
      </Campo>
    );
  },
);
Select.displayName = 'Select';

export const Card: React.FC<{ titulo?: string; className?: string; children?: React.ReactNode }> = ({ titulo, className, children }) => (
  <section className={`ui-card${className ? ` ${className}` : ''}`}>
    {titulo && <h3 className="ui-card__titulo">{titulo}</h3>}
    {children}
  </section>
);

export const Badge: React.FC<{ tono?: 'neutro' | 'acento' | 'exito' | 'aviso' | 'peligro' | 'info'; children?: React.ReactNode }> = ({ tono = 'neutro', children }) => (
  <span className={`ui-badge${tono !== 'neutro' ? ` ui-badge--${tono}` : ''}`}>{children}</span>
);

export const Avatar: React.FC<{ src?: string | null; nombre: string; tamano?: number }> = ({ src, nombre, tamano = 32 }) => {
  const [fallo, setFallo] = React.useState(false);
  const iniciales = nombre.trim().split(/\s+/).slice(0, 2).map((p) => p[0]?.toUpperCase() ?? '').join('') || '?';
  return (
    <span className="ui-avatar" style={{ '--ui-avatar-size': `${tamano}px` } as React.CSSProperties} role="img" aria-label={nombre}>
      {src && !fallo ? <img src={src} alt="" onError={() => setFallo(true)} /> : iniciales}
    </span>
  );
};

export const EstadoVacio: React.FC<{ titulo: string; descripcion?: string; children?: React.ReactNode }> = ({ titulo, descripcion, children }) => (
  <div className="ui-estado">
    <span className="ui-estado__titulo">{titulo}</span>
    {descripcion && <span>{descripcion}</span>}
    {children}
  </div>
);

export const Cargando: React.FC<{ lineas?: number }> = ({ lineas = 3 }) => (
  <div role="status" aria-label="Cargando">
    {Array.from({ length: lineas }, (_, i) => (
      <span key={i} className="ui-skeleton" style={{ width: `${100 - i * 15}%` }} />
    ))}
  </div>
);

export const ErrorPanel: React.FC<{ mensaje: string; onReintentar?: () => void }> = ({ mensaje, onReintentar }) => (
  <div className="ui-estado ui-estado--error" role="alert">
    <span className="ui-estado__titulo">Algo salió mal</span>
    <span>{mensaje}</span>
    {onReintentar && <Button variante="secundario" onClick={onReintentar}>Reintentar</Button>}
  </div>
);
