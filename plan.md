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
- [x] Enviar mensajes a un canal y por DM
  criterio: `send_message` usa `POST /channels/{id}/messages`; `send_dm` abre el DM (`POST /users/@me/channels`) y envía; validan longitud (≤ 2000) antes de llamar; tests con el Discord falso
- [x] Gestionar roles de un miembro
  criterio: `add_role` / `remove_role` usan `PUT`/`DELETE /guilds/{id}/members/{user_id}/roles/{role_id}`; MembersView usa el servidor activo (no un ID fijo); tests con el Discord falso
- [x] Silenciar (timeout) a un miembro
  criterio: `timeout_member` usa `PATCH /guilds/{id}/members/{user_id}` con `communication_disabled_until` (máx. 28 días; 0 minutos quita el timeout); test con el Discord falso

Verificación manual en tu servidor de pruebas:
- Canales, roles y miembros coinciden con Discord.
- Enviar un mensaje y un DM; dar y quitar un rol; silenciar 1 minuto.
- Invitar el bot a un segundo servidor con el botón y cambiar entre ambos: cada uno muestra sus permisos y su propia configuración.

## FASE 3 — Mapa en vivo (Gateway)
Leer todo al conectar y mantenerlo al día con los eventos de Discord.

- [x] Aplicar eventos al mapa como función pura
  criterio: en src-tauri/src/mapa.rs, una función aplica un evento de `twilight_model` (GUILD_UPDATE, CHANNEL_CREATE/UPDATE/DELETE, ROLE_*, GUILD_MEMBER_ADD/UPDATE/REMOVE, THREAD_*) a un `ServerMap` y devuelve un resumen del cambio; tests unitarios por tipo de evento, sin red
- [x] Conectar el Gateway mientras el bot está activo
  criterio: `start_bot` lanza una tarea tokio con `twilight-gateway` (intents: GUILDS, GUILD_MEMBERS) que aplica cada evento al mapa con la función anterior; `stop_bot` la detiene; ningún `MutexGuard` cruza un `.await`; cargo test pasa
- [x] Sincronización completa al conectar
  criterio: al conectar se carga guild + canales + roles + miembros del servidor activo por REST y se emite el evento Tauri `sync-progress` (paso actual y total); test de la función de sync con el Discord falso
- [x] La UI se actualiza sin polling
  criterio: el backend emite `map-updated` tras aplicar eventos (agrupados con debounce de ~500 ms); App.tsx escucha `map-updated` y `sync-progress` y deja de consultar el estado cada 5 s; tsc y npm run build pasan
- [x] Guardar el mapa y el historial de cambios
  criterio: el mapa se guarda en `server_maps` como mucho cada 30 s si cambió, y cada cambio resumido se inserta en `incidents`; al arrancar se carga el último mapa guardado; tests de la persistencia con una BD SQLite temporal
- [x] Botón "Resincronizar mapa"
  criterio: un comando `resync_map` repite la sincronización completa y la UI tiene un botón en Dashboard que lo llama y muestra el progreso; tsc pasa

Verificación manual en tu servidor de pruebas:
- Crear y borrar un canal y un rol en Discord y ver el cambio en ARGUS sin recargar.
- Desconectar la red 1 minuto y comprobar que se reconecta solo.

## FASE 4 — Prueba real (cierre del MVP)
Comprobar con Discord de verdad, en tus servidores de prueba y con otro usuario, que todo lo anterior funciona.

- [x] Tests de integración contra un servidor real, desactivados por defecto
  criterio: src-tauri/tests/integracion_discord.rs con tests `#[ignore]` que leen ARGUS_TEST_TOKEN, ARGUS_TEST_GUILD, ARGUS_TEST_CHANNEL, ARGUS_TEST_ROLE y ARGUS_TEST_USER del entorno y, si falta alguna, terminan sin hacer nada; usan las mismas funciones que los comandos para: validar el token, comprobar que ARGUS_TEST_GUILD está entre los servidores del bot, leer canales/roles/miembros, calcular los permisos del bot, enviar un mensaje a ARGUS_TEST_CHANNEL, enviar un DM a ARGUS_TEST_USER, dar y quitar ARGUS_TEST_ROLE a ARGUS_TEST_USER y aplicar y retirar un timeout de 1 minuto; ningún valor real queda en el repo; `cargo test` normal no los ejecuta; docs/PRUEBA_REAL.md explica cómo preparar el servidor y correrlos con `cargo test -- --ignored`

Prueba final manual (el dueño, con la app compilada desde el acceso directo):
- Correr los tests de integración contra el servidor de pruebas (docs/PRUEBA_REAL.md).
- Registrar dos bots y cambiar entre ellos; con cada uno, cambiar entre dos servidores.
- En cada servidor: canales, roles y miembros correctos; permisos del bot correctos; configuración propia que se conserva al volver.
- Con otro usuario de Discord: recibir el DM, recibir y perder el rol, timeout de 1 minuto.
- Cambios hechos en Discord (canal, rol, miembro que entra o sale) aparecen en ARGUS sin recargar.
- Quitar el bot de un servidor: desaparece de la lista sin errores.

## FASE 5 — Interfaz y temas
Rehacer la GUI con las skills de diseño (`ui-tokens` + `estilo-*`, en ~/.claude/skills): temas intercambiables, tema por defecto `discord`. Cada tarea sigue la skill `ui-tokens` y la del estilo indicado, y cubre los estados vacío, cargando y error.

- [x] Sistema de temas intercambiables
  criterio: los 10 `tema.css` de las skills estilo-* (minimal, discord, cyberpunk, calido, datos, anime, manga, glass, retro, brutalista) están copiados en src/styles/temas/ e importados; las fuentes se instalan con los paquetes @fontsource que indica cada skill y se importan en src/main.tsx; `Config` (Rust y TS) tiene `ui_theme` (por defecto "discord") y `ui_mode` (por defecto "") con `#[serde(default)]`; al arrancar y al cambiarlos se aplican a `document.documentElement.dataset.tema/.modo`; SettingsView tiene una sección "Apariencia" con una muestra de cada tema (fondo, acento y texto) y su variante de modo si existe; tsc, npm run build y cargo test pasan
- [x] Migrar los estilos a los tokens del contrato
  criterio: src/styles.css usa solo variables del contrato ui-tokens (desaparecen --bg-primary, --accent-cyan, etc.); fuera de src/styles/temas/ no queda ningún color literal (`#hex`, `rgb(`) en src/ — ni en CSS ni en estilos inline de los .tsx —; existe una base común (html/body, foco visible con --focus, scrollbars, `prefers-reduced-motion`); tsc y npm run build pasan
- [x] Componentes base compartidos
  criterio: src/components/ui/ contiene Button (primario/secundario/peligro/ghost, tamaños, estado cargando y deshabilitado), Input, Select, Card, Badge, Avatar (imagen o iniciales), EstadoVacio, Cargando (skeleton) y ErrorPanel (mensaje + reintentar), todos solo con tokens y con foco visible; las vistas existentes empiezan a usarlos en lugar de botones/inputs sueltos; tsc y npm run build pasan
- [x] Estructura de la ventana estilo Discord
  criterio: App muestra, siguiendo la skill estilo-discord: riel de servidores a la izquierda (iconos redondos del servidor o iniciales, squircle en hover/activo, píldora del activo, botón "+" que abre el enlace de invitación), columna de navegación con el bot activo (avatar y nombre) y las secciones, cabecera de la sección y área principal; cambiar de servidor desde el riel equivale al selector del Header (que se elimina o se integra); funciona igual con cualquier tema porque solo usa tokens; tsc y npm run build pasan
- [x] Rediseñar el panel del servidor (Dashboard)
  criterio: Dashboard usa Card/Badge/Button: cabecera con icono y nombre del servidor y estado del Gateway (en vivo / desconectado), KPIs (miembros, canales, roles) con números tabulares, lista de permisos del bot con ✓/✗ y explicación de los que faltan, botón Resincronizar con progreso y actividad reciente (cambios del mapa); estados vacío (sin bot o sin servidor, con la acción para resolverlo), cargando y error; tsc y npm run build pasan
- [x] Rediseñar Canales, Roles y Miembros
  criterio: ChannelsView muestra un árbol por categorías con icono por tipo de canal y búsqueda; RolesView lista los roles con su color, posición y resumen de permisos; MembersView lista avatar, nombre, roles como Badge y acciones (rol, DM, timeout) en un menú, desactivadas con explicación si al bot le falta el permiso; las tres con estados vacío/cargando/error y búsqueda; tsc y npm run build pasan
- [x] Rediseñar Ajustes, Logs y notificaciones
  criterio: SettingsView en secciones con título y descripción (Bots, Este servidor, Apariencia, General), formularios con etiqueta, ayuda y error por campo; LogsView con filtros por nivel y búsqueda, marca de tiempo en --text-3 y nivel como Badge; Notification como toasts apilables (éxito/aviso/error) con cierre y que desaparecen solos; tsc y npm run build pasan

Revisión manual (el dueño):
- Abrir la app y recorrer todas las vistas con cada tema; nada ilegible ni roto en ninguno.
- Cambiar de tema en Apariencia y reiniciar: el tema elegido se conserva.

## FASE 6 — Pulido visual con las skills de diseño
Cada tarea nombra las skills que el agente debe cargar y seguir (Dédalo las carga y el revisor comprueba sus reglas). Tema por defecto: discord; todo debe verse bien con los 10 temas.

- [x] Limpiar el CSS según la skill ui-tokens
  criterio: siguiendo la skill ui-tokens, en src/ (fuera de src/styles/temas/) no queda ningún color, sombra ni radio literal (ni `rgba(`/`#hex` en Vistas.css, ui.css u otros) y solo se usan variables del contrato (desaparecen las que no existen, como --radius-md); los estados vacío, cargando y error de todas las vistas usan EstadoVacio, Cargando y ErrorPanel; tsc y npm run build pasan
- [x] Rasgos propios de los estilos minimal, discord, cyberpunk, calido y datos
  criterio: usando las skills estilo-minimal, estilo-discord, estilo-cyberpunk, estilo-calido y estilo-datos, existe src/styles/temas/rasgos.css con reglas acotadas a cada `[data-tema="…"]` que aplican los rasgos característicos que describe cada skill (p. ej. cyberpunk: títulos en mayúsculas con espaciado y esquinas cortadas en tarjetas clave; datos: tablas compactas con números tabulares; calido: controles grandes y botones con icono y texto; discord: squircle y píldora del riel); los componentes no cambian de API; tsc y npm run build pasan
- [x] Rasgos propios de los estilos anime, manga, glass, retro y brutalista
  criterio: usando las skills estilo-anime, estilo-manga, estilo-glass, estilo-retro y estilo-brutalista, rasgos.css añade sus rasgos acotados a cada `[data-tema="…"]` (anime: bordes de tinta y sombras planas que se hunden al pulsar; manga: tramas de puntos en cabeceras y bocadillos en tooltips; glass: fondo `--fondo-app` y paneles de vidrio con respaldo sólido; retro: bordes de bloque, `image-rendering: pixelated` y animaciones a pasos; brutalista: bordes negros gruesos y sombras duras que se desplazan); con `prefers-reduced-motion` no hay animaciones decorativas; tsc y npm run build pasan
- [x] Vista previa real de cada tema en Apariencia
  criterio: siguiendo la skill ui-tokens, la sección Apariencia de SettingsView muestra para cada tema una miniatura que es un trozo de UI real (barra lateral, tarjeta, botón primario y un badge) envuelta en un contenedor con `data-tema` (y `data-modo` si aplica) de ese tema, de modo que se ve con sus propios colores, fuentes y rasgos; el tema activo está marcado y se puede elegir con teclado; tsc y npm run build pasan
- [x] Pulir la ventana principal con estilo-discord
  criterio: usando las skills estilo-discord y diseno-interfaces, el riel de servidores, la columna de navegación (bot activo con avatar y punto de estado del Gateway, secciones con icono) y la cabecera siguen las reglas de layout y componentes de estilo-discord; jerarquía tipográfica de como máximo 3 tamaños por vista; tooltips con el nombre completo de cada servidor en el riel; tsc y npm run build pasan
- [x] Pulir el panel del servidor y las vistas de datos
  criterio: usando la skill diseno-interfaces, Dashboard, ChannelsView, RolesView y MembersView tienen jerarquía clara (título de vista, acciones arriba a la derecha, contenido en tarjetas o tablas), números tabulares alineados, textos de estados vacíos que dicen qué hacer, y búsqueda con atajo `/`; ninguna vista desborda en horizontal a 900 px de ancho (minWidth de la ventana); tsc y npm run build pasan
- [x] Accesibilidad y teclado
  criterio: siguiendo las skills diseno-interfaces y ui-tokens, todos los controles son alcanzables con Tab en orden lógico, tienen foco visible con --focus y `aria-label` cuando solo muestran un icono; los menús y diálogos se cierran con Escape y devuelven el foco; los toasts usan `role="status"` (o `alert` para errores); tsc y npm run build pasan

Revisión manual (el dueño):
- Recorrer todas las vistas con cada uno de los 10 temas (y sus modos): cada uno se reconoce por su estilo y nada es ilegible.
- Usar la app solo con teclado durante un minuto.

## FASE 7 — Control total del servidor
Si el bot tiene Administrador (o el permiso concreto), ARGUS puede gestionar todo el servidor desde la interfaz. Discord mantiene dos límites que ni un administrador salta y la app los explica: no se toca un rol igual o superior al rol más alto del bot, ni se expulsa/banea/silencia al dueño. Backend: cada tarea con tests contra el Discord falso (wiremock); toda acción comprueba antes el permiso con el cálculo de `bot_permissions` (Administrador lo concede todo) y, tras cambiar algo, actualiza el mapa (`set_map` + evento `map-updated`) para que se vea sin el Gateway.

- [x] Backend: canales y categorías
  criterio: comandos `create_channel` (nombre, tipo texto/voz/categoría/anuncios/foro, categoría padre, tema, NSFW, slowmode), `update_channel` (mismos campos y mover de categoría), `delete_channel` y `reorder_channels` (posiciones) con twilight; sin «Gestionar canales» devuelven «Al bot le falta el permiso «Gestionar canales»» sin llamar a Discord; tras cada cambio el mapa del servidor se actualiza; tests con el Discord falso de cada comando y del caso sin permiso
- [x] Backend: roles y jerarquía
  criterio: comandos `create_role` (nombre, color, separado, mencionable, permisos como texto de bits), `update_role`, `delete_role` y `reorder_roles`; una función compartida comprueba la jerarquía: tocar un rol con posición igual o mayor que el rol más alto del bot devuelve «Ese rol está por encima del rol del bot: súbelo en Ajustes del servidor → Roles» (el dueño del servidor no aplica: es el bot quien actúa); tests con el Discord falso, incluidos jerarquía y falta de permiso
- [x] Backend: permisos de canal
  criterio: comandos `set_channel_permission` (rol o miembro, bits permitidos y denegados) y `delete_channel_permission` con twilight; exigen «Gestionar roles» (o Administrador) y respetan la jerarquía para overwrites de rol; el mapa del canal refleja los overwrites nuevos; tests con el Discord falso
- [x] Backend: moderación de miembros
  criterio: comandos `kick_member` (motivo), `ban_member` (motivo y días de mensajes a borrar 0–7), `unban_member`, `list_bans` y `set_nickname`; rechazan al dueño del servidor y a miembros cuyo rol más alto es igual o superior al del bot con un mensaje claro; el motivo va en el registro de auditoría (cabecera `X-Audit-Log-Reason` de twilight); tests con el Discord falso, incluidos dueño y jerarquía
- [x] Backend: ajustes del servidor y registro de auditoría
  criterio: comandos `get_guild_settings_discord` y `update_guild_discord` (nombre, descripción, nivel de verificación, filtro de contenido, notificaciones por defecto, canal del sistema, canal y tiempo AFK) que exigen «Gestionar servidor»; `audit_log` devuelve las últimas 50 entradas con acción, autor, objetivo, motivo y fecha legibles en español; tests con el Discord falso
- [x] Backend: mensajes, invitaciones y webhooks
  criterio: comandos `channel_messages` (últimos 50, con autor y fecha), `delete_message`, `bulk_delete_messages` (2–100 y solo de menos de 14 días; los más antiguos se rechazan con explicación), `pin_message` / `unpin_message`, `create_invite` (caducidad y usos máximos) / `list_invites` / `delete_invite` y `list_webhooks` / `create_webhook` / `delete_webhook`, cada uno exigiendo su permiso; tests con el Discord falso
- [x] Interfaz: diálogo de confirmación y gestión de canales
  criterio: siguiendo las skills ui-tokens, estilo-discord y diseno-interfaces, existe un componente Dialogo reutilizable en src/components/ui (foco atrapado, Escape cierra, devuelve el foco) y una variante de confirmación peligrosa que pide escribir el nombre del elemento para borrarlo; ChannelsView permite crear (botón en la cabecera y en cada categoría), editar (nombre, tema, categoría, slowmode) y borrar canales y categorías; cada botón se desactiva con la explicación si falta el permiso; tras la acción la lista se actualiza y sale un toast; tsc y npm run build pasan
- [x] Interfaz: gestión de roles
  criterio: siguiendo las skills ui-tokens, estilo-discord y diseno-interfaces, RolesView permite crear y editar roles en un diálogo con nombre, selector de color, «mostrar por separado», «mencionable» y permisos agrupados (General, Miembros, Texto, Voz, Avanzado) con interruptores y un atajo «Administrador»; borrar con confirmación por nombre; subir/bajar posición; los roles iguales o superiores al del bot se ven bloqueados con la explicación de la jerarquía; tsc y npm run build pasan
- [x] Interfaz: permisos de canal
  criterio: siguiendo las skills ui-tokens y diseno-interfaces, al abrir un canal se ve su panel de permisos: lista de overwrites (roles y miembros) y, para cada uno, los permisos en tres estados (permitir / heredar / denegar); se pueden añadir roles o miembros y quitar overwrites; los cambios se guardan con un botón y se descartan con otro; tsc y npm run build pasan
- [x] Interfaz: moderación de miembros
  criterio: siguiendo las skills ui-tokens y diseno-interfaces, el menú de cada miembro en MembersView añade cambiar apodo, expulsar y banear (diálogo con motivo y días de mensajes a borrar, confirmación); el dueño y los miembros por encima del bot muestran esas acciones desactivadas con la explicación; una pestaña «Baneados» lista los baneos con motivo y permite desbanear; tsc y npm run build pasan
- [x] Interfaz: ajustes del servidor y auditoría
  criterio: siguiendo las skills ui-tokens, estilo-discord y diseno-interfaces, una nueva sección «Servidor» en la navegación permite editar nombre, descripción, nivel de verificación, filtro de contenido, notificaciones y canales del sistema/AFK (desplegables con los canales reales), con Guardar/Descartar; debajo, el registro de auditoría con filtro por tipo de acción; tsc y npm run build pasan
- [x] Interfaz: mensajes, invitaciones y webhooks
  criterio: siguiendo las skills ui-tokens, estilo-discord y diseno-interfaces, al abrir un canal de texto se ven sus últimos mensajes (autor, fecha, contenido, adjuntos como enlace) con acciones borrar, fijar/desfijar y selección múltiple para borrado en bloque (avisando del límite de 14 días); el canal tiene además pestañas «Invitaciones» (crear con caducidad y usos, copiar enlace, borrar) y «Webhooks» (crear, copiar URL, borrar); tsc y npm run build pasan

Verificación manual en tu servidor de pruebas (con el bot como Administrador):
- Crear, renombrar, mover y borrar un canal y una categoría.
- Crear un rol con color y permisos, asignarlo y borrarlo; comprobar que un rol por encima del bot aparece bloqueado.
- Cambiar los permisos de un canal para un rol y ver el efecto en Discord.
- Con una cuenta secundaria: apodo, expulsar, banear y desbanear.
- Editar el nombre y la verificación del servidor; ver las acciones en el registro de auditoría.
- Borrar un mensaje, borrar varios en bloque, fijar uno, crear una invitación y un webhook.

## FASE 8 — MCP de ARGUS
Un ejecutable aparte, `argus-mcp` (MCP por stdio), para que cualquier cliente MCP (Claude Code, Dédalo, etc.) gestione Discord con los bots registrados en ARGUS. Funciona con la app cerrada: lee la misma base de datos (`~/.config/argus/argus.db`, o la ruta de la variable `ARGUS_DB`), descifra el token del bot activo igual que la app y usa el servidor activo cuando una herramienta no recibe `guild_id`. Expone todo lo que hace la app; los únicos límites son los permisos de Discord y la jerarquía del bot, con los mismos mensajes de error que en la app. Reutiliza la lógica de `commands.rs` y `discord.rs` (nada duplicado). Tests sin red: Discord falso (wiremock) y una base de datos temporal; nunca un token real.

- [x] Separar la lógica en una biblioteca compartida
  criterio: src-tauri pasa a tener `src/lib.rs` con los módulos (commands, crypto, db, discord, mapa, mensajes, persistencia, state, sync, gateway y los tests) y `main.rs` solo arranca la app usando la biblioteca; las funciones internas que hacen el trabajo de cada comando (p. ej. `crear_canal`, `enviar_a_canal`, `leer_auditoria`) son accesibles desde la biblioteca con un `&AppState` y la URL base opcional, sin depender de `AppHandle`/`State` de Tauri (los avisos al mapa/frontend quedan en los envoltorios `#[tauri::command]`); `db.rs` expone una función para abrir la base de datos por ruta que respeta `ARGUS_DB`; comportamiento idéntico: los tests de cargo pasan sin cambiar lo que comprueban, tsc y npm run build pasan
- [x] Binario `argus-mcp`: arranque, estado y servidores
  criterio: nuevo binario `src/bin/argus-mcp.rs` con el crate `rmcp` (SDK oficial de MCP en Rust, transporte stdio); al arrancar abre la base de datos, carga el bot activo y su token descifrado (sin bot activo, cada herramienta responde «No hay bot activo en ARGUS: añade o elige uno en la app» sin caerse) y nunca escribe el token en stdout, stderr ni en respuestas; herramientas `estado` (bot activo con nombre oficial, servidor activo), `listar_servidores` y `elegir_servidor` (cambia el servidor activo en la base de datos, igual que la app); cada herramienta tiene descripción en español y esquema de parámetros; tests: llamadas a las herramientas en proceso contra el Discord falso y una BD temporal, y una prueba de humo que lanza el binario, hace `initialize` + `tools/list` por stdio y comprueba que aparecen las tres herramientas; cargo test pasa
- [x] MCP: herramientas de lectura
  criterio: `argus-mcp` añade herramientas de solo lectura que reutilizan la lógica existente: `mapa_servidor`, `listar_canales`, `listar_roles`, `listar_miembros` (con búsqueda y límite), `info_miembro`, `permisos_bot`, `info_servidor`, `ajustes_servidor`, `registro_auditoria`, `leer_mensajes` (canal, últimos N ≤ 50), `listar_baneos`, `listar_invitaciones` y `listar_webhooks` (sin exponer el token del webhook: la URL completa solo se da al crearlo); `guild_id` es opcional en todas (por defecto el servidor activo) y las respuestas son JSON compacto con nombres además de IDs; tests con el Discord falso de cada herramienta y del caso sin servidor activo; cargo test pasa
- [x] MCP: herramientas de mensajes, canales y roles
  criterio: `argus-mcp` añade `enviar_mensaje`, `enviar_dm`, `borrar_mensaje`, `borrar_mensajes` (2–100, menos de 14 días), `fijar_mensaje`, `desfijar_mensaje`, `crear_canal`, `editar_canal`, `borrar_canal`, `ordenar_canales`, `crear_rol`, `editar_rol`, `borrar_rol`, `ordenar_roles`, `dar_rol`, `quitar_rol`, `permiso_canal` y `quitar_permiso_canal`, reutilizando la lógica de la app (mismos chequeos de permiso y jerarquía y mismos mensajes de error); los permisos se aceptan como texto de bits o como lista de nombres legibles (p. ej. `["ManageMessages","ViewChannel"]`); tests con el Discord falso de cada herramienta, incluidos falta de permiso y jerarquía; cargo test pasa
- [x] MCP: herramientas de moderación, servidor, invitaciones y webhooks
  criterio: `argus-mcp` añade `expulsar`, `banear` (motivo y días 0–7), `desbanear`, `silenciar` (timeout con duración) , `cambiar_apodo`, `editar_servidor` (mismos campos que la app), `crear_invitacion`, `borrar_invitacion`, `crear_webhook` y `borrar_webhook`, reutilizando la lógica de la app (dueño y jerarquía rechazados con el mismo mensaje; el motivo va al registro de auditoría); tests con el Discord falso de cada herramienta, incluidos dueño, jerarquía y falta de permiso; cargo test pasa
- [x] Integración del MCP en la app y documentación
  criterio: siguiendo las skills ui-tokens y diseno-interfaces, SettingsView tiene una sección «MCP» que muestra la ruta del binario `argus-mcp` y un bloque copiable con el comando para registrarlo en Claude Code (`claude mcp add argus -- <ruta>`) y el JSON equivalente para otros clientes, con botón Copiar y toast; la configuración de Tauri incluye `argus-mcp` en el paquete de la app; existe `docs/MCP.md` en español con instalación, lista de herramientas con sus parámetros, el uso del servidor activo y la advertencia de que el MCP puede hacer todo lo que permitan los permisos del bot; tsc, npm run build y cargo test pasan

Verificación manual en tu servidor de pruebas:
- Registrar `argus-mcp` en Claude Code y pedirle el estado y el mapa del servidor.
- Pedirle que cree un canal y un rol, envíe un mensaje y luego los borre; comprobarlo en Discord y en el registro de auditoría.
- Con la app abierta, ver que los cambios hechos por el MCP aparecen en el mapa en vivo.

## FASE 9 — Idiomas: inglés por defecto, español opcional
ARGUS se publica para cualquiera: la interfaz arranca en **inglés** y el usuario puede cambiarla a **español** en Ajustes. Mismo enfoque que Dédalo: el texto fuente del código está en inglés y una función `t()` lo traduce con un catálogo español; si falta una entrada se muestra el inglés. Solo se traduce la interfaz de ARGUS. **No se traduce nada que venga de Discord**: nombres de servidores, canales, categorías, roles y miembros, ni los términos propios de Discord (nombres de los permisos como «Manage Channels» y tipos de acción del registro de auditoría), que se muestran siempre con su nombre oficial en inglés en los dos idiomas (una sola fuente, p. ej. `src/permisos.ts`); sí se traducen las ayudas y explicaciones que ARGUS da sobre ellos. Tampoco los identificadores guardados.

- [x] Base de idiomas y selector
  criterio: existe `src/i18n.ts` con `t(texto, vars?)` (texto fuente en inglés, placeholders `{nombre}`) y el catálogo español en `src/i18n/es.ts` (objeto inglés → español); `Config` (state.rs, `src/types`) tiene `ui_language` con valor por defecto `"en"` (con `#[serde(default)]`, una config guardada sin el campo carga como `"en"`, test de Rust que lo comprueba); el store expone el idioma y al cambiarlo la UI se vuelve a pintar al instante y se guarda con `save_config`; `document.documentElement.lang` sigue al idioma; Ajustes → Apariencia tiene un selector «Language / Idioma» con English y Español; existe `scripts/comprobar-i18n.mjs` y el script `"i18n:check"` en package.json, que falla si algún literal pasado a `t('…')`/`t("…")` en `src/` no está en el catálogo español; `npm run i18n:check` pasa

- [x] Traducir la estructura de la ventana
  criterio: usando `t()`, quedan en inglés por defecto y traducidos al español los textos de App.tsx (secciones), Sidebar, GuildRail, Header, Notification, EstadoVista, `src/components/ui/` (incluidos Dialogo y ConfirmarPeligroso), `src/carga.ts` y la pantalla de carga de index.html (en index.html el texto inicial pasa a inglés, «Loading…», y carga.ts lo traduce); `npm run i18n:check` y `npm run build` pasan

- [x] Traducir el Panel y los Logs
  criterio: Dashboard.tsx (incluidas las explicaciones de cada permiso del bot) y LogsView.tsx usan `t()` en todos sus textos visibles, tooltips y mensajes; en inglés por defecto y en español con el selector; `npm run i18n:check` y `npm run build` pasan

- [x] Traducir Canales
  criterio: ChannelsView.tsx, PermisosCanal.tsx y ActividadCanal.tsx usan `t()` en textos visibles, placeholders, tooltips, diálogos de confirmación, notificaciones y en lo que escriben en Logs (los prefijos como `[CANALES]` pasan a inglés, p. ej. `[CHANNELS]`); los nombres de los permisos se muestran con su nombre oficial de Discord en inglés sin traducir; `npm run i18n:check` y `npm run build` pasan

- [x] Traducir Roles y Miembros
  criterio: RolesView.tsx y MembersView.tsx usan `t()` en textos visibles, los títulos de los grupos de permisos (los permisos en sí con su nombre oficial de Discord, sin traducir), placeholders, tooltips (incluido el motivo de una acción desactivada), diálogos y notificaciones; `npm run i18n:check` y `npm run build` pasan

- [x] Traducir Servidor y Ajustes
  criterio: ServidorView.tsx (ajustes del servidor y registro de auditoría; los tipos de acción con su nombre oficial de Discord, sin traducir) y SettingsView.tsx (bots, este servidor, MCP, apariencia, general, nombres de los temas) usan `t()`; `npm run i18n:check` y `npm run build` pasan

- [ ] Mensajes del backend en el idioma elegido
  criterio: los mensajes de error y de estado que Rust devuelve a la UI (commands.rs, discord.rs, mensajes.rs, sync.rs, gateway.rs, db.rs) tienen el texto fuente en inglés y un módulo `src-tauri/src/i18n.rs` con el catálogo español y una función que traduce según `ui_language`; los comandos devuelven el mensaje en el idioma de la config; tests de Rust: el mismo error (p. ej. token inválido y falta de permiso contra el Discord falso) sale en inglés por defecto y en español con `ui_language = "es"`; un texto sin entrada en el catálogo sale en inglés; `cargo test` pasa

- [ ] MCP en el idioma elegido
  criterio: las descripciones de las herramientas de `argus-mcp` quedan en inglés; los mensajes de sus respuestas y errores (incluido «No hay bot activo en ARGUS…») usan el mismo `i18n.rs` y salen en el idioma guardado en la config de la BD (inglés por defecto); un test de `mcp.rs` lo comprueba en los dos idiomas; tests/mcp_humo.rs sigue pasando; docs/MCP.md en inglés con los mensajes de ejemplo en inglés

- [ ] Sin textos sueltos y documentación
  criterio: `scripts/comprobar-i18n.mjs` también falla si queda en `src/**/*.tsx`/`*.ts` (fuera de `src/i18n/`) un literal visible en español (con á, é, í, ó, ú, ñ, ¿ o ¡) y pasa sin excepciones; README.md queda en inglés (mismo contenido y capturas) con un enlace «Español» a un README.es.md nuevo con la traducción, y README.es.md enlaza de vuelta «English»; AGENTS.md y CONTEXTO.md dicen «UI en inglés por defecto, español con i18n (`t()` y catálogo)» en lugar de «Español en la UI»; `npm run i18n:check`, `npm run build` y `cargo test` pasan

Verificación manual:
- Abrir ARGUS: todo en inglés. Ajustes → Language → Español: todo cambia al instante; cerrar y abrir: sigue en español.
- Provocar un error (token inválido) en los dos idiomas.
- Con el MCP: pedir algo sin bot activo y ver el mensaje en el idioma elegido.

## Después del MVP (sin criterios todavía: se detallan al empezar cada bloque)
- Plantillas: esquema, vista de plantillas, vista previa en árbol, aplicador ordenado (roles → categorías → canales → overwrites), validación de permisos, crear servidor nuevo desde plantilla.
- Personalidad del bot: perfiles (prompt, modelo, canales, reglas), escuchar MESSAGE_CREATE, filtros (mención, canal, cooldown, ignorar bots), proveedor LLM configurable (Ollama / compatible OpenAI), memoria corta por canal, anti-spam, modo observador, comandos slash.
- Vigilancia y logs: niveles y timestamps, eventos con filtros, vista timeline, alertas nativas, health check del Gateway, interacción opcional con otros bots con reglas anti-bucle.
- Integraciones: Telegram, backup automático, agentes CLI (el MCP propio está en la FASE 8).
- UX y rendimiento: listas virtualizadas, paginación de miembros en la UI, métricas, revisar CPU/RAM.
