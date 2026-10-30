# Ficha técnica — ARGUS

Especificación técnica de lo que **existe hoy**. Visión y decisiones:
[`CONTEXTO.md`](../CONTEXTO.md). Orden de trabajo: [`plan.md`](../plan.md).

---

## Stack

| Capa | Tecnología |
|---|---|
| Escritorio | Tauri 2 (CLI 2.12) |
| Frontend | React 18 + TypeScript 5 + Vite 5, estado con Zustand 4, iconos lucide-react |
| Backend | Rust (edición 2021), tokio |
| Discord | twilight-http (REST) / twilight-gateway (eventos en vivo) / twilight-model 0.15 |
| Base de datos | SQLite vía sqlx 0.8 (migraciones en `src-tauri/migrations/`) |
| Cifrado | aes-gcm 0.10 (AES-256-GCM), clave derivada de `machine-uid` |
| Plugins Tauri | store, sql, log, shell, fs, dialog, notification, global-shortcut, clipboard-manager, opener |

Pendiente de limpiar: `reqwest` y `tokio-tungstenite` siguen en `Cargo.toml`
pero el código ya no los usa (twilight los sustituyó).

## Datos (SQLite, `migrations/001–004`)

| Tabla | Contenido |
|---|---|
| `config` | Config de la app como JSON (clave → valor) |
| `bots` | Bots registrados: id, nombre, token cifrado, guilds permitidos |
| `guilds` | Caché de servidores: id, nombre, icono, miembros |
| `server_maps` | Mapa del servidor (JSON) por guild |
| `templates` | Plantillas de servidor (JSON) |
| `cli_agents` | Agentes CLI configurados (opencode/codex/claude) |
| `vigilance` | Preset y filtros de vigilancia |
| `messages` + `messages_fts` | Historial de mensajes con búsqueda FTS5 |
| `incidents` | Cambios/incidencias por guild (estilo Dédalo) |
| `settings` | Ajustes globales (clave → valor) |
| `guild_settings` | Ajustes propios de cada servidor, por bot (`004`) |

Las migraciones `002` y `003` añaden a `bots` el avatar oficial y el
`application_id` (para el enlace de invitación).

Los tipos Rust (`state.rs`, `db.rs`) y TypeScript (`src/types/index.ts`) están
sincronizados; un `Option<T>` de Rust es `T | null` en TS.

## Comandos Tauri

78 comandos en `src-tauri/src/commands.rs`, registrados en `lib.rs`. Los de
Discord llaman a la API real con twilight (`discord.rs`, `mensajes.rs`) y
comprueban los permisos y la jerarquía del bot antes de actuar; el mapa del
servidor lo mantienen el Gateway (`gateway.rs`, `sync.rs`, `mapa.rs`) y se
guarda en `server_maps` (`persistencia.rs`).
[`AUDITORIA_COMANDOS.md`](AUDITORIA_COMANDOS.md) es la auditoría inicial de la
FASE 0 (histórica).

## MCP

`argus-mcp` (`src/bin/argus-mcp.rs` + `mcp.rs`) es un servidor MCP por stdio que
usa la misma biblioteca `argus_lib` y la misma BD que la app: 44 herramientas
con los mismos chequeos de permisos. Ver [`MCP.md`](MCP.md).

## Seguridad

- Tokens cifrados con AES-GCM antes de guardarse (`crypto.rs`, con tests). Nunca
  se guardan en texto plano ni en `localStorage`.
- La config y la BD viven en `~/.config/argus/` (Windows: `%APPDATA%\argus\`).
- Nada en el código está atado a un servidor, canal o bot concreto.

## Pruebas

- `cargo test` (en `src-tauri/`): tests unitarios y de comandos (`tests_*.rs`),
  más `tests/mcp_humo.rs`, que lanza `argus-mcp` y lista sus herramientas.
- `tests/integracion_discord.rs`: contra un servidor real, desactivados por
  defecto (`cargo test -- --ignored`, ver [`PRUEBA_REAL.md`](PRUEBA_REAL.md)).
- CI (`.github/workflows/ci.yml`): frontend y `cargo test` en Linux y Windows.
- Las pruebas contra Discord usan un **Discord falso local** (`wiremock`): el
  cliente twilight se apunta a él con `ClientBuilder::proxy`. Ningún test usa red
  ni token real.
- La verificación real la hace el dueño en un servidor de pruebas, siguiendo la
  lista manual al final de cada fase de `plan.md`.

## Verificación (gate de Dédalo)

| Check | Comando |
|---|---|
| Formato Rust | `cargo fmt --check` (en `src-tauri/`) |
| Compilación Rust | `cargo check` |
| Tests Rust | `cargo test` |
| Tipos TS | `tsc --noEmit` |
| Build | `npm run build` y `cargo build` (check propio `99_custom`) |
| Criterio | Juez de Dédalo contra la línea `criterio:` de la tarea |

## Distribución

`npm run tauri build` genera los binarios (`src-tauri/target/release/argus` y
`argus-mcp`) y los paquetes en `src-tauri/target/release/bundle/`: `.deb`,
`.rpm` y `.AppImage` en Linux; `.msi` y `.exe` en Windows (el CI los sube como
artefacto `argus-windows`). Los paquetes llevan `argus-mcp` junto a la app.
Los iconos están en `src-tauri/icons/` y se generan desde `icon.svg`.
En Linux, `scripts/abrir-argus.sh` compila desde `main` y abre la app (ver README).
