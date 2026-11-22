import React, { useEffect, useState } from 'react';
import { Button, Dialogo, Input } from './ui';
import { t } from '../i18n';

export interface PlantillaGuardada { id: string; omitidos: string[] }

/** Diálogo común para guardar una plantilla desde un snapshot o desde el plan. */
export const GuardarPlantilla: React.FC<{
  abierto: boolean;
  nombreInicial: string;
  onCerrar: () => void;
  guardar: (nombre: string, descripcion: string, etiquetas: string) => Promise<PlantillaGuardada>;
}> = ({ abierto, nombreInicial, onCerrar, guardar }) => {
  const [nombre, setNombre] = useState(nombreInicial);
  const [descripcion, setDescripcion] = useState('');
  const [etiquetas, setEtiquetas] = useState('');
  const [ocupado, setOcupado] = useState(false);
  const [error, setError] = useState('');
  const [hecho, setHecho] = useState<PlantillaGuardada | null>(null);
  useEffect(() => {
    if (abierto) { setNombre(nombreInicial); setDescripcion(''); setEtiquetas(''); setError(''); setHecho(null); }
  }, [abierto, nombreInicial]);
  const enviar = async () => {
    setOcupado(true); setError('');
    try { setHecho(await guardar(nombre.trim(), descripcion.trim(), etiquetas)); }
    catch (e) { setError(`${t('Could not save the template.')} ${String(e)}`); }
    finally { setOcupado(false); }
  };
  const cerrar = () => { if (!ocupado) onCerrar(); };
  return <Dialogo abierto={abierto} titulo={t('Save as template')} onCerrar={cerrar}
    pie={hecho ? <Button onClick={cerrar}>{t('Close')}</Button>
      : <Button disabled={!nombre.trim() || ocupado} onClick={() => void enviar()}>{t('Save')}</Button>}>
    {hecho ? <div role="status">
      <p>{t('Template saved to your library.')}</p>
      {hecho.omitidos.length > 0 && <><p>{t('Not included in the template:')}</p><ul>{hecho.omitidos.map((o, i) => <li key={i}>{o}</li>)}</ul></>}
    </div> : <>
      <Input etiqueta={t('Template name')} value={nombre} maxLength={100} disabled={ocupado} onChange={e => setNombre(e.target.value)} />
      <Input etiqueta={t('Description')} value={descripcion} maxLength={1000} disabled={ocupado} onChange={e => setDescripcion(e.target.value)} />
      <Input etiqueta={t('Tags (comma separated)')} value={etiquetas} disabled={ocupado} onChange={e => setEtiquetas(e.target.value)} />
      <p>{t('Members, messages and tokens are never saved in a template.')}</p>
      {error && <p role="alert">{error}</p>}
    </>}
  </Dialogo>;
};
