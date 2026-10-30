# ARCHITECTURE_MAP.md — ARGUS

Mapa de lo que **existe hoy** en el repo (FASES 0–8 de plan.md hechas). Visión y
decisiones: CONTEXTO.md. Lo que aún no existe (plantillas, personalidad,
vigilancia con alertas, backup) no tiene archivos todavía.

## Árbol actual

```
ARGUS/
├── AGENTS.md / CLAUDE.md     # reglas operativas para agentes
├── CONTEXTO.md               # visión y decisiones (fuente única)
├── plan.md                   # plan de trabajo (formato Dédalo)
├── lecciones.md              # errores ya corregidos (no repetirlos)
├── README.md
├── docs/
│   ├── AUDITORIA_COMANDOS.md # estado de cada comando Tauri
│   ├── FICHA_TECNICA.md      # stack, datos y comandos
│   ├── MCP.md                # argus-mcp: instalación y herramientas
│   └── PRUEBA_REAL.md        # tests contra un servidor de Discord real
├── info/                     # privado, gitignored: los agentes NO lo leen
├── scripts/abrir-argus.sh    # lanzador de Linux: recompila desde main si cambió y abre
├── .github/workflows/ci.yml  # tests en Linux y Windows + instalador de Windows
├── index.html                # incluye la pantalla de carga (antes de React)
├── package.json, tsconfig*.json, vite.config.ts
├── src/                      # Frontend React + TypeScript
│   ├── main.tsx              # entrada React, fuentes y temas
│   ├── main.rs               # sobrante de una plantilla (Hello world), no se usa
│   ├── App.tsx               # layout, carga inicial y eventos del backend (sin polling)
│   ├── carga.ts              # avanza y quita la pantalla de carga de index.html
│   ├── temas.ts              # lista de temas y aplicarTema (data-tema / data-modo)
│   ├── styles.css, styles/layout.css
│   ├── styles/temas/*.css    # 10 temas (contrato de tokens de ui-tokens) + rasgos.css
│   ├── api/tauri.ts          # invoke → comandos Rust y suscripción a eventos
│   ├── store/useStore.ts     # Zustand
│   ├── types/index.ts        # tipos sincronizados con state.rs / db.rs
│   └── components/
│       ├── GuildRail.tsx     # columna de servidores del bot activo
│       ├── Sidebar.tsx       # secciones y bots
│       ├── Header.tsx        # cabecera de la sección, estado del Gateway
│       ├── Dashboard.tsx     # panel del servidor y permisos del bot
│       ├── ChannelsView.tsx  # canales y categorías (crear, editar, borrar, mover)
│       ├── PermisosCanal.tsx # overwrites de un canal
│       ├── ActividadCanal.tsx# mensajes, invitaciones y webhooks de un canal
│       ├── RolesView.tsx     # roles, permisos y jerarquía
│       ├── MembersView.tsx   # miembros y moderación
│       ├── ServidorView.tsx  # ajustes del servidor y registro de auditoría
│       ├── LogsView.tsx      # historial de cambios del mapa
│       ├── SettingsView.tsx  # bots, este servidor, MCP, apariencia, general
│       ├── EstadoVista.tsx   # estados cargando / error / vacío comunes
│       ├── Notification.tsx
│       └── ui/               # componentes base (Button, Card, Input, diálogos…)
└── src-tauri/                # Backend Rust (Tauri 2) — cargo se ejecuta aquí
    ├── Cargo.toml, Cargo.lock # twilight-http/gateway/model 0.15; default-run = argus
    ├── build.rs              # tauri-build + manifiesto de Windows en todos los ejecutables
    ├── windows-app-manifest.xml
    ├── tauri.conf.json
    ├── migrations/001–004_*.sql
    ├── tests/
    │   ├── integracion_discord.rs # contra Discord real, #[ignore] (docs/PRUEBA_REAL.md)
    │   └── mcp_humo.rs       # lanza argus-mcp: initialize + tools/list
    └── src/
        ├── main.rs           # arranque de la app (llama a argus_lib)
        ├── lib.rs            # biblioteca argus_lib: setup de Tauri, BD, tray, registro de comandos
        ├── bin/argus-mcp.rs  # binario MCP (stdio); lee la misma BD que la app
        ├── mcp.rs            # herramientas MCP sobre la misma lógica que los comandos
        ├── state.rs          # Config, AppState, tipos del mapa (Channel, Role…)
        ├── commands.rs       # comandos Tauri
        ├── discord.rs        # cliente REST (twilight-http), URL base configurable, errores en español
        ├── mensajes.rs       # mensajes, invitaciones y webhooks
        ├── gateway.rs        # Gateway (twilight-gateway) mientras el bot está activo
        ├── sync.rs           # sincronización completa del servidor al conectar
        ├── mapa.rs           # aplica eventos del Gateway al ServerMap (función pura)
        ├── persistencia.rs   # guarda mapas e historial en server_maps
        ├── db.rs             # SQLite (sqlx); ruta por sistema operativo
        ├── crypto.rs         # AES-GCM para tokens (+ tests)
        └── tests_*.rs        # tests de comandos contra el Discord falso (wiremock)
```

## Flujo

```
UI (React) ──invoke──► commands.rs ──► discord.rs / mensajes.rs ──► API REST de Discord
     ▲                      │
     │ eventos              └──► db.rs (SQLite, tokens cifrados con crypto.rs)
     │ map-updated,
     │ sync-progress,       gateway.rs ──► mapa.rs ──► persistencia.rs
     └─ bot-status-change ◄─┘ (Gateway de Discord)

Cliente MCP ──stdio──► argus-mcp (bin) ──► mcp.rs ──► misma lógica y misma BD
```

Los tests no usan la red: el cliente de Discord acepta una URL base y los tests
apuntan a un Discord falso local (wiremock).
