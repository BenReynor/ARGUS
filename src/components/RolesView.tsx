import React, { useState } from 'react';
import { useServerMap, useShowNotification } from '../store/useStore';
import { listRoles } from '../api/tauri';
import { Shield, Users, Hash, Copy, Search, Settings, Trash2, Plus, Edit2, Eye, EyeOff, X } from 'lucide-react';
import type { Role } from '../types';
import { Button, Input } from './ui';

export const RolesView: React.FC = () => {
  const serverMap = useServerMap();
  const showNotification = useShowNotification();
  const [search, setSearch] = useState('');
  const [editingRole, setEditingRole] = useState<Role | null>(null);

  const roles = serverMap?.roles || [];
  const filtered = roles.filter(r => 
    r.name.toLowerCase().includes(search.toLowerCase())
  ).sort((a, b) => b.position - a.position);

  const copyId = (id: string) => {
    navigator.clipboard.writeText(id);
    showNotification('ID copiado', 'info');
  };

  const getPermsText = (perms: string) => {
    if (perms === 'Administrator') return '👑 Administrator';
    return perms;
  };

  return (
    <div className="roles-view">
      <div className="view-header">
        <h2>ROLES DEL SERVIDOR</h2>
        <div className="header-controls">
          <div className="search-box">
            <Search size={16} />
            <Input
              type="text"
              placeholder="Filtrar roles..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          <Button>
            <Plus size={14} /> NUEVO ROL
          </Button>
        </div>
      </div>

      <div className="roles-table-container">
        <table className="roles-table">
          <thead>
            <tr>
              <th>ROL</th>
              <th>COLOR</th>
              <th>POS</th>
              <th>MIEMBROS</th>
              <th>PERMISOS</th>
              <th>MENCIONABLE</th>
              <th>SEPARADO</th>
              <th>ACCIONES</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((role) => (
              <tr key={role.id}>
                <td>
                  <div className="role-cell">
                    <div className="role-color-indicator" style={{ backgroundColor: `#${role.color.toString(16).padStart(6, '0')}` }} />
                    <span className="role-name">{role.name}</span>
                  </div>
                </td>
                <td>
                  <div className="color-preview" style={{ backgroundColor: `#${role.color.toString(16).padStart(6, '0')}` }} />
                  <span className="mono">#{role.color.toString(16).padStart(6, '0').toUpperCase()}</span>
                </td>
                <td className="mono">{role.position}</td>
                <td>
                  <span className="member-count">{role.member_count}</span>
                  {role.member_count > 0 && (
                    <Users size={12} className="member-icon" aria-label={`${role.member_count} miembros`} />
                  )}
                </td>
                <td>
                  <span className="perms-text">{getPermsText(role.permissions)}</span>
                </td>
                <td>
                  <span className={`bool-badge ${role.mentionable ? 'yes' : 'no'}`}>
                    {role.mentionable ? 'SÍ' : 'NO'}
                  </span>
                </td>
                <td>
                  <span className={`bool-badge ${role.hoist ? 'yes' : 'no'}`}>
                    {role.hoist ? 'SÍ' : 'NO'}
                  </span>
                </td>
                <td>
                  <div className="action-buttons">
                    <button className="icon-btn" onClick={() => copyId(role.id)} title="Copiar ID">
                      <Copy size={12} />
                    </button>
                    <button className="icon-btn" onClick={() => setEditingRole(role)} title="Editar">
                      <Edit2 size={12} />
                    </button>
                    <button className="icon-btn danger" title="Eliminar">
                      <Trash2 size={12} />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {editingRole && (
        <div className="modal-overlay" onClick={() => setEditingRole(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3>EDITAR ROL: {editingRole.name}</h3>
              <button className="icon-btn" onClick={() => setEditingRole(null)}>
                <X size={16} />
              </button>
            </div>
            <div className="modal-body">
              <div className="form-group">
                <label>Nombre</label>
                <Input defaultValue={editingRole.name} />
              </div>
              <div className="form-group">
                <label>Color (hex)</label>
                <input type="color" defaultValue={`#${editingRole.color.toString(16).padStart(6, '0')}`} />
              </div>
              <div className="form-row">
                <label className="checkbox-label">
                  <input type="checkbox" defaultChecked={editingRole.mentionable} /> Mencionable
                </label>
                <label className="checkbox-label">
                  <input type="checkbox" defaultChecked={editingRole.hoist} /> Separado en lista
                </label>
              </div>
            </div>
            <div className="modal-footer">
              <Button variante="secundario" onClick={() => setEditingRole(null)}>Cancelar</Button>
              <Button>Guardar cambios</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};