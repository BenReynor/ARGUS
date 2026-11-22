import type { Operation } from './activity.ts';
import type { ServerSnapshotItem } from './api/tauri.ts';
import { summarize } from './auditFindings.ts';
import type { AuditReport, AuditSummary } from './auditFindings.ts';

export interface StoredAudit { checked_at: string; summary: AuditSummary }

const auditKey = (bot: string, guild: string) => `last-audit:${bot}:${guild}`;

/** Guarda solo el resumen de la auditoría (sin hallazgos) para mostrarlo en el Panel sin llamar a Discord. */
export function saveAuditSummary(storage: Pick<Storage, 'setItem'>, bot: string, guild: string, report: AuditReport): void {
  const value: StoredAudit = { checked_at: report.checked_at, summary: summarize(report) };
  try { storage.setItem(auditKey(bot, guild), JSON.stringify(value)); } catch { /* sin espacio: se ignora */ }
}

export function readAuditSummary(storage: Pick<Storage, 'getItem'>, bot: string, guild: string): StoredAudit | null {
  try {
    const data = JSON.parse(storage.getItem(auditKey(bot, guild)) || 'null');
    if (!data || typeof data.checked_at !== 'string' || !data.summary || typeof data.summary.total !== 'number') return null;
    return data as StoredAudit;
  } catch { return null; }
}

export function latestSnapshot(items: ServerSnapshotItem[]): ServerSnapshotItem | null {
  return items.reduce<ServerSnapshotItem | null>((a, s) => (!a || Date.parse(s.created_at) > Date.parse(a.created_at) ? s : a), null);
}

/** Últimas operaciones del servidor, de la más reciente a la más antigua. */
export function recentOperations(ops: Operation[], guild: string, limit = 3): Operation[] {
  return ops.filter(o => o.guild_id === guild)
    .sort((a, b) => Date.parse(b.started_at) - Date.parse(a.started_at))
    .slice(0, limit);
}
