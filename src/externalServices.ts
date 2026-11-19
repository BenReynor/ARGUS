// No configuration connectors or browser recipes are implemented yet.
// Keep this offline: absence of a tool must never be interpreted as availability.
export type ExternalService = { id: string; configurationTools: string[] };

export const externalServices: readonly ExternalService[] = [];

export function configurationToolAvailable(serviceId: string, tool: string): boolean {
  return externalServices.some(service => service.id === serviceId && service.configurationTools.includes(tool));
}
