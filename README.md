# ARGUS

Aplicación de escritorio para **administrar tus bots de Discord**: registra tus
bots, elige con cuál trabajar y gestiona los servidores donde está (canales,
roles, miembros y permisos), con el mapa del servidor actualizado en vivo.

Tauri 2 · React + TypeScript · Rust · SQLite · [twilight](https://twilight.rs)

> ARGUS trabaja con **bots propios** mediante la API oficial de Discord (token de
> bot). No es un selfbot: automatizar cuentas de usuario viola los Términos de
> Servicio de Discord.

---

## Estado

MVP completo en código, pendiente de la prueba final en un servidor real.

| Fase | Contenido | Estado |
|---|---|---|
| 0 · Fundamentos | Modelo multi-servidor, persistencia, tipos Rust ↔ TS, selector de servidor | ✅ |
| 1 · Bots genéricos | Registrar bots por token (nombre y avatar oficiales), elegir el bot activo, listar sus servidores | ✅ |
| 2 · Datos y acciones reales | Invitar el bot, aviso de intents, canales/roles/miembros reales, permisos del bot y panel por servidor, configuración por servidor, mensajes y DM, roles, timeout | ✅ |
| 3 · Mapa en vivo | Gateway: sincronización completa, cambios al instante, mapa e historial guardados, resincronizar | ✅ |
| 4 · Prueba real | Tests de integración contra un servidor de pruebas ([`docs/PRUEBA_REAL.md`](docs/PRUEBA_REAL.md)) + revisión manual | 🧪 |

Después del MVP: edición completa del servidor, plantillas de servidor,
personalidad del bot (respuestas con un modelo de lenguaje configurable),
vigilancia y logs, notificaciones, backup y servidor MCP propio. El detalle está
en [`CONTEXTO.md`](CONTEXTO.md) y el orden de trabajo en [`plan.md`](plan.md).

---

## Requisitos

- **Rust** estable (probado con 1.99) y **Node** 22+
- Linux: `libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`
- Un bot de Discord creado en el [Developer Portal](https://discord.com/developers/applications)
  e invitado a tu servidor con los permisos que vayas a usar
  (Gestionar canales, Gestionar roles, Moderar miembros…).

## Desarrollo

```bash
git clone https://github.com/BenReynor/ARGUS.git
cd ARGUS
npm install
npm run tauri dev        # app completa con recarga en caliente
```

Comprobaciones (las mismas que exige el gate de Dédalo):

```bash
cargo fmt   --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test  --manifest-path src-tauri/Cargo.toml
npm run build            # tsc + vite build
```

Los tests no usan la red: el cliente de Discord se apunta a un Discord falso local.

## Producción

```bash
npm run tauri build
# binario:  src-tauri/target/release/argus
# paquetes: src-tauri/target/release/bundle/ (.deb, .rpm, .AppImage)
```

---

## Uso (al terminar la FASE 1)

1. **Ajustes → Añadir bot**: pega el token. ARGUS lo valida con Discord y muestra
   el nombre y el avatar oficiales del bot.
2. Elige el **bot activo** en la barra lateral.
3. Elige el **servidor** en el selector de la cabecera: aparecen los servidores
   donde está ese bot.

## Seguridad

- Los tokens se guardan **cifrados** (AES-GCM) en la base de datos local
  (`~/.config/argus/`); nunca en texto plano ni en el almacenamiento del navegador.
- Nada de la app está atado a un servidor, canal o bot concreto.
- Las acciones destructivas piden confirmación.

## Documentación

- [`CONTEXTO.md`](CONTEXTO.md): visión, decisiones y estado (fuente principal)
- [`plan.md`](plan.md): tareas por fase, cada una con su criterio de aceptación
- [`ARCHITECTURE_MAP.md`](ARCHITECTURE_MAP.md): qué hay en cada archivo
- [`docs/FICHA_TECNICA.md`](docs/FICHA_TECNICA.md): stack, datos y comandos
- [`docs/AUDITORIA_COMANDOS.md`](docs/AUDITORIA_COMANDOS.md): estado de cada comando Tauri

## Desarrollo con Dédalo

El proyecto se desarrolla con [Dédalo](https://github.com/BenReynor/dedalo):
`plan.md` usa su formato y cada tarea se verifica con el gate
(cargo fmt/check/test, tsc y build). Las reglas para agentes están en
[`AGENTS.md`](AGENTS.md).
