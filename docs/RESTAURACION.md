# Restauración manual de snapshots

En **Backups**, **Restore** abre las opciones y genera la propuesta inicial.
**Generate restore proposal** la regenera al cambiar las opciones. Estas operaciones
solo consultan Discord: no cambian el servidor. La propuesta muestra cada acción,
sus argumentos, lo que no se puede recuperar y una duración aproximada (dos
segundos por acción, ampliable por los límites de Discord).

- **Only missing items**, seleccionado inicialmente: recrea los roles y canales
  borrados y añade los roles de miembros que faltan. Conserva los cambios de los
  elementos existentes, salvo recuperar permisos de roles recreados y devolver
  canales supervivientes a categorías recreadas.
- **Also undo changes**: además restaura ajustes del servidor, roles, canales,
  overwrites y asignaciones de miembros existentes.
- **Delete items not in the snapshot**, desactivado inicialmente: propone borrar
  canales y roles sobrantes. Antes de Apply hay que escribir el nombre del
  servidor y reconocer que el borrado es irreversible. Los canales que presentan
  conflictos de nombre se conservan.

**Apply** ejecuta exactamente la propuesta guardada en memoria. El backend
comprueba el bot y servidor activos, los permisos y la jerarquía. Crea roles en
orden, restaura sus posiciones, crea categorías antes de sus canales, traduce
las referencias del snapshot a los IDs nuevos y recupera los roles de los
miembros presentes. Se reservan primero todas las coincidencias de roles por ID.
Después se buscan por nombre entre los roles aún disponibles: cada rol actual
solo puede corresponder a un rol del snapshot. Si no queda coincidencia se
propone recrearlo, aunque otro rol superviviente tenga el mismo nombre;
si el nombre no es único se indica un conflicto, se conservan los candidatos
y sus asignaciones incluso con deshacer cambios y borrar sobrantes. Si varios
roles perdidos del snapshot compiten por una coincidencia, también se señala
conflicto en vez de asignarla arbitrariamente.
Los canales borrados cuyo nombre ya existe se indican como conflicto.

**Stop** deja terminar la petición en curso. **Continue restore** continúa sin
repetir acciones correctas; ante un error se detiene antes de intentar acciones
dependientes y permite volver a intentar la acción fallida. El progreso y los
IDs creados se conservan durante la sesión de ARGUS, aunque se generen más de
32 propuestas. Generar propuestas nuevas no elimina las anteriores ni su
progreso. Después de cerrar ARGUS se necesita generar otra propuesta.

Discord no permite recuperar mensajes ni reincorporar miembros expulsados o
baneados que no hayan vuelto. Se señalan los miembros ausentes, roles gestionados
que no se pueden recrear, conflictos de nombres y dependencias no disponibles.
Esta restauración de estructura no reproduce emojis, stickers, webhooks ni
baneos; la propuesta lo indica expresamente.

Las pruebas de `snapshots/restore/tests.rs` usan exclusivamente Discord falso:
servidor vaciado, comparación de nombres, orden, permisos efectivos y roles de
miembros; Stop y continuación, error transitorio y reintento, conflictos,
opciones y confirmación reforzada.

Un canal con permisos sin resolver se crea cerrado: un overwrite de @everyone
niega ViewChannel y no se conservan allows que puedan reabrirlo. La propuesta
explica el conflicto. Los permisos de un rol recreado se reponen también en los
canales supervivientes; las categorías recreadas recuperan sus hijos vivos.

## Motor compartido

El botón, MCP y las órdenes de Discord llaman a `snapshots::restore::preview`
y `apply_cancelable`; no hay un segundo algoritmo de emparejamiento.

- MCP: `info_restauracion` sin `snapshot_id` lista snapshots; con ese campo
  genera la propuesta. `undo` y `delete_extra` son false por defecto.
  Tras la aprobación explícita, `aplicar_restauracion` recibe su `id` y
  `confirmation` (nombre del servidor si se borran sobrantes). Cada servidor
  MCP mantiene sus propuestas durante su sesión. Un ID desconocido falla;
  nunca se regenera o aplica una propuesta diferente automáticamente.
- Discord: un autor autorizado escribe `restore <snapshot-id>` o
  `restore <snapshot-id> undo`, con una mención o desde un canal habilitado.
  Requiere permisos de estructura y borrado de roles en las órdenes de Discord.
  Publica todos los pasos, conflictos y duración, sin usar un agente de IA;
  espera siempre el «yes» del autor, incluso con confirmación automática.
  «Stop» usa la cancelación de FASE 17 y detiene el motor entre peticiones.
  El borrado de sobrantes se configura y confirma por nombre en la app o MCP.
