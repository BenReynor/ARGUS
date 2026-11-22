import type { OperationStep } from './activity';

export interface DiagnosticCheck extends OperationStep {
  name: string;
  explanation: string;
  checked_at: string | null;
}
export const groups = {
  diagnostico_local: ['configuration', 'database', 'keyring'],
  diagnostico_discord: ['gateway', 'build_permissions'],
  diagnostico_agentes: ['claude_binary', 'claude_session', 'claude_response', 'codex_binary', 'codex_session', 'codex_response', 'opencode_binary', 'opencode_session', 'opencode_response', 'mcp_binary', 'mcp_session', 'mcp_response'],
} as const;
export type Group = keyof typeof groups;
/** Grupos (comandos) que hay que llamar: todos, o solo el que contiene `only`. */
export const groupsToRun = (only?: string): Group[] =>
  (Object.keys(groups) as Group[]).filter(g => !only || (groups[g] as readonly string[]).includes(only));
export const initialChecks = (): DiagnosticCheck[] => Object.values(groups).flat().map((name, step_index) => ({ name, step_index, action: name, status: 'pending', explanation: '', checked_at: null }));
export function mergeChecks(current: DiagnosticCheck[], updates: Partial<DiagnosticCheck>[], only?: string): DiagnosticCheck[] {
  return current.map(c => { const update = updates.find(u => u.name === c.name); return update && (!only || c.name === only) ? { ...c, ...update } : c; });
}
export const allCorrect = (checks: DiagnosticCheck[]) => checks.length > 0 && checks.every(c => c.status === 'correct');
