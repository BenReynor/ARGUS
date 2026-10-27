# plan.md — ARGUS

Plan de trabajo de ARGUS: app de escritorio (Tauri 2 + React + TypeScript + Rust)
para administrar **bots propios de Discord** (cualquier usuario, cualquier bot,
cualquier servidor). Visión y decisiones en CONTEXTO.md.

## Convenciones de este plan
- `- [ ]` pendiente · `- [x]` hecha · `- [B]` bloqueada. Cada tarea lleva su línea
  `criterio:` (qué archivos, qué comportamiento, qué test).
- Una tarea = un cambio pequeño y verificable por el gate (cargo fmt/check/test en
  `src-tauri/`, tsc, npm run build).
- Tests SIN red: el cliente de Discord se construye con una URL base de pruebas y
  los tests usan un Discord falso local (`wiremock`). Nunca un token real.
- Nada hardcodeado: ningún ID/nombre de servidor, canal, bot o usuario en el código.
- Las verificaciones manuales (servidor de pruebas del dueño) van como viñetas
  sin casilla al final de cada fase: el loop no las toma.
- Idioma: español (UI, mensajes de error y documentación).

---

## FASE 0 — Fundamentos (refactor no funcional)
Preparar el modelo de datos para multi-servidor sin romper lo actual.
El gate de Dédalo ya compila y prueba todo (cargo fmt/check/test en src-tauri, tsc,
npm run build): no hace falta una tarea de "verificar build".

- [x] Cambiar `AppState` para soportar guilds + mapas por guild (`server_maps: HashMap<String, ServerMap>`, `guilds_cache`)
- [x] Definir tipos enriquecidos (`Channel`, `Role`, `Member`) con permisos bitfield, overwrites, avatar, joined_at
- [x] Añadir comandos Tauri `get_active_guild` / `set_active_guild`
- [x] Corregir `save_config` / `save_config_full`: ignoran la config recibida
  criterio: en src-tauri/src/commands.rs, save_config y save_config_full guardan el parámetro `config` en `state.config` antes de llamar a `save_config_to_db`; `cargo check` ya no avisa `unused variable: config`
- [x] Cargar la configuración guardada al arrancar
  criterio: el setup de src-tauri/src/main.rs llama a `load_config_from_db` (con `tauri::async_runtime`) y un error solo se registra con `tracing::error!` (el crate `log` no es dependencia) sin abortar el arranque; `cargo check` ya no avisa que `load_config_from_db` nunca se usa
- [x] Quitar el `block_on` de `cleanup_user_context` (hace panic dentro del runtime de tokio)
  criterio: en src-tauri/src/state.rs `cleanup_user_context` es `async fn` y usa `.await`; no queda ningún `block_on` en state.rs; sus llamadas (si las hay) usan `.await`
- [x] Añadir a `Config` (state.rs): `auto_sync_map`, `sync_interval_secs`, `bot_engine`
  criterio: Config tiene auto_sync_map: bool (por defecto true), sync_interval_secs: u64 (por defecto 300) y bot_engine: String (por defecto "rust"), con `#[serde(default = ...)]` para que una config guardada antigua siga cargando; un test `#[cfg(test)]` en state.rs deserializa un JSON de Config sin esos campos y comprueba los valores por defecto
- [x] Auditar `src-tauri/src/commands.rs` y listar los comandos mock a sustituir
  criterio: existe docs/AUDITORIA_COMANDOS.md con una tabla de TODOS los `#[tauri::command]` de commands.rs (nombre, estado: real / mock / stub-TODO, endpoint de Discord que lo sustituirá si aplica); cada comando registrado en `generate_handler!` de main.rs aparece en la tabla
- [x] Sincronizar los tipos TS de `src/types` con los tipos Rust
  criterio: src/types/index.ts refleja los campos de Channel, Role, Member y Config de state.rs; un `Option<T>` de Rust (serde lo envía como null) es `campo: T | null` en TS; incluye overwrites, permissions_bitfield, joined_at, avatar, active_guild_id, auto_sync_map, sync_interval_secs y bot_engine; los defaults de src/store/useStore.ts coinciden con Config::default() de Rust; las pantallas que usan Config (SettingsView, Dashboard, Sidebar) se adaptan sin quitar funcionalidad; tsc y npm run build pasan
- [x] Añadir selector de servidor (guild) en el Header
  criterio: src/api/tauri.ts exporta getActiveGuild/setActiveGuild (invoke de get_active_guild/set_active_guild); Header.tsx muestra un `<select>` con los guilds de listGuilds y al cambiarlo llama a setActiveGuild y guarda la guild activa en src/store/useStore.ts; tsc y npm run build pasan

## FASE 1 — Bots genéricos (multi-bot de verdad, sin nada hardcodeado)
Que cualquier usuario registre sus bots y elija uno, sin rastro de un servidor concreto.

- [x] Quitar del código todo lo específico de un servidor o bot
  criterio: en src/ y src-tauri/src no queda ningún ID numérico de Discord, ni "ISD", "Site-14" ni "Agente Albert" (incluidos ChannelsView, Dashboard, MembersView, Sidebar, Header, SettingsView, useStore, state.rs y commands.rs); `Config::default()` y el store de TS tienen `active_guild_id` = null; los comandos que necesitan guild devuelven el error "No hay servidor seleccionado" si no hay ninguno (sin valor por defecto); tsc y cargo test pasan
- [x] Módulo cliente de Discord con twilight-http y URL base configurable
  criterio: src-tauri/src/discord.rs expone una función que construye un `twilight_http::Client` a partir de un token y una URL base opcional (para tests: `ClientBuilder::proxy("127.0.0.1:PUERTO".into(), true)` y `.ratelimiter(None)`); se añade `wiremock` como dev-dependency; un test levanta un Discord falso que responde a `GET /api/v10/users/@me` y comprueba que el cliente obtiene el usuario; ningún test usa red real
- [x] Registrar un bot validando su token contra Discord
  criterio: `add_bot` recibe solo el token, llama a `GET /users/@me` y guarda el bot con el `id`, `username` y `avatar` oficiales (nuevas columnas por migración 002 si hacen falta) y el token cifrado; un token rechazado (401) devuelve "Token inválido" sin guardar nada; tests con el Discord falso para el caso válido y el 401
- [x] Elegir el bot activo desde la UI
  criterio: la Sidebar lista los bots registrados con su nombre y avatar oficiales y permite elegir el activo (`set_active_bot`), que se persiste en la BD; SettingsView tiene "Añadir bot" (campo de token, sin nombre) y "Quitar bot" con confirmación; tsc y npm run build pasan
- [x] Retirar discord-mcp: `start_bot` valida el token del bot activo
  criterio: `start_bot` ya no lanza `npx @pasympa/discord-mcp` ni ningún proceso externo; descifra el token del bot activo, valida con `GET /users/@me` y deja `bot_status` como conectado con el nombre oficial; sin bot activo devuelve "No hay bot seleccionado"; test con el Discord falso
- [x] Listar los servidores del bot activo
  criterio: `list_guilds` llama a `GET /users/@me/guilds` con el bot activo, actualiza la tabla guilds y devuelve id, nombre e icono; el selector del Header usa esa lista; test con el Discord falso (2 guilds)

Verificación manual en tu servidor de pruebas:
- Registrar el bot con su token y ver su nombre y avatar oficiales.
- Ver el servidor de pruebas en el selector del Header y seleccionarlo.

## FASE 2 — Datos y acciones reales por REST
Que las vistas muestren el servidor real y las acciones funcionen. Cada tarea con test contra el Discord falso.

- [x] Enlace para invitar el bot a un servidor
  criterio: al registrar el bot se obtiene su id de aplicación con `GET /applications/@me` (twilight `current_user_application`) y se guarda; un comando `invite_url` construye `https://discord.com/oauth2/authorize?client_id=<id>&scope=bot%20applications.commands&permissions=<n>` con los permisos que ARGUS usa (ver canales, enviar mensajes, leer historial, gestionar canales, gestionar roles, moderar miembros, expulsar, banear) definidos en una constante documentada; la UI tiene un botón "Invitar a un servidor" que abre el enlace con tauri-plugin-opener; test unitario del enlace y test con el Discord falso de `/applications/@me`
- [x] Avisar si falta el intent de miembros
  criterio: al registrar el bot y en `start_bot` se leen los `flags` de `GET /applications/@me`; si no incluyen GATEWAY_GUILD_MEMBERS (1<<14) ni GATEWAY_GUILD_MEMBERS_LIMITED (1<<15) se devuelve un aviso (no un error) "Activa «Server Members Intent» en el Developer Portal → Bot" que la UI muestra junto al bot; tests con el Discord falso para el caso con y sin flag
- [x] Leer canales y roles reales
  criterio: `list_channels` y `list_roles` llaman a `GET /guilds/{id}/channels` y `GET /guilds/{id}/roles` del servidor activo y convierten la respuesta a los tipos Channel y Role de state.rs (con overwrites y permisos); tests con el Discord falso
- [x] Leer miembros con paginación
  criterio: `list_members` pagina `GET /guilds/{id}/members?limit=1000&after=<último id>` hasta agotar; `get_member_info` usa `GET /guilds/{id}/members/{user_id}`; test con el Discord falso que sirve 2 páginas
- [x] Errores de Discord legibles
  criterio: un único conversor en discord.rs traduce las respuestas de error a mensajes en español: 401 token inválido, 403 "al bot le falta el permiso X" (cuando Discord lo indica), 404 no encontrado, 429 límite de peticiones (twilight reintenta; si se agota, mensaje claro); todos los comandos de Discord lo usan; tests de cada caso
- [x] Permisos del bot en el servidor activo
  criterio: un comando `bot_permissions(guild_id)` calcula los permisos efectivos del bot en ese servidor (rol @everyone + roles del miembro bot vía `GET /guilds/{id}/members/{bot_id}` y los roles del servidor; Administrator o ser dueño = todos) y devuelve la lista con nombres en español; tests unitarios del cálculo (sin roles, con rol que concede, con Administrator) y test con el Discord falso
- [x] Panel del servidor activo
  criterio: Dashboard muestra nombre, icono, número aproximado de miembros (`GET /guilds/{id}?with_counts=true`) y los permisos del bot en ese servidor, marcando los que faltan; las acciones de MembersView/ChannelsView que necesitan un permiso ausente se desactivan con un texto que lo explica; al cambiar de servidor en el Header el panel se recarga; tsc y npm run build pasan
- [x] Configuración propia de cada servidor
  criterio: migración 003 crea `guild_settings (bot_id, guild_id, data, updated_at, PRIMARY KEY(bot_id, guild_id))`; comandos `get_guild_settings` / `save_guild_settings` con un struct `GuildSettings` (alias opcional, notas, módulos activados: vigilancia, personalidad y plantillas, todos false por defecto; `#[serde(default)]`); al cambiar de servidor la UI carga los ajustes de ese servidor y SettingsView tiene una sección "Este servidor"; tests de guardar y leer con una BD SQLite temporal
- [ ] Enviar mensajes a un canal y por DM
  criterio: `send_message` usa `POST /channels/{id}/messages`; `send_dm` abre el DM (`POST /users/@me/channels`) y envía; validan longitud (≤ 2000) antes de llamar; tests con el Discord falso
- [ ] Gestionar roles de un miembro
  criterio: `add_role` / `remove_role` usan `PUT`/`DELETE /guilds/{id}/members/{user_id}/roles/{role_id}`; MembersView usa el servidor activo (no un ID fijo); tests con el Discord falso
- [ ] Silenciar (timeout) a un miembro
  criterio: `timeout_member` usa `PATCH /guilds/{id}/members/{user_id}` con `communication_disabled_until` (máx. 28 días; 0 minutos quita el timeout); test con el Discord falso

Verificación manual en tu servidor de pruebas:
- Canales, roles y miembros coinciden con Discord.
- Enviar un mensaje y un DM; dar y quitar un rol; silenciar 1 minuto.
- Invitar el bot a un segundo servidor con el botón y cambiar entre ambos: cada uno muestra sus permisos y su propia configuración.

## FASE 3 — Mapa en vivo (Gateway)
Leer todo al conectar y mantenerlo al día con los eventos de Discord.

- [ ] Aplicar eventos al mapa como función pura
  criterio: en src-tauri/src/mapa.rs, una función aplica un evento de `twilight_model` (GUILD_UPDATE, CHANNEL_CREATE/UPDATE/DELETE, ROLE_*, GUILD_MEMBER_ADD/UPDATE/REMOVE, THREAD_*) a un `ServerMap` y devuelve un resumen del cambio; tests unitarios por tipo de evento, sin red
- [ ] Conectar el Gateway mientras el bot está activo
  criterio: `start_bot` lanza una tarea tokio con `twilight-gateway` (intents: GUILDS, GUILD_MEMBERS) que aplica cada evento al mapa con la función anterior; `stop_bot` la detiene; ningún `MutexGuard` cruza un `.await`; cargo test pasa
- [ ] Sincronización completa al conectar
  criterio: al conectar se carga guild + canales + roles + miembros del servidor activo por REST y se emite el evento Tauri `sync-progress` (paso actual y total); test de la función de sync con el Discord falso
- [ ] La UI se actualiza sin polling
  criterio: el backend emite `map-updated` tras aplicar eventos (agrupados con debounce de ~500 ms); App.tsx escucha `map-updated` y `sync-progress` y deja de consultar el estado cada 5 s; tsc y npm run build pasan
- [ ] Guardar el mapa y el historial de cambios
  criterio: el mapa se guarda en `server_maps` como mucho cada 30 s si cambió, y cada cambio resumido se inserta en `incidents`; al arrancar se carga el último mapa guardado; tests de la persistencia con una BD SQLite temporal
- [ ] Botón "Resincronizar mapa"
  criterio: un comando `resync_map` repite la sincronización completa y la UI tiene un botón en Dashboard que lo llama y muestra el progreso; tsc pasa

Verificación manual en tu servidor de pruebas:
- Crear y borrar un canal y un rol en Discord y ver el cambio en ARGUS sin recargar.
- Desconectar la red 1 minuto y comprobar que se reconecta solo.

## FASE 4 — Prueba real (cierre del MVP)
Comprobar con Discord de verdad, en tus servidores de prueba y con otro usuario, que todo lo anterior funciona.

- [ ] Tests de integración contra un servidor real, desactivados por defecto
  criterio: src-tauri/tests/integracion_discord.rs con tests `#[ignore]` que leen ARGUS_TEST_TOKEN, ARGUS_TEST_GUILD, ARGUS_TEST_CHANNEL, ARGUS_TEST_ROLE y ARGUS_TEST_USER del entorno y, si falta alguna, terminan sin hacer nada; usan las mismas funciones que los comandos para: validar el token, comprobar que ARGUS_TEST_GUILD está entre los servidores del bot, leer canales/roles/miembros, calcular los permisos del bot, enviar un mensaje a ARGUS_TEST_CHANNEL, enviar un DM a ARGUS_TEST_USER, dar y quitar ARGUS_TEST_ROLE a ARGUS_TEST_USER y aplicar y retirar un timeout de 1 minuto; ningún valor real queda en el repo; `cargo test` normal no los ejecuta; docs/PRUEBA_REAL.md explica cómo preparar el servidor y correrlos con `cargo test -- --ignored`

Prueba final manual (el dueño, con la app compilada desde el acceso directo):
- Correr los tests de integración contra el servidor de pruebas (docs/PRUEBA_REAL.md).
- Registrar dos bots y cambiar entre ellos; con cada uno, cambiar entre dos servidores.
- En cada servidor: canales, roles y miembros correctos; permisos del bot correctos; configuración propia que se conserva al volver.
- Con otro usuario de Discord: recibir el DM, recibir y perder el rol, timeout de 1 minuto.
- Cambios hechos en Discord (canal, rol, miembro que entra o sale) aparecen en ARGUS sin recargar.
- Quitar el bot de un servidor: desaparece de la lista sin errores.

## Después del MVP (sin criterios todavía: se detallan al empezar cada bloque)
- Edición completa del servidor: crear/editar/borrar canales, categorías y roles; permisos y overwrites; ban/kick con confirmación.
- Plantillas: esquema, vista de plantillas, vista previa en árbol, aplicador ordenado (roles → categorías → canales → overwrites), validación de permisos, crear servidor nuevo desde plantilla.
- Personalidad del bot: perfiles (prompt, modelo, canales, reglas), escuchar MESSAGE_CREATE, filtros (mención, canal, cooldown, ignorar bots), proveedor LLM configurable (Ollama / compatible OpenAI), memoria corta por canal, anti-spam, modo observador, comandos slash.
- Vigilancia y logs: niveles y timestamps, eventos con filtros, vista timeline, alertas nativas, health check del Gateway, interacción opcional con otros bots con reglas anti-bucle.
- Integraciones: Telegram, backup automático, MCP propio de ARGUS, agentes CLI.
- UX y rendimiento: i18n ES/EN, temas, listas virtualizadas, paginación de miembros en la UI, métricas, revisar CPU/RAM.
