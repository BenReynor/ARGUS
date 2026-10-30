# ARGUS

Aplicación de escritorio para **administrar tus bots de Discord**: registra tus
bots, elige con cuál trabajar y gestiona los servidores donde está (canales,
roles, miembros, permisos y moderación), con el mapa del servidor actualizado en
vivo. Con su servidor MCP, un asistente de IA (Claude Code u otro) puede manejar
tus bots igual que tú desde la app.

Tauri 2 · React + TypeScript · Rust · SQLite · [twilight](https://twilight.rs)

> ARGUS trabaja con **bots propios** mediante la API oficial de Discord (token de
> bot). No es un selfbot: automatizar cuentas de usuario viola los Términos de
> Servicio de Discord.

---

## Qué hace

- **Varios bots:** registra tus bots con su token; ARGUS lee de Discord su nombre
  y avatar oficiales. Eliges el bot activo en la barra lateral y, en la columna de
  servidores, cualquiera de los servidores donde está.
- **Control total del servidor** (lo que permitan los permisos del bot):
  canales y categorías, roles y jerarquía, permisos por canal, moderación
  (timeout, expulsar, banear), ajustes del servidor, registro de auditoría,
  mensajes, invitaciones y webhooks. Las acciones destructivas piden confirmación.
- **Mapa en vivo:** por el Gateway de Discord; los cambios hechos en Discord
  aparecen al instante, sin recargar, y el mapa y su historial se guardan.
- **Configuración por servidor:** cada servidor conserva sus propios ajustes en ARGUS.
- **MCP propio (`argus-mcp`):** deja que Claude Code, Dédalo u otro cliente MCP
  gestione Discord con tus bots (44 herramientas), incluso con la app cerrada.
  Ver [`docs/MCP.md`](docs/MCP.md).
- **10 temas** (Discord por defecto, minimal, cyberpunk, cálido, datos, anime,
  manga, glass, retro y brutalista) con vista previa en Ajustes → Apariencia.
- Pantalla de carga con barra de progreso al abrir.

## Estado

| Fase | Contenido | Estado |
|---|---|---|
| 0–3 · MVP | Fundamentos, bots genéricos, datos y acciones reales por REST, mapa en vivo por Gateway | ✅ |
| 4 · Prueba real | Tests de integración contra un servidor de pruebas ([`docs/PRUEBA_REAL.md`](docs/PRUEBA_REAL.md)) + revisión manual | 🧪 falta la revisión manual |
| 5–6 · Interfaz | Temas intercambiables, ventana estilo Discord, pulido visual y accesibilidad | ✅ |
| 7 · Control total del servidor | Canales, roles, permisos, moderación, ajustes, auditoría, mensajes, invitaciones y webhooks | ✅ |
| 8 · MCP de ARGUS | Binario `argus-mcp` con la misma lógica y permisos que la app | ✅ (falta probarlo con un cliente real) |

Siguiente, sin fecha: plantillas de servidor, personalidad del bot (respuestas con
un modelo de lenguaje configurable), vigilancia con alertas, notificaciones y
backup. El detalle está en [`CONTEXTO.md`](CONTEXTO.md) y el orden de trabajo en
[`plan.md`](plan.md).

| Sistema | Estado |
|---|---|
| **Linux** | ✅ Desarrollo y uso real. |
| **Windows** | 🧪 Experimental: el CI pasa los tests y genera el instalador, pero no se ha probado en un PC real. |

---

## Requisitos

- **Rust** estable (probado con 1.99) y **Node** 22+
- Linux: `libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`
- Windows: WebView2 (viene con Windows 10/11) y, para compilar, las Build Tools
  de Visual Studio (C++)
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
El CI (`.github/workflows/ci.yml`) corre lo mismo en Linux y Windows en cada cambio.

## Producción

```bash
npm run tauri build
# binario:  src-tauri/target/release/argus
# paquetes: src-tauri/target/release/bundle/ (.deb, .rpm, .AppImage; en Windows .msi y .exe)
```

El paquete incluye `argus-mcp` junto al ejecutable de la app.

### Windows (experimental)

El CI pasa los tests en Windows y genera el instalador en cada cambio de `main`
(GitHub → Actions → CI → artefacto **argus-windows**: `.msi` y `.exe`). Aún no
se ha probado en un PC real. Como el instalador no está firmado, Windows muestra
«Windows protegió tu PC»: **Más información → Ejecutar de todas formas**. Los
datos se guardan en `%APPDATA%\argus\`. El lanzador `scripts/abrir-argus.sh`
(que recompila desde `main`) es solo para Linux.

---

## Uso

1. **Ajustes → Bots → Añadir bot**: pega el token del bot (Developer Portal →
   tu aplicación → Bot → Reset Token). ARGUS lo valida con Discord y muestra el
   nombre y el avatar oficiales.
2. Si el bot aún no está en tu servidor, pulsa **Invitar a un servidor**: abre
   el enlace de Discord con los permisos básicos (ver canales, mensajes,
   gestionar canales y roles, timeout, expulsar y banear). Para el control
   total, dale después un rol con **Administrador** y súbelo en la jerarquía por
   encima de los roles que deba gestionar.
3. Elige el **bot activo** en la barra lateral y el **servidor** en la columna de
   la izquierda. Las secciones Panel, Canales, Miembros, Roles, Servidor y Logs
   trabajan sobre ese servidor.
4. Si ARGUS avisa de que falta el *intent* de miembros, actívalo en el Developer
   Portal (Bot → Privileged Gateway Intents → Server Members Intent).

### Acceso directo en Linux (se actualiza solo)

`scripts/abrir-argus.sh` abre ARGUS compilado desde la última versión de `main`.
Si `main` cambió, recompila antes en una copia aparte del repo (así nunca
empaqueta trabajo a medias de otra rama), con una ventana de espera que muestra
el paso y el porcentaje. Úsalo como comando del acceso directo:

```ini
[Desktop Entry]
Type=Application
Name=ARGUS
Exec=/ruta/a/ARGUS/scripts/abrir-argus.sh
Icon=argus
```

Tras cada compilación instala el icono `argus` en el tema del usuario. Los datos
del lanzador (binario, log de compilación) están en `~/.local/share/argus-app/`.
`scripts/abrir-argus.sh --compilar` solo actualiza, sin abrir la app.

## Seguridad

- Los tokens se guardan **cifrados** (AES-GCM) en la base de datos local
  (`~/.config/argus/`; en Windows `%APPDATA%\argus\`); nunca en texto plano ni en el almacenamiento del navegador.
- Nada de la app está atado a un servidor, canal o bot concreto.
- Las acciones destructivas piden confirmación.

## Documentación

- [`CONTEXTO.md`](CONTEXTO.md): visión, decisiones y estado (fuente principal)
- [`plan.md`](plan.md): tareas por fase, cada una con su criterio de aceptación
- [`ARCHITECTURE_MAP.md`](ARCHITECTURE_MAP.md): qué hay en cada archivo
- [`docs/FICHA_TECNICA.md`](docs/FICHA_TECNICA.md): stack, datos y comandos
- [`docs/AUDITORIA_COMANDOS.md`](docs/AUDITORIA_COMANDOS.md): estado de cada comando Tauri
- [`docs/MCP.md`](docs/MCP.md): el servidor MCP `argus-mcp` y sus herramientas
- [`docs/PRUEBA_REAL.md`](docs/PRUEBA_REAL.md): tests contra un servidor de Discord real

## Desarrollo con Dédalo

El proyecto se desarrolla con [Dédalo](https://github.com/BenReynor/dedalo):
`plan.md` usa su formato y cada tarea se verifica con el gate
(cargo fmt/check/test, tsc y build). Las reglas para agentes están en
[`AGENTS.md`](AGENTS.md).
