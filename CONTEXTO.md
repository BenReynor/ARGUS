# CONTEXTO.md — ARGUS

Fuente única de visión y decisiones. Si otro documento (README, FICHA_TECNICA,
ARCHITECTURE_MAP, AGENTS) contradice este, manda este.

## Qué es

App de escritorio (Tauri 2 + React + TypeScript + Rust) centrada en **construir
y configurar servidores de Discord con ayuda de IA**: planes, canales, roles,
permisos, propuestas que el usuario aprueba y comprobación del resultado. ARGUS
registra los bots propios del usuario y conecta la IA con los bots y servicios
externos que este autorice para configurar el servidor.

Dirección confirmada por el autor el 2026-11-19: ARGUS coordina la construcción
y la configuración; las funciones especializadas de comunidad se delegan a otros
bots mediante integraciones autorizadas. No se añaden módulos propios de
bienvenida, verificación, autoroles, tickets o niveles, ni un programador genérico
de tareas. Las capacidades ya implementadas no se eliminan por este cambio de
prioridad; los snapshots y backups automáticos existentes se conservan.

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
| Prioridad del producto | Construcción de servidores con IA y conexión autorizada con otros bots. |
| Nuevas funciones acordadas | Biblioteca de plantillas propias, auditoría de permisos, diagnóstico y verificación visual de resultados; conectores MCP para otros bots cuando existan o sean viables. |
| Servicio 24/7 | Opcional y desactivado por defecto; no reemplaza el funcionamiento de escritorio. |
| Funciones delegadas | Bienvenida, verificación, autoroles, tickets y niveles corresponden a otros bots; sin módulos propios nuevos ni programador genérico de tareas. |

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

## Asistente IA (FASE 10): la función principal

Construir servidores enormes en minutos: el usuario escribe el **plan del
servidor** (Markdown, uno por servidor) y habla con un agente por línea de
comandos (`claude`, `codex` u `opencode`, modelo a elegir) en un chat dentro de ARGUS.

- El agente solo se conecta a `argus-mcp` y **solo tiene herramientas de
  lectura**. En modo Read conversa; en modo Execute devuelve una **propuesta
  estructurada** (lista de acciones de las herramientas de escritura).
- **El usuario aprueba** la propuesta (Apply) y es **ARGUS quien la ejecuta**,
  en orden (roles → categorías → canales → permisos → roles de miembros), con
  la misma lógica y chequeos que las herramientas MCP. Lo ejecutado es
  exactamente lo aprobado.
- Plan y conversación se guardan por servidor; la sesión del agente se reanuda.
- El token del bot nunca se pasa al agente: argus-mcp lo lee de la BD.

## Actualizaciones firmadas (FASE 18)

El actualizador de Tauri comprueba al arrancar y desde Settings → About. Informa
versión y notas, y solo instala tras pulsar Update; verifica firma y versión
firmada y reinicia. Fuera de AppImage en Linux ofrece descarga manual. El CI
firma con los secretos de Actions y verifica los artefactos con la pública del
autor antes de publicar `latest.json`. Detalles en docs/FICHA_TECNICA.md.

## Siguiente etapa acordada (FASES 20–25 de plan.md)

1. **Resultados visuales y verificables:** estados claros con texto e iconos,
   progreso, motivo del fallo y una comprobación posterior cuando la operación
   lo permita. Distinguir petición aceptada de cambio realmente verificado;
   resultados parciales o no verificables nunca aparecen como éxito total.
   Historial persistente por bot, servidor y operación, visible para usuario e IA.
2. **Diagnóstico integrado:** comprobar conexión, permisos, agentes, MCP y
   almacenamiento sin modificar Discord ni consumir modelos de pago por defecto.
   Cada comprobación tiene resultado, explicación y siguiente paso.
3. **Biblioteca de plantillas creadas por el usuario:** guardar, organizar,
   duplicar, importar/exportar y aplicar planes o estructuras propias a otro
   servidor autorizado, con vista previa y aprobación; sin arrastrar tokens ni
   IDs del servidor de origen. Reutiliza el compilador y las propuestas actuales.
4. **Auditoría de permisos:** calcular accesos efectivos y comparar con la
   intención indicada por el usuario, su plan o un snapshot. Explicar hallazgos
   y proponer correcciones que se aprueban antes de aplicarse.
5. **Conectores para otros bots:** catálogo de integraciones por MCP/API oficial
   o adaptador autorizado, con sus capacidades y limitaciones reales. No asumir
   que un bot ofrece MCP ni que otro bot puede configurarlo con mensajes.
   La IA propone; las escrituras se ejecutan mediante un adaptador controlado
   después de la aprobación y se verifican cuando el servicio lo permita.
6. **Servicio sin ventana:** opción separada, desactivada por defecto y sin
   duplicar conexiones al Gateway. Requiere un equipo encendido y conectado.
   La interfaz distingue cerrar, dejar en segundo plano y detener el servicio.

Actualmente, cerrar la ventana **ya la oculta y mantiene Argus en la bandeja**;
Salir termina el proceso. Esa conducta no equivale a disponer de un servicio
independiente de la interfaz. Los avisos temporales y el progreso por acción ya
existen: la siguiente etapa los unifica, conserva y añade verificación posterior.

Las fases nuevas son trabajo pendiente; no describen funciones ya implementadas.
Las ideas anteriores de un ecosistema de módulos propios quedan reemplazadas por
la integración con otros bots. La financiación por donaciones sigue siendo una
idea independiente: nunca bloquear funciones por no donar.

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
