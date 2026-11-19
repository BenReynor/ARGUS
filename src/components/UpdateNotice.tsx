import React, { useEffect, useRef } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useUpdates, updates } from '../store/updates';
import { DOWNLOAD_URL } from '../actualizaciones';
import { t } from '../i18n';
import { Button, Card } from './ui';

export const UpdateNotice: React.FC = () => {
  const { notice, installing, checking, status } = useUpdates();
  const noticeRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (notice) noticeRef.current?.scrollIntoView({ block: 'nearest' });
  }, [notice]);
  if (!notice) return null;
  return <div ref={noticeRef}><Card>
    <div role="status">
      <h3>{t('ARGUS {version} is available', { version: notice.version })}</h3>
      <p style={{ whiteSpace: 'pre-wrap' }}>{notice.notes || t('No release notes provided.')}</p>
      {notice.manual && <p>{t('This package cannot update itself. Download the new version and install it with your package manager.')}</p>}
      {status === 'failed' && <p role="alert">{t('Update failed. The signature must be valid and the download must finish before installation. Please try again.')}</p>}
    </div>
    {notice.manual
      ? <a href={DOWNLOAD_URL} onClick={(event) => { event.preventDefault(); void openUrl(DOWNLOAD_URL); }}>{t('Download update')}</a>
      : <Button cargando={installing} disabled={checking} onClick={() => void updates.update()}>{installing ? t('Updating…') : t('Update')}</Button>}
    <Button variante="ghost" disabled={installing} onClick={() => updates.dismiss()}>{t('Later')}</Button>
  </Card></div>;
};
