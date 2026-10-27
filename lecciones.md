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

## 2026-10-26 23:37 — Tarea bloqueada: Cargar la configuración guardada al arrancar
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 2: [GATE] ✗ [rust] cargo check falló src/main.rs:98:21: error[E0433]: cannot find module or crate `log` in this scope: use of unresolved module or unlinked crate `log` error: could not compile `argus` (bin "argus") due to 1 previous error; 20 warnings emitted [GATE] ✗ [rust] cargo test falló error: could not compile `argus` (bin "argus" test) due to 1 previous error; 20 warnings emitted error[E0433]: cannot find module or crate `log` in this scope 98 |                     log::error!("No se pudo cargar la configuración guardada: {e}"); warning: unused import: `Error as AeadError` 3 |     Aes256Gcm, Error as AeadError, Nonce, For more information about this error, try `rustc --explain E0433`. [GATE] ✗ build Rust falló (cargo build) 613 | pub async fn export_all_data(state: State<'_, AppState>) -> Result<Vec<u8>, String> { |                              ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` warning: unused variable: `state` --> src/commands.rs:619:26 | 619 | pub async fn import_data(state: State<'_, AppState>, _data: Vec<u8>) -> Result<String, String> { |                          ^^^^^ help: if this is intentional, prefix it with an underscore: `_sta
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.
