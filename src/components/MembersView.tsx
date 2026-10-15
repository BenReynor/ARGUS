import React, { useState } from 'react';
import { useServerMap, useSetSelectedMember, useSelectedMember, useAddLog, useShowNotification } from '../store/useStore';
import { listMembers, sendDM, addRole, removeRole, timeoutMember, getMemberInfo } from '../api/tauri';
import { Users, User, Bot, Mail, Shield, Ban, MoreVertical, Copy, Search, ChevronRight, X } from 'lucide-react';
import type { Member, Role } from '../types';

export const MembersView: React.FC = () => {
  const serverMap = useServerMap();
  const setSelectedMember = useSetSelectedMember();
  const selectedMember = useSelectedMember();
  const addLog = useAddLog();
  const showNotification = useShowNotification();
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

  const handleRole = async (userId: string, roleId: string, add: boolean) => {
    try {
      if (add) await addRole('1541513296375717921', userId, roleId);
      else await removeRole('1541513296375717921', userId, roleId);
      showNotification(add ? 'Rol añadido' : 'Rol quitado', 'success');
    } catch { showNotification('Error', 'error'); }
  };

  const handleTimeout = async (userId: string, username: string) => {
    const mins = prompt(`Minutos de timeout para ${username}:`, '10');
    if (mins) {
      try {
        await timeoutMember('1541513296375717921', userId, parseInt(mins));
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
    return role ? `#${role.color.toString(16).padStart(6, '0')}` : 'var(--text-secondary)';
  };

  return (
    <div className="members-view">
      <div className="view-header">
        <h2>MIEMBROS</h2>
        <div className="header-controls">
          <select value={sortBy} onChange={(e) => setSortBy(e.target.value as any)} className="select">
            <option value="name">Nombre</option>
            <option value="roles">Roles</option>
            <option value="status">Estado</option>
          </select>
          <div className="search-box">
            <Search size={16} />
            <input
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
                    {member.bot ? <Bot size={16} className="bot-icon" /> : <User size={16} />}
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
                        <span key={i} className="role-badge" style={{ backgroundColor: role ? `#${role.color.toString(16).padStart(6, '0')}33` : 'var(--bg-tertiary)', borderColor: getRoleColor(roleName) }}>
                          {roleName}
                        </span>
                      );
                    })}
                    {member.roles.length > 3 && <span className="role-badge more">+{member.roles.length - 3}</span>}
                  </div>
                </td>
                <td>
                  <span className={`type-badge ${member.bot ? 'bot' : 'human'}`}>
                    {member.bot ? 'BOT' : 'HUMANO'}
                  </span>
                </td>
                <td>
                  <div className="action-buttons">
                    {!member.bot && (
                      <>
                        <button className="icon-btn" onClick={(e) => { e.stopPropagation(); handleDM(member.user_id, member.username); }} title="DM">
                          <Mail size={12} />
                        </button>
                        <button className="icon-btn" onClick={(e) => { e.stopPropagation(); handleTimeout(member.user_id, member.username); }} title="Timeout">
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
                    <span key={roleName} className="role-badge-detail" style={{ backgroundColor: role ? `#${role.color.toString(16).padStart(6, '0')}33` : 'var(--bg-tertiary)', borderColor: getRoleColor(roleName) }}>
                      {roleName}
                    </span>
                  );
                })}
              </div>
            </div>
            <div className="detail-actions">
              {!selectedMember.bot && (
                <>
                  <button className="btn-secondary" onClick={() => handleDM(selectedMember.user_id, selectedMember.username)}>
                    <Mail size={14} /> ENVIAR DM
                  </button>
                  <button className="btn-warning" onClick={() => handleTimeout(selectedMember.user_id, selectedMember.username)}>
                    <Ban size={14} /> TIMEOUT
                  </button>
                </>
              )}
              <button className="btn-ghost" onClick={() => copyId(selectedMember.user_id)}>
                <Copy size={14} /> COPIAR ID
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};