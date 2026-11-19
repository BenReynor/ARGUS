# Protección ante borrados masivos

Mientras funciona el Gateway, ARGUS cuenta los borrados de canales (incluidas
categorías) y roles que conoce en el mapa de cada servidor. Por defecto, cinco
borrados combinados en sesenta segundos activan la protección. Los duplicados,
los hilos y los objetos desconocidos no cuentan. Los ajustes se guardan por bot
y servidor en Ajustes → Este servidor: protección, umbral (2–100), ventana
(1–3600 segundos) y retirada automática de roles (desactivada por defecto).

El detector conserva el mapa anterior al primer borrado de la ráfaga. Cuando
caduca un borrado, también descarta su copia previa; conserva la copia anterior
al primer borrado aún vigente, incluidos los cambios legítimos intermedios.
Al alcanzar el umbral lo guarda como snapshot, antes de consultar auditoría, sin
intentar reconstruir el pasado a partir de REST. Contiene la información
conocida de canales, categorías, overwrites, roles y asignaciones a miembros.
No contiene mensajes, imágenes ni ajustes avanzados que el mapa no conoce.
Los snapshots de emergencia se conservan fuera de la rotación diaria automática.
Cada nuevo grupo que alcance el umbral puede producir otra copia; la primera
copia permite recuperar el estado anterior al ataque completo.

La auditoría se consulta hasta tres veces porque Discord puede publicarla con
retraso. Solo se atribuye un borrado si coinciden objetivo, tipo de acción y
fecha (margen de quince segundos respecto al evento recibido). Sin acceso a
View Audit Log o si Discord sigue sin publicarlo se informa de responsable
desconocido: no se sanciona a nadie sin esa correlación. Se conserva el snapshot
aunque la auditoría falle. Se muestra una notificación nativa, un aviso en Logs
y una alerta persistente por bot/servidor con Restore. Las alertas sobreviven al
reinicio y se recuperan al seleccionar el servidor.

Restore abre `SnapshotRestore` y genera la propuesta mediante
`snapshots::restore::preview`, igual que la restauración manual. El usuario
revisa y pulsa Apply; usa los mismos comandos Apply/Stop y el mismo motor.
El detector no implementa acciones de restauración.

Si el usuario activa la retirada de roles, ARGUS consulta los roles actuales
del responsable identificado y retira los editables. Omite roles gestionados y
al propio bot activo. Discord exige Manage Roles y una jerarquía suficiente;
los errores quedan visibles en la alerta. La notificación se muestra antes de
esta mitigación para no esperar a sus límites de peticiones.

## Verificación

Los tests `destruction::tests` usan eventos JSON del Gateway y Discord falso
local, con SQLite en memoria. Cubren mapa previo, mezcla de borrados,
expiración, duplicados, aislamiento de servidor, desactivación, configuración,
correlación de auditoría, persistencia aun sin permisos, mitigación opcional y
propuesta del motor de restauración existente sin escrituras.

Comprobación manual del dueño: en su servidor de pruebas, con permiso de
auditoría y las notificaciones del escritorio permitidas, borrar hasta alcanzar
el umbral y comprobar la notificación nativa y la identidad indicada en Logs.
Restore debe abrir una propuesta y esperar Apply antes de modificar Discord.
