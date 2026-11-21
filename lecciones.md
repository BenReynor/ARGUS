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

## 2026-11-19 — Actualizaciones firmadas desbloqueadas
- Se recuperó la implementación bloqueada y se instaló la clave pública real del
  autor (Minisign 315B1B8F56AEB0CA). La privada y su contraseña quedan en Actions.
- El CI prueba la pareja de claves antes de empaquetar y verifica cada artefacto
  antes de generar latest.json. La versión también forma parte de la firma.
- Para compilar paquetes locales sin la privada, usar tauri.unsigned.conf.json;
  --no-bundle y el modo de desarrollo no requieren firma.
- Pruebas sin red: firma real de un archivo con claves efímeras, rechazo de otra
  clave/archivo/versión, consentimiento del updater y preferencias persistentes.
- La primera instalación real de AppImage/Windows sigue siendo una comprobación
  manual de release; no confundir la prueba con claves efímeras con haber usado
  o inspeccionado los secretos reales del autor.

## 2026-11-19 — Retirados los comandos antiguos con éxito ficticio
- `apply_template`, `export_all_data`, `import_data` y `uninstall_wipe` seguían
  registrados en Tauri aunque no ejecutaban la operación ni tenían llamadas
  desde la interfaz. Se retiraron sus handlers, funciones y textos de éxito.
- Una función pendiente no debe devolver éxito. Antes de reintroducirla,
  implementar el comportamiento real y la confirmación que corresponda.
- Para copias portátiles usar la API de `backup.rs`, que exige contraseña,
  vista previa y confirmación. No reactivar los antiguos endpoints ZIP.

## 2026-11-19 13:50 — Tarea bloqueada: Historial de operaciones con estados compartidos
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 2: [GATE] ✗ juez: criterio NO cumplido (Nunca se registran los estados verificando ni correcto; todas las acciones aceptadas quedan como no verificadas.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 14:06 — Tarea bloqueada: Historial de operaciones con estados compartidos
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 1: [GATE] ✗ juez: criterio NO cumplido (Una operación cuyos pasos fallan se registra como parcial; además, se omiten destinos como message_ids.) | 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                            ^^^^^^ warning: unused import: `Manager` --> src/backup.rs:8:24 8 | use tauri::{AppHandle, Manager, State}; |                        ^^^^^^^
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 14:19 — Tarea bloqueada: Historial de operaciones con estados compartidos
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 1: [GATE] ✗ juez: criterio NO cumplido (El servidor fijado no se aplica a acciones sin guild_id: pueden ejecutarse en otro servidor que el registrado.) | 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                            ^^^^^^ warning: unused import: `Manager` --> src/backup.rs:8:24 8 | use tauri::{AppHandle, Manager, State}; |                        ^^^^^^^
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 14:52 — Tarea bloqueada: Comprobar el resultado tras construir o modificar el servidor
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 1: [GATE] ✗ juez: criterio NO cumplido (El motor marca éxito al aceptar la acción, sin verificar el resultado ni reconciliar timeouts ambiguos.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 14:58 — Tarea bloqueada: Panel visual de resultados y avisos consistentes
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 2: [GATE] ✗ juez: criterio NO cumplido (El panel solo registra avisos: faltan historial de operaciones, resumen de pasos, progreso y pruebas requeridas.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 15:07 — Tarea bloqueada: Motor de comprobaciones sin efectos sobre Discord
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 3: [GATE] ✗ juez: criterio NO cumplido (Lee el token del bot para consultar permisos y faltan pruebas con agentes, MCP y procesos colgados.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 15:15 — Tarea bloqueada: Pantalla de diagnóstico con acciones para resolver problemas
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 4: [GATE] ✗ juez: criterio NO cumplido (Diagnóstico define estados propios y no reutiliza los estados compartidos de la fase 20.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 15:27 — Tarea bloqueada: Guardar y organizar las plantillas propias
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 6: [GATE] ✗ juez: criterio NO cumplido (Solo permite editar metadatos; no permite editar la estructura de una plantilla guardada.) | 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                            ^^^^^^ warning: unused import: `Manager` --> src/backup.rs:8:24 8 | use tauri::{AppHandle, Manager, State}; |                        ^^^^^^^
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 15:37 — Corregida en reintento: Importar y exportar plantillas de la biblioteca
- Síntoma: El gate falló antes de pasar: [GATE] ✗ juez: criterio NO cumplido (No existe el flujo de importación y exportación de plantillas ni sus validaciones y pruebas.); [GATE] ✗ criterio: 98_criterio.sh falló
- Causa: [GATE] ✗ juez: criterio NO cumplido (No existe el flujo de importación y exportación de plantillas ni sus validaciones y pruebas.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Evita exactamente ese fallo y compruébalo con `.dedalo/estado/gate --solo 98_criterio.sh` antes de terminar (98_criterio.sh).

## 2026-11-19 15:42 — Tarea bloqueada: Aplicar una plantilla mediante el motor de propuestas
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 8: [GATE] ✗ juez: criterio NO cumplido (Faltan la aplicación de plantillas mediante planes, la vista previa de destino y los tests exigidos.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 15:48 — Tarea bloqueada: Usar la biblioteca desde la IA
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 9: [GATE] ✗ juez: criterio NO cumplido (El asistente solo recibe la plantilla seleccionada; no puede listar ni consultar por sí mismo las disponibles.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 15:57 — Tarea bloqueada: Calcular accesos efectivos y detectar hallazgos comprobables
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 10: [GATE] ✗ juez: criterio NO cumplido (Omite permisos sensibles como Manage Messages y no destaca los concedidos por overwrites de canal.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 16:01 — Tarea bloqueada: Informe visual y correcciones aprobadas
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 11: [GATE] ✗ juez: criterio NO cumplido (Falta la vista de hallazgos y el flujo de corrección, aprobación y comprobación posterior.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 16:06 — Corregida en reintento: Explicar la auditoría a la IA sin concederle escrituras
- Síntoma: El gate falló antes de pasar: [GATE] ✗ juez: criterio NO cumplido (No existe la herramienta MCP para consultar el informe ni sus pruebas de lectura sin cambios.); [GATE] ✗ criterio: 98_criterio.sh falló
- Causa: [GATE] ✗ juez: criterio NO cumplido (No existe la herramienta MCP para consultar el informe ni sus pruebas de lectura sin cambios.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Evita exactamente ese fallo y compruébalo con `.dedalo/estado/gate --solo 98_criterio.sh` antes de terminar (98_criterio.sh).

## 2026-11-19 16:11 — Tarea bloqueada: Catálogo de conectores y capacidades reales
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 13: [GATE] ✗ juez: criterio NO cumplido (El catálogo no está integrado: no permite habilitar varios conectores ni probarlos desde diagnóstico.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 16:14 — Tarea bloqueada: Vincular cuentas y permisos sin compartir credenciales con la IA
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 14: [GATE] ✗ juez: criterio NO cumplido (Solo hay un catálogo sin conectores implementados; faltan autorización, credenciales, desconexión y pruebas.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 16:17 — Tarea bloqueada: Lectura de varios MCP con aislamiento entre servicios
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 15: [GATE] ✗ juez: criterio NO cumplido (Falta la capa multiconector con límites y aislamiento; tampoco existen los tests con dos MCP falsos.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 16:20 — Tarea bloqueada: Propuestas para configurar otros bots y comprobar su resultado
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 16: [GATE] ✗ juez: criterio NO cumplido (No existe el adaptador ni el flujo de propuestas, ejecución aprobada y verificación para otros bots.) warning: unused variable: `state` --> src/commands.rs:2346:5 | 2346 |     state: State<'_, AppState>, |     ^^^^^ help: if this is intentional, prefix it with an underscore: `_state` --> src/commands.rs:2369:5 2369 |     state: State<'_, AppState>, --> src/commands.rs:2532:5 2532 |     state: State<'_, AppState>,
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 16:31 — Tarea bloqueada: Motor de servicio opcional y una sola conexión por bot
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 19: [GATE] ✗ juez: criterio NO cumplido (No existe servicio independiente ni canal local; la interfaz indica que aún no está disponible.) |     ^^^^^^^^^^^^^ | = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default warning: use of deprecated function `base64::encode`: Use Engine::encode --> src/crypto.rs:46:16 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                            ^^^^^^
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 16:36 — Tarea bloqueada: Arranque opcional y diagnóstico del servicio
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 20: [GATE] ✗ juez: criterio NO cumplido (Faltan el servicio independiente, sus controles, el diagnóstico diferenciado y las pruebas correspondientes.) |     ^^^^^^^^^^^^^ | = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default warning: use of deprecated function `base64::encode`: Use Engine::encode --> src/crypto.rs:46:16 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                            ^^^^^^
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-19 17:39 — Tarea bloqueada: Historial de operaciones con estados compartidos
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 1: [GATE] ✗ juez: criterio NO cumplido (La regresión cambia el servidor antes de la propuesta; durante ella vuelve a fijar el mismo servidor.) |     ^^^^^^^^^^^^^ | = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default warning: use of deprecated function `base64::encode`: Use Engine::encode --> src/crypto.rs:46:16 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                            ^^^^^^
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-20 22:04 — Tarea bloqueada: Verificar canales y categorías tras crearlos o editarlos
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 2: [GATE] ✗ juez: criterio NO cumplido (La verificación no compara el tipo solicitado: puede marcar como verificado un canal creado con tipo incorrecto.) |     ^^^^^^^^^^^^^ | = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default warning: use of deprecated function `base64::encode`: Use Engine::encode --> src/crypto.rs:46:16 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                            ^^^^^^
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-20 22:18 — Tarea bloqueada: Verificar roles, posiciones y permisos
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: gate_fail en la iteración 1: [GATE] ✗ juez: criterio NO cumplido (Falta un test de cambio de servidor a mitad de una operación de roles o permisos de canal.) |     ^^^^^^^^^^^^^ | = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default warning: use of deprecated function `base64::encode`: Use Engine::encode --> src/crypto.rs:46:16 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                            ^^^^^^
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.

## 2026-11-20 22:49 — Corregida en reintento: Diagnóstico de agentes y MCP
- Síntoma: El gate falló antes de pasar: [GATE] ✗ juez: criterio NO cumplido (La sesión de MCP siempre queda sin comprobar; no se determina si está disponible.); [GATE] ✗ criterio: 98_criterio.sh falló
- Causa: [GATE] ✗ juez: criterio NO cumplido (La sesión de MCP siempre queda sin comprobar; no se determina si está disponible.) |     ^^^^^^^^^^^^^ | = note: `#[warn(unused_imports)]` (part of `#[warn(unused)]`) on by default warning: use of deprecated function `base64::encode`: Use Engine::encode --> src/crypto.rs:46:16 46 |     Ok(base64::encode(combined)) |                ^^^^^^ = note: `#[warn(deprecated)]` on by default warning: use of deprecated function `base64::decode`: Use Engine::decode --> src/crypto.rs:52:28 52 |     let combined = base64::decode(ciphertext_b64) |                          
- Prevención: Evita exactamente ese fallo y compruébalo con `.dedalo/estado/gate --solo 98_criterio.sh` antes de terminar (98_criterio.sh).

## 2026-11-20 23:01 — Tarea bloqueada: Pantalla de diagnóstico
- Síntoma: El agente no cumplió el criterio tras 2 intentos.
- Causa: revisor_fail en la iteración 7: Los cambios cubren diagnóstico accesible, estados compartidos, progreso, reintentos y pruebas de resultados y cancelación; no veo secretos ni cambios ajenos evidentes. Hay un error en la vista: al fallar el diagnóstico global, los estados se marcan `unverified`, pero el botón de cada comprobación solo llama a `diagnostico_agentes`; reintentar comprobaciones de configuración, base de datos, keyring, Gateway o permisos las consulta al grupo incorrecto y no las actualiza.
- Prevención: Reformular la tarea en plan.md, dividirla en pasos más pequeños o revisar los criterios.
