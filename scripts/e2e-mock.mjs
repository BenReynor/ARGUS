// Backend de Tauri simulado para el recorrido e2e: datos demo ficticios, sin red
// ni tokens. Se inyecta en la página antes de que cargue la app. Todo comando
// invoke que no esté aquí se anota en window.__e2eSinSimular y el recorrido falla.

/** Se serializa y ejecuta dentro de la página: no puede usar variables de fuera. */
function backendSimulado(idioma) {
  const ahora = new Date('2026-11-20T12:00:00Z').getTime();
  const iso = (dias) => new Date(ahora - dias * 86400000).toISOString();

  const GUILD = '100000000000000001';
  const BOT = '200000000000000001';
  const guild = { id: GUILD, name: 'Demo Server', member_count: 64, icon: null, joined_at: iso(90) };
  const guild2 = { id: '100000000000000002', name: 'Sandbox', member_count: 12, icon: null, joined_at: iso(30) };

  const roles = [
    ['Admin', 0xe74c3c, 1, '8', 2], ['Moderator', 0x3498db, 2, '1099511627776', 4], ['Builder', 0x2ecc71, 3, '268435456', 3],
    ['Helper', 0xf1c40f, 4, '0', 6], ['Artist', 0x9b59b6, 5, '0', 9], ['Gamer', 0xe67e22, 6, '0', 15], ['Member', 0x95a5a6, 7, '0', 60],
  ].map(([name, color, i, perms, count]) => ({
    id: `30000000000000000${i}`, name, color, position: 10 - i, permissions: perms, permissions_bitfield: perms,
    member_count: count, mentionable: i % 2 === 0, hoist: i < 4,
  }));
  roles.push({ id: GUILD, name: '@everyone', color: 0, position: 0, permissions: '1024', permissions_bitfield: '1024', member_count: 64, mentionable: false, hoist: false });

  const cats = [['Information', 1], ['Community', 2], ['Voice', 3], ['Staff', 4]].map(([name, i]) => ({
    id: `40000000000000000${i}`, name, channel_type: 4, parent_id: null, position: i, overwrites: null, permissions_bitfield: null,
  }));
  const canales = [...cats];
  const nombres = [
    ['rules', 0, 1], ['announcements', 5, 1], ['welcome', 0, 1], ['general', 0, 2], ['off-topic', 0, 2], ['art', 0, 2], ['gaming', 0, 2],
    ['General Voice', 2, 3], ['Music', 2, 3], ['Gaming Voice', 2, 3], ['mod-chat', 0, 4], ['logs', 0, 4],
  ];
  nombres.forEach(([name, tipo, cat], i) => canales.push({
    id: `5000000000000000${String(i + 1).padStart(2, '0')}`, name, channel_type: tipo, parent_id: `40000000000000000${cat}`, position: i, topic: tipo === 0 ? `Talk about ${name}` : null,
    overwrites: cat === 4 ? [{ id: roles[1].id, type_: 0, allow: '1024', deny: '0' }, { id: GUILD, type_: 0, allow: '0', deny: '1024' }] : [],
    permissions_bitfield: '1024',
  }));

  const nombresMiembro = ['Aria', 'Bruno', 'Carla', 'Diego', 'Elena', 'Fabio', 'Gala', 'Hugo', 'Irene', 'Joel', 'Kira', 'Luis', 'Mara', 'Nico', 'Olga', 'Pablo'];
  const miembros = [];
  for (let i = 0; i < 64; i++) {
    const rs = [roles[6].id];
    if (i < 2) rs.push(roles[0].id);
    else if (i < 6) rs.push(roles[1].id);
    else if (i < 9) rs.push(roles[2].id);
    else if (i < 15) rs.push(roles[3].id);
    if (i % 7 === 0) rs.push(roles[4].id);
    if (i % 4 === 0) rs.push(roles[5].id);
    miembros.push({
      user_id: `6000000000000${String(i + 1).padStart(5, '0')}`.slice(0, 18), username: `${nombresMiembro[i % 16].toLowerCase()}${i + 1}`,
      nickname: i % 3 === 0 ? `${nombresMiembro[i % 16]} ${i + 1}` : null, roles: rs, bot: i === 63, joined_at: iso(i * 3 + 1), avatar: null, flags: 0,
    });
  }
  const categorias = {};
  for (const c of cats) categorias[c.id] = canales.filter((x) => x.parent_id === c.id);

  const mapa = { guild, channels: canales, members: miembros, roles, categories: categorias, last_updated: iso(0) };

  const bot = { id: BOT, name: 'Demo Bot', token_ciphertext: '', avatar: null, application_id: '200000000000000009', allowed_guilds: [GUILD], created_at: iso(60), updated_at: iso(1) };
  const config = {
    bots: [bot], active_bot_id: BOT, active_guild_id: GUILD, guilds_cache: [guild, guild2], multi_guild_mode: false,
    llm_chat_provider: 'claude', llm_chat_model: 'sonnet', llm_chat_url: '', llm_chat_api_key_ciphertext: '',
    plan_provider: 'claude', ask_provider: 'claude', cli_agents: {}, ai_time_limit_minutes: 10, ai_order_time_limit_minutes: 10,
    context_limit_per_user: 20, context_limit_per_channel: 50, auto_cleanup_enabled: false, archive_retention_months: 6,
    vigilance: { preset: 'default', enabled_events: [], filters: {} }, backup: { enabled: true, interval_hours: 24, retention_count: 7 },
    language: idioma, auto_update: false, telemetry_opt_in: false, anti_spam_cooldown_secs: 5,
    responder_triggers: { mention: true, slash_command: true, prefix: null, allowed_channels: [], blocked_channels: [], respond_to_bots: false, respond_in_dms: false },
    alerts: { enabled: true, sound: false, only_when_unfocused: true, telegram_enabled: false, telegram_events: [] },
    auto_sync_map: true, sync_interval_secs: 300, bot_engine: 'twilight', ui_theme: 'discord', ui_mode: 'dark', ui_language: idioma,
  };
  const estadoBot = { running: true, connected: true, bot_name: 'Demo Bot', guild_name: guild.name, guild_id: GUILD, member_count: 64, latency_ms: 42, last_error: null, warning: null };
  const ajustesGuild = {
    enabled_connectors: [], moderacion_no_preguntar: false, alias: null, notes: '', vigilance_enabled: true,
    destruction: { enabled: true, threshold: 5, window_seconds: 60, remove_roles: false }, personality_enabled: false, templates_enabled: true,
    chat_agente: 'claude', chat_modelo: 'sonnet', chat_personalidad: '', chat_canales: [], chat_espera_s: 30, chat_max_diario: 50, chat_respuestas_hoy: 0,
    chat_contexto: 10, chat_usuarios: [], chat_ordenes: false, chat_ordenes_usuarios: [], chat_ordenes_confirmar: true, ordenes_max_acciones: 10,
    ordenes_permitidas: { estructura: true, borrar_canales_roles: false, silenciar: true, expulsar: false, banear: false, borrar_mensajes: true, avisos: true },
    ordenes_agente: 'claude', ordenes_modelo: 'sonnet',
  };
  const snapshots = [
    { id: 'snap-1', name: 'Before the redesign', created_at: iso(5), size_bytes: 48213, automatic: false, summary: { channels: 16, roles: 8, members: 64, emojis: 4, stickers: 0, webhooks: 1, bans: 2 } },
    { id: 'snap-2', name: 'Automatic backup', created_at: iso(1), size_bytes: 49102, automatic: true, summary: { channels: 16, roles: 8, members: 64, emojis: 4, stickers: 0, webhooks: 1, bans: 2 } },
  ];
  const auditoria = {
    guild_id: GUILD, checked_at: iso(0), incomplete: false, missing_data: [],
    findings: [
      { tipo: 'role_grant', role_id: roles[3].id, role_name: 'Helper', permission: 'Administrator', permission_bit: 3, permissions_bits: '8', checked_at: iso(0) },
      { tipo: 'channel_overwrite_grant', channel_id: '500000000000000011', target_id: roles[5].id, target_type: 0, permission: 'Manage Messages', permission_bit: 13, evidence_bits: '8192', checked_at: iso(0) },
      { tipo: 'privacy_discrepancy', channel_id: '500000000000000012', expected_can_view: false, actual_can_view: true, expectation_source: 'plan', expectation_evidence: 'logs is staff only', checked_at: iso(0) },
    ],
  };
  const operaciones = [
    { id: 'op-1', bot_id: BOT, guild_id: GUILD, status: 'verified', started_at: iso(2), steps: [
      { step_index: 0, action: 'create_role', status: 'verified', reason: null }, { step_index: 1, action: 'create_channel', status: 'verified', reason: null } ] },
    { id: 'op-2', bot_id: BOT, guild_id: GUILD, status: 'partial', started_at: iso(1), reason: 'One channel could not be verified', steps: [
      { step_index: 0, action: 'create_channel', status: 'verified', reason: null }, { step_index: 1, action: 'set_channel_permission', status: 'failed', reason: 'Missing Permissions' } ] },
  ];
  const diagnostico = (nombre, estado) => ({ id: nombre, title: nombre, status: estado, explanation: 'Everything looks fine.', next_step: null, duration_ms: 12 });

  const sinSimular = (window.__e2eSinSimular = []);
  const vacio = () => null;
  const ok = () => 'OK';
  const manejadores = {
    get_config: () => config, save_config: ok, backup_window_preferences: () => ({ auto_start: false, minimize_to_tray: true }),
    get_bot_status: () => estadoBot, start_bot: () => estadoBot, stop_bot: () => estadoBot,
    refresh_server_map: () => mapa, get_server_map: () => mapa, resync_map: () => mapa,
    list_guilds: () => [guild, guild2], get_active_guild: () => GUILD, set_active_guild: () => GUILD, guild_info: () => guild,
    bot_permissions: () => ['ViewChannel', 'ManageChannels', 'ManageRoles', 'KickMembers', 'BanMembers'],
    get_guild_settings: () => ajustesGuild, save_guild_settings: ok,
    list_channels: () => canales, list_members: () => miembros, list_roles: () => roles,
    moderation_context: () => ({ owner_id: miembros[0].user_id, bot_id: BOT, bot_top_position: 9 }), list_bans: () => [{ user_id: '700000000000000001', username: 'spammer42', reason: 'Spam' }],
    get_guild_settings_discord: () => ({ name: guild.name, description: 'A demo community', verification_level: 1, explicit_content_filter: 2, default_message_notifications: 1, system_channel_id: canales[6].id, afk_channel_id: null, afk_timeout: 300 }),
    audit_log: () => [{ id: '800000000000000001', action_type: 11, accion: 'Channel created', autor: 'aria1', objetivo: 'general', reason: null, fecha: iso(1) }],
    channel_messages: () => [], list_invites: () => [], list_webhooks: () => [],
    destruction_alerts: () => [], listar_operaciones: () => operaciones, permission_audit: () => auditoria,
    server_snapshots: () => ({ return_roles: false, return_roles_ready: true, snapshots, schedule: { enabled: true, keep_last: 5, interval_hours: 24, last_error: null } }),
    app_mass_kick: vacio, keyring_available: () => true, updater_install_supported: () => true,
    listar_agentes_ia: () => [{ agente: 'claude', ruta: '/usr/bin/claude', modelos: ['sonnet', 'opus', 'haiku'] }],
    listar_avisos_recientes: () => [], leer_plan_servidor: () => '# Demo plan\n\n## Information\n- text: rules\n- text: announcements\n', leer_contexto_servidor: () => 'A friendly community about art and games.',
    guardar_plan_servidor: vacio, guardar_contexto_servidor: vacio, leer_conversacion: vacio, guardar_conversacion: vacio, nueva_conversacion: vacio,
    resumir_plan: () => ({ categorias: 2, canales: 5, roles: 3, texto_libre: 0, tiene_estructura: true, avisos: [] }), coste_descripciones_plan: () => ({ descripciones: 0, llamadas: 0, confirmar: false }),
    list_templates: () => [], invite_url: () => 'https://discord.com/oauth2/authorize?client_id=demo', ruta_mcp: () => '/opt/argus/argus-mcp',
    skill_destino: () => ({ path: '/home/demo/.claude/skills/argus', exists: false }),
    connector_states: () => [], backup_schedule: () => ({ enabled: false, conversations: false, folder: '' }),
    servicio_estado: () => ({ enabled: false, running: false, autostart: false, detail: null }), diagnostico_servicio: () => [diagnostico('service', 'verified')],
    'plugin:updater|check': vacio,
    'plugin:event|listen': (a) => { escuchas.push(a); return escuchas.length; }, 'plugin:event|unlisten': vacio,
  };
  const escuchas = [];

  window.__e2eManejadores = manejadores;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  const callbacks = {};
  let siguiente = 1;
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback: (cb) => { const i = siguiente++; callbacks[i] = cb; return i; },
    unregisterCallback: (i) => { delete callbacks[i]; },
    convertFileSrc: (p) => p,
    invoke: async (cmd, args) => {
      const h = manejadores[cmd];
      if (!h) {
        sinSimular.push(cmd);
        throw `command ${cmd} is not simulated`;
      }
      return JSON.parse(JSON.stringify((await h(args || {})) ?? null));
    },
  };
}

export async function instalarMock(pagina, idioma = 'en') {
  await pagina.addInitScript(backendSimulado, idioma);
}
