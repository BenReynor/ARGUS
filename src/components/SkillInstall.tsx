import { useState } from 'react';
import { skillDestino, instalarSkill } from '../api/tauri';
import { useShowNotification } from '../store/useStore';
import { t } from '../i18n';
import { Button, Select } from './ui';

const AGENTES = [
  { id: 'claude', nombre: 'Claude Code' },
  { id: 'codex', nombre: 'Codex' },
  { id: 'opencode', nombre: 'OpenCode' },
];

type Pendiente = { agente: string; path: string; exists: boolean };

/** Instala la skill de ARGUS en la carpeta de skills del agente elegido, tras confirmar la ruta. */
export function SkillInstall() {
  const showNotification = useShowNotification();
  const [agente, setAgente] = useState('claude');
  const [pendiente, setPendiente] = useState<Pendiente | null>(null);
  const [ocupado, setOcupado] = useState(false);

  const error = (e: unknown) => showNotification(typeof e === 'string' ? e : t('Could not install the skill'), 'error');

  const preparar = async () => {
    try {
      const d = await skillDestino(agente);
      setPendiente({ agente, path: d.path, exists: d.exists });
    } catch (e) {
      error(e);
    }
  };

  const confirmar = async () => {
    if (!pendiente) return;
    setOcupado(true);
    try {
      const ruta = await instalarSkill(pendiente.agente, pendiente.exists);
      showNotification(t('Skill installed in {path}', { path: ruta }), 'success');
      setPendiente(null);
    } catch (e) {
      error(e);
    } finally {
      setOcupado(false);
    }
  };

  return (
    <div className="ajustes__codigo">
      <span className="ajustes__codigo-etiqueta">{t('ARGUS skill for agents')}</span>
      <p>{t('Teaches the agent to use the ARGUS MCP with few tokens. It is copied to the agent’s skills folder only after you confirm.')}</p>
      <Select
        etiqueta={t('Agent')}
        value={agente}
        onChange={(e) => {
          setAgente(e.target.value);
          setPendiente(null);
        }}
      >
        {AGENTES.map((a) => (
          <option key={a.id} value={a.id}>{a.nombre}</option>
        ))}
      </Select>
      {!pendiente ? (
        <Button variante="secundario" onClick={preparar}>{t('Install skill')}</Button>
      ) : (
        <div role="group" aria-label={t('Confirm skill installation')}>
          <p>{t('It will be written to:')}</p>
          <pre><code>{pendiente.path}</code></pre>
          {pendiente.exists && (
            <p className="ajustes__aviso" role="alert">{t('A skill already exists there. Continuing will overwrite it.')}</p>
          )}
          <Button onClick={confirmar} disabled={ocupado}>
            {pendiente.exists ? t('Overwrite') : t('Confirm and install')}
          </Button>{' '}
          <Button variante="secundario" onClick={() => setPendiente(null)} disabled={ocupado}>{t('Cancel')}</Button>
        </div>
      )}
    </div>
  );
}
