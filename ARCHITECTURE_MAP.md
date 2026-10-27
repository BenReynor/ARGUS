# ARCHITECTURE_MAP.md — ARGUS

Mapa de lo que **existe hoy** en el repo. Lo que todavía no existe aparece en la
sección «Planeado» con la fase de plan.md que lo crea: no lo importes ni lo
edites como si ya estuviera. Visión y decisiones: CONTEXTO.md.

## Árbol actual

```
ARGUS/
├── AGENTS.md                 # reglas operativas para agentes
├── CONTEXTO.md               # visión y decisiones (fuente única)
├── plan.md                   # plan de trabajo (formato Dédalo)
├── lecciones.md              # errores ya corregidos (no repetirlos)
├── README.md
├── docs/
│   ├── AUDITORIA_COMANDOS.md # estado real/mock de cada comando Tauri
│   └── FICHA_TECNICA.md
├── info/                     # privado, gitignored: los agentes NO lo leen
├── index.html, package.json, tsconfig*.json, vite.config.ts
├── src/                      # Frontend React + TypeScript
│   ├── main.tsx              # entrada React
│   ├── main.rs               # sobrante de una plantilla (Hello world), no se usa
│   ├── App.tsx               # layout, pestañas, polling de estado cada 5 s
│   ├── styles.css
│   ├── api/tauri.ts          # invoke → comandos Rust
│   ├── store/useStore.ts     # Zustand
│   ├── types/index.ts        # tipos sincronizados con state.rs / db.rs
│   └── components/
│       ├── Header.tsx        # selector de servidor (guild) activo
│       ├── Sidebar.tsx       # navegación
│       ├── Dashboard.tsx
│       ├── ChannelsView.tsx
│       ├── MembersView.tsx
│       ├── RolesView.tsx
│       ├── SettingsView.tsx  # token (vía add_bot/update_bot), servidor, opciones
│       ├── LogsView.tsx
│       └── Notification.tsx
└── src-tauri/                # Backend Rust (Tauri 2) — cargo se ejecuta aquí
    ├── Cargo.toml            # incluye twilight-http/gateway/model 0.15
    ├── tauri.conf.json
    ├── migrations/001_init.sql
    └── src/
        ├── main.rs           # setup: plugins, migraciones, carga de config, tray
        ├── state.rs          # Config, AppState, tipos del mapa (Channel, Role…)
        ├── commands.rs       # 41 comandos Tauri (Discord: casi todos mock)
        ├── db.rs             # SQLite (sqlx): bots, guilds, server_maps, …
        └── crypto.rs         # AES-GCM para tokens (+ tests)
```

## Flujo actual

```
UI (React) ──invoke──► commands.rs ──► AppState (Mutex) / db.rs (SQLite)
                              │
                              └─► start_bot: lanza `npx @pasympa/discord-mcp`
                                  (se retira en la FASE 1)
```

Los datos de Discord que muestra la UI hoy son mock o vienen de un mapa en
memoria que nada rellena (ver docs/AUDITORIA_COMANDOS.md).

## Planeado (MVP)

| Módulo | Fase | Qué hará |
|---|---|---|
| `src-tauri/src/discord.rs` | 1–2 | Cliente `twilight-http` con URL base configurable para tests; conversión de errores a español |
| `src-tauri/migrations/002_*.sql` | 1 | Columnas del bot oficial (id, username, avatar) si hacen falta |
| `src-tauri/src/mapa.rs` | 3 | Aplicar eventos del Gateway al `ServerMap` (función pura) |
| Tarea Gateway en `commands.rs`/`mapa.rs` | 3 | `twilight-gateway`, sync completa, eventos `map-updated` y `sync-progress` |

Después del MVP (sin diseño cerrado): plantillas, personalidad/LLM, vigilancia,
Telegram, backup, MCP propio, i18n y temas. Ver CONTEXTO.md.
