# Snapshots del servidor

La sección **Backups** pertenece al bot y servidor seleccionados. **Create
snapshot** consulta Discord por REST y solo guarda el resultado si todas las
lecturas e imágenes se completan. Requiere el intent de miembros y acceso a las
listas de baneos y webhooks; los errores se muestran en la vista.

La migración `011_server_snapshots.sql` añade las tablas de snapshots y
programación. El JSON versionado conserva ajustes del servidor, roles, canales y
categorías, overwrites con ID y nombre, miembros y sus roles, emojis y stickers
con sus archivos en base64, nombres y canales de webhooks y baneos. Los ajustes
conservan todos los campos REST del servidor, incluidos banner, splash,
discovery splash y la barra de progreso de boosts. Se descargan e incluyen el
icono del servidor, los banners/splashes y los iconos de roles; los hashes se
conservan junto a sus imágenes. La exportación no depende de que estos recursos
sigan existiendo en Discord. Si una imagen no se descarga, la captura falla sin
guardar una copia parcial; la importación rechaza imágenes ausentes o corruptas.
Los tokens y URLs privadas de los webhooks no se guardan. La lista muestra fecha local, tamaño
del JSON en KiB y contadores de contenido.

Se puede renombrar, borrar con confirmación y exportar a `.argussnapshot`.
Importar añade una copia guardada y valida la versión, estructura y servidor de
origen; no modifica Discord. Los archivos contienen datos de miembros y baneos:
deben guardarse en un lugar privado.

La programación es opcional y se persiste por bot y servidor. Mientras ARGUS
está abierto, comprueba al arrancar y cada minuto si ha transcurrido el intervalo
desde la última copia correcta: 6, 12 (por defecto) o 24 horas. Conserva los N
snapshots automáticos más recientes (14 por defecto, una semana a intervalos de
12 horas; entre 1 y 365), sin borrar copias manuales ni las de otros servidores o
bots. **Create snapshot** sigue disponible en cualquier momento y no retrasa la
próxima copia automática. Un fallo no actualiza la fecha de éxito: se reintenta en la próxima comprobación y se
muestra el último error en Backups. No requiere mantener seleccionado ese bot o
servidor. La migración `014_snapshot_intervals.sql` conserva la activación, la
retención elegida, el último éxito y el último error de las programaciones
existentes, con su frecuencia anterior de 24 horas. Las nuevas usan 12 horas y
14 copias por defecto.

Pruebas: `cargo test --manifest-path src-tauri/Cargo.toml snapshots::`. Se usa
exclusivamente Discord falso local, incluidas las imágenes de CDN; también se
comprueban persistencia, aislamiento, archivos, paginación y retención.
