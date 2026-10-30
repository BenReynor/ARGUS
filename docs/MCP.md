# ARGUS MCP

`argus-mcp` is an [MCP](https://modelcontextprotocol.io) server (stdio transport)
that lets Claude Code, Dédalo or any other MCP client manage Discord with the bots
you have registered in ARGUS.

> ⚠️ **Warning.** The MCP can do **everything the active bot's permissions allow**:
> delete channels and roles, kick or ban members, send messages, create webhooks
> and invites… The only limits are Discord's permissions and the bot's role
> hierarchy. Connect it only to trusted clients and, ideally, try it first on a
> test server.

## How it works

- It is a separate executable that **works with the app closed**.
- It reads the same database as the app (`~/.config/argus/argus.db`; on Windows
  `%APPDATA%\argus\argus.db`; or the path in the `ARGUS_DB` environment variable)
  and decrypts the active bot's token the same way ARGUS does. The token is never
  written to stdout, stderr or responses.
- It uses the same logic as the app: the same permission and hierarchy checks and
  the same error messages.
- Tool descriptions are always in English. Responses and errors use the language
  saved in the ARGUS configuration (Settings → General; English by default,
  Spanish optional), read from the database on every call.
- With no active bot, every tool answers «No active bot in ARGUS: add or choose
  one in the app» (in Spanish: «No hay bot activo en ARGUS: añade o elige uno en
  la app») without crashing.

## Installation

1. Install ARGUS and add at least one bot (Settings → Bots). The `argus-mcp`
   binary ships next to the app executable.
2. Open **Settings → MCP**: it shows the exact path of `argus-mcp` and the
   ready-to-copy blocks with the **Copy** button.
3. Register it in your client:

**Claude Code**

```bash
claude mcp add argus -- /path/to/argus-mcp
```

**Other clients** (MCP JSON configuration):

```json
{
  "mcpServers": {
    "argus": {
      "command": "/path/to/argus-mcp",
      "args": []
    }
  }
}
```

If you run ARGUS from source, build the binary with
`cargo build --release --bin argus-mcp --manifest-path src-tauri/Cargo.toml`
(it ends up in `src-tauri/target/release/`). `npm run tauri build` includes it in
the packages next to the app executable (Tauri bundles every binary of the crate).

## Active server

Almost every tool accepts an **optional** `guild_id`. If you omit it, they use the
**active server**: the one you choose in the app or with `elegir_servidor`, which
saves the change in the database (the app will see it). With no active server and
no `guild_id`, the tool returns a clear error, for example «No active server in
ARGUS: pass `guild_id` or choose one with `elegir_servidor`».

Permissions (`permisos`, `permitir`, `denegar`) are accepted as a bits string
(`"8192"`) or as a list of names (`["ManageMessages","ViewChannel"]`).

## Tools

Tool and parameter names are identifiers and stay as they are. In the tables,
`guild_id?` is the optional server ID described above. A trailing `?` marks an
optional parameter.

### Status and servers

| Tool | Parameters | Description |
|---|---|---|
| `estado` | — | Active bot (official name) and active server. |
| `listar_servidores` | — | Servers the active bot is in. |
| `elegir_servidor` | `guild_id` | Changes the active server. |

### Read

| Tool | Parameters | Description |
|---|---|---|
| `mapa_servidor` | `guild_id?` | Server map: categories, channels and roles. |
| `listar_canales` | `guild_id?` | Channels with type and category. |
| `listar_roles` | `guild_id?` | Roles with color, position and permissions. |
| `listar_miembros` | `guild_id?`, `buscar?`, `limite?` (default 25, max. 100) | Members, searchable by user, nickname or ID. |
| `info_miembro` | `guild_id?`, `user_id` | Details of a member and their roles. |
| `permisos_bot` | `guild_id?` | The bot's effective permissions on the server. |
| `info_servidor` | `guild_id?` | General server data. |
| `ajustes_servidor` | `guild_id?` | Server settings (verification, filters, AFK…). |
| `registro_auditoria` | `guild_id?` | Latest audit log entries. |
| `leer_mensajes` | `guild_id?`, `channel_id`, `limite?` (default and max. 50) | Latest messages of a channel. |
| `listar_baneos` | `guild_id?` | Banned users. |
| `listar_invitaciones` | `guild_id?` | Active invites. |
| `listar_webhooks` | `guild_id?` | Webhooks, without exposing their token. |

### Messages

| Tool | Parameters | Description |
|---|---|---|
| `enviar_mensaje` | `channel_id`, `contenido` (≤ 2000) | Sends a message to a channel. |
| `enviar_dm` | `user_id`, `contenido` (≤ 2000) | Direct message to a user. |
| `borrar_mensaje` | `guild_id?`, `channel_id`, `message_id` | Deletes a message. |
| `borrar_mensajes` | `guild_id?`, `channel_id`, `message_ids` (2–100, under 14 days old) | Bulk delete. |
| `fijar_mensaje` | `guild_id?`, `channel_id`, `message_id` | Pins a message. |
| `desfijar_mensaje` | `guild_id?`, `channel_id`, `message_id` | Unpins a message. |

### Channels

| Tool | Parameters | Description |
|---|---|---|
| `crear_canal` | `guild_id?`, `nombre`, `tipo?` (texto, voz, categoria, anuncios, foro), `categoria_id?`, `tema?`, `nsfw?`, `slowmode_segundos?` (0–21600) | Creates a channel. |
| `editar_canal` | `guild_id?`, `channel_id`, `nombre?`, `categoria_id?` (empty = no category), `tema?`, `nsfw?`, `slowmode_segundos?` | Edits a channel. |
| `borrar_canal` | `guild_id?`, `channel_id` | Deletes a channel. |
| `ordenar_canales` | `guild_id?`, `posiciones` (list of `{id, posicion}`) | Reorders channels. |
| `permiso_canal` | `guild_id?`, `channel_id`, `target_id`, `tipo` (rol or miembro), `permitir?`, `denegar?` | Sets the permissions of a role or member in a channel. |
| `quitar_permiso_canal` | `guild_id?`, `channel_id`, `target_id`, `tipo` | Removes that overwrite. |

### Roles

| Tool | Parameters | Description |
|---|---|---|
| `crear_rol` | `guild_id?`, `nombre`, `color?` (numeric RGB or `#RRGGBB`), `separado?`, `mencionable?`, `permisos?` | Creates a role. |
| `editar_rol` | `guild_id?`, `role_id`, `nombre?`, `color?`, `separado?`, `mencionable?`, `permisos?` (replace the current ones) | Edits a role. |
| `borrar_rol` | `guild_id?`, `role_id` | Deletes a role. |
| `ordenar_roles` | `guild_id?`, `posiciones` (list of `{id, posicion}`) | Reorders roles. |
| `dar_rol` | `guild_id?`, `user_id`, `role_id` | Gives a role to a member. |
| `quitar_rol` | `guild_id?`, `user_id`, `role_id` | Removes a role from a member. |

### Moderation

The `motivo` goes to Discord's audit log. You cannot act on the server owner or on
members with a role equal to or above the bot's.

| Tool | Parameters | Description |
|---|---|---|
| `expulsar` | `guild_id?`, `user_id`, `motivo?` | Kicks a member. |
| `banear` | `guild_id?`, `user_id`, `motivo?`, `dias_mensajes?` (0–7) | Bans a user. |
| `desbanear` | `guild_id?`, `user_id`, `motivo?` | Removes a ban. |
| `silenciar` | `guild_id?`, `user_id`, `minutos` (max. 40320; 0 removes the timeout), `motivo?` | Times out a member. |
| `cambiar_apodo` | `guild_id?`, `user_id`, `apodo?` (≤ 32; empty clears it), `motivo?` | Changes the nickname. |

### Server, invites and webhooks

| Tool | Parameters | Description |
|---|---|---|
| `editar_servidor` | `guild_id?`, `nombre?`, `descripcion?`, `nivel_verificacion?` (0–4), `filtro_contenido_explicito?` (0–2), `notificaciones_por_defecto?` (0–1), `canal_del_sistema_id?`, `canal_afk_id?`, `afk_segundos?` (60, 300, 900, 1800, 3600) | Edits server settings. |
| `crear_invitacion` | `guild_id?`, `channel_id`, `caducidad_segundos?` (0–604800, default 86400), `usos_maximos?` (0–100) | Creates an invite. |
| `borrar_invitacion` | `guild_id?`, `codigo` | Revokes an invite. |
| `crear_webhook` | `guild_id?`, `channel_id`, `nombre` (1–80) | Creates a webhook; the full URL is only shown on creation. |
| `borrar_webhook` | `guild_id?`, `webhook_id` | Deletes a webhook. |

## Environment variables

| Variable | Effect |
|---|---|
| `ARGUS_DB` | Path of the database if it is not the default (`~/.config/argus/argus.db`; on Windows `%APPDATA%\argus\argus.db`). |

## Troubleshooting

- **«No active bot in ARGUS»**: open the app, add a bot and choose it as active.
- **«No active server in ARGUS»**: pass `guild_id` or use `elegir_servidor`.
- **Missing permission or hierarchy**: the message says which permission the bot
  needs, or which role must be moved up in the server's role list.
