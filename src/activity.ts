export interface OperationStep {
  step_index: number; action: string; status: string; reason?: string | null;
}
export interface Operation {
  id: string; bot_id: string; guild_id: string; status: string;
  started_at: string; steps: OperationStep[]; reason?: string | null;
}
export interface ActivityFilters { type: string; guild: string; status: string }
export const emptyFilters: ActivityFilters = { type: '', guild: '', status: '' };
export function filterOperations(operations: Operation[], filters: ActivityFilters): Operation[] {
  return operations.filter(op => (!filters.guild || op.guild_id === filters.guild)
    && (!filters.status || op.status === filters.status)
    && (!filters.type || op.steps.some(step => step.action === filters.type)));
}
export function readFilters(storage: Pick<Storage, 'getItem'>, bot: string): ActivityFilters {
  try {
    const data = JSON.parse(storage.getItem(`activity-filters:${bot}`) || '{}');
    return { type: typeof data.type === 'string' ? data.type : '', guild: typeof data.guild === 'string' ? data.guild : '', status: typeof data.status === 'string' ? data.status : '' };
  } catch { return { ...emptyFilters }; }
}
export function saveFilters(storage: Pick<Storage, 'setItem'>, bot: string, filters: ActivityFilters): void {
  storage.setItem(`activity-filters:${bot}`, JSON.stringify(filters));
}

export function operationProgress(steps: OperationStep[]) {
  const done = steps.filter(step => !['pending', 'in_progress', 'verifying'].includes(step.status)).length;
  return { done, total: steps.length };
}
export function operationNotice(status: string): 'success' | 'warning' | 'error' | 'info' {
  if (status === 'correct') return 'success';
  if (status === 'failed') return 'error';
  if (['partial', 'unverified', 'skipped', 'cancelled', 'interrupted'].includes(status)) return 'warning';
  return 'info';
}
