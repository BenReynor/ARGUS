# ARGUS

[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri)](https://tauri.app)
[![React](https://img.shields.io/badge/React-18-61DAFB?logo=react)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-1.99-orange?logo=rust)](https://rust-lang.org)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

**Centro de control universal para bots de Discord.**  
Multi-bot, multi-servidor, sync tiempo real, plantillas, personalidad dual (Executor + Responder), chat unificado, vigilancia configurable, backup automático, MCP propio, notificaciones Telegram, desinstalación atómica.

---

## ✨ Características

| Área | Detalle |
|---|---|
| **Multi-bot + Multi-servidor** | Múltiples bots configurables, selector de guild en header, mapa independiente por guild |
| **Sync inteligente** | Full sync al iniciar + Gateway events (create/update/delete channels, roles, members, bans, threads, voice) |
| **Plantillas** | JSON versionado: roles → categorías → canales → overwrites. Preview árbol + progreso pasos |
| **Executor (Rust)** | `create_channel`, `create_role`, `apply_template`, `set_overwrite`, `send_message`... <300ms p95 |
| **Responder (Configurable)** | **Nemotron 3 Ultra Free** (default, gratis), Ollama local, OpenRouter, OpenAI, Groq, CLI agents (opencode/codex/claude). Tools: todas las acciones Executor + `search_logs` + `get_map` |
| **Chat unificado** | GUI: `/chat` (Nemotron/Ollama), `/exec` (Rust), `/plan` (CLI), `/ask` (CLI). Discord: mención + slash |
| **Memoria híbrida** | JSONL histórico (inmutable, rotación mensual) + SQLite contexto activo (límites + limpieza manual/auto) |
| **Vigilancia** | 4 presets + editor avanzado. Tabla virtualizada, filtros, FTS5, export CSV/JSON |
| **Backup** | Automático cada N horas, ZIP versionado, retención configurable, restore 1-click |
| **MCP propio** | `argus-mcp` stdio: tools + resources + instructions. Control desde opencode/Claude |
| **Notificaciones Telegram** | Alertas críticas via webhook bot, eventos configurables |
| **Desinstalación atómica** | Un botón: borra BD, config, logs, backups, cache, atajos, keys derivadas |
| **i18n** | Español / Inglés (override en Settings) |
| **Atajos** | Ctrl+K palette, Ctrl+1-9 tabs, `/` focus chat, Ctrl+B sidebar |
| **Temas** | 9 curados (Dark, Light, Cyberpunk, Hacker, Zinc/Amber, Gray/Red, Stone/Emerald, Zinc/Sky, Slate/Teal) |

---

## 🚀 Inicio rápido

### Requisitos

- **Rust 1.99+** (`source ~/.cargo/env`)
- **Node 22+** (nvm)
- **GTK/WebKit** deps (Linux): `libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`
- **Ollama** (opcional, para chat local): `curl -fsSL https://ollama.com/install.sh | sh`
- **CLI Agents** (opcional, para `/plan` `/ask`): `opencode`, `codex`, `claude`

### Desarrollo

```bash
# Clonar
git clone https://github.com/BenReynor/ARGUS.git
cd ARGUS

# Rust env
source ~/.cargo/env

# Dependencias frontend
npm install

# Desarrollo (frontend + backend con hot-reload)
npm run tauri dev
# o ./launch-dev.sh
```

### Producción

```bash
# Build completo (binario + bundles .deb/.rpm/.AppImage)
npm run tauri build

# Binario en: src-tauri/target/release/argus
# Bundles en: src-tauri/target/release/bundle/
```

---

## ⚙️ Configuración

### Primer arranque

1. Abre **Settings → IA / Agentes**
2. **Proveedor chat**: **Nemotron 3 Ultra Free** (default, sin config), o Ollama (auto-detecta `http://localhost:11434`), o añade OpenRouter/OpenAI/Groq
3. **Agentes CLI**: Auto-detecta `opencode`/`codex`/`claude` con sesión. Override manual si hace falta.
4. **Bots**: Settings → Bots → "Añadir bot" → pega Bot Token + nombre + guilds permitidas
5. **Probar conexión**: botón valida token y permisos (ManageGuild/Channels/Roles/ViewAuditLog)
6. **Telegram** (opcional): Settings → General → Telegram → Bot Token + Chat ID + eventos a notificar
7. **Personalidad**: Settings → IA → edita `system_prompt`, reglas, canales permitidos

### Atajos de teclado (configurables)

| Acción | Default |
|---|---|
| Command Palette | `Ctrl+K` |
| Focus chat | `/` |
| Tabs 1-9 | `Ctrl+1` … `Ctrl+9` |
| Toggle sidebar | `Ctrl+B` |
| Nuevo `/exec` | `Ctrl+E` |
| Nuevo `/plan` | `Ctrl+P` |

---

## 🏗️ Arquitectura

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│   USUARIO   │────►│  RESPONDER   │────►│  EXECUTOR   │
│  (Discord/  │     │  (Nemotron/  │     │  (Rust)     │
│   GUI Chat) │     │   Ollama/    │     │  "Hace"     │
└─────────────┘     │   CLI Agent) │     └──────┬──────┘
                    │  "Piensa"    │            │
                    └──────────────┘            │
                                                ▼
                                        ┌─────────────┐
                                        │   DISCORD   │
                                        │   GATEWAY   │
                                        └─────────────┘
```

- **Executor**: Rust nativo, acciones Discord directas, loading steps en UI
- **Responder**: Proveedor configurable (Nemotron default), razona, decide, llama tools del Executor
- **Gateway**: `twilight-gateway` → eventos → `server_maps` en memoria + BD + JSONL histórico
- **REST**: `DiscordHttp` con rate limiter (buckets + backoff exponencial)

---

## 📦 Estructura del proyecto

```
ARGUS/
├── src/                    # Frontend React
│   ├── components/         # UI (Header, Sidebar, Dashboard, Channels, Members, Roles, Settings, Logs, Chat, Templates, Metrics, CommandPalette)
│   ├── store/              # Zustand + selectors
│   ├── api/                # invoke wrappers
│   ├── i18n/               # ES/EN
│   ├── hooks/              # Custom hooks
│   └── types/              # Tipos sincronizados con Rust
├── src-tauri/              # Backend Rust
│   ├── src/
│   │   ├── main.rs         # Tauri setup + bot_core spawn
│   │   ├── state.rs        # AppState + Config + BD
│   │   ├── commands.rs     # 25+ comandos Tauri
│   │   ├── db.rs           # SQLite pool + queries
│   │   ├── crypto.rs       # AES-GCM tokens
│   │   ├── bot_core/       # Gateway, Events, Sync, HTTP, Executor, Responder, IPC
│   │   └── mcp/            # Servidor MCP propio
│   ├── migrations/         # SQL versionado
│   └── Cargo.toml
├── docs/                   # FICHA_TECNICA.md, ARCHITECTURE_MAP.md
├── plan.md                 # Plan de trabajo (formato Dédalo)
├── CONTEXTO.md             # Contexto permanente
└── info/                   # Credenciales (gitignored)
```

---

## 🛠️ Desarrollo

### Comandos útiles

```bash
# Frontend only
npm run build          # tsc + vite build
npm run dev            # Vite dev server

# Backend only
cd src-tauri && cargo build
cd src-tauri && cargo test
cd src-tauri && cargo watch -x run

# Full stack
npm run tauri dev      # Desarrollo integrado
npm run tauri build    # Producción

# Lint / Format
npm run lint           # ESLint + Prettier
cd src-tauri && cargo fmt --check
cd src-tauri && cargo clippy
```

### Tests

```bash
# Rust
cd src-tauri && cargo test --workspace

# Frontend
npm run test           # Vitest + React Testing Library

# CI local (simula GitHub Actions)
act push
```

---

## 🔐 Seguridad

- **Tokens cifrados**: AES-GCM, key derivada de machine-id + salt (no en repo)
- **BD permisos**: `0600` archivo, `0700` directorio `~/.config/argus/`
- **Validación permisos bot**: Al `start_bot` verifica `ManageGuild/Channels/Roles/ViewAuditLog`
- **MCP stdio only**: Sin puertos de red expuestos
- **Desinstalación atómica**: `uninstall_wipe()` borra todo rastro incluyendo keys derivadas

---

## 📚 Documentación

- [`docs/FICHA_TECNICA.md`](docs/FICHA_TECNICA.md) — Especificación técnica completa
- [`docs/ARCHITECTURE_MAP.md`](docs/ARCHITECTURE_MAP.md) — Mapa de arquitectura detallado
- [`CONTEXTO.md`](CONTEXTO.md) — Contexto permanente del proyecto
- [`plan.md`](plan.md) — Plan de trabajo (formato Dédalo)

---

## 🤝 Contribuir

1. Fork + branch (`feat/nueva-funcionalidad`)
2. Commits convencionales (`feat:`, `fix:`, `docs:`, `test:`)
3. `npm run build && cd src-tauri && cargo test` deben pasar
4. PR con descripción clara

---

## 📄 Licencia

MIT — ver [`LICENSE`](LICENSE).

---

## 🙏 Créditos

- **Dédalo** — Patrón arquitectura (motor separado + MCP + estado en archivos)
- **Tauri** — Framework desktop Rust + Web
- **twilight** — Discord library Rust
- **Zustand** — State management simple