# plan.md — ARGUS

Plan de trabajo de ARGUS: app de escritorio (Tauri 2 + React + TypeScript + Rust)
para controlar el bot propio de Discord **"Agente Albert"**. Objetivo: convertir el
panel actual (mocks) en un **centro de orquestación** del bot: multi-servidor,
sincronización automática del mapa, plantillas para armar servidores, personalidad
y reglas configurables, comandos desde Discord, lectura de logs, interacción con
otros bots y optimización.

## Convenciones de este plan
- `- [ ]` pendiente · `- [x]` hecha · `- [B]` bloqueada
- Una tarea por línea, pequeña y verificable.
- No romper el build en cada iteración (`npm run build` / `cargo build`).
- Idioma: español (UI y conversación).
- `source ~/.cargo/env` antes de cualquier comando cargo.

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
- [ ] Sincronizar los tipos TS de `src/types` con los tipos Rust
  criterio: src/types/index.ts refleja los campos de Channel, Role, Member y Config de state.rs; un `Option<T>` de Rust (serde lo envía como null) es `campo: T | null` en TS; incluye overwrites, permissions_bitfield, joined_at, avatar, active_guild_id, auto_sync_map, sync_interval_secs y bot_engine; los defaults de src/store/useStore.ts coinciden con Config::default() de Rust; las pantallas que usan Config (SettingsView, Dashboard, Sidebar) se adaptan sin quitar funcionalidad; tsc y npm run build pasan
- [x] Añadir selector de servidor (guild) en el Header
  criterio: src/api/tauri.ts exporta getActiveGuild/setActiveGuild (invoke de get_active_guild/set_active_guild); Header.tsx muestra un `<select>` con los guilds de listGuilds y al cambiarlo llama a setActiveGuild y guarda la guild activa en src/store/useStore.ts; tsc y npm run build pasan

## FASE 1 — Conexión real a Discord API (sustituir mocks)
Que `list_*` y operaciones dejen de ser mock.

- [ ] Crear cliente Discord REST en Rust (`reqwest`) con `Authorization: Bot <token>`
- [ ] Implementar `list_guilds` real: `GET /users/@me/guilds`
- [ ] Implementar `list_channels`: `GET /guilds/{id}/channels`
- [ ] Implementar `list_roles`: `GET /guilds/{id}/roles`
- [ ] Implementar `list_members` con paginación por cursor (`after`), límite 1000
- [ ] Implementar `get_member_info` real: `GET /guilds/{id}/members/{user_id}`
- [ ] Implementar `send_message` real: `POST /channels/{id}/messages`
- [ ] Implementar `send_dm` real: crear DM + enviar mensaje
- [ ] Implementar `add_role` / `remove_role` reales (`PUT`/`DELETE`)
- [ ] Implementar `timeout_member` real: `PATCH /guilds/{id}/members/{user_id}`
- [ ] Manejar errores y rate-limit (`X-RateLimit-*`, `Retry-After`, reintentos)
- [ ] Validar token al arrancar / en "Probar conexión" (endpoint `/users/@me`)
- [B] Verificar en UI: canales, miembros y roles reales del servidor (manual: un agente sin pantalla no puede verificar la UI)

## FASE 2 — Sincronización inteligente del mapa
"Leer todo al prender + detectar cambios y actualizar".

- [ ] Añadir cliente Gateway en Rust (`tokio-tungstenite` o `twilight-gateway`)
- [ ] Mantener tarea de Gateway viva mientras `bot_status.running`
- [ ] Full sync al iniciar: guild + channels + roles + members + categorías
- [ ] Emitir progreso de sync al frontend ("Sincronizando... X/Y")
- [ ] Aplicar eventos a `server_map` en memoria: `GUILD_UPDATE`, `CHANNEL_*`, `ROLE_*`, `GUILD_MEMBER_*`
- [ ] Soportar hilos de foro (`THREAD_*`) en el mapa
- [ ] Emitir evento Tauri `map-updated` y refrescar UI sin polling
- [ ] Registrar "diff" de cambios (canal creado/rol modificado/miembro unido)
- [ ] Persistir snapshot del mapa (no en cada evento, con debounce)
- [ ] Reconexión automática del Gateway + heartbeats
- [ ] Botón "Resincronizar mapa" manual en la UI

## FASE 3 — Plantillas para armar servidores
Crear estructuras completas desde plantillas reutilizables.

- [ ] Definir esquema de plantilla (`roles`, `categories`, `channels`, `overwrites`, `default_perms`)
- [ ] Crear pestaña/vista "Plantillas" (crear/editar/clonar/eliminar)
- [ ] Persistencia de plantillas (store/FS)
- [ ] Preview en árbol antes de aplicar
- [ ] Aplicador ordenado: roles → categorías → canales → overwrites
- [ ] Validación de permisos del bot (ManageGuild/Channels/Roles)
- [ ] Aplicar plantilla a servidor existente
- [ ] (Opcional) Crear servidor nuevo desde plantilla
- [B] Verificar en UI: aplicar plantilla de prueba y comprobar resultado (manual)

## FASE 4 — Personalidad, reglas y conversación
Dar personalidad configurable al bot y control desde Discord.

- [ ] Modelo de datos `Persona` (nombre, system_prompt, temperatura, modelo, canales permitidos, roles bloqueados, reglas)
- [ ] Modelo de datos `Rule` (trigger, acción, alcance, enabled)
- [ ] Vista "Personalidad": editor de prompt + reglas + toggles
- [ ] Escuchar `MESSAGE_CREATE` desde el Gateway
- [ ] Lógica de respuesta: filtros (mención, canal, cooldown, ignorar bots)
- [ ] Integración LLM: proveedor local (Ollama) + fallback a API externa
- [ ] Memoria ligera: últimos N mensajes por canal
- [ ] Anti-spam / rate-limit de respuestas
- [ ] Modo observador (vigilar sin responder)
- [ ] Comandos Discord (prefijo `!argus` y/o slash) para controlar el bot

## FASE 5 — Logs, interop con otros bots y vigilancia
- [ ] Timestamps + niveles (INFO/WARN/ERR) en los logs del bot
- [ ] Logs de eventos Discord (joins/leaves, bans, cambios de rol/canal) con filtros
- [ ] Interacción con otros bots (opt-in vía `respond_to_bots` + reglas anti-bucle)
- [ ] Registrar incidencias de cambios del mapa (`incidencias.jsonl`) con la lógica de Dédalo
- [ ] Alertas vía `tauri-plugin-notification` para eventos críticos
- [ ] Vista "Vigilancia": timeline de eventos + búsqueda
- [ ] Health check: latencia, reconexión, estado del Gateway

## FASE 6 — Optimización y UX
- [ ] Sustituir clones costosos por `Arc<RwLock<...>>` / mutex fino
- [ ] Throttle/debounce de eventos Gateway
- [ ] Virtualizar listas largas en React (tanstack/react-virtual)
- [ ] Sustituir polling de estado (5s) por eventos del backend
- [ ] Paginación de miembros en UI (evitar cargar todo de golpe)
- [ ] Métricas: eventos/s, req/s, cache hits
- [ ] Revisar consumo CPU/RAM y reducir

## FASE 7 — Patrón Dédalo (motor separado + MCP propio) — opcional
Inspirado en Dédalo: motor independiente de la GUI + servidor MCP propio de ARGUS.

- [ ] Separar el motor del bot de la capa GUI/UI
- [ ] Exponer un servidor MCP propio de ARGUS (herramientas: estado, mapa, plantillas, personalidad)
- [ ] Estado en archivos observables (mapa, incidencias, logs)
- [ ] Manual embebido en `instructions` del MCP
- [ ] Integrar con Dédalo si aporta (plantillas ↔ plan.md, automatismos)
