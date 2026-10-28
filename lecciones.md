# Lecciones aprendidas — memoria del proyecto

Cada lección evita repetir un error ya corregido (corrección única de errores).
El agente lee este archivo antes de cada tarea, así que escribe aquí TODO lo que
no quieras que vuelva a pasar. El dedalo añade una entrada automática cuando
una tarea queda bloqueada ([B]).

Formato recomendado:
- Síntoma: qué falló.
- Causa: por qué falló.
- Prevención: qué hacer para no repetirlo (o qué cambiar en plan.md).
- Prueba: cómo verificar que está resuelto (test/check).

Añade las tuyas con el botón "+ Nueva lección" de la pestaña Memoria, o a mano.
- Síntoma: `cargo` falla con "could not find Cargo.toml" desde la raíz.
  Causa: es una app Tauri; el crate Rust vive en `src-tauri/`.
  Prevención: usa `cargo check --manifest-path src-tauri/Cargo.toml` (igual para fmt/test) o trabaja dentro de `src-tauri/`.
  Prueba: el gate de Dédalo corre cargo fmt/check/test en src-tauri.

- Síntoma: "future cannot be sent between threads safely" en un `#[tauri::command]`.
  Causa: un `std::sync::MutexGuard` (o un `Box<dyn Error>` sin `Send + Sync`) sigue vivo a través de un `.await`; `drop(guard)` no basta.
  Prevención: toma el lock dentro de un bloque `{ ... }` o clona los datos antes del `.await`; usa el alias `DynError` de state.rs.
  Prueba: `cargo check` sin errores.

- Síntoma: "cannot find module or crate `log`" al registrar un error.
  Causa: `log` no está en Cargo.toml; el proyecto usa `tracing`.
  Prevención: usa `tracing::error!` / `tracing::info!`; no añadas dependencias sin que la tarea lo pida.
  Prueba: `cargo check --manifest-path src-tauri/Cargo.toml`.

## 2026-10-26 23:57 — Tarea bloqueada: Sincronizar los tipos TS de `src/types` con los tipos Rust
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: revisor_fail en la iteración 5: El tipo de `Config` coincide con las structs de `state.rs` y `db.rs`, y los valores por defecto del store igualan a los de Rust. Sin embargo, `SettingsView.tsx:38` llama a `saveConfig(formData)` con un objeto de 4 campos y `saveConfig` exige `Config` completo (`src/api/tauri.ts:83`), así que `tsc` (y con él `npm run build`) fallaría. No pude ejecutar `tsc` porque Bash está deshabilitado en esta sesión; la conclusión viene de leer el código. Además, en runtime `save_config` recibiría un JSON incompleto para la struct Rust `Config`, que no tiene `serde(default)` en la mayoría de campos.
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

- Síntoma: la app se cerraba al arrancar ("state() called before manage() for Pool<Sqlite>"); el gate estaba en verde.
  Causa: nadie abría ni registraba el pool de SQLite (`app.manage`) y `get_pool` lo pedía igual; ningún test arranca la app completa.
  Prevención: todo estado que se pida con `app.state::<T>()` se registra con `.manage()`/`app.manage()` antes de usarlo; usa `try_state` y devuelve un error en vez de panic; no crees runtimes de tokio propios en `setup` (usa `tauri::async_runtime`).
  Prueba: `db::abrir_bd` tiene test; tras tocar main.rs, arrancar la app (`npm run tauri dev`) y comprobar que no se cierra.

## 2026-10-28 10:22 — Tarea bloqueada: Backend: ajustes del servidor y registro de auditoría
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: agente_fallo en la iteración 5: claude salió con rc=1 sin cambios
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-10-28 10:23 — Tarea bloqueada: Backend: mensajes, invitaciones y webhooks
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: agente_fallo en la iteración 6: claude salió con rc=1 sin cambios
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-10-28 10:23 — Tarea bloqueada: Interfaz: diálogo de confirmación y gestión de canales
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: agente_fallo en la iteración 7: claude salió con rc=1 sin cambios
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-10-28 10:23 — Tarea bloqueada: Interfaz: gestión de roles
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: agente_fallo en la iteración 8: claude salió con rc=1 sin cambios
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-10-28 10:23 — Tarea bloqueada: Interfaz: permisos de canal
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: agente_fallo en la iteración 9: claude salió con rc=1 sin cambios
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-10-28 10:23 — Tarea bloqueada: Interfaz: moderación de miembros
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: agente_fallo en la iteración 10: claude salió con rc=1 sin cambios
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-10-28 10:23 — Tarea bloqueada: Interfaz: ajustes del servidor y auditoría
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: agente_fallo en la iteración 11: claude salió con rc=1 sin cambios
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-10-28 10:23 — Tarea bloqueada: Interfaz: mensajes, invitaciones y webhooks
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: agente_fallo en la iteración 12: claude salió con rc=1 sin cambios
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.
