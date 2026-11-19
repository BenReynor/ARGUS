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

## 2026-10-30 01:04 — Tarea bloqueada: Traducir Canales
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: revisor_fail en la iteración 4: La tarea no nombra skill, así que no cargué ninguna. El diff cumple casi todo el criterio: los tres componentes usan `t()` para la UI, los permisos pasan por `permisos.ts` con nombres oficiales, y `es.ts` cubre las claves nuevas. Un grep de acentos en los tres componentes solo encuentra comentarios. Falta un texto visible en español: `ChannelsView.tsx` sigue con `addLog('[CANALES] ' + mensaje)`, y ese log se muestra en la vista de Logs aunque la interfaz esté en inglés. `ActividadCanal` sí traduce sus etiquetas de log, así que es un descuido del mismo tipo. Corregirlo es pequeño: envolverlo en `t('[CHANNELS] {message}', { message: mensaje })` y añadir la entrada en `es.ts`.
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-17 22:40 — Tarea bloqueada: Documentación de seguridad
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 11: [GATE] ✗ juez: criterio NO cumplido (Falta SECURITY.md en inglés en la raíz; la versión inglesa está en docs/SECURITY.en.md.) warning: unused variable: `state` --> src/commands.rs:2615:29 | 2615 | async fn export_all_data_en(state: State<'_, AppState>) -> Result<Vec<u8>, String> { |                             ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2627:25 2627 | async fn import_data_en(state: State<'_, AppState>, _data: Vec<u8>) -> Result<String, String> { |                         ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2639:28 2639 | async fn uninstall_wipe_en(state: State<'_, AppState>) -> Result<String, String> { |                            ^^^^^ help: if this is intentional, prefix it with an underscore: `_state`
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-18 18:00 — Tarea bloqueada: Actualizaciones automáticas firmadas
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 1: [GATE] ✗ juez: criterio NO cumplido (tauri.conf.json conserva una clave pública provisional sin privada disponible; el propio CI bloquea la release.) warning: unused variable: `state` --> src/commands.rs:2369:5 | 2369 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2393:5 2393 |     state: State<'_, AppState>, --> src/commands.rs:2564:5 2564 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-18 18:40 — Tarea bloqueada: Guardar snapshots del servidor
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: revisor_fail en la iteración 3: La importación acepta imágenes corruptas si contienen base64 válido: `validate_image()` solo comprueba que los bytes no estén vacíos, sin validar PNG, GIF o JSON. Esto contradice la garantía documentada; los tests incluso usan `fixture-image`, que no es una imagen válida, y no detectan el problema.
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-18 19:14 — Tarea bloqueada: Restaurar manualmente
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: revisor_fail en la iteración 2: Los roles con nombres ambiguos se omiten sin reservar sus IDs; con `delete_extra`, todos esos candidatos se proponen para borrar aunque puedan corresponder al snapshot. Con `undo`, también se eliminan sus asignaciones de miembros porque no entraron en `role_map`. La ambigüedad debería conservarlos y señalar el conflicto.
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-18 19:32 — Tarea bloqueada: Detectar destrucción masiva y avisar
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: revisor_fail en la iteración 3: Restore no recupera la jerarquía cuando se elimina una categoría: recrea la categoría, pero omite reasignar los canales supervivientes, que Discord deja sin padre. La rama de canales existentes solo restaura overwrites; aplicar la propuesta deja esos canales fuera de su categoría original.
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-18 20:16 — Tarea bloqueada: Restaurar manualmente
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: revisor_fail en la iteración 1: El emparejamiento por nombre no distingue roles gestionados de roles editables: un rol normal borrado puede vincularse a un rol de integración con el mismo nombre. Así no se recrea el rol perdido y Apply intenta modificar o asignar un rol gestionado, operaciones que Discord rechaza, deteniendo la restauración.
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-18 23:25 — Corregida en reintento: Expulsar a todos los miembros
- Síntoma: El gate falló antes de pasar: [GATE] ✗ juez: criterio NO cumplido (No protege al ejecutor real: acepta como actor cualquier ID del servidor, pudiendo expulsar a quien lanza la acción.); [GATE] ✗ criterio: 98_criterio.sh falló
- Causa: [GATE] ✗ juez: criterio NO cumplido (No protege al ejecutor real: acepta como actor cualquier ID del servidor, pudiendo expulsar a quien lanza la acción.) warning: unused variable: `state` --> src/commands.rs:2617:29 | 2617 | async fn export_all_data_en(state: State<'_, AppState>) -> Result<Vec<u8>, String> { |                             ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2629:25 2629 | async fn import_data_en(state: State<'_, AppState>, _data: Vec<u8>) -> Result<String, String> { |                         ^^^^^ help: if this is inte
- Prevención: Evita exactamente ese fallo y compruébalo con `.dedalo/estado/gate --solo 98_criterio.sh` antes de terminar (98_criterio.sh).
