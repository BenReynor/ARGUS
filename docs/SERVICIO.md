# Servicio sin ventana (opcional)

Proceso de ARGUS sin interfaz (`argus --service`) que mantiene conectado el bot
activo. **Está desactivado por defecto** y no sustituye el uso normal de escritorio.
Necesita un equipo encendido y con conexión.

## Controles (Ajustes → Servicio en segundo plano)

- **Habilitar el servicio**: guarda el ajuste `service_enabled`. Sin él, el proceso
  `--service` termina al arrancar y *Iniciar* está deshabilitado.
- **Iniciar / Detener**: *Iniciar* lanza `argus --service` como proceso del usuario;
  *Detener* envía una orden de parada ordenada por el canal local.
- **Estado y último error**: detenido, en marcha con Gateway conectado/caído o sin
  informar. El último error (p. ej. el bot ya está conectado desde la interfaz)
  se guarda en `service-last-error.json` y se borra al conectar bien.
- **Iniciar el servicio al iniciar sesión**: desactivado por defecto.
- **Diagnosticar servicio**: distingue *servicio detenido*, *interfaz desconectada
  del servicio* (el canal existe pero no responde o rechaza la autenticación) y
  *Gateway caído* (servicio e interfaz bien, bot desconectado).

## Qué queda activo

| Acción | Interfaz | Servicio | Bot conectado |
|---|---|---|---|
| Cerrar la ventana (con «minimizar a la bandeja») | oculta en la bandeja | sin cambios | sí, si lo conectó la interfaz |
| Cerrar la ventana (sin bandeja) / Salir | termina | sin cambios | solo si lo conectó el servicio |
| Detener el servicio | sin cambios | termina | no (salvo que la interfaz lo conecte) |

Solo hay **una conexión al Gateway por bot**: un lock de archivo lo impide entre
interfaz y servicio. Si la interfaz ya tiene el bot conectado, el servicio fallará y
mostrará ese error.

## Arranque con el sistema

Entrada **por usuario, sin privilegios y sin servicio del sistema**:

- Linux: `~/.config/autostart/argus-service.desktop` (`Exec="<argus>" --service`).
- Windows: valor `argus-service` en `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
- Otros sistemas: no disponible.

Desactivar la casilla borra la entrada. No se instala systemd de sistema, ni
Servicios de Windows, ni tareas programadas, ni se pide administrador.
Si ARGUS se mueve de carpeta, desactiva y vuelve a activar la opción.

## Seguridad

El canal local es TCP solo en `127.0.0.1`, con secreto en `service-channel.json`
(0600 en Linux), autenticación por desafío y sin tokens de bot.

## Pruebas

Los tests automáticos usan carpetas temporales y un canal en el mismo proceso: no
lanzan el servicio, no tocan `~/.config/autostart` ni el registro.

## Checklist manual (en el servidor de pruebas)

### Linux
1. Ajustes → Servicio: con todo apagado, estado «Detenido»; *Iniciar* deshabilitado.
2. Habilitar → *Iniciar*. Con la interfaz **sin** bot conectado, el estado pasa a
   «En marcha · Gateway conectado» y `ps` muestra `argus --service`.
3. *Diagnosticar servicio*: tres comprobaciones correctas.
4. Conectar el bot desde la interfaz: debe rechazarse por el lock (una conexión).
5. Cerrar y reabrir la interfaz: el servicio sigue y el estado se recupera.
6. *Detener*: el proceso desaparece y el diagnóstico dice «servicio detenido».
7. Matar el proceso (`kill`) y diagnosticar: «servicio detenido» (archivo de canal obsoleto).
8. Activar «al iniciar sesión»: existe `~/.config/autostart/argus-service.desktop`.
   Cerrar sesión y entrar: el servicio arranca solo. Desactivar: el archivo desaparece.
9. Token inválido o bot ya conectado: «Último error» muestra la causa.

### Windows
1. Mismos pasos 1–7; el servicio no abre ventana de consola (Administrador de tareas).
2. Activar «al iniciar sesión»: `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v argus-service`
   devuelve el valor; tras reiniciar sesión el servicio arranca. Desactivar: el valor se borra.
3. Comprobar que no se pidió UAC en ningún paso y que no hay un servicio en `services.msc`.
4. `service-channel.json` está en `%APPDATA%\argus\` (hereda los permisos del perfil).

Windows solo está probado en CI: esta checklist es la primera comprobación real.
