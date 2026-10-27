# Auditoría de comandos Tauri (`src-tauri/src/commands.rs`)

Leyenda de estado:
- **real**: hace trabajo efectivo (estado en memoria, SQLite, proceso local); no necesita Discord API.
- **mock**: devuelve datos inventados o no hace nada real, aunque su firma aparenta hacerlo.
- **stub-TODO**: marcado con `TODO` en el código; devuelve un valor fijo.
- **parcial**: real en parte, pero depende de datos mock o tiene valores hardcodeados.

Todos los comandos de la tabla están registrados en `generate_handler!` de `main.rs`,
salvo los dos últimos (ver «No registrados»).

## Tabla

| Comando | Estado | Notas | Endpoint de Discord que lo sustituirá |
|---|---|---|---|
| `start_bot` | parcial | Lanza `npx @pasympa/discord-mcp` como proceso hijo; `guild_id` por defecto y `guild_name` hardcodeados (ISD). | Fase 2: Gateway propio (`GET /gateway/bot` + WSS); validar token con `GET /users/@me`; nombre de guild con `GET /guilds/{id}` |
| `stop_bot` | real | Mata el proceso hijo y limpia el estado. | — |
| `get_bot_status` | real | Lee `bot_status` en memoria. | — |
| `list_guilds` | parcial | Devuelve `config.guilds_cache`, que nadie puebla. | `GET /users/@me/guilds` |
| `get_active_guild` | real | Lee el estado. | — |
| `set_active_guild` | real | Escribe el estado (no persiste en BD). | — |
| `list_channels` | parcial | Lee del mapa en memoria; falla con «Mapa no disponible» si no hay mapa. | `GET /guilds/{id}/channels` |
| `list_members` | parcial | Lee del mapa en memoria (igual que arriba). | `GET /guilds/{id}/members?limit=1000&after=` (paginado) |
| `list_roles` | parcial | Lee del mapa en memoria (igual que arriba). | `GET /guilds/{id}/roles` |
| `send_message` | mock | Solo hace `println!` y devuelve texto de éxito. | `POST /channels/{id}/messages` |
| `send_dm` | mock | Solo `println!` y texto de éxito. | `POST /users/@me/channels` + `POST /channels/{id}/messages` |
| `get_member_info` | mock | Devuelve un `Member` ficticio ("Usuario"). | `GET /guilds/{id}/members/{user_id}` |
| `add_role` | mock | Devuelve texto de éxito sin actuar. | `PUT /guilds/{id}/members/{user_id}/roles/{role_id}` |
| `remove_role` | mock | Devuelve texto de éxito sin actuar. | `DELETE /guilds/{id}/members/{user_id}/roles/{role_id}` |
| `timeout_member` | mock | Devuelve texto de éxito sin actuar. | `PATCH /guilds/{id}/members/{user_id}` (`communication_disabled_until`) |
| `get_config` | real | Clona `state.config`. | — |
| `get_config_full` | real | Idéntico a `get_config`. | — |
| `save_config` | real | Guarda en memoria y en BD. | — |
| `save_config_full` | real | Idéntico a `save_config` (solo cambia el mensaje). | — |
| `get_server_map` | parcial | Lee el mapa de la guild activa; guild por defecto hardcodeada. | — (lo rellenará el full sync de Gateway/REST) |
| `refresh_server_map` | mock | Guild inventada (nombre y `member_count: 20` fijos, `joined_at` = ahora); compone el mapa con `list_*`, que fallan si no hay mapa previo. | `GET /guilds/{id}` + `/channels` + `/roles` + `/members` |
| `list_bots` | real | Lee `config.bots`. | — |
| `add_bot` | real | Cifra el token y guarda en memoria y BD. | — (validación opcional: `GET /users/@me`) |
| `update_bot` | real | Idem; `created_at` se reescribe en cada actualización. | — |
| `delete_bot` | real | Borra de memoria y BD. | — |
| `set_active_bot` | real | Escribe el estado. | — |
| `list_templates` | real | SQLite. | — |
| `save_template` | real | SQLite. | — |
| `delete_template` | real | SQLite. | — |
| `apply_template` | stub-TODO | Lee la plantilla y devuelve éxito sin tocar Discord. | `POST /guilds/{id}/roles`, `POST /guilds/{id}/channels` (categorías, canales y `permission_overwrites`) |
| `get_vigilance_config` | real | Lee `config.vigilance`. | — |
| `save_vigilance_config` | real | Memoria + SQLite. | — |
| `list_cli_agents` | real | Lee `config.cli_agents`. | — |
| `save_cli_agent` | real | Memoria + SQLite. | — |
| `search_messages` | real | Búsqueda FTS en SQLite. | — |
| `cleanup_context` | real | Limpia contexto en SQLite. | — |
| `export_all_data` | stub-TODO | Devuelve `vec![]` (sin ZIP). | — (local) |
| `import_data` | stub-TODO | Ignora los datos y devuelve «Importado». | — (local) |
| `uninstall_wipe` | stub-TODO | No borra nada; devuelve éxito. | — (local) |

## No registrados en `generate_handler!`

Tienen `#[tauri::command]` pero no están en `main.rs`, así que el frontend no puede invocarlos
(el compilador avisa «never used»). Son duplicados de `get_active_guild` / `set_active_guild`:

| Comando | Estado | Notas | Endpoint de Discord |
|---|---|---|---|
| `get_active_guild_cmd` | real (sin registrar) | Duplicado de `get_active_guild`. | — |
| `set_active_guild_cmd` | real (sin registrar) | Duplicado de `set_active_guild`. | — |

## Resumen de comandos mock/stub a sustituir

- **Mock (Discord):** `send_message`, `send_dm`, `get_member_info`, `add_role`, `remove_role`, `timeout_member`, `refresh_server_map`.
- **Parciales (dependen de datos reales de Discord):** `list_guilds`, `list_channels`, `list_members`, `list_roles`, `get_server_map`, `start_bot`.
- **Stub-TODO:** `apply_template` (Discord API); `export_all_data`, `import_data`, `uninstall_wipe` (locales, sin Discord).
