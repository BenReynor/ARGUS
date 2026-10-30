# AGENTS.md — ARGUS

Reglas operativas para agentes. Qué es ARGUS y por qué: CONTEXTO.md. Qué archivos
existen: ARCHITECTURE_MAP.md. Errores ya corregidos: lecciones.md.

## Reglas
- ARGUS es un producto general: **no escribas en el código IDs, nombres de
  servidor, canal, bot o usuario**. El servidor y el bot activos los elige el
  usuario en la app.
- **No leas `info/`** (credenciales privadas, gitignored) ni variables de entorno
  con tokens. Los tests nunca usan un token ni la red real: usan el Discord falso
  local (ver plan.md).
- No uses `sudo` ni instales paquetes del sistema.
- Tokens y claves: siempre cifrados (crypto.rs). Nunca en `localStorage` ni en logs.
- UI en inglés por defecto, español con i18n (`t()` y catálogo); mensajes de error igual: el texto
  fuente va en inglés dentro de `t()` (frontend `src/i18n.ts`, backend
  `src-tauri/src/i18n.rs`) y su traducción en el catálogo español. Documentación
  y commits del proyecto siguen en español.

## Entorno
- Rust/cargo 1.99 y Node 22 ya están en el PATH. Ejecuta `cargo` tal cual (sin
  `source`, `export PATH` ni rutas absolutas), desde la raíz con
  `--manifest-path src-tauri/Cargo.toml`.
- Dependencias GTK/WebKit ya instaladas.

## Comandos
```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test  --manifest-path src-tauri/Cargo.toml
cargo fmt   --manifest-path src-tauri/Cargo.toml
npm run build          # tsc && vite build (frontend)
npm run tauri dev      # app en desarrollo (no lo necesitas para el gate)
```

## Convenciones de código
- Rust: ningún `std::sync::MutexGuard` vivo a través de un `.await` (toma el lock
  en un bloque `{ ... }` o clona antes); errores con el alias `DynError` de
  state.rs; logs con `tracing` (el crate `log` no es dependencia).
- `tauri.conf.json`: `plugins.store` / `plugins.log` = null (no poner `{}`).
- TS: un `Option<T>` de Rust se tipa como `T | null`. Iconos Lucide como
  `any`/`Record<number, any>`; variables CSS en estilos inline con
  `as React.CSSProperties`; evita `as const` en arrays pasados como props (TS4104).
