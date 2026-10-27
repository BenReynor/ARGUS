# CONTEXTO.md — ARGUS

## Qué es ARGUS

App de escritorio (Tauri 2 + React + TypeScript + Rust) para **controlar bots propios de Discord**: encender/apagar, mapa del servidor en tiempo real, gestión de canales/miembros/roles, logs de vigilancia, plantillas para armar servidores, personalidad dual (Executor + Responder), chat bidireccional (GUI + Discord), backup automático, MCP propio, desinstalación atómica.

**No es un selfbot** (viola ToS). Se maneja el bot propio vía **Discord API REST + Gateway** con token de bot legítimo.

El sistema es **multi-bot**: funciona con cualquier bot que el usuario configure (no hardcodeado a "Agente Albert").

## Objetivo final

Un **centro de orquestación completo** para bots personales que permita:

1. **Multi-servidor + Multi-bot**: cambiar de guild en el header, mapa sincronizado por guild, múltiples bots configurables
2. **Sync inteligente**: leer todo al prender + detectar cambios en tiempo real vía Gateway
3. **Plantillas**: aplicar estructura completa (roles, categorías, canales, overwrites) a servidores donde el bot tiene permisos
4. **Personalidad dual**: 
   - **Executor (Rust)**: acciones Discord rápidas y confiables (ms)
   - **Responder (proveedor configurable)**: razona, conversa, decide qué hacer (Nemotron 3 Ultra Free, Ollama local, OpenRouter, OpenAI, Groq, CLI agents como opencode/codex/claude)
5. **Chat unificado**: en la app (`/chat`, `/exec`, `/plan`, `/ask`) y en Discord (mención, slash)
6. **Vigilancia configurable**: joins, leaves, bans, role/channel changes, message deletes con presets + editor avanzado
7. **Memoria persistente**: archivo histórico JSONL (inmutable) + contexto activo SQLite (límites configurables, limpieza manual/auto)
8. **Backup automático**: cada N horas, ZIP versionado, retención configurable
9. **MCP propio**: controlar ARGUS desde opencode/Claude Desktop
10. **Desinstalación atómica**: un botón borra todo rastro
11. **Notificaciones Telegram**: alerts críticos via webhook

## Principios de diseño

- **Privacidad first**: tokens cifrados localmente, proveedores locales por defecto, sin telemetría obligatoria
- **Control total**: tú eliges proveedor IA por acción (`/chat`=Nemotron/Ollama, `/plan`=opencode, `/exec`=Rust nativo)
- **Observabilidad**: estado en archivos/BD (estilo Dédalo), nada oculto
- **Rendimiento**: Rust para paths calientes, React virtualizado, debounce, RwLock
- **Accesibilidad**: WCAG 2.2 AA, navegación teclado, command palette, temas
- **Extensibilidad**: plugin system (FASE 7), MCP, hooks

## Stack técnico

| Capa | Tecnología |
|---|---|
| Frontend | React 18 + TypeScript + Vite + Zustand |
| UI | CSS custom properties + 9 temas curados + i18n (ES/EN) |
| Backend | Rust + Tauri 2 + twilight-gateway/http + sqlx + tokio |
| BD | SQLite (WAL) + FTS5 + JSONL histórico rotativo |
| Cifrado | AES-GCM (aes-gcm crate) |
| IA Chat Default | **Nemotron 3 Ultra Free** (gratis, rápido, sin API key) |
| IA Chat Alternativos | Ollama local, OpenRouter, OpenAI, Groq (configurable) |
| IA Code/Plan | opencode / codex / claude (CLI agents, auto-detect + manual override) |
| MCP | Servidor stdio propio (`argus-mcp`) |
| Update | Tauri Updater (GitHub Releases) |
| CI/CD | GitHub Actions (test + build + release) |
| Notificaciones | Telegram Bot API (webhook) |

## Convenciones

- **Idioma**: Español (UI y conversación), Inglés opcional
- **Commits**: Convencionales (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`)
- **Tareas**: Una por commit, verificable (`npm run build && cargo test`)
- **Config**: SQLite `config` table (key-value JSON) + `settings` table
- **Rutas config**: `~/.config/argus/` (BD, logs, backups, cache)
- **Formato fechas**: ISO 8601 UTC en BD, locale en UI

## Estructura de datos clave

```typescript
// Config principal (simplificada)
interface Config {
  // Bots configurables (no hardcodeado a uno solo)
  bots: BotConfig[];               // cada bot: token, nombre, guilds permitidos
  active_bot_id: string | null;
  active_guild_id: string | null;
  guilds: Guild[];
  multi_guild_mode: boolean;
  
  // IA - Proveedor por defecto: Nemotron 3 Ultra Free
  llm_chat_provider: "nemotron" | "ollama" | "openrouter" | "openai" | "groq" | "cli";
  llm_chat_model: string;
  llm_chat_url: string;
  llm_chat_api_key: string;        // cifrado (vacío para Nemotron/Ollama local)
  plan_provider: "auto" | "opencode" | "codex" | "claude" | "nemotron";
  ask_provider: "auto" | "opencode" | "codex" | "claude" | "nemotron";
  cli_agents: Record<string, CliAgentConfig>;
  
  // Contexto/Memoria
  context_limit_per_user: number;      // default 10
  context_limit_per_channel: number;   // default 50
  auto_cleanup_enabled: boolean;
  archive_retention_months: number;
  
  // Vigilancia
  vigilance_preset: "minimal" | "standard" | "full" | "custom";
  vigilance_enabled_events: string[];
  vigilance_filters: VigilanceFilters;
  
  // Backup
  backup_enabled: boolean;
  backup_interval_hours: number;       // default 24
  backup_retention_count: number;      // default 30
  
  // Telegram notifications
  telegram_enabled: boolean;
  telegram_bot_token: string;          // cifrado
  telegram_chat_id: string;
  telegram_events: string[];           // qué eventos notificar
  
  // General
  language: "es" | "en";
  auto_update: boolean;
  telemetry_opt_in: boolean;
  anti_spam_cooldown_secs: number;     // default 30
  responder_triggers: ResponderTriggers;
  alerts_config: AlertsConfig;
}
```

## Flujo de trabajo típico

1. **Arranque**: App carga config BD → valida token del bot activo → `start_bot` → spawn `bot_core`
2. **Bot-core**: Conecta Gateway → `full_sync(active_guild)` → emite `map-updated`
3. **UI**: Recibe mapa → render Channels/Members/Roles → usuario interactúa
4. **Usuario en GUI**: `/exec crear_canal` → Executor (Rust) → steps loading → Discord API → `map-updated`
5. **Usuario en Discord**: `@bot hola` → Gateway → Responder (proveedor configurado) → piensa → llama tools Executor → responde
6. **Eventos Gateway**: Cualquier cambio → upsert `server_maps` + `insert_incident` → broadcast → UI actualiza
7. **Backup**: Cada 24h → `export_all_data()` → ZIP → retención 30
8. **Alertas**: Eventos críticos → notificación nativa + Telegram webhook

## Seguridad

- Tokens **nunca en texto plano**: `discord_token`, `llm_chat_api_key`, `telegram_bot_token` cifrados AES-GCM
- Key derivada de `machine-id` + salt almacenado en config (no en repo)
- BD permisos `0600`, directorio `0700`
- Validación permisos bot al `start_bot` (ManageGuild/Channels/Roles/ViewAuditLog)
- Desinstalación atómica borra todo incluyendo keys derivadas
- MCP stdio only (sin puertos de red)

## Integración con Dédalo

- `plan.md` formato Dédalo (`- [ ]`/`- [x]`/`- [B]`)
- `incidencias.jsonl` estilo Dédalo (rotación mensual)
- `lecciones.md` para memoria de proyecto
- MCP `dedalo` disponible para orquestar desarrollo de ARGUS mismo