import React, { useEffect } from 'react';
import { useNotifications, useHideNotification } from '../store/useStore';
import type { ToastItem, ToastType } from '../store/useStore';
import { t } from '../i18n';
import { CheckCircle, AlertCircle, Info, AlertTriangle, X } from 'lucide-react';

const DURACION_MS: Record<ToastType, number> = { success: 4000, info: 4000, warning: 6000, error: 8000 };

const icons: Record<ToastType, any> = {
  success: CheckCircle,
  error: AlertCircle,
  info: Info,
  warning: AlertTriangle,
};

const colors: Record<ToastType, string> = {
  success: 'var(--success)',
  error: 'var(--danger)',
  info: 'var(--info)',
  warning: 'var(--warning)',
};

const Toast: React.FC<{ toast: ToastItem; onClose: (id: number) => void }> = ({ toast, onClose }) => {
  useEffect(() => {
    const timer = setTimeout(() => onClose(toast.id), DURACION_MS[toast.type]);
    return () => clearTimeout(timer);
  }, [toast.id, toast.type, onClose]);

  const Icon = icons[toast.type];
  return (
    <div
      className={`toast toast-${toast.type}`}
      role={toast.type === 'error' ? 'alert' : 'status'}
      style={{ '--toast-color': colors[toast.type] } as React.CSSProperties}
    >
      <Icon size={18} className="toast__icono" />
      <span className="toast__texto">{toast.message}</span>
      <button className="icon-btn" aria-label={t('Close notification')} onClick={() => onClose(toast.id)}>
        <X size={14} />
      </button>
    </div>
  );
};

export const Notification: React.FC = () => {
  const notifications = useNotifications();
  const hideNotification = useHideNotification();

  if (notifications.length === 0) return null;

  return (
    <div className="toasts" aria-live="polite">
      {notifications.map((n) => (
        <Toast key={n.id} toast={n} onClose={hideNotification} />
      ))}
    </div>
  );
};
