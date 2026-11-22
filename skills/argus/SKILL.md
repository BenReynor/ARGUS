---
name: argus
description: Use the ARGUS MCP server to inspect and configure a Discord server with few tokens. Use when the user asks to build, change, audit or check a Discord server through ARGUS.
---

# ARGUS: configure Discord servers cheaply

ARGUS exposes your bots through the `argus-mcp` MCP server. Every call costs
tokens, so read little, once, and write in one batch.

## 1. Orient once

1. Call `estado` to see the active bot and server. Do not call it again.
2. Call `mapa_servidor` once. It returns categories, channels and roles in a
   single answer; keep it as your working copy for the whole conversation.
3. Only if something is missing, use a narrow read (`listar_roles`,
   `listar_canales`, `permisos_bot`, `informe_permisos`). Never re-read what the
   map already told you.

If there are several servers, use `listar_servidores` and ask the user which
one; do not guess.

## 2. Use names, not IDs

Where a tool takes an ID for a role, channel, category or member, its
description says whether a name is accepted too (most do). Write `Moderators`
or `#rules`, not a numeric ID. Inside a proposal, refer to things created in
that same proposal by name. Never invent an ID and never ask the user for one
unless a name is ambiguous.

## 3. Group changes into one proposal

You do not write directly: writes (`crear_canal`, `crear_rol`, `permiso_canal`,
`editar_servidor`, `expulsar`, `banear`, …) are proposed as a list of actions
that the user approves in ARGUS, and ARGUS runs exactly what was approved.

- Put everything in a single proposal, ordered: roles, categories, channels,
  permissions, then member roles.
- For a ready-made structure, check `listar_plantillas` and `consultar_plantilla`,
  and use `proponer_plantilla` to prepare applying one.
- To fix an audit finding, use `proponer_correccion` instead of hand-building it.
- To restore a snapshot, read `listar_snapshots`, `comparar_snapshot` and
  `info_restauracion`, then propose the restore action. Never call
  `aplicar_restauracion` yourself.
- Never say a change is done before the user approved it and ARGUS ran it.

## 4. Do not repeat reads

- Reuse the map and earlier answers; read again only after a change was
  applied, and then only the affected part.
- Before repeating an operation that may already be done, check the current
  state instead of redoing it.
- Do not page through members or messages (`listar_miembros`, `leer_mensajes`)
  unless the task needs them; ask for a small limit.

## 5. Check the result

An accepted request is not a verified change. After the user applies a
proposal, read the affected part once (`mapa_servidor`, or `informe_permisos`
for permissions) and compare it with what you proposed. Report results as
correct, partial, failed, cancelled or unverified, and say plainly what could
not be verified. A permissions report is partial evidence: never claim a server
is safe from it.

## Safety

- The bot can only do what its permissions allow; check `permisos_bot` before
  proposing something it cannot do.
- Destructive actions (delete, ban, mass changes) need the user's explicit
  confirmation in ARGUS.
- Never ask for or print the bot token.
