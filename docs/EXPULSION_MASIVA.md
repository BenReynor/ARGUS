# Expulsión masiva desde Backups

La acción **Kick all members** solo existe en la ventana de ARGUS. No está
registrada en el MCP, las propuestas de IA ni las órdenes de Discord.

1. Indica **tu propio ID de miembro de Discord** y pulsa **Send verification
   code**. El bot envía un código privado por DM a esa cuenta. Escríbelo en
   ARGUS para demostrar que la controlas. Caduca a los cinco minutos y queda
   vinculado al bot y servidor; no se guarda en disco ni se muestra en logs.
   Si el DM falla o el código no coincide, no se permite preparar la acción.
   El backend fija el ejecutor verificado: no acepta otro ID al preparar ni
   cambiarlo al confirmar. El código permite revisar los roles durante esos
   cinco minutos; una vez confirmada, la protección dura toda la operación.
2. Pulsa **Review member removal**. La vista previa excluye al dueño, al bot,
   al ejecutor verificado y a los roles con Administrator. Puedes cancelar el diálogo,
   cambiar los roles excluidos y volver a revisar. Excluir @everyone conserva
   a todos.
3. El primer diálogo muestra cuántos miembros se expulsarán y el servidor.
   El segundo pide escribir exactamente su nombre.
4. Antes de la primera expulsión, ARGUS vuelve a leer el servidor. Si cambia la
   vista previa, exige revisarla de nuevo. Después guarda un snapshot completo
   automático en SQLite. Si la captura o el guardado falla, no expulsa a nadie.
5. El progreso muestra **N of M** (**N de M** en español). **Stop** evita la
   próxima expulsión; una petición ya en curso puede terminar. Cambiar de vista
   no cancela el trabajo: al volver al mismo bot y servidor se recupera su estado.

Twilight comparte el control de límites y sus reintentos entre peticiones;
además se espacia cada miembro un segundo. Antes de expulsar se vuelven a
comprobar los roles excluidos. Discord sigue imponiendo los permisos y la
jerarquía del bot. Los miembros con roles iguales o superiores al bot se
muestran como omitidos, junto con los errores individuales al terminar.

Un snapshot no puede volver a meter a miembros expulsados. La lista de ausentes
y las invitaciones de Backups sirven para facilitar su regreso voluntario.
Los resultados de la operación se mantienen durante la sesión de ARGUS; el
snapshot queda guardado en la base de datos.

Pruebas sin tokens reales: `cargo test --manifest-path src-tauri/Cargo.toml mass_kick`.
La comprobación visual final en un servidor de pruebas corresponde al dueño.
