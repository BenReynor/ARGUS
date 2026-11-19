# Ficha técnica — ARGUS

Especificación técnica de lo que **existe hoy**. Visión y decisiones:
[`CONTEXTO.md`](../CONTEXTO.md). Orden de trabajo: [`plan.md`](../plan.md).

---

## Stack

| Capa | Tecnología |
|---|---|
| Escritorio | Tauri 2 (CLI 2.12) |
| Frontend | React 18 + TypeScript 5 + Vite 5, estado con Zustand 4, iconos lucide-react |
| Backend | Rust (edición 2021), tokio |
| Discord | twilight-http (REST) / twilight-gateway (eventos en vivo) / twilight-model 0.15 |
| Base de datos | SQLite vía sqlx 0.8 (migraciones en `src-tauri/migrations/`) |
| Cifrado | aes-gcm 0.10 (AES-256-GCM), clave derivada de `machine-uid` |
| Plugins Tauri | store, log, shell, fs, dialog, notification, global-shortcut, clipboard-manager, opener |

## Datos (SQLite, `migrations/001–004`)

| Tabla | Contenido |
|---|---|
| `config` | Config de la app como JSON (clave → valor) |
| `bots` | Bots registrados: id, nombre, token cifrado, guilds permitidos |
| `guilds` | Caché de servidores: id, nombre, icono, miembros |
| `server_maps` | Mapa del servidor (JSON) por guild |
| `templates` | Plantillas de servidor (JSON) |
| `cli_agents` | Agentes CLI configurados (opencode/codex/claude) |
| `vigilance` | Preset y filtros de vigilancia |
| `messages` + `messages_fts` | Historial de mensajes con búsqueda FTS5 |
| `incidents` | Cambios/incidencias por guild (estilo Dédalo) |
| `settings` | Ajustes globales (clave → valor) |
| `guild_settings` | Ajustes propios de cada servidor, por bot (`004`) |

Las migraciones `002` y `003` añaden a `bots` el avatar oficial y el
`application_id` (para el enlace de invitación).

Los tipos Rust (`state.rs`, `db.rs`) y TypeScript (`src/types/index.ts`) están
sincronizados; un `Option<T>` de Rust es `T | null` en TS.

## Comandos Tauri

Los comandos de `src-tauri/src/commands.rs` están registrados en `lib.rs`. Los de
Discord llaman a la API real con twilight (`discord.rs`, `mensajes.rs`) y
comprueban los permisos y la jerarquía del bot antes de actuar; el mapa del
servidor lo mantienen el Gateway (`gateway.rs`, `sync.rs`, `mapa.rs`) y se
guarda en `server_maps` (`persistencia.rs`).
[`AUDITORIA_COMANDOS.md`](AUDITORIA_COMANDOS.md) es la auditoría inicial de la
FASE 0 (histórica).

### Comandos antiguos retirados

`apply_template`, `export_all_data`, `import_data` y `uninstall_wipe` ya no
están expuestos por Tauri ni tienen implementación ficticia. No tenían llamadas
en la interfaz actual. Las copias cifradas `.argusbak` usan
`export_configuration`, `preview_configuration` e `import_configuration`;
la creación desde planes y la restauración de snapshots usan propuestas con
confirmación. No existe una función implementada de borrado total de la app.

## MCP

`argus-mcp` (`src/bin/argus-mcp.rs` + `mcp.rs`) es un servidor MCP por stdio que
usa la misma biblioteca `argus_lib` y la misma BD que la app: 44 herramientas
con los mismos chequeos de permisos. Ver [`MCP.md`](MCP.md).

## Seguridad

- Tokens cifrados con AES-GCM antes de guardarse (`crypto.rs`, con tests). Nunca
  se guardan en texto plano ni en `localStorage`.
- La config y la BD viven en `~/.config/argus/` (Windows: `%APPDATA%\argus\`).
- Nada en el código está atado a un servidor, canal o bot concreto.

## Pruebas

- `cargo test` (en `src-tauri/`): tests unitarios y de comandos (`tests_*.rs`),
  más `tests/mcp_humo.rs`, que lanza `argus-mcp` y lista sus herramientas.
- `tests/integracion_discord.rs`: contra un servidor real, desactivados por
  defecto (`cargo test -- --ignored`, ver [`PRUEBA_REAL.md`](PRUEBA_REAL.md)).
- CI (`.github/workflows/ci.yml`): frontend y `cargo test` en Linux y Windows.
- Las pruebas contra Discord usan un **Discord falso local** (`wiremock`): el
  cliente twilight se apunta a él con `ClientBuilder::proxy`. Ningún test usa red
  ni token real.
- La verificación real la hace el dueño en un servidor de pruebas, siguiendo la
  lista manual al final de cada fase de `plan.md`.

## Verificación (gate de Dédalo)

| Check | Comando |
|---|---|
| Formato Rust | `cargo fmt --check` (en `src-tauri/`) |
| Compilación Rust | `cargo check` |
| Tests Rust | `cargo test` |
| Tipos TS | `tsc --noEmit` |
| Build | `npm run build` y `cargo build` (check propio `99_custom`) |
| Criterio | Juez de Dédalo contra la línea `criterio:` de la tarea |

## Distribución

`npm run tauri build` genera los binarios (`src-tauri/target/release/argus` y
`argus-mcp`) y los paquetes en `src-tauri/target/release/bundle/`: `.deb`,
`.rpm` y `.AppImage` en Linux; `.msi` y `.exe` en Windows (el CI los sube como
artefacto `argus-windows`). Los paquetes llevan `argus-mcp` junto a la app.
Los iconos están en `src-tauri/icons/` y se generan desde `icon.svg`.
En Linux, `scripts/abrir-argus.sh` compila desde `main` y abre la app (ver README).

## Actualizaciones firmadas (Tauri 2)

El plugin updater consulta al arrancar (por defecto) y desde **Settings → About →
Check for updates** el manifiesto
`https://github.com/BenReynor/ARGUS/releases/latest/download/latest.json`.
El ajuste persistente `Config.auto_update` significa «comprobar al arrancar»,
**nunca instalar automáticamente**. Puede desactivarse en About. Las preferencias
antiguas sin ese campo reciben `true`.

El aviso muestra la versión y las notas como texto, sin ejecutar HTML remoto.
Solo al pulsar **Update** se descarga el paquete, Tauri verifica su firma con la
clave pública configurada y lo instala; después ARGUS reinicia. Si la descarga,
la firma o la instalación fallan, se informa y no se reinicia. **Later** cierra el
aviso sin descargar. Windows puede cerrar el proceso durante la instalación.
En Linux solo AppImage admite autoactualización: el `.deb` (y otras instalaciones
fuera de AppImage) muestra las notas y un enlace de descarga a la release para
instalar manualmente con el gestor de paquetes. Sin red, el arranque no muestra
errores; una comprobación manual indica que no ha podido consultar. Timeout: 10 s.

### Identidad de firma del autor

La clave pública de producción está configurada en `plugins.updater.pubkey`.
Su identificador Minisign es **315B1B8F56AEB0CA**; la proporcionó el autor el
2026-11-19. La clave privada queda en GitHub Actions: nunca forma parte del
repositorio, de los instaladores ni de esta configuración.

Los secretos de Actions deben llamarse `TAURI_SIGNING_PRIVATE_KEY` (contenido de
la clave privada cifrada) y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (su contraseña).
El CI firma un archivo pequeño y verifica el resultado con la pública antes de
empezar el empaquetado: si falta un secreto, la contraseña es incorrecta o las
claves no coinciden, la release se detiene sin publicar instaladores.

Conservar una copia cifrada de la clave y su contraseña fuera del PC; perder la
privada impide actualizar las instalaciones que confían en la pública actual.
Para una futura rotación, generar otra pareja con Tauri signer y ejecutar
`node scripts/configurar-clave-updater.mjs /ruta/segura/argus.key.pub`: ese script
solo acepta un archivo **público** `.pub`, valida su formato y actualiza la
configuración. Nunca se debe pasar la clave privada al script.

La instalación en AppImage y Windows de prueba debe comprobarse con dos
versiones publicadas: el aviso no descarga nada hasta pulsar Update, una firma
ajena se rechaza y la app se reinicia solo tras instalar correctamente. Comprobar
también la descarga manual del `.deb`, la opción desactivada y el arranque sin red.
Las instalaciones antiguas necesitan instalar manualmente la primera versión
que incluye este actualizador; no pueden aprender una clave nueva por sí solas.

### CI de release

Al subir un tag `v*`, ambos empaquetadores reciben los dos secretos anteriores.
`createUpdaterArtifacts: true` genera las firmas `.sig` de AppImage y los
instaladores de Windows. PR y ramas generan Windows sin firma mediante
`tauri.unsigned.conf.json`; no necesitan secretos ni publican manifiesto.

Tras pasar los tests, el job de release crea un borrador con notas de GitHub,
reúne instaladores y firmas y ejecuta `scripts/crear-latest.mjs`. Este exige un
AppImage y un instalador NSIS y un MSI con sus respectivas firmas y que la versión del tag
coincida con `tauri.conf.json`, Cargo y los archivos de npm. Una discrepancia
se detecta antes de empaquetar, sin crear una release parcial. Produce `latest.json` con versión, fecha, notas y
las plataformas `linux-x86_64`, `windows-x86_64` (NSIS) y `windows-x86_64-msi`. Se suben todos los paquetes,
firmas, manifiesto y `SHA256SUMS` antes de publicar el borrador como última release.
El manifiesto se genera solo tras verificar criptográficamente cada firma con
la clave pública configurada. Tauri exige también que la versión anunciada
coincida con la versión incluida en el comentario firmado (`requireSignedVersion`).
No se incluyen secretos en el manifiesto ni en los artefactos.

Pruebas sin red: `tests/actualizaciones.test.ts` (precedencia SemVer, aviso,
consentimiento, descarga manual, error de firma, fallo de red y manifiesto) y
`state::tests::update_check_defaults_and_disabled_preference_survive_reload`.
`tests/firmaUpdater.test.ts` genera claves efímeras con Tauri signer, firma un
archivo y comprueba aceptación, rechazo de otra clave, datos modificados y
versión incorrecta; nunca usa la privada real ni la red.
Referencia del protocolo y firma: https://v2.tauri.app/plugin/updater/.
