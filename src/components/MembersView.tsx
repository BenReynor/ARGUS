import React, { useState } from 'react';
import { useServerMap, useSetSelectedMember, useSelectedMember, useAddLog, useShowNotification, useConfig, useGuildPanel, motivoPermisoAusente } from '../store/useStore';
import { listMembers, sendDM, addRole, removeRole, timeoutMember, getMemberInfo } from '../api/tauri';
import { Button, Input, Select, Avatar, Badge } from './ui';
import { Users, Mail, Shield, Ban, MoreVertical, Copy, Search, ChevronRight, X } from 'lucide-react';
import type { Member, Role } from '../types';

export const MembersView: React.FC = () => {
  const serverMap = useServerMap();
  const setSelectedMember = useSetSelectedMember();
  const selectedMember = useSelectedMember();
  const addLog = useAddLog();
  const guildId = useConfig().active_guild_id;
  const showNotification = useShowNotification();
  const motivoTimeout = motivoPermisoAusente(useGuildPanel().permissions, 'Moderar miembros');
  const [search, setSearch] = useState('');
  const [sortBy, setSortBy] = useState<'name' | 'roles' | 'status'>('name');

  const members = serverMap?.members || [];
  const roles = serverMap?.roles || [];
  const filtered = members.filter(m => 
    m.username.toLowerCase().includes(search.toLowerCase()) ||
    m.nickname?.toLowerCase().includes(search.toLowerCase())
  );

  const sorted = [...filtered].sort((a, b) => {
    if (sortBy === 'name') return (a.nickname || a.username).localeCompare(b.nickname || b.username);
    if (sortBy === 'roles') return b.roles.length - a.roles.length;
    return 0;
  });

  const handleDM = async (userId: string, username: string) => {
    const content = prompt(`DM a ${username}:`);
    if (content) {
      try {
        await sendDM(userId, content);
        showNotification('DM enviado', 'success');
      } catch { showNotification('Error', 'error'); }
    }
  };

  const handleRole = async (member: Member, role: Role, add: boolean) => {
    try {
      if (!guildId) throw new Error('No hay servidor seleccionado');
      if (add) await addRole(guildId, member.user_id, role.id);
      else await removeRole(guildId, member.user_id, role.id);
      setSelectedMember({
        ...member,
        roles: add ? [...member.roles, role.name] : member.roles.filter(n => n !== role.name),
      });
      showNotification(add ? 'Rol añadido' : 'Rol quitado', 'success');
    } catch (e) { showNotification(typeof e === 'string' ? e : 'Error al cambiar el rol', 'error'); }
  };

  const handleTimeout = async (userId: string, username: string) => {
    const mins = prompt(`Minutos de timeout para ${username}:`, '10');
    if (mins) {
      try {
        if (!guildId) { showNotification('No hay servidor seleccionado', 'error'); return; }
        await timeoutMember(guildId, userId, parseInt(mins));
        showNotification(`Timeout ${mins} min`, 'success');
      } catch { showNotification('Error', 'error'); }
    }
  };

  const copyId = (id: string) => {
    navigator.clipboard.writeText(id);
    showNotification('ID copiado', 'info');
  };

  const getRoleColor = (roleName: string) => {
    const role = roles.find(r => r.name === roleName);
    return role ? `#${role.color.toString(16).padStart(6, '0')}` : 'var(--text-2)';
  };

  return (
    <div className="members-view">
      <div className="view-header">
        <h2>MIEMBROS</h2>
        <div className="header-controls">
          <Select value={sortBy} onChange={(e) => setSortBy(e.target.value as any)} aria-label="Ordenar por">
            <option value="name">Nombre</option>
            <option value="roles">Roles</option>
            <option value="status">Estado</option>
          </Select>
          <div className="search-box">
            <Search size={16} />
            <Input
              type="text"
              placeholder="Buscar miembro..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
        </div>
      </div>

      <div className="members-table-container">
        <table className="members-table">
          <thead>
            <tr>
              <th>MIEMBRO</th>
              <th>ID</th>
              <th>ROLES</th>
              <th>TIPO</th>
              <th>ACCIONES</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((member) => (
              <tr key={member.user_id} onClick={() => setSelectedMember(member)}>
                <td>
                  <div className="member-cell">
                    <Avatar src={member.avatar} nombre={member.nickname || member.username} tamano={28} />
                    <div>
                      <span className="member-name">{member.nickname || member.username}</span>
                      {member.username !== (member.nickname || member.username) && (
                        <span className="member-username">@{member.username}</span>
                      )}
                    </div>
                  </div>
                </td>
                <td className="mono copyable" onClick={(e) => { e.stopPropagation(); copyId(member.user_id); }}>
                  {member.user_id}
                </td>
                <td>
                  <div className="roles-badges">
                    {member.roles.slice(0, 3).map((roleName, i) => {
                      const role = roles.find(r => r.name === roleName);
                      return (
                        <span key={i} className="role-badge" style={{ backgroundColor: role ? `#${role.color.toString(16).padStart(6, '0')}33` : 'var(--surface-2)', borderColor: getRoleColor(roleName) }}>
                          {roleName}
                        </span>
                      );
                    })}
                    {member.roles.length > 3 && <span className="role-badge more">+{member.roles.length - 3}</span>}
                  </div>
                </td>
                <td>
                  <Badge tono={member.bot ? 'acento' : 'neutro'}>
                    {member.bot ? 'BOT' : 'HUMANO'}
                  </Badge>
                </td>
                <td>
                  <div className="action-buttons">
                    {!member.bot && (
                      <>
                        <button className="icon-btn" onClick={(e) => { e.stopPropagation(); handleDM(member.user_id, member.username); }} title="DM">
                          <Mail size={12} />
                        </button>
                        <button className="icon-btn" disabled={!!motivoTimeout} onClick={(e) => { e.stopPropagation(); handleTimeout(member.user_id, member.username); }} title={motivoTimeout ?? 'Timeout'}>
                          <Ban size={12} />
                        </button>
                      </>
                    )}
                    <button className="icon-btn" onClick={(e) => { e.stopPropagation(); copyId(member.user_id); }} title="Copiar ID">
                      <Copy size={12} />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {selectedMember && (
        <div className="member-detail-panel">
          <div className="panel-header">
            <h3>{selectedMember.nickname || selectedMember.username}</h3>
            <button className="icon-btn" onClick={() => setSelectedMember(null)}>
              <X size={16} />
            </button>
          </div>
          <div className="detail-content">
            <div className="detail-section">
              <h4>INFORMACIÓN</h4>
              <div className="detail-grid">
                <div><label>ID</label><span className="mono">{selectedMember.user_id}</span></div>
                <div><label>Username</label><span>@{selectedMember.username}</span></div>
                <div><label>Nickname</label><span>{selectedMember.nickname || '—'}</span></div>
                <div><label>Tipo</label><span>{selectedMember.bot ? 'Bot' : 'Humano'}</span></div>
              </div>
            </div>
            <div className="detail-section">
              <h4>ROLES</h4>
              <div className="roles-list">
                {selectedMember.roles.map((roleName) => {
                  const role = roles.find(r => r.name === roleName);
                  return (
                    <span key={roleName} className="role-badge-detail" style={{ backgroundColor: role ? `#${role.color.toString(16).padStart(6, '0')}33` : 'var(--surface-2)', borderColor: getRoleColor(roleName) }}>
                      {roleName}
                    </span>
                  );
                })}
              </div>
              <div className="roles-list">
                {roles.filter(r => r.name !== '@everyone').map((role) => {
                  const tiene = selectedMember.roles.includes(role.name);
                  return (
                    <Button key={role.id} variante="ghost" tamano="sm" onClick={() => handleRole(selectedMember, role, !tiene)} title={tiene ? 'Quitar rol' : 'Añadir rol'}>
                      {tiene ? '− ' : '+ '}{role.name}
                    </Button>
                  );
                })}
              </div>
            </div>
            <div className="detail-actions">
              {!selectedMember.bot && (
                <>
                  <Button variante="secundario" onClick={() => handleDM(selectedMember.user_id, selectedMember.username)}>
                    <Mail size={14} /> ENVIAR DM
                  </Button>
                  <Button variante="peligro" disabled={!!motivoTimeout} title={motivoTimeout ?? undefined} onClick={() => handleTimeout(selectedMember.user_id, selectedMember.username)}>
                    <Ban size={14} /> TIMEOUT
                  </Button>
                  {motivoTimeout && <span className="member-username">{motivoTimeout}</span>}
                </>
              )}
              <Button variante="ghost" onClick={() => copyId(selectedMember.user_id)}>
                <Copy size={14} /> COPIAR ID
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};