# Copias de configuración

En **Settings → Backup**, escribe una contraseña (mínimo 8 caracteres) y su
confirmación para **Export configuration**. Elige si quieres incluir las
conversaciones. El resultado es un archivo `.argusbak`; guarda también la
contraseña en un lugar seguro. No hay recuperación de contraseña.

Incluye bots y sus tokens, preferencias (incluido idioma, tema y comportamiento
de ventana), ajustes completos por servidor (personalidad, respuestas y órdenes),
contextos, planes, plantillas, agentes y vigilancia. Opcionalmente incluye las
conversaciones del asistente y el historial de mensajes. Los mapas de Discord,
incidentes y logs no forman parte de la configuración.

Para **Import configuration**, escribe la contraseña de la copia y selecciona
el archivo. ARGUS muestra fecha, nombres de bots y cantidades por categoría.
Elige **Merge** o **Replace**, revisa el resumen y pulsa **Confirm import**.
El bot debe estar detenido. Fusionar añade registros ausentes y conserva los
registros locales con la misma clave; no mezcla campos dentro de un ajuste de
servidor. Reemplazar elimina la configuración y las conversaciones locales;
si la copia no incluye conversaciones, estas quedan vacías. La aplicación
recarga la interfaz al completar la importación.

El formato v1 cifra el documento completo con AES-256-GCM, usando una clave
derivada de la contraseña mediante Argon2id (19 MiB, dos pasadas, una vía), sal
aleatoria de 16 bytes y nonce aleatorio de 12 bytes. Los tokens se descifran
localmente solo en memoria al exportar; al importar se vuelven a cifrar con la
clave local del equipo de destino. La clave del equipo de origen no viaja en
el archivo ni se necesita para restaurarlo. Una contraseña incorrecta o una
alteración del archivo falla antes de escribir. La importación usa parámetros
SQL, tablas y columnas validadas y una única transacción con rollback.

**Daily automatic backup** usa la contraseña y la opción de conversaciones
seleccionadas al activarlo. La contraseña de automatización se conserva cifrada
con la clave local; nunca se guarda en el navegador ni en el archivo exportado.
La automatización queda desactivada en un equipo nuevo hasta que se configure.
Mientras ARGUS está abierto, comprueba al arrancar y cada hora si falta la copia
del día UTC, la escribe en `backups/` junto a la base de datos y conserva las
siete últimas copias diarias. La ruta se muestra en Ajustes. Al desactivar la
opción se borra la contraseña guardada para automatización. Los archivos se
escriben mediante un temporal y renombrado; en Unix sus permisos son 0600.

Pruebas sin red: `cargo test --manifest-path src-tauri/Cargo.toml backup --lib`.
Cubren roundtrip completo, secretos, contraseña incorrecta, corrupción, rollback,
fusión/reemplazo, conversaciones opcionales y retención diaria.
