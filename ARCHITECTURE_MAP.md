# ARCHITECTURE_MAP.md — ARGUS

## Árbol de directorios

```
ARGUS/
├── AGENTS.md                    # Instrucciones para agentes (este archivo)
├── plan.md                      # Plan de trabajo (formato Dédalo)
├── CONTEXTO.md                  # Contexto permanente del proyecto
├── README.md                    # Documentación técnica usuario
├── docs/
│   ├── FICHA_TECNICA.md         # Especificación técnica (estilo Dédalo)
│   └── ARCHITECTURE_MAP.md      # Este archivo
├── info/                        # Credenciales (gitignored)
│   ├── CONTRASENAS.txt
│   ├── Bot.txt
│   ├── ISD_SERVER_MAP.txt
│   └── discord_oauth_url.txt
├── launch-dev.sh                # Dev con cargo env
├── abrir-opencode.sh
├── package.json
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
├── index.html
├── src/                         # Frontend React + TypeScript
│   ├── main.tsx                 # Entrada React
│   ├── App.tsx                  # Layout, tabs, polling 5s
│   ├── api/
│   │   └── tauri.ts             # Puentes invoke → comandos Rust
│   ├── components/
│   │   ├── Header.tsx           # Selector guild, estado bot, idioma
│   │   ├── Sidebar.tsx          # Navegación tabs
│   │   ├── Dashboard.tsx        # Estado general, sync status
│   │   ├── ChannelsView.tsx     # Árbol canales + búsqueda
│   │   ├── MembersView.tsx      # Lista miembros virtualizada
│   │   ├── RolesView.tsx        # Tabla roles + permisos
│   │   ├── SettingsView.tsx     # 7 secciones (IA, Vigilancia, Plantillas, Memoria, Backup, General, Desinstalar)
│   │   ├── LogsView.tsx         # Vigilancia/Logs con filtros + presets
│   │   ├── ChatView.tsx         # Chat con prefijos /chat /exec /plan /ask
│   │   ├── TemplatesView.tsx    # CRUD plantillas + preview + aplicar
│   │   ├── MetricsView.tsx      # Dashboard métricas + export CSV
│   │   ├── CommandPalette.tsx   # Ctrl+K búsqueda global
│   │   ├── Notification.tsx     # Toast notifications
│   │   └── ExecutorProgress.tsx # Loading steps para /exec
│   ├── i18n/
│   │   ├── index.ts             # Config i18next
│   │   ├── es.json              # Español
│   │   └── en.json              # Inglés
│   ├── store/
│   │   └── useStore.ts          # Zustand + selectors + persistencia
│   ├── hooks/
│   │   ├── useTranslation.ts
│   │   ├── useKeyboardShortcuts.ts
│   │   └── useDebounce.ts
│   ├── types/
│   │   └── index.ts             # Tipos sincronizados con Rust
│   └── styles/
│       ├── globals.css          # Variables CSS + temas
│       └── themes/              # 9 temas curados
├── src-tauri/                   # Backend Rust (Tauri 2)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── icons/                   # Iconos ojo de Argus
│   ├── build.rs
│   ├── migrations/
│   │   └── 001_init.sql         # Esquema SQLite completo
│   └── src/
│       ├── main.rs              # Setup Tauri, tray, plugins, bot_core spawn
│       ├── lib.rs               # Re-exports
│       ├── state.rs             # AppState, Config, persistencia BD
│       ├── commands.rs          # Comandos Tauri (invoke)
│       ├── db.rs                # Pool SQLite + queries
│       ├── crypto.rs            # AES-GCM cifrado tokens
│       ├── bot_core/
│       │   ├── mod.rs           # Entrada, run(), shutdown
│       │   ├── gateway.rs       # twilight-gateway + reconexión
│       │   ├── events.rs        # Handlers Event::* → server_maps
│       │   ├── sync.rs          # full_sync paginado
│       │   ├── http.rs          # DiscordHttp client (reqwest) con rate limiter
│       │   ├── executor.rs      // ExecutorAction + progress
│       │   ├── responder.rs     // CLI agent wrapper (opencode/codex/claude)
│       │   └── ipc.rs           // Canales mpsc/broadcast Tauri ↔ bot_core
│       └── mcp/
│           ├── mod.rs           // Servidor MCP stdio
│           ├── tools.rs         // Tools MCP expuestas
│           └── resources.rs     // Resources argus://
```

## Descripción por módulo

### Frontend (`src/`)

| Módulo | Responsabilidad |
|---|---|
| `App.tsx` | Layout principal, tabs, polling estado bot (5s), render vista activa |
| `Header.tsx` | Selector guild (bandera+nombre), estado bot, idioma, command palette trigger |
| `Sidebar.tsx` | Navegación tabs + atajos Ctrl+1-9 |
| `Dashboard.tsx` | Resumen: guild activa, estado bot, último sync, métricas rápidas |
| `ChannelsView.tsx` | Árbol categorías/canales, búsqueda, crear/editar/eliminar |
| `MembersView.tsx` | Lista virtualizada, paginación cursor, roles, acciones (rol, timeout, DM) |
| `RolesView.tsx` | Tabla roles jerárquica, permisos bitfield, overwrites, crear/editar |
| `SettingsView.tsx` | **7 secciones**: IA/Agentes, Vigilancia, Plantillas, Memoria, Backup, General, Desinstalar |
| `LogsView.tsx` | Vigilancia: tabla virtualizada, filtros, presets, editor avanzado, export |
| `ChatView.tsx` | Chat con prefijos `/chat` `/exec` `/plan` `/ask`, streaming, steps, historial |
| `TemplatesView.tsx` | CRUD plantillas, editor JSON + preview árbol, aplicar con progreso |
| `MetricsView.tsx` | Dashboard tiempo real (eventos/s, exec duration, cache hits) + CSV |
| `CommandPalette.tsx` | Ctrl+K: busca comandos, guilds, canales, miembros, plantillas, settings |
| `ExecutorProgress.tsx` | Componente loading steps reutilizable para `/exec` y plantillas |

### Backend (`src-tauri/src/`)

| Módulo | Responsabilidad |
|---|---|
| `main.rs` | Tauri setup, plugins, tray, menu, spawn `bot_core`, event loop |
| `state.rs` | `AppState`, `Config` (multi-guild, IA, vigilancia, backup, i18n), persistencia BD |
| `commands.rs` | 25+ comandos Tauri: guilds, maps, templates, agents, vigilance, chat, exec, uninstall |
| `db.rs` | `SqlitePool`, migraciones, queries CRUD todas las tablas |
| `crypto.rs` | `encrypt/decrypt` AES-GCM para tokens (discord_token, llm_api_key) |
| `bot_core/mod.rs` | Orquestador: spawn gateway, http, executor, responder; manejo shutdown |
| `bot_core/gateway.rs` | Conexión `twilight-gateway`, identify, resume, exponential backoff, heartbeats |
| `bot_core/events.rs` | Match `Event::*` → upsert `server_maps` + `insert_incident` + broadcast |
| `bot_core/sync.rs` | `full_sync(guild_id)`: channels + roles + members (cursor 1000) + categorías |
| `bot_core/http.rs` | `DiscordHttp` + rate limiter (buckets globales + por endpoint) |
| `bot_core/executor.rs` | `ExecutorAction` enum, `execute()` con steps + progress broadcast |
| `bot_core/responder.rs` | CLI agent wrapper (JSON-RPC stdio), tools expuestas, system prompt inyectado |
| `bot_core/ipc.rs` | Canales `tokio::sync::mpsc`/`broadcast` para comunicación Tauri ↔ bot_core |
| `mcp/mod.rs` | Servidor MCP stdio (lib `mcp`), instructions embebido |
| `mcp/tools.rs` | 10 tools: `get_status`, `get_map`, `apply_template`, `execute_action`, etc. |
| `mcp/resources.rs` | Resources: `argus://map/{guild_id}`, `argus://incidents`, `argus://templates`... |

## Flujo de datos principal

```
Usuario (GUI / Discord)
       │
       ▼
┌──────────────────┐
│   Tauri App      │◄──── Eventos Tauri (map-updated, exec-progress, chat-message)
│  (Commands +     │
│   State + UI)    │
└────────┬─────────┘
         │ invoke / canales Rust
         ▼
┌──────────────────┐
│   bot_core       │
│  (module interno)│
├──────────────────┤
│  gateway.rs      │◄──── Discord Gateway (Event::*)
│  http.rs         │────► Discord REST API
│  executor.rs     │────► Ejecuta acciones (create_channel, role, template...)
│  responder.rs    │────► CLI Agent (opencode/codex/claude) via JSON-RPC stdio
└──────────────────┘
         │
         ▼
┌──────────────────┐
│   SQLite (WAL)   │
│  config, maps,   │
│  templates,      │
│  messages,       │
│  incidents...    │
└──────────────────┘
         │
         ▼
┌──────────────────┐
│   JSONL histórico│
│  logs/messages_  │
│  {guild}_{MM}.jsonl
│  incidents_...   │
└──────────────────┘
```

## Decisiones arquitecturales clave

1. **Bot-core como módulo** (no binario separado): deploy simple, IPC por canales Rust, cero serialización extra.
2. **SQLite principal + JSONL histórico**: contexto activo consultable + auditoría inmutable.
3. **Executor (Rust) + Responder (CLI)**: manos rápidas + cerebro flexible.
4. **Multi-guild nativo**: `HashMap<guild_id, ServerMap>` + selector en Header.
5. **Cifrado tokens**: AES-GCM con key derivada de machine-id + salt.
6. **MCP propio**: binario `argus-mcp` compartiendo `argus-core` lib.
7. **Desinstalación atómica**: `uninstall_wipe()` borra todo rastro.