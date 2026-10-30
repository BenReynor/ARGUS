# CONTEXTO.md — ARGUS

Fuente única de visión y decisiones. Si otro documento (README, FICHA_TECNICA,
ARCHITECTURE_MAP, AGENTS) contradice este, manda este.

## Qué es

App de escritorio (Tauri 2 + React + TypeScript + Rust) para **administrar bots
propios de Discord**: registrar tus bots, elegir con cuál trabajar, ver y editar
los servidores donde está (canales, roles, miembros, permisos), mapa del servidor
en vivo y, más adelante, darle personalidad y automatismos.

- **Producto general**, para cualquier usuario: nada atado a un servidor, canal o
  bot concreto. Ningún ID, nombre de servidor ni nombre de bot en el código.
- **Bots propios, no selfbots** (los selfbots violan los ToS de Discord): se usa la
  API oficial (REST + Gateway) con el token del bot.
- **Multi-bot**: el usuario registra uno o varios bots con su token; ARGUS lee su
  nombre y avatar oficiales de Discord (`GET /users/@me`) y el usuario elige el
  bot activo. Los servidores disponibles son los del bot activo.

## Decisiones tomadas

| Tema | Decisión |
|---|---|
| Conexión a Discord | **twilight** en Rust: `twilight-http` (REST, gestiona rate-limit) y `twilight-gateway` (eventos, heartbeats, reconexión). Se retira `npx @pasympa/discord-mcp` y los clientes a mano (`reqwest`/`tokio-tungstenite` para Discord). |
| Bots | Multi-bot. Token cifrado (AES-GCM) por bot en SQLite; nombre/avatar desde Discord, no escritos por el usuario. |
| Servidor activo | Lo elige el usuario entre los guilds del bot activo. Sin valor por defecto en el código. |
| Pruebas | Tests automáticos **sin red**: el cliente de Discord acepta una URL base de pruebas y los tests usan un Discord falso local. El loop de Dédalo nunca recibe un token real. La verificación real la hace el dueño en su **servidor de pruebas** (checklist manual al final de cada fase). |
| MVP | Fases 1–3 de plan.md: bots genéricos, datos y acciones reales por REST, mapa en vivo por Gateway. |

## Estado actual (lo que existe de verdad)

- FASES 0–8 de plan.md hechas: multi-bot, datos y acciones reales por REST,
  mapa en vivo por Gateway, interfaz con 10 temas, control total del servidor
  (canales, roles, permisos, moderación, ajustes, auditoría, mensajes,
  invitaciones, webhooks) y el MCP `argus-mcp` (docs/MCP.md).
- Backend Rust (`src-tauri/src`): biblioteca `argus_lib` compartida por la app y
  por el binario `argus-mcp`; cliente de Discord con twilight (`discord.rs`),
  comandos Tauri en `commands.rs`, SQLite con sqlx (`db.rs`), cifrado AES-GCM
  (`crypto.rs`). Qué hace cada archivo: ARCHITECTURE_MAP.md.
- Frontend (`src/`): React + Zustand; vistas Panel, Canales, Miembros, Roles,
  Servidor, Logs y Ajustes; temas en `src/styles/temas/`.
- Pendiente: la verificación manual de la FASE 4 en el servidor de pruebas y
  probar `argus-mcp` con un cliente MCP real; Windows solo está probado en CI.

## Después del MVP (visión, sin criterios todavía)

Cada bloque se convierte en tareas con `criterio:` cuando le toque.

1. ~~**Edición completa del servidor**~~: hecho en la FASE 7.
2. **Plantillas**: guardar la estructura de un servidor y aplicarla a otro
   (roles → categorías → canales → overwrites), con vista previa y validación de
   permisos del bot.
3. **Personalidad del bot**: perfiles (nombre, prompt de sistema, modelo, canales
   permitidos, reglas), respuesta a menciones/slash, memoria corta por canal,
   anti-spam, modo observador. Proveedor LLM configurable (Ollama local u otro
   compatible con la API de OpenAI); proveedores y costes se verifican al empezar.
4. **Vigilancia y logs**: eventos (altas/bajas, bans, cambios de rol/canal,
   mensajes borrados) con filtros y timeline; alertas nativas.
5. **Integraciones**: notificaciones Telegram, backup automático, agentes CLI
   (opencode/codex/claude). El MCP propio ya existe (FASE 8).
6. **UX**: i18n ES/EN, paleta de comandos, listas virtualizadas (los temas ya
   están, FASES 5–6).

## Principios

- **Privacidad**: tokens y claves cifrados localmente; nunca en texto plano ni en
  `localStorage`; sin telemetría.
- **Nada hardcodeado** de un servidor, canal, bot o usuario.
- **Observabilidad**: estado en BD/archivos, errores visibles en la UI, en el idioma elegido.
- **Rendimiento**: Rust en lo caliente; nada de bloquear el runtime (`block_on`
  dentro de tokio) ni retener un `MutexGuard` a través de un `.await`.

## Seguridad

- Tokens cifrados con AES-GCM; clave derivada del `machine-id` + sal en config.
- Validar permisos del bot antes de una acción y mostrar un error claro si falta.
- Las acciones destructivas (borrar canal/rol, ban) piden confirmación en la UI.

## Convenciones

- UI en inglés por defecto con español opcional (i18n, FASE 9); documentación
  interna en español; commits convencionales (`feat:`, `fix:`…).
- Una tarea por commit, verificable por el gate: `cargo fmt/check/test` (en
  `src-tauri/`), `tsc` y `npm run build`.
- Rutas de datos: `~/.config/argus/` (BD, logs, backups); en Windows `%APPDATA%\argus\`.
- Fechas: ISO 8601 UTC en BD, formato local en la UI.

## Integración con Dédalo

`plan.md` usa el formato de Dédalo (`- [ ]` / `- [x]` / `- [B]`, una línea
`criterio:` por tarea). Las tareas manuales van como viñetas simples, sin casilla,
para que el loop no las tome.
