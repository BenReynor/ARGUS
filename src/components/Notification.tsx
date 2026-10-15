import React, { useEffect } from 'react';
import { useNotification, useHideNotification } from '../store/useStore';
import { CheckCircle, AlertCircle, Info, AlertTriangle, X } from 'lucide-react';

export const Notification: React.FC = () => {
  const notification = useNotification();
  const hideNotification = useHideNotification();

  useEffect(() => {
    if (notification) {
      const timer = setTimeout(() => {
        hideNotification();
      }, 5000);
      return () => clearTimeout(timer);
    }
  }, [notification, hideNotification]);

  if (!notification) return null;

  const icons = {
    success: CheckCircle,
    error: AlertCircle,
    info: Info,
    warning: AlertTriangle,
  };

  const colors = {
    success: 'var(--accent-green)',
    error: 'var(--accent-red)',
    info: 'var(--accent-cyan)',
    warning: 'var(--accent-orange)',
  };

  const Icon = icons[notification.type];

  return (
    <div className={`notification toast-${notification.type}`} style={{ '--toast-color': colors[notification.type] } as React.CSSProperties}>
      <Icon size={20} />
      <span>{notification.message}</span>
      <button className="icon-btn" onClick={hideNotification}>
        <X size={14} />
      </button>
    </div>
  );
};