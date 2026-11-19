import React from 'react';
import { TriangleAlert } from 'lucide-react';
import { useActiveGuildId, useGuildPanel } from '../store/useStore';
import { t } from '../i18n';

/** Aviso informativo: los permisos efectivos vienen del cálculo del backend. */
export const AvisoPermisosBot: React.FC = () => {
  const guildId = useActiveGuildId();
  const panel = useGuildPanel();
  if (!guildId || panel.loading || panel.guild?.id !== guildId || !panel.permissions?.includes('Administrador')) return null;

  return (
    <aside role="note" style={{ display: 'flex', alignItems: 'flex-start', gap: 'var(--sp-2)', padding: 'var(--sp-3)', border: 'var(--border-w) solid var(--warning)', borderRadius: 'var(--radius)' }}>
      <TriangleAlert size={20} aria-hidden="true" style={{ flexShrink: 0, color: 'var(--warning)' }} />
      <div>
        <strong>{t('The active bot has Administrator on this server.')}</strong>
        <p>{t('Fewer permissions reduce the possible damage from a mistaken order or a compromised bot. ARGUS does not need Administrator. In Discord, remove it from every role assigned to the bot and grant only the permissions for the features you use:')}</p>
        <ul>
          <li>{t('Replies and message history: View Channels, Send Messages and Read Message History in the allowed channels.')}</li>
          <li>{t('Server structure and member roles: Manage Channels and Manage Roles. Keep the bot role above the roles it manages.')}</li>
          <li>{t('Moderation: Moderate Members for timeouts, Kick Members for kicks, Ban Members for bans, Manage Messages for deleting or pinning messages, and Manage Nicknames for nicknames.')}</li>
          <li>{t('Other server tools: Manage Server for settings and invites, View Audit Log for the audit log, Create Invite for creating invites, and Manage Webhooks for webhooks.')}</li>
        </ul>
        <small>{t('Channel permission overrides still apply. Only grant access to the channels the bot needs.')}</small>
      </div>
    </aside>
  );
};
