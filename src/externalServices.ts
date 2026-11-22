/** Versioned catalogue of official interfaces, not a claim that ARGUS implements them. */
export type ExternalService = {
  id: string; service: string; method: string; requirements: string; permissions: string;
  readTools: string; writeTools: string; limitations: string; implemented: boolean; tested: boolean;
};
export const externalServices: readonly ExternalService[] = [
  { id: 'github', service: 'GitHub', method: 'Official REST API', requirements: 'GitHub App installation or authorized account', permissions: 'Permissions depend on the selected endpoints', readTools: 'List installation repositories', writeTools: 'No ARGUS write tools', limitations: 'No ARGUS adapter is implemented', implemented: false, tested: false },
  { id: 'discord', service: 'Discord', method: 'Official REST API', requirements: 'Authorized bot account', permissions: 'Permissions depend on the selected endpoints', readTools: 'Server, channel and role information', writeTools: 'No external bot configuration tools', limitations: 'Other bots do not necessarily provide an API or MCP', implemented: false, tested: false },
];
export function configurationToolAvailable(serviceId: string, tool: string, catalogue: readonly ExternalService[] = externalServices): boolean {
  return catalogue.some(service => service.id === serviceId && service.implemented && service.tested && service.writeTools === tool);
}
export function enabledConnectorServices(catalogue: readonly ExternalService[], ids: readonly string[]): ExternalService[] {
  return catalogue.filter(service => ids.includes(service.id));
}
export type ConnectorResult = 'not_implemented' | 'untested' | 'available' | 'failed';
export async function testConnector(service: ExternalService, probe?: () => Promise<boolean>): Promise<ConnectorResult> {
  if (!service.implemented) return 'not_implemented';
  if (!service.tested || !probe) return 'untested';
  try { return await probe() ? 'available' : 'failed'; } catch { return 'failed'; }
}
export function toggleConnector(ids: string[], id: string, enabled: boolean): string[] {
  return enabled ? [...new Set([...ids, id])] : ids.filter(item => item !== id);
}
