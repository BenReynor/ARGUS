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
