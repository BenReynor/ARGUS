# Plan estructurado del servidor

El plan del servidor del [asistente de IA](ASISTENTE.md) admite una **sintaxis
estructurada**. ARGUS la convierte **él mismo, sin IA**, en una propuesta: es
instantáneo, gratis y exacto. La IA solo escribe los textos que marques con
`generar:` (descripciones), todos en **una sola llamada sin herramientas**.

Lo que no sigue la sintaxis es **texto libre**: se conserva y se trata como
antes, y puedes pedírselo al asistente en el chat.

- [Sintaxis](#sintaxis)
- [Contexto del servidor](#contexto-del-servidor)
- [Ejemplo: servidor temático de 10 categorías](#ejemplo-servidor-temático-de-10-categorías)
- [Ejemplo: 120 canales con descripciones generadas](#ejemplo-120-canales-con-descripciones-generadas)
- [Construir desde el plan](#construir-desde-el-plan)
- [Ahorrar tokens](#ahorrar-tokens)
- [Avisos del plan](#avisos-del-plan)

(Documento en español; la interfaz de la app y [ASISTENTE.md](ASISTENTE.md) están
en inglés. Versión en inglés: [PLAN_SERVIDOR.md](PLAN_SERVIDOR.md).)

## Sintaxis

Las palabras clave valen en **español o inglés**, sin distinguir mayúsculas ni
tildes. `//` inicia un comentario (hasta el final de la línea). Las líneas en
blanco se ignoran.

### Roles

```
## Rol: Supervisor | color: #ffaa00 | permisos: ManageMessages, ModerateMembers
## Rol: Director | color: #e74c3c | separado
```

| Atributo | Inglés | Significado |
|---|---|---|
| `color` | `color` | Color hexadecimal, `#rrggbb` |
| `permisos` | `permissions` | Nombres de permisos de Discord, separados por comas |
| `separado` | `hoist` | Muestra el rol aparte en la lista de miembros |
| `mencionable` | `mentionable` | Cualquiera puede mencionar el rol |

Cabecera: `## Rol:` o `## Role:`.

### Categorías y canales

```
## Categoría: Supervisión | ver: Supervisor, Director | escribir: Supervisor
- texto: informes | descripción: "Informes diarios."
- voz: Sala de control
- anuncios: novedades
- foro: ideas
```

- `## Categoría:` (o `## Category:`) abre una categoría. Las líneas
  `- tipo: nombre` que siguen pertenecen a ella, hasta la siguiente cabecera.
- Tipos de canal: `texto`/`text`, `voz`/`voice`, `anuncios`/`announcement`,
  `foro`/`forum`.
- Los atributos se separan con `|`.

### Rangos

`{1..10}` repite la línea una vez por número. Los ceros se conservan si los
escribes: `{01..10}` da `01`…`10`.

```
- texto: patrulla-{1..3}     // patrulla-1, patrulla-2, patrulla-3
- voz: sala-{01..03}         // sala-01, sala-02, sala-03
```

Dentro de una descripción literal, `{n}` es el número actual.

### Descripciones

`descripción` (también `description`, `tema`, `topic`) admite:

- un **literal**: `descripción: "Normas del área."` — sin IA ni tokens;
- **generada**: `descripción: generar: épica, sobre la vigilancia` — el texto tras
  `generar:` es la indicación para la IA.

En una **categoría**, la descripción es el valor por defecto de sus canales que
no tengan una. Máximo 1024 caracteres por descripción (límite de Discord).

### Permisos

| Atributo | Inglés | Efecto en el canal/categoría |
|---|---|---|
| `ver: A, B` | `view` | Oculto para `@everyone`; visible para A y B |
| `escribir: A` | `write` | A ve y escribe (en voz: se conecta y habla) |
| `solo leer: A` | `read only` | A ve pero no escribe |
| `ocultar a: A` | `hide from` | A no lo ve |

`@everyone` vale como rol. En un canal, los permisos se **suman** a los de su
categoría.

### Variables

```
@var tema = medieval
- texto: taberna-{1..5} | descripción: generar: ambiente, $tema
```

`@var nombre = valor` puede ir en cualquier parte, antes de usarse. `$nombre` se
sustituye en nombres, descripciones literales e indicaciones `generar:`, y se
combina con rangos (`$area-{1..3}`). Una variable sin definir da un aviso y el
texto se deja tal cual.

### Texto libre

Cualquier otra línea (notas, ideas, peticiones al asistente) se guarda como texto
libre y nunca se pierde.

## Contexto del servidor

Por servidor (**AI Assistant → Context & personality**) puedes escribir un
**contexto** en Markdown: temática, tono, historia, estilo de nombres y de
descripciones. ARGUS lo añade a:

- el prompt del **constructor** (sección «Server context»), para que los nombres
  y las descripciones `generar:` encajen con tu mundo;
- el prompt del **chat del bot en Discord**, recortado a 1500 caracteres.

Lo escribes una vez y no lo repites en cada plan o petición. Se puede
importar/exportar como `.md`. Ejemplo:

```markdown
Servidor de rol de fantasía oscura. Tono: solemne, algo irónico.
Nombres: minúsculas, guiones, sin emojis. Descripciones: una o dos frases,
en segunda persona.
```

La **personalidad** del bot está en la misma pestaña.

## Ejemplo: servidor temático de 10 categorías

```
// Servidor temático: La Biblioteca Hundida
@var tema = biblioteca inundada, fantasía oscura

## Rol: Archivista | color: #7aa2f7 | permisos: ManageMessages, ModerateMembers | separado
## Rol: Escriba | color: #9ece6a | mencionable
## Rol: Lector | color: #c0caf5

## Categoría: Bienvenida | solo leer: @everyone
- texto: reglas | descripción: "Léelas antes de entrar."
- anuncios: novedades | descripción: "Anuncios oficiales."
- texto: roles | descripción: "Elige tus roles."

## Categoría: Sala de lectura | descripción: generar: acogedora, sobre $tema
- texto: general
- texto: recomendaciones
- voz: Sala de lectura

## Categoría: Las Estanterías | descripción: generar: misteriosa, sobre $tema
- texto: estante-{1..6}
- foro: libros-perdidos

## Categoría: Scriptorium | ver: Escriba, Archivista | escribir: Escriba, Archivista
- texto: borradores
- texto: opiniones
- voz: Sesión de escritura

## Categoría: Taberna
- texto: charla-taberna | descripción: "Temas libres."
- voz: Taberna-{1..3}

## Categoría: Juegos
- texto: noche-de-juegos
- voz: Sala de juegos-{1..2}

## Categoría: Galería de arte | descripción: generar: admirativa, sobre $tema
- texto: galería
- texto: charla-arte
- foro: encargos

## Categoría: Soporte
- texto: ayuda | descripción: "Pide ayuda."
- texto: sugerencias | descripción: "Ideas para el servidor."

## Categoría: Archivo | solo leer: @everyone
- texto: noticias-antiguas
- texto: eventos-pasados

## Categoría: Personal | ver: Archivista
- texto: charla-staff
- texto: registros | descripción: "Registro de moderación."
- voz: Sala del staff
```

Son 3 roles, 10 categorías y unos 30 canales. Solo las categorías con `generar:`
usan la IA, y los canales sin descripción propia heredan la de su categoría.

> Nota: `{n}` solo funciona dentro de descripciones literales. Para nombres
> numerados usa un rango (`Taberna-{1..3}`).

## Ejemplo: 120 canales con descripciones generadas

Discord permite como mucho **50 canales por categoría** y **500 por servidor**
(las categorías cuentan para los 500). Para 120 canales, repártelos en tres
categorías con rangos:

```
@var zona = estación de metro abandonada

## Rol: Explorador | color: #e0af68

## Categoría: Sectores A | ver: Explorador | escribir: Explorador | descripción: generar: breve, atmosférica, sobre $zona
- texto: sector-{001..040}

## Categoría: Sectores B | ver: Explorador | escribir: Explorador | descripción: generar: breve, atmosférica, sobre $zona
- texto: sector-{041..080}

## Categoría: Sectores C | ver: Explorador | escribir: Explorador | descripción: generar: breve, atmosférica, sobre $zona
- texto: sector-{081..120}
```

Así salen **120 canales de texto** (`sector-001` … `sector-120`), visibles y con
escritura solo para `Explorador`. Cada uno recibe **su propia descripción
generada** a partir de la indicación de la categoría.

ARGUS compila la estructura en local (sin IA) y solo llama a la IA para escribir
las 120 descripciones: las pendientes van en **tandas de 40**, o sea 3 llamadas al
agente, cada una sin herramientas. Si el agente falla o se deja alguna, la
propuesta llega igualmente con esas descripciones vacías y un aviso.

Si una categoría pasaría de 50 canales (contando los que ya tiene), ARGUS crea
los 50 primeros y avisa de cuántos omitió, para que Aplicar nunca falle a medias.
También avisa si el servidor pasaría de 500.

Si no quieres IA en absoluto, usa una descripción literal con el número:

```
- texto: sector-{001..040} | descripción: "Sector {n} de la estación."
```

## Construir desde el plan

1. En modo **Execute**, con un plan estructurado, pulsa **Build from plan**.
2. ARGUS analiza y compila el plan **en local**, saltándose lo que ya existe
   (roles por nombre; canales por nombre dentro de su categoría).
3. Si hay descripciones `generar:`, **una** llamada las escribe (en tandas de 40
   si son muchas).
4. La tarjeta de propuesta indica cuántas acciones salen de la estructura y
   cuántas descripciones escribió la IA. Si hay texto libre, lo avisa y sugiere
   pedírselo al asistente en el chat.
5. Revisas y pulsas **Apply**; ARGUS lo ejecuta en orden (roles → categorías →
   canales → permisos).

También puedes usar el modo **Plan**: el asistente escribe o mejora el plan
estructurado (siempre partiendo de lo que haya ahora en el editor) y muestra una
vista de cambios con **Apply to plan** / **Discard**. Construir sigue siendo sin
IA.

Con las órdenes de Discord activadas (ver
[ASISTENTE.md](ASISTENTE.md#orders-from-discord)), un miembro autorizado puede
adjuntar un plan `.md` o `.txt` (máx. 200 KB) en un mensaje al bot: ARGUS lo
compila sin IA, responde con el resumen y los avisos, y lo aplica cuando el autor
confirma.

El botón **Manual** junto al editor muestra esta sintaxis, e **Insert template**
inserta un plan de ejemplo comentado.

## Ahorrar tokens

- **Usa estructura en vez de prosa.** Una línea con rango la compila ARGUS gratis;
  pedir al chat «crea 120 canales» gasta tokens y llamadas a herramientas.
- **Las descripciones literales no cuestan nada.** Usa `generar:` solo donde
  quieras variedad, y con una indicación corta.
- **Pon la descripción en la categoría** para que cada canal sin la suya herede la
  misma base generada, en vez de una indicación por línea.
- **Escribe el contexto una vez** en el contexto del servidor, en lugar de
  repetir temática y tono en cada petición o indicación.
- **Usa variables** (`@var`) para escribir la temática una sola vez.
- **Usa el modo Plan para los cambios** («añade 3 canales de voz a Taberna»):
  envía el editor, no una conversación larga, y construir sigue siendo gratis.
- **Elige un modelo barato o gratis** (ver
  [ASISTENTE.md](ASISTENTE.md#2-choose-a-model)); las descripciones no necesitan
  el más potente.
- **Empieza una conversación nueva** cuando ya no necesites la larga: el
  historial se reenvía en cada turno.
- **Revisa los avisos** antes de construir para no pagar una segunda pasada.

## Avisos del plan

Bajo el editor, **Plan warnings** lista cada problema como «Line N: motivo»: un
rango inválido, un color mal escrito, una variable sin definir. Corrígelos antes
de pulsar **Build from plan**.

Ver también: [ASISTENTE.md](ASISTENTE.md) · [MCP.md](MCP.md).
