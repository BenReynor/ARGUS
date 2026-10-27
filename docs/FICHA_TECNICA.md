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
| Discord | twilight-http / twilight-gateway / twilight-model 0.15 (en `Cargo.toml`; se integran en las FASES 1–3) |
| Base de datos | SQLite vía sqlx 0.8 (migraciones en `src-tauri/migrations/`) |
| Cifrado | aes-gcm 0.10 (AES-256-GCM), clave derivada de `machine-uid` |
| Plugins Tauri | store, sql, log, shell, fs, dialog, notification, global-shortcut, clipboard-manager, opener |

Dependencias heredadas que se retiran cuando twilight las sustituya:
`reqwest` y `tokio-tungstenite` (clientes Discord a mano) y el proceso externo
`npx @pasympa/discord-mcp` que lanza hoy `start_bot`.

## Datos (SQLite, `001_init.sql`)

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

Los tipos Rust (`state.rs`, `db.rs`) y TypeScript (`src/types/index.ts`) están
sincronizados; un `Option<T>` de Rust es `T | null` en TS.

## Comandos Tauri

41 comandos en `src-tauri/src/commands.rs`. Los de configuración, bots,
plantillas, vigilancia y agentes CLI trabajan contra SQLite; los de Discord son
mock o leen un mapa en memoria que nada rellena todavía. Estado de cada uno y el
endpoint que lo sustituirá: [`AUDITORIA_COMANDOS.md`](AUDITORIA_COMANDOS.md).

## Seguridad

- Tokens cifrados con AES-GCM antes de guardarse (`crypto.rs`, con tests). Nunca
  se guardan en texto plano ni en `localStorage`.
- La config y la BD viven en `~/.config/argus/`.
- Nada en el código está atado a un servidor, canal o bot concreto.

## Pruebas

- `cargo test` (en `src-tauri/`): tests unitarios en Rust, por ejemplo cifrado y
  compatibilidad de la config guardada.
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

`npm run tauri build` genera el binario (`src-tauri/target/release/argus`) y los
paquetes `.deb`, `.rpm` y `.AppImage` en `src-tauri/target/release/bundle/`.
Los iconos están en `src-tauri/icons/`; el `.icns` de macOS todavía es un PNG
renombrado.
