# Ficha Técnica — ARGUS

> Especificación técnica para desarrolladores y agentes IA.  
> Estilo inspirado en [Dédalo](https://github.com/BenReynor/dedalo).

---

## Qué es

App de escritorio (Tauri 2 + React + TypeScript + Rust) para **controlar bots propios de Discord**: encender/apagar, mapa del servidor en tiempo real, gestión de canales/miembros/roles, logs de vigilancia, plantillas para armar servidores, personalidad dual (Executor + Responder), chat bidireccional (GUI + Discord), backup automático, MCP propio, notificaciones Telegram, desinstalación atómica.

- **Repo**: https://github.com/BenReynor/ARGUS (rama `main`)
- **Multi-bot**: soporta cualquier bot configurado por el usuario

---

## Requisitos

| Requisito | Versión |
|---|---|
| Rust | ≥ 1.99 (`source ~/.cargo/env`) |
| Node | ≥ 22 (nvm) |
| Sistema | Linux (ref), macOS, Windows |
| GTK/WebKit | `libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev` |
| Ollama | Opcional (chat local) |
| CLI Agents | Opcional: `opencode`, `codex`, `claude` |

---

## Dependencias principales

### Frontend (`package.json`)

| Paquete | Versión | Uso |
|---|---|---|
| `react` | 18 | UI |
| `react-dom` | 18 | Render |
| `zustand` | 4 | State management |
| `i18next` | 23 | i18n ES/EN |
| `@tanstack/react-virtual` | 3 | Listas virtualizadas |
| `lucide-react` | 0.4 | Iconos |
| `monaco-editor` | 0.45 | Editor JSON plantillas |
| `vite` | 5 | Build tool |
| `typescript` | 5 | Types |

### Backend (`Cargo.toml`)

| Crate | Versión | Uso |
|---|---|---|
| `tauri` | 2 | Framework desktop |
| `tauri-plugin-sql` | 2 | SQLite (rusqlite) |
| `tauri-plugin-updater` | 2 | Auto-update GitHub Releases |
| `twilight-gateway` | 0.15 | Discord Gateway |
| `twilight-http` | 0.15 | Discord REST |
| `twilight-model` | 0.15 | Tipos Discord |
| `twilight-standby` | 0.15 | Espera eventos |
| `sqlx` | 0.7 | SQLite async + migraciones |
| `reqwest` | 0.12 | HTTP client |
| `tokio` | 1 | Async runtime |
| `tracing` | 0.1 | Structured logging |
| `aes-gcm` | 0.10 | Cifrado tokens |
| `mcp` | 0.2 | Servidor MCP stdio |
| `serde` | 1 | Serialización |
| `anyhow` / `thiserror` | 1 | Error handling |
| `machine-uid` | 0.3 | Machine ID para deriva key |

---

## Arquitectura

### Componentes principales

```
┌─────────────────────────────────────────────────────────────┐
│                      TAURI APP                               │
├─────────────────────────────────────────────────────────────┤
│  Frontend (React)                                           │
│  ├── Header (guild selector, bot selector, bot status, lang)│
│  ├── Sidebar (tabs + shortcuts)                             │
│  ├── Dashboard / Channels / Members / Roles                 │
│  ├── Settings (7 secciones)                                 │
│  ├── Logs/Vigilancia (tabla virtual + presets + FTS5)       │
│  ├── Chat (/chat /exec /plan /ask + streaming + steps)      │
│  ├── Templates (CRUD + preview + apply + progress)          │
│  ├── Metrics (dashboard + CSV)                              │
│  └── CommandPalette (Ctrl+K)                                │
├─────────────────────────────────────────────────────────────┤
│  Backend (Rust)                                             │
│  ├── State (AppState + Config multi-bot/multi-guild + BD)   │
│  ├── Commands (25+ invoke handlers)                         │
│  ├── DB (SQLite WAL + FTS5 + migraciones)                   │
│  ├── Crypto (AES-GCM tokens)                                │
│  ├── Bot-Core Module                                        │
│  │   ├── Gateway (twilight + reconexión + resume)           │
│  │   ├── Events (handlers → server_maps + incidents)        │
│  │   ├── Sync (full_sync paginado)                          │
│  │   ├── HTTP (DiscordHttp + rate limiter)                  │
│  │   ├── Executor (actions + progress broadcast)            │
│  │   ├── Responder (Nemotron/Ollama/OpenRouter/CLI + JSON-RPC)│
│  │   └── IPC (mpsc/broadcast channels Tauri ↔ bot_core)     │
│  └── MCP Server (argus-mcp stdio)                           │
└─────────────────────────────────────────────────────────────┘
```

### Flujo de datos

1. **Inicio**: `start_bot` → valida permisos → spawn `bot_core::run()`
2. **Gateway**: Conecta → `GUILD_CREATE` × N → `full_sync(active_guild)` → `MapUpdated` events
3. **UI**: Recibe `map-updated` → actualiza store → render vistas
4. **Usuario GUI**: `/exec` → `Executor` (Rust) → Discord REST → `MapUpdated`
5. **Usuario Discord**: `@bot` → Gateway → `Responder` (Nemotron/Ollama/CLI) → razona → llama tools Executor → responde
6. **Eventos**: Cualquier `Event::*` → upsert `server_maps` + `insert_incident` → broadcast
7. **Persistencia**: `server_maps` debounce 5s/50 cambios → SQLite; JSONL append-only rotativo

---

## Base de Datos (SQLite)

### Tablas

| Tabla | Clave | Descripción |
|---|---|---|
| `config` | `key` | Key-value JSON (config principal multi-bot) |
| `bots` | `id` | Bots configurados (token cifrado, nombre, guilds permitidas) |
| `guilds` | `id` | Cache guilds (name, icon, member_count) |
| `server_maps` | `guild_id` | Mapa completo JSON por guild |
| `templates` | `id` | Plantillas versionadas (roles, categorías, canales, overwrites) |
| `cli_agents` | `id` | Config agentes CLI (mode, binary_path, model) |
| `vigilance` | `id` | Preset + enabled_events + filters JSON |
| `messages` | `id` | Contexto activo (guild, channel, user, content, role, provider, ts) |
| `messages_fts` | `rowid` | FTS5 búsqueda full-text |
| `incidents` | `id` | Eventos vigilancia (guild, type, payload JSON, ts) |
| `settings` | `key` | Settings globales (backup, language, telegram, etc.) |

### Archivos históricos (JSONL rotativo mensual)

```
~/.config/argus/
├── argus.db                    # SQLite principal
├── logs/
│   ├── messages_{guild_id}_{YYYY-MM}.jsonl
│   └── incidents_{guild_id}_{YYYY-MM}.jsonl
├── backups/
│   └── argus_backup_{timestamp}.zip
└── cache/
```

---

## Proveedores IA (Responder)

| Proveedor | Modelo default | Config | Uso |
|---|---|---|---|
| **Nemotron 3 Ultra Free** | `nemotron-3-ultra-free` | Solo endpoint (gratis) | **Default chat** |
| Ollama | `llama3.1:8b` | `base_url` (default localhost:11434) | Local, privado |
| OpenRouter | `auto` | `api_key` | 100+ modelos |
| OpenAI | `gpt-4o-mini` | `api_key` | Estándar |
| Groq | `llama-3.1-8b-instant` | `api_key` | Ultra rápido |
| CLI Agents | `opencode`/`codex`/`claude` | Auto-detect + manual | Tareas complejas (`/plan`, `/ask`) |

**Selección por comando:**
- `/chat` → `llm_chat_provider` (default Nemotron)
- `/exec` → Executor Rust (siempre)
- `/plan` → `plan_provider` (default auto CLI)
- `/ask` → `ask_provider` (default auto CLI)
- Override inline: `/plan:opencode ...`

---

## MCP Server (argus-mcp)

**Transporte**: stdio (JSON-RPC 2.0)  
**Binario**: `argus-mcp` (comparte `argus-core` lib)

### Tools (10)

| Tool | Descripción |
|---|---|
| `get_status` | Estado bot + guild activa |
| `get_map(guild_id?)` | Mapa completo servidor |
| `list_guilds` | Guilds donde está el bot |
| `list_bots` | Bots configurados |
| `apply_template(guild_id, template_id)` | Aplica plantilla con progreso |
| `execute_action(action)` | Ejecuta acción Executor |
| `set_personality(prompt, rules, channels)` | Actualiza personalidad |
| `get_vigilance` | Config vigilancia actual |
| `search_logs(query, guild_id?)` | Busca en logs (FTS5) |
| `export_data` / `import_data` | Backup/restore |

### Resources

| URI | Descripción |
|---|---|
| `argus://map/{guild_id}` | Mapa servidor (JSON) |
| `argus://incidents` | Incidentes recientes |
| `argus://templates` | Lista plantillas |
| `argus://config` | Config actual |
| `argus://manual` | Manual embebido |

---

## Notificaciones Telegram

| Config | Descripción |
|---|---|
| `telegram_enabled` | Habilitar/deshabilitar |
| `telegram_bot_token` | Token del bot notificador (cifrado) |
| `telegram_chat_id` | Chat ID destino |
| `telegram_events` | Array de eventos: `critical`, `ban`, `role_change`, `channel_change`, `member_join`, `member_leave` |

**Formato mensaje**: `[CRITICAL] Guild: ISD | Ban: @usuario por @mod | Razón: spam`

---

## Seguridad

| Medida | Implementación |
|---|---|
| Tokens cifrados | AES-GCM (`aes-gcm` crate), key = HKDF(machine-id + salt) |
| BD permisos | Archivo `0600`, directorio `0700` |
| Validación permisos bot | `GET /guilds/{id}/members/@me` → check bits al `start_bot` |
| MCP | Solo stdio, sin red, tools de solo lectura + acciones controladas |
| Desinstalación | `uninstall_wipe()`: BD, config, logs, backups, cache, keys, atajos |
| Rate limiting | Buckets globales + por endpoint, backoff exponencial, `Retry-After` |
| Telemetría | Opt-in only, contadores anónimos |

---

## Rendimiento (targets)

| Métrica | Objetivo |
|---|---|
| Startup frío | < 2s |
| Sync mapa (1k canales) | < 5s |
| Memoria RSS | < 300 MB |
| UI frame rate | 60 fps sostenido |
| Executor latency (create_channel) | < 300 ms p95 |
| Responder primer token (Nemotron) | < 2s |
| Tests coverage | > 80% Rust core, > 70% frontend |

---

## CI/CD (GitHub Actions)

```yaml
# .github/workflows/ci.yml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --workspace
      - run: npm ci && npm run build
  
  release:
    needs: test
    if: startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: tauri-apps/tauri-action@v0
        with:
          tagName: ${{ github.ref_name }}
          releaseName: 'ARGUS ${{ github.ref_name }}'
```

---

## Distribución

| OS | Artefactos | Notas |
|---|---|---|
| Linux | `.deb`, `.rpm`, `.AppImage`, `.tar.gz` | AppImage portable |
| macOS | `.dmg` (arm64) | Sin firmar (control+click → Abrir) |
| Windows | `.msi`, `.exe` (NSIS) | SmartScreen: "Más info → Ejecutar" |

**Updater**: Tauri Updater → GitHub Releases (`latest.json`)

---

## Comandos de desarrollo

```bash
# Rust env
source ~/.cargo/env

# Frontend
npm run dev              # Vite dev server
npm run build            # tsc + vite build
npm run lint             # ESLint + Prettier

# Backend
cd src-tauri && cargo build
cd src-tauri && cargo test
cd src-tauri && cargo watch -x run

# Full stack
npm run tauri dev        # Desarrollo integrado
npm run tauri build      # Producción

# DB
cd src-tauri && sqlx migrate run
cd src-tauri && sqlx migrate add nombre
```

---

## Extensibilidad (FASE 7+)

### Plugin System

```rust
trait ArgusPlugin {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn commands(&self) -> Vec<TauriCommand>;
    fn tools(&self) -> Vec<McpTool>;
    fn hooks(&self) -> PluginHooks;
}
```

- Loader: `plugins/` (dynamic libs) o WASM
- Ejemplo: `argus-plugin-welcome` (mensaje bienvenida personalizable)

---

## Referencias

- [Tauri 2 Docs](https://tauri.app/v2/)
- [twilight-rs](https://github.com/twilight-rs/twilight)
- [Dédalo](https://github.com/BenReynor/dedalo) — Patrón arquitectura
- [Nemotron 3 Ultra Free](https://www.nvidia.com/en-us/research/nemotron-3-ultra/) — LLM gratis NVIDIA
- [Ollama](https://ollama.com) — LLM local
- [opencode](https://opencode.ai) / [codex](https://github.com/openai/codex) / [claude](https://claude.ai/code) — CLI Agents