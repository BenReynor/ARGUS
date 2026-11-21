import React, { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from './ui';
import { t } from '../i18n';
import { extraerSugerencia, SugerenciaPendiente } from '../sugerenciaPlantilla';

/** A model's output is untrusted: the backend validates it before display and import. */
export function SugerenciaPlantilla({ texto }: { texto: string }) {
  const data = extraerSugerencia(texto);
  const sugerencia = useMemo(() => data ? new SugerenciaPendiente(data) : null, [data]);
  const [descartada, setDescartada] = useState(false);
  const [vista, setVista] = useState(false);
  const [error, setError] = useState(false);
  const [busy, setBusy] = useState(false);
  if (!data || !sugerencia || descartada) return null;
  const revisar = async () => {
    setBusy(true); setError(false);
    try { await sugerencia.revisar(async (documento) => { await invoke('preview_template_file', { data: documento }); }); setVista(true); }
    catch { setError(true); } finally { setBusy(false); }
  };
  const aceptar = async () => {
    setBusy(true); setError(false);
    try { await sugerencia.aceptar(async (documento) => { await invoke('import_template_file', { data: documento, confirmed: true }); }); setDescartada(true); }
    catch { setError(true); } finally { setBusy(false); }
  };
  return <section aria-label={t('Suggested template')}>
    <h4>{t('Suggested template')}</h4>
    {vista && <><p>{t('Accepting saves a new copy in the library.')}</p><pre>{data}</pre></>}
    {error && <p role="alert">{t('Could not save template. Check the name and structure.')}</p>}
    {vista ? <Button disabled={busy} onClick={() => void aceptar()}>{t('Accept and save copy')}</Button> : <Button disabled={busy} onClick={() => void revisar()}>{t('Review suggestion')}</Button>}
    <Button variante="secundario" disabled={busy} onClick={() => { sugerencia.descartar(); setDescartada(true); }}>{t('Discard')}</Button>
  </section>;
}
