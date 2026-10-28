# MCP de ARGUS

`argus-mcp` es un servidor [MCP](https://modelcontextprotocol.io) (transporte stdio)
que deja a Claude Code, Dédalo o cualquier otro cliente MCP gestionar Discord con
los bots que has registrado en ARGUS.

> ⚠️ **Advertencia.** El MCP puede hacer **todo lo que permitan los permisos del
> bot activo**: borrar canales y roles, expulsar o banear miembros, enviar
> mensajes, crear webhooks e invitaciones… Los únicos límites son los permisos de
> Discord y la jerarquía de roles del bot. Conéctalo solo a clientes de confianza
> y, mejor, pruébalo primero en un servidor de pruebas.

## Cómo funciona

- Es un ejecutable aparte que **funciona con la app cerrada**.
- Lee la misma base de datos que la app (`~/.config/argus/argus.db`, o la ruta de
  la variable de entorno `ARGUS_DB`) y descifra el token del bot activo igual que
  ARGUS. El token nunca se escribe en stdout, stderr ni en las respuestas.
- Usa la misma lógica que la app: mismos chequeos de permisos y jerarquía y mismos
  mensajes de error en español.
- Sin bot activo, cada herramienta responde «No hay bot activo en ARGUS: añade o
  elige uno en la app» sin caerse.

## Instalación

1. Instala ARGUS y añade al menos un bot (Ajustes → Bots). El binario `argus-mcp`
   va empaquetado junto al ejecutable de la app.
2. Abre **Ajustes → MCP**: ahí ves la ruta exacta de `argus-mcp` y los bloques
   listos para copiar con el botón **Copiar**.
3. Regístralo en tu cliente:

**Claude Code**

```bash
claude mcp add argus -- /ruta/a/argus-mcp
```

**Otros clientes** (JSON de configuración MCP):

```json
{
  "mcpServers": {
    "argus": {
      "command": "/ruta/a/argus-mcp",
      "args": []
    }
  }
}
```

Si ejecutas ARGUS desde el código fuente, compila el binario con
`cargo build --release --bin argus-mcp --manifest-path src-tauri/Cargo.toml`
(queda en `src-tauri/target/release/`). Para empaquetar la app con el MCP incluido,
ejecuta antes `npm run build:mcp`, que lo copia a `src-tauri/binaries/` (la
configuración `bundle.externalBin` de Tauri lo añade al paquete).

## Servidor activo

Casi todas las herramientas aceptan un `guild_id` **opcional**. Si lo omites, usan
el **servidor activo**: el que eliges en la app o con `elegir_servidor`, que
guarda el cambio en la base de datos (la app lo verá). Si no hay servidor activo y
no pasas `guild_id`, la herramienta devuelve un error claro.

Los permisos (`permisos`, `permitir`, `denegar`) se aceptan como texto de bits
(`"8192"`) o como lista de nombres (`["ManageMessages","ViewChannel"]`).

## Herramientas

En las tablas, `guild_id?` es el ID de servidor opcional descrito arriba. Un `?`
al final marca un parámetro opcional.

### Estado y servidores

| Herramienta | Parámetros | Descripción |
|---|---|---|
| `estado` | — | Bot activo (nombre oficial) y servidor activo. |
| `listar_servidores` | — | Servidores donde está el bot activo. |
| `elegir_servidor` | `guild_id` | Cambia el servidor activo. |

### Lectura

| Herramienta | Parámetros | Descripción |
|---|---|---|
| `mapa_servidor` | `guild_id?` | Mapa del servidor: categorías, canales y roles. |
| `listar_canales` | `guild_id?` | Canales con tipo y categoría. |
| `listar_roles` | `guild_id?` | Roles con color, posición y permisos. |
| `listar_miembros` | `guild_id?`, `buscar?`, `limite?` (25 por defecto, máx. 100) | Miembros, con búsqueda por usuario, apodo o ID. |
| `info_miembro` | `guild_id?`, `user_id` | Detalle de un miembro y sus roles. |
| `permisos_bot` | `guild_id?` | Permisos efectivos del bot en el servidor. |
| `info_servidor` | `guild_id?` | Datos generales del servidor. |
| `ajustes_servidor` | `guild_id?` | Ajustes del servidor (verificación, filtros, AFK…). |
| `registro_auditoria` | `guild_id?` | Últimas entradas del registro de auditoría. |
| `leer_mensajes` | `guild_id?`, `channel_id`, `limite?` (por defecto y máx. 50) | Últimos mensajes de un canal. |
| `listar_baneos` | `guild_id?` | Usuarios baneados. |
| `listar_invitaciones` | `guild_id?` | Invitaciones activas. |
| `listar_webhooks` | `guild_id?` | Webhooks, sin exponer su token. |

### Mensajes

| Herramienta | Parámetros | Descripción |
|---|---|---|
| `enviar_mensaje` | `channel_id`, `contenido` (≤ 2000) | Envía un mensaje a un canal. |
| `enviar_dm` | `user_id`, `contenido` (≤ 2000) | Mensaje directo a un usuario. |
| `borrar_mensaje` | `guild_id?`, `channel_id`, `message_id` | Borra un mensaje. |
| `borrar_mensajes` | `guild_id?`, `channel_id`, `message_ids` (2–100, de menos de 14 días) | Borrado en bloque. |
| `fijar_mensaje` | `guild_id?`, `channel_id`, `message_id` | Fija un mensaje. |
| `desfijar_mensaje` | `guild_id?`, `channel_id`, `message_id` | Quita un mensaje fijado. |

### Canales

| Herramienta | Parámetros | Descripción |
|---|---|---|
| `crear_canal` | `guild_id?`, `nombre`, `tipo?` (texto, voz, categoria, anuncios, foro), `categoria_id?`, `tema?`, `nsfw?`, `slowmode_segundos?` (0–21600) | Crea un canal. |
| `editar_canal` | `guild_id?`, `channel_id`, `nombre?`, `categoria_id?` (vacío = sin categoría), `tema?`, `nsfw?`, `slowmode_segundos?` | Edita un canal. |
| `borrar_canal` | `guild_id?`, `channel_id` | Borra un canal. |
| `ordenar_canales` | `guild_id?`, `posiciones` (lista de `{id, posicion}`) | Reordena canales. |
| `permiso_canal` | `guild_id?`, `channel_id`, `target_id`, `tipo` (rol o miembro), `permitir?`, `denegar?` | Fija permisos de un rol o miembro en un canal. |
| `quitar_permiso_canal` | `guild_id?`, `channel_id`, `target_id`, `tipo` | Quita ese overwrite. |

### Roles

| Herramienta | Parámetros | Descripción |
|---|---|---|
| `crear_rol` | `guild_id?`, `nombre`, `color?` (RGB numérico), `separado?`, `mencionable?`, `permisos?` | Crea un rol. |
| `editar_rol` | `guild_id?`, `role_id`, `nombre?`, `color?`, `separado?`, `mencionable?`, `permisos?` (sustituyen a los actuales) | Edita un rol. |
| `borrar_rol` | `guild_id?`, `role_id` | Borra un rol. |
| `ordenar_roles` | `guild_id?`, `posiciones` (lista de `{id, posicion}`) | Reordena roles. |
| `dar_rol` | `guild_id?`, `user_id`, `role_id` | Da un rol a un miembro. |
| `quitar_rol` | `guild_id?`, `user_id`, `role_id` | Quita un rol a un miembro. |

### Moderación

El `motivo` queda en el registro de auditoría de Discord. No se puede actuar sobre
el dueño del servidor ni sobre miembros con un rol igual o superior al del bot.

| Herramienta | Parámetros | Descripción |
|---|---|---|
| `expulsar` | `guild_id?`, `user_id`, `motivo?` | Expulsa a un miembro. |
| `banear` | `guild_id?`, `user_id`, `motivo?`, `dias_mensajes?` (0–7) | Banea a un usuario. |
| `desbanear` | `guild_id?`, `user_id`, `motivo?` | Quita un baneo. |
| `silenciar` | `guild_id?`, `user_id`, `minutos` (máx. 40320; 0 quita el silencio), `motivo?` | Timeout de un miembro. |
| `cambiar_apodo` | `guild_id?`, `user_id`, `apodo?` (≤ 32; vacío lo borra), `motivo?` | Cambia el apodo. |

### Servidor, invitaciones y webhooks

| Herramienta | Parámetros | Descripción |
|---|---|---|
| `editar_servidor` | `guild_id?`, `nombre?`, `descripcion?`, `nivel_verificacion?` (0–4), `filtro_contenido_explicito?` (0–2), `notificaciones_por_defecto?` (0–1), `canal_del_sistema_id?`, `canal_afk_id?`, `afk_segundos?` (60, 300, 900, 1800, 3600) | Edita ajustes del servidor. |
| `crear_invitacion` | `guild_id?`, `channel_id`, `caducidad_segundos?` (0–604800, 86400 por defecto), `usos_maximos?` (0–100) | Crea una invitación. |
| `borrar_invitacion` | `guild_id?`, `codigo` | Revoca una invitación. |
| `crear_webhook` | `guild_id?`, `channel_id`, `nombre` (1–80) | Crea un webhook; la URL completa solo se muestra al crearlo. |
| `borrar_webhook` | `guild_id?`, `webhook_id` | Borra un webhook. |

## Variables de entorno

| Variable | Efecto |
|---|---|
| `ARGUS_DB` | Ruta de la base de datos si no es la de por defecto (`~/.config/argus/argus.db`). |

## Problemas frecuentes

- **«No hay bot activo en ARGUS»**: abre la app, añade un bot y elígelo como activo.
- **«No hay servidor activo»**: pasa `guild_id` o usa `elegir_servidor`.
- **Falta de permiso o jerarquía**: el mensaje indica qué permiso necesita el bot o
  qué rol debe subir en la lista de roles del servidor.
