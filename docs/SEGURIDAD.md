# Seguridad de ARGUS

[English](../SECURITY.md)

ARGUS administra bots propios mediante la API oficial de Discord. Sus controles
reducen el riesgo de una orden equivocada o no autorizada; no garantizan que una
decisión del modelo sea correcta ni protegen un PC comprometido.

## Qué protege ARGUS

### Tokens y clave local

Los tokens se guardan cifrados con AES-256-GCM en SQLite, en
`~/.config/argus/` (Linux) o `%APPDATA%\argus\` (Windows). No se guardan en
`localStorage` ni se incluyen en logs o respuestas MCP. El agente no recibe el
token del bot: el backend y `argus-mcp` lo descifran cuando necesitan usarlo.

La clave maestra aleatoria de 32 bytes se guarda en el llavero del sistema
(Secret Service en Linux, Credential Manager en Windows). La app y el MCP
comparten esa clave. Los tokens antiguos se migran en una transacción desde el
cifrado derivado del machine-id, sin perder bots. Si no hay llavero disponible,
los nuevos tokens conservan el cifrado antiguo y Settings muestra un aviso.
Ese mecanismo antiguo ofrece menos protección: el identificador de la máquina
no es un secreto. Desbloquea el llavero y reinicia para migrar. Los tokens ya
migrados requieren su clave original; copiar solo la BD no recupera su acceso.

El cifrado protege los tokens almacenados, no toda la base de datos ni los
planes, conversaciones y logs. Un proceso con acceso a tu sesión, llavero o
memoria puede comprometerlos. Protege también los archivos locales y sus copias.

### Órdenes desde Discord

Las órdenes están desactivadas por defecto. Al activarlas, solo se aceptan de
los usuarios de la lista específica de órdenes (`chat_ordenes_usuarios`); una
lista vacía no autoriza a nadie. Poder conversar con el bot no autoriza a darle
órdenes. Los canales y usuarios del chat se configuran por servidor.

El constructor recibe el mensaje original del autor autorizado, con la marca
`ORDEN[...]` como pista, sin los mensajes previos de otros usuarios. El prompt
del chat trata ese historial como datos no fiables, no como instrucciones.
Estos controles reducen la superficie de prompt injection, pero no la eliminan.

En **Discord replies → Allow from Discord** se eligen los grupos de acciones
permitidos. Por defecto se permite todo salvo borrar canales/roles y banear.
Una acción no permitida rechaza la propuesta con un motivo; los permisos de
acciones se vuelven a comprobar al confirmar. Estos ajustes son para órdenes
de Discord, no restringen las acciones de la app ni el MCP externo.

### Confirmación y ejecución

En el asistente integrado, el agente solo dispone de herramientas MCP de
lectura. Para construir o moderar devuelve una propuesta estructurada: el
usuario la revisa y pulsa **Apply**; ARGUS ejecuta las acciones aprobadas,
ordenadas y validadas, con los chequeos de permisos y jerarquía de Discord.

Desde Discord, la confirmación está activada por defecto. Solo el autor de la
propuesta puede confirmarla en el mismo servidor y canal; las propuestas
caducadas no se aplican y una confirmación no ejecuta dos veces la misma.
Se puede descartar con «no» o «cancelar».

Aunque se desactive la confirmación, las acciones irreversibles siempre la
exigen: borrar canales, roles, mensajes, invitaciones o webhooks; banear o
expulsar; y enviar mensajes, DMs o avisos que el destinatario ya puede haber
recibido. La propuesta avisa de estas acciones. Las demás pueden aplicarse
directamente si desactivas la confirmación.

### Límites y alcance

Las órdenes de texto y de archivo tienen un máximo de **50 acciones** por
defecto, configurable entre **1 y 500** (`ordenes_max_acciones`). Una propuesta
que lo supere se rechaza antes de publicarse o ejecutarse. Este tope no se aplica
a Build/Moderation en la app. Hay esperas por usuario y control de concurrencia;
no constituyen un presupuesto de uso o de coste del proveedor.

Los permisos efectivos y la jerarquía del bot limitan lo que Discord permite.
Una ejecución puede fallar parcialmente: la confirmación no equivale a una
transacción con rollback y no recupera contenido borrado.

## Riesgos que quedan

- **Prompt injection:** mensajes, nombres, descripciones, planes o archivos
  pueden contener instrucciones maliciosas. El aislamiento y la validación no
  hacen infalible al modelo. Revisa acciones y argumentos, no solo su resumen.
- **Datos enviados al modelo:** el proveedor del agente/modelo recibe el
  contexto que se le envía, incluidos los datos que el agente lee mediante sus
  herramientas. ARGUS no tiene telemetría propia; usar IA sí puede transmitir
  mensajes, miembros, planes y conversaciones al proveedor elegido. Revisa qué
  contexto compartes y las condiciones de ese proveedor antes de usar datos
  sensibles. El cifrado local de tokens no cifra ese contexto frente al modelo.
- **MCP externo con todo permitido en tu PC:** `argus-mcp` puede funcionar con
  la app cerrada y expone herramientas de escritura. Un cliente con aprobación
  automática puede borrar o modificar todo lo que permitan el bot y su
  jerarquía, sin el paso Apply del asistente integrado. La lista de órdenes,
  Allow from Discord, su confirmación y su tope no protegen ese acceso. El MCP
  no otorga por sí mismo acceso general al PC, pero un agente externo con
  herramientas de shell/archivos puede usar además esos permisos locales.
- **Cuenta o PC comprometidos:** una cuenta autorizada robada puede dar órdenes;
  malware bajo tu sesión puede acceder a datos y credenciales. ARGUS no es una
  barrera frente al dueño de la sesión ni frente a un cliente MCP de confianza
  que se vuelva malicioso.

## Configuración recomendada

1. Usa un bot **sin Administrator**. Concede solo los permisos necesarios para
   las funciones que uses y coloca su rol solo por encima de los roles que deba
   gestionar. Menos permisos reducen el daño posible.
2. Mantén corta la lista de usuarios autorizados para dar órdenes. Limita los
   canales del chat y desactiva órdenes si no las necesitas.
3. Mantén **la confirmación activada** y revisa la propuesta completa antes de
   aprobar. Restringe Allow from Discord y baja el tope de acciones a lo necesario.
4. Conecta el MCP solo a clientes de confianza y exige aprobación para las
   escrituras. Evita modos con todo permitido y revisa las herramientas locales
   disponibles para el agente. Prueba primero en un servidor de pruebas.
5. Mantén el llavero disponible, protege tu sesión y no compartas tokens, BD,
   conversaciones o logs sin revisarlos y eliminar datos sensibles.

Consulta también [el asistente](ASISTENTE.md) y [el MCP](MCP.md).

## Cómo reportar un problema de seguridad

No publiques tokens, claves, datos privados ni una prueba explotable en un
issue público. Usa **Security → Advisories → Report a vulnerability** del
[repositorio ARGUS](https://github.com/BenReynor/ARGUS) si está habilitado. Si no
hay canal privado disponible, abre un
[issue](https://github.com/BenReynor/ARGUS/issues) que solo solicite un canal
privado de contacto, sin describir la vulnerabilidad ni adjuntar datos sensibles.

En el reporte privado incluye la versión o commit, sistema operativo, función
afectada (app, órdenes o MCP), configuración relevante sin secretos, pasos
reproducibles con datos ficticios, resultado esperado/observado e impacto.
No pruebes en servidores ajenos. Si un token quedó expuesto, detén el bot y los
clientes MCP afectados, regenera el token en el Developer Portal y actualízalo
en ARGUS; borrar el mensaje o el log no revoca la credencial.
