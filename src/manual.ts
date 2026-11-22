/** Manual de la app: secciones y buscador. Módulo puro (sin React ni i18n) para poder probarlo. */
export interface SeccionManual { id: string; tabs: string[]; title: string; que: string; como: string; limites: string }

export type Traductor = (texto: string) => string;

/** Secciones del manual por áreas; `tabs` indica en qué vistas de la barra lateral aplican. */
export function seccionesManual(t: Traductor): SeccionManual[] {
  return [
    { id: 'bots', tabs: ["dashboard","settings"], title: t("Bots and servers"), que: t("Register one or more of your own bots with their token (Settings). ARGUS reads the official name and avatar from Discord."), como: t("Pick the active bot in the sidebar and the active server in the server rail; turn the bot on or off with the button below the bot list."), limites: t("Only official bot tokens are supported (no selfbots). Servers shown are the ones the active bot belongs to. Tokens are stored encrypted.") },
    { id: 'channels', tabs: ["channels","roles"], title: t("Channels and roles"), que: t("Create, rename, move, reorder and delete categories and channels, edit channel permission overwrites, and manage roles and their permissions."), como: t("Open Channels or Roles, pick an item and edit it. Destructive actions ask for confirmation. Changes go to Discord through the official API."), limites: t("The bot needs the matching permissions and a role above the roles it edits; managed roles cannot be edited. Missing permissions are reported before acting.") },
    { id: 'members', tabs: ["members"], title: t("Members and moderation"), que: t("List and search members, assign or remove roles, change nicknames, timeout, kick and ban."), como: t("Open Members, search for someone and use the actions on their row. Kick, ban and timeout ask for confirmation."), limites: t("Limited by the bot role hierarchy and permissions: it cannot act on the owner or on members with a higher role.") },
    { id: 'assistant', tabs: ["asistente","servidor"], title: t("AI assistant and plan"), que: t("Write the server plan (Markdown, one per server) and chat with an agent (claude, codex or opencode) that proposes changes."), como: t("In AI Assistant write or generate the plan, choose Read to talk or Execute to get a proposal, review it and press Apply. ARGUS runs exactly what you approved, in order."), limites: t("The agent only has read tools and never sees your bot token. It needs its command-line tool installed; model usage may be billed by its provider.") },
    { id: 'templates', tabs: ["asistente","servidor"], title: t("Templates"), que: t("Save plans or server structures as your own templates, duplicate, organize, import and export them."), como: t("Save a template from a plan or snapshot, preview it against the target server and apply it through an approved proposal."), limites: t("Templates never carry tokens or IDs from the source server. Applying one always needs your approval.") },
    { id: 'snapshots', tabs: ["backups"], title: t("Snapshots and restore"), que: t("Snapshots save the structure of a server (roles, categories, channels, permissions). Automatic backups are kept and can be exported."), como: t("In Backups create or pick a snapshot, compare it with the current server and prepare a restore. Review the proposal and apply it."), limites: t("Restore recreates structure, not messages or members. Roles are matched by name; ambiguous or managed roles are flagged, not changed.") },
    { id: 'masskick', tabs: ["members","dashboard"], title: t("Mass kick and destruction alerts"), que: t("Kick many members at once with a preview, and get warned when the Gateway sees mass deletions of channels, roles or bans."), como: t("Mass kick: set the filter, review the list and confirm. Alerts appear at the top of the app and can lead to a restore from a snapshot."), limites: t("Mass kick protects the server owner and the bot, needs explicit confirmation and cannot be undone. Alerts need the bot to be on.") },
    { id: 'audit', tabs: ["permission-audit"], title: t("Permission audit"), que: t("Computes effective access per role and channel and compares it with your intention, plan or a snapshot."), como: t("Open Permission audit, run it, read each finding with its explanation and prepare corrections. Corrections are approved before being applied and checked afterwards."), limites: t("It only reads until you approve a fix. Results reflect the data last loaded from Discord.") },
    { id: 'diagnostics', tabs: ["settings","logs"], title: t("Diagnostics"), que: t("Checks connection, permissions, agents, MCP and storage without changing Discord or using paid models."), como: t("Run the checks from Settings; each one shows a result, an explanation and a next step, and can be retried on its own."), limites: t("A check may report that something could not be verified; that is not the same as success.") },
    { id: 'activity', tabs: ["activity","logs"], title: t("Activity and history"), que: t("A persistent history of operations per bot, server and operation with step status, and a live log."), como: t("Open Activity to filter operations and see each step; Logs shows the running log of the app."), limites: t("Accepted is not the same as verified: partial or unverifiable results are never shown as full success. Logs are not kept forever.") },
    { id: 'connectors', tabs: ["settings","asistente"], title: t("Connectors"), que: t("A catalog of integrations with other bots and services, with their real capabilities and limits."), como: t("Enable the connectors you trust in Settings and link accounts there. The AI proposes; writes run through a controlled adapter after your approval."), limites: t("Not every bot offers MCP or an API. Credentials are stored encrypted and are never shared with the AI.") },
    { id: 'mcp', tabs: ["settings","asistente"], title: t("MCP and skill"), que: t("argus-mcp exposes ARGUS tools to AI agents, and an installable skill teaches agents how to use ARGUS."), como: t("Configure your agent with the MCP entry shown in Settings and install the skill into your agent folder; existing files are not overwritten without asking."), limites: t("The MCP gives read tools to the assistant; writes only run after approval in ARGUS.") },
    { id: 'service', tabs: ["settings"], title: t("Background service"), que: t("An optional service that keeps the bot connected without the window."), como: t("Enable it in Settings, then start or stop it. Closing the window only hides ARGUS in the tray; Quit ends the process; stopping the service is a separate action."), limites: t("Off by default. It needs a computer that stays on and connected, and it never opens a second Gateway connection per bot.") },
    { id: 'security', tabs: ["settings"], title: t("Security and privacy"), que: t("Tokens and keys are encrypted locally, there is no telemetry, and destructive actions need confirmation."), como: t("Manage bots, backups and updates in Settings. Updates are signed and install only after you press Update."), limites: t("Tokens are never written to logs or localStorage and are not given to the AI. Portable backups need a password.") },
  ];
}

/** Secciones de una vista de la barra lateral; en la propia vista «manual» devuelve todas. */
export function seccionesDeVista(secciones: SeccionManual[], vista: string): SeccionManual[] {
  if (vista === 'manual') return secciones;
  return secciones.filter((s) => s.tabs.includes(vista));
}

/** Una sección arranca abierta al buscar (todas las coincidencias) o si es la de la vista que abrió el manual con «?». */
export function seccionAbierta(filtrando: boolean, vista: string, destacada: boolean): boolean {
  return filtrando || (vista !== 'manual' && destacada);
}

const norm = (s: string) => s.toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '');

/** Busca por título y contenido (todas las palabras; sin distinguir mayúsculas ni tildes). */
export function buscarManual(secciones: SeccionManual[], consulta: string): SeccionManual[] {
  const palabras = norm(consulta).split(/\s+/).filter(Boolean);
  if (!palabras.length) return secciones;
  return secciones.filter((s) => {
    const texto = norm([s.title, s.que, s.como, s.limites].join(' '));
    return palabras.every((p) => texto.includes(p));
  });
}
