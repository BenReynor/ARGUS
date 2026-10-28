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
- [ ] Interfaz: mensajes, invitaciones y webhooks
  criterio: siguiendo las skills ui-tokens, estilo-discord y diseno-interfaces, al abrir un canal de texto se ven sus últimos mensajes (autor, fecha, contenido, adjuntos como enlace) con acciones borrar, fijar/desfijar y selección múltiple para borrado en bloque (avisando del límite de 14 días); el canal tiene además pestañas «Invitaciones» (crear con caducidad y usos, copiar enlace, borrar) y «Webhooks» (crear, copiar URL, borrar); tsc y npm run build pasan

Verificación manual en tu servidor de pruebas (con el bot como Administrador):
- Crear, renombrar, mover y borrar un canal y una categoría.
- Crear un rol con color y permisos, asignarlo y borrarlo; comprobar que un rol por encima del bot aparece bloqueado.
- Cambiar los permisos de un canal para un rol y ver el efecto en Discord.
- Con una cuenta secundaria: apodo, expulsar, banear y desbanear.
- Editar el nombre y la verificación del servidor; ver las acciones en el registro de auditoría.
- Borrar un mensaje, borrar varios en bloque, fijar uno, crear una invitación y un webhook.

## Después del MVP (sin criterios todavía: se detallan al empezar cada bloque)
- Plantillas: esquema, vista de plantillas, vista previa en árbol, aplicador ordenado (roles → categorías → canales → overwrites), validación de permisos, crear servidor nuevo desde plantilla.
- Personalidad del bot: perfiles (prompt, modelo, canales, reglas), escuchar MESSAGE_CREATE, filtros (mención, canal, cooldown, ignorar bots), proveedor LLM configurable (Ollama / compatible OpenAI), memoria corta por canal, anti-spam, modo observador, comandos slash.
- Vigilancia y logs: niveles y timestamps, eventos con filtros, vista timeline, alertas nativas, health check del Gateway, interacción opcional con otros bots con reglas anti-bucle.
- Integraciones: Telegram, backup automático, MCP propio de ARGUS, agentes CLI.
- UX y rendimiento: i18n ES/EN, listas virtualizadas, paginación de miembros en la UI, métricas, revisar CPU/RAM.
