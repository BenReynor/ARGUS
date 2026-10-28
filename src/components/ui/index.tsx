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

const ENFOCABLES = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface DialogoProps {
  abierto: boolean;
  titulo: string;
  descripcion?: string;
  onCerrar: () => void;
  pie?: React.ReactNode;
  children?: React.ReactNode;
}

/** Diálogo modal: atrapa el foco, Escape cierra y al cerrar devuelve el foco al elemento previo. */
export const Dialogo: React.FC<DialogoProps> = ({ abierto, titulo, descripcion, onCerrar, pie, children }) => {
  const ref = React.useRef<HTMLDivElement>(null);
  const idTitulo = React.useId();
  const idDesc = React.useId();
  const cerrarRef = React.useRef(onCerrar);
  cerrarRef.current = onCerrar;

  React.useEffect(() => {
    if (!abierto) return;
    const previo = document.activeElement as HTMLElement | null;
    const caja = ref.current;
    const enfocables = () => Array.from(caja?.querySelectorAll<HTMLElement>(ENFOCABLES) ?? []);
    (caja?.querySelector<HTMLElement>('[data-autofocus]') ?? enfocables()[0] ?? caja)?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.stopPropagation(); cerrarRef.current(); return; }
      if (e.key !== 'Tab') return;
      const lista = enfocables();
      if (lista.length === 0) { e.preventDefault(); return; }
      const primero = lista[0];
      const ultimo = lista[lista.length - 1];
      const activo = document.activeElement;
      if (e.shiftKey && (activo === primero || !caja?.contains(activo))) { e.preventDefault(); ultimo.focus(); }
      else if (!e.shiftKey && (activo === ultimo || !caja?.contains(activo))) { e.preventDefault(); primero.focus(); }
    };
    document.addEventListener('keydown', onKey, true);
    return () => {
      document.removeEventListener('keydown', onKey, true);
      previo?.focus?.();
    };
  }, [abierto]);

  if (!abierto) return null;
  return (
    <div className="ui-dialogo__fondo" onMouseDown={(e) => { if (e.target === e.currentTarget) onCerrar(); }}>
      <div ref={ref} className="ui-dialogo" role="dialog" aria-modal="true" aria-labelledby={idTitulo} aria-describedby={descripcion ? idDesc : undefined} tabIndex={-1}>
        <h2 className="ui-dialogo__titulo" id={idTitulo}>{titulo}</h2>
        {descripcion && <p className="ui-dialogo__desc" id={idDesc}>{descripcion}</p>}
        <div className="ui-dialogo__cuerpo">{children}</div>
        {pie && <div className="ui-dialogo__pie">{pie}</div>}
      </div>
    </div>
  );
};

interface ConfirmarPeligrosoProps {
  abierto: boolean;
  titulo: string;
  descripcion?: string;
  /** Nombre que el usuario debe escribir para habilitar el borrado. */
  nombre: string;
  textoConfirmar?: string;
  onConfirmar: () => Promise<void> | void;
  onCerrar: () => void;
}

/** Confirmación destructiva: el botón solo se activa al escribir el nombre exacto del elemento. */
export const ConfirmarPeligroso: React.FC<ConfirmarPeligrosoProps> = ({ abierto, titulo, descripcion, nombre, textoConfirmar = 'Borrar', onConfirmar, onCerrar }) => {
  const [escrito, setEscrito] = React.useState('');
  const [trabajando, setTrabajando] = React.useState(false);
  React.useEffect(() => { if (abierto) { setEscrito(''); setTrabajando(false); } }, [abierto]);
  const coincide = escrito === nombre;
  const confirmar = async () => {
    if (!coincide || trabajando) return;
    setTrabajando(true);
    try { await onConfirmar(); } finally { setTrabajando(false); }
  };
  return (
    <Dialogo
      abierto={abierto}
      titulo={titulo}
      descripcion={descripcion}
      onCerrar={onCerrar}
      pie={
        <>
          <Button variante="secundario" onClick={onCerrar}>Cancelar</Button>
          <Button variante="peligro" disabled={!coincide} cargando={trabajando} onClick={confirmar}>{textoConfirmar}</Button>
        </>
      }
    >
      <form onSubmit={(e) => { e.preventDefault(); void confirmar(); }}>
        <Input
          data-autofocus
          etiqueta={`Escribe «${nombre}» para confirmar`}
          value={escrito}
          onChange={(e) => setEscrito(e.target.value)}
          autoComplete="off"
          spellCheck={false}
        />
      </form>
    </Dialogo>
  );
};
