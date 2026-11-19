/** Only fixed labels and outcomes enter the report; raw errors are discarded. */
export type DiagnosticOutcome = 'available' | 'unavailable' | 'failed';

export async function checkKeyring(check: () => Promise<boolean>): Promise<DiagnosticOutcome> {
  try { return await check() ? 'available' : 'unavailable'; }
  catch { return 'failed'; }
}

export function systemName(platform: string): string {
  if (/win/i.test(platform)) return 'Windows';
  if (/mac/i.test(platform)) return 'macOS';
  if (/linux/i.test(platform)) return 'Linux';
  return 'Other';
}

export function diagnosticReport(version: string, platform: string, result: DiagnosticOutcome, translate: (key: string) => string): string {
  // Do not propagate arbitrary input, including error strings or environment values.
  const safeVersion = /^\d+\.\d+\.\d+(?:[-+][a-zA-Z0-9.-]+)?$/.test(version) ? version : 'unknown';
  const status = result === 'available' ? translate('Available') : result === 'unavailable' ? translate('Unavailable') : translate('Check failed');
  return [translate('ARGUS diagnostic report'), `${translate('Version')}: ${safeVersion}`,
    `${translate('System')}: ${systemName(platform)}`, `${translate('Keyring')}: ${status}`,
    `${translate('Database, Gateway, permissions, agents and MCP')}: ${translate('Not checked')}`,
    translate('Raw errors and private data are excluded.')].join('\n');
}
