# AGENTS.md — ARGUS

## Qué es ARGUS
App de escritorio (Tauri 2 + React + TypeScript + Rust) para controlar el bot propio
de Discord **"Agente Albert"**: encender/apagar, mapa del servidor, canales, miembros,
roles, logs. No es un selfbot (viola ToS) — se maneja el bot propio vía MCP y Discord API.

- Repo (privado): https://github.com/BenReynor/ARGUS (rama `main`)
- Ubicación local: `/home/benjametalsp/Escritorio/Programas_Propios/ARGUS`
- Servidor Discord: "ISD Internal Security Department | Site-14", guild `1541513296375717921`

## Info sensible (NO subir a git — `info/` está en .gitignore)
- `info/CONTRASENAS.txt` — token del bot, clave pública, contraseña sudo
- `info/Bot.txt` — credenciales del bot
- `info/ISD_SERVER_MAP.txt` — mapa completo del servidor (canales, roles, miembros, IDs)
- `info/discord_oauth_url.txt` — URL de invitación del bot
- El token también está en `~/.bashrc` como `DISCORD_TOKEN`

## Entorno (IMPORTANTE)
- Rust/cargo 1.99: **siempre** `source /home/benjametalsp/.cargo/env` en shells no interactivas
- Node 22.23.3 (nvm)
- Dependencias GTK ya instaladas (libgtk-3-dev, libwebkit2gtk-4.1-dev, etc.)
- `sudo` pide contraseña (está en `info/CONTRASENAS.txt`)
- `cargo build` se ejecuta desde `src-tauri/` (no desde la raíz)
- MCP de Discord disponible en OpenCode: servidor `discord` (@pasympa/discord-mcp)

## Comandos
```bash
source ~/.cargo/env
npm run tauri dev      # desarrollo (o ./launch-dev.sh)
npm run tauri build    # producción → src-tauri/target/release/argus
npm run build          # solo frontend (tsc && vite build)
```

## Estructura
```
ARGUS/
├── AGENTS.md               # este archivo
├── info/                   # credenciales y mapa (gitignored)
├── launch-dev.sh           # lanza dev con cargo env
├── src/                    # React
│   ├── App.tsx             # layout, tabs, polling de estado (5s)
│   ├── components/         # Sidebar, Header, Dashboard, ChannelsView,
│   │                       # MembersView, RolesView, SettingsView,
│   │                       # LogsView, Notification
│   ├── store/useStore.ts   # Zustand + selectors
│   └── api/tauri.ts        # puentes invoke → comandos Rust
└── src-tauri/              # Rust
    ├── Cargo.toml          # tauri + plugins (store, log, shell, fs, dialog...)
    ├── tauri.conf.json     # plugins.store/log = null (¡no poner {})
    ├── icons/              # iconos propios (ojo de Argus) ya generados
    └── src/
        ├── main.rs         # setup, tray, generate_context!
        ├── state.rs        # AppState, config persistence (store plugin)
        └── commands.rs     # comandos Tauri expuestos al frontend
```

## Estado actual (última sesión)
- ✅ Build de producción OK: 3 bundles en `src-tauri/target/release/bundle/`
  (`.deb`, `.rpm`, `.AppImage`)
- ✅ Icono propio generado (ojo de Argus, estilo cyberpunk)
- ✅ Accesos directos "ARGUS" en escritorio y menú → apuntan al binario release
- ✅ Push a GitHub hecho (commit "Fix build: TypeScript types, icons, and Tauri config")
- ⚠️ Warning menor: `load_config` never used (src/state.rs:116)

## Pendiente / ideas siguientes
- Conectar los comandos Tauri a la API real de Discord (ahora hay datos mock en commands.rs)
- Icono `.icns` es un PNG renombrado (suficiente en Linux; para macOS habría que generar ICNS real)
- Regenerar accesos directos si cambia la ruta del binario

## Convenciones
- Idioma: español (UI y conversación)
- Iconos Lucide se tipan como `any`/`Record<number, any>` para evitar errores TS
- CSS con variables custom → castear `as React.CSSProperties` cuando se pasan `--vars`
- `as const` en arrays que se pasan como props causa TS4104 (usar tipado explícito)
