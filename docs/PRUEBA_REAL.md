# Prueba real contra un servidor de Discord

Los tests de `src-tauri/tests/integracion_discord.rs` ejercitan las mismas
funciones que los comandos de la app contra Discord de verdad. Están marcados
`#[ignore]`: `cargo test` normal no los ejecuta. Nunca se guarda ningún valor real
en el repo; todo llega por variables de entorno.

## Preparar el servidor de pruebas

1. Crea un servidor de pruebas **solo para esto** y un bot propio en el
   [Developer Portal](https://discord.com/developers/applications). Activa
   «Server Members Intent» en la pestaña Bot.
2. Invita el bot con permisos de: Gestionar roles, Moderar miembros, Enviar
   mensajes y Ver canales.
3. Crea un canal de texto donde el bot pueda escribir y un rol de prueba. En
   Ajustes del servidor → Roles, el rol del bot debe estar **por encima** del rol
   de prueba.
4. Elige un usuario de prueba (una cuenta secundaria) que esté en el servidor,
   acepte mensajes directos de miembros del servidor y **no sea el dueño ni
   administrador** (Discord no permite silenciarlos).
5. Activa el modo desarrollador en Discord y copia con «Copiar ID» los IDs del
   servidor, canal, rol y usuario.

## Ejecutar

```bash
export ARGUS_TEST_TOKEN=...    # token del bot
export ARGUS_TEST_GUILD=...    # ID del servidor
export ARGUS_TEST_CHANNEL=...  # ID del canal de texto
export ARGUS_TEST_ROLE=...     # ID del rol de prueba
export ARGUS_TEST_USER=...     # ID del usuario de prueba
cargo test --manifest-path src-tauri/Cargo.toml --test integracion_discord -- --ignored
```

Si falta alguna variable, cada test termina sin hacer nada (y sin fallar).

## Qué comprueban

Validar el token, que el servidor esté entre los del bot, leer canales, roles y
miembros, calcular los permisos del bot, enviar un mensaje al canal y un DM al
usuario, dar y quitar el rol al usuario, y aplicar y retirar un timeout de 1
minuto. El rol y el timeout se retiran siempre al final, incluso si el paso de
aplicarlos falla.

Los mensajes enviados quedan en el canal y en el DM; bórralos a mano si molestan.
