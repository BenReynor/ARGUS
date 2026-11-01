# Structured server plan

The server plan of the [AI assistant](ASISTENTE.md) can use a **structured
syntax**. ARGUS turns it into a proposal **itself, with no AI**: it is instant,
free and exact. The AI only writes the texts you mark with `generate:`
(descriptions), all in **one single call without tools**.

Anything that does not follow the syntax is **free text**. It is kept and treated
as before: you can ask the assistant in the chat to build it.

- [Syntax](#syntax)
- [Server context](#server-context)
- [Example: themed server with 10 categories](#example-themed-server-with-10-categories)
- [Example: 120 channels with generated descriptions](#example-120-channels-with-generated-descriptions)
- [Build from plan](#build-from-plan)
- [Saving tokens](#saving-tokens)
- [Plan warnings](#plan-warnings)

## Syntax

Keywords work in **English or Spanish**, ignoring upper/lower case and accents.
`//` starts a comment (to the end of the line). Blank lines are ignored.

### Roles

```
## Role: Supervisor | color: #ffaa00 | permissions: ManageMessages, ModerateMembers
## Role: Director | color: #e74c3c | hoist
```

| Attribute | Spanish | Meaning |
|---|---|---|
| `color` | `color` | Hex colour, `#rrggbb` |
| `permissions` | `permisos` | Discord permission names, comma-separated |
| `hoist` | `separado` | Show the role separately in the member list |
| `mentionable` | `mencionable` | Anyone can mention the role |

Header: `## Role:` or `## Rol:`.

### Categories and channels

```
## Category: Supervision | view: Supervisor, Director | write: Supervisor
- text: reports | description: "Daily reports."
- voice: Control room
- announcement: news
- forum: ideas
```

- `## Category:` (or `## Categoría:`) opens a category. The `- type: name` lines
  below it belong to it, until the next header.
- Channel types: `text`/`texto`, `voice`/`voz`, `announcement`/`anuncios`,
  `forum`/`foro`.
- Attributes are separated by `|`.

### Ranges

`{1..10}` repeats the line once per number. Zeros are kept if you write them:
`{01..10}` gives `01`…`10`.

```
- text: patrol-{1..3}        // patrol-1, patrol-2, patrol-3
- voice: room-{01..03}       // room-01, room-02, room-03
```

Inside a literal description, `{n}` is the current number.

### Descriptions

`description` (also `descripción`, `topic`, `tema`) takes either:

- a **literal**: `description: "Rules of the area."` — no AI, no tokens;
- **generated**: `description: generate: epic, about the watch` — the text after
  `generate:` is the hint for the AI.

On a **category**, the description is the default for its channels that have none.
A description is at most 1024 characters (a Discord limit).
**Voice channels have no description** on Discord: one set on them (or inherited
from their category) is ignored, and the AI does not write it.

### Permissions

| Attribute | Spanish | Effect on the channel/category |
|---|---|---|
| `view: A, B` | `ver` | Hidden from `@everyone`; visible to A and B |
| `write: A` | `escribir` | A sees and writes (in voice: connects and speaks) |
| `read only: A` | `solo leer` | A sees but cannot write |
| `hide from: A` | `ocultar a` | A cannot see it |

`@everyone` is valid as a role. In a channel the permissions are **added** to
those of its category.

### Variables

```
@var theme = medieval
- text: tavern-{1..5} | description: generate: atmosphere, $theme
```

`@var name = value` can go anywhere, before it is used. `$name` is replaced in
names, literal descriptions and `generate:` hints, and combines with ranges
(`$area-{1..3}`). An undefined variable gives a warning and the text stays as is.

### Title

```
@title = SCP Foundation
```

Optional. When you export the plan, the title is the file name
(`SCP Foundation.md`). `@titulo` also works.

### Free text

Any other line (notes, ideas, requests to the assistant) is kept as free text
and never lost.

## Server context

Per server (**AI Assistant → Context & personality**) you can write a Markdown
**context**: theme, tone, lore, naming style and description style. ARGUS adds it
to:

- the **builder** prompt (section “Server context”), so names and `generate:`
  descriptions fit your world;
- the **bot's Discord chat** prompt, trimmed to 1500 characters.

Write it once and you don't repeat it in every plan or request. You can
import/export it as `.md`. Example:

```markdown
Dark-fantasy roleplay server. Tone: solemn, a bit ironic.
Names: lowercase, hyphens, no emoji. Descriptions: one or two sentences,
second person.
```

The bot's **personality** lives in the same tab.

## Example: themed server with 10 categories

```
// Themed server: The Sunken Library
@var tema = flooded library, dark fantasy

## Role: Archivist | color: #7aa2f7 | permissions: ManageMessages, ModerateMembers | hoist
## Role: Scribe | color: #9ece6a | mentionable
## Role: Reader | color: #c0caf5

## Category: Welcome | read only: @everyone
- text: rules | description: "Read before entering."
- announcement: news | description: "Official announcements."
- text: roles | description: "Choose your roles."

## Category: Reading Room | description: generate: inviting, about $tema
- text: general
- text: recommendations
- voice: Reading hall

## Category: The Stacks | description: generate: mysterious, about $tema
- text: shelf-{1..6}
- forum: lost-books

## Category: Scriptorium | view: Scribe, Archivist | write: Scribe, Archivist
- text: drafts
- text: feedback
- voice: Writing session

## Category: Tavern
- text: tavern-chat | description: "Off-topic."
- voice: Tavern-{1..3}

## Category: Games
- text: game-night
- voice: Game room-{1..2}

## Category: Art Gallery | description: generate: admiring, about $tema
- text: gallery
- text: art-chat
- forum: commissions

## Category: Support
- text: help | description: "Ask for help."
- text: suggestions | description: "Ideas for the server."

## Category: Archive | read only: @everyone
- text: old-news
- text: past-events

## Category: Staff | view: Archivist
- text: staff-chat
- text: logs | description: "Moderation log."
- voice: Staff room
```

This is 3 roles, 10 categories and about 30 channels. Only the categories with
`generate:` use the AI, and the channels without their own description take the
category's.

> Note: `{n}` only works inside literal descriptions. Use a range
> (`Tavern-{1..3}`) for numbered names.

## Example: 120 channels with generated descriptions

Discord allows at most **50 channels per category** and **500 per server**
(categories count towards the 500). For 120 channels, split them into three
categories with ranges:

```
@var zone = abandoned subway station

## Role: Explorer | color: #e0af68

## Category: Sectors A | view: Explorer | write: Explorer | description: generate: short, atmospheric, about $zone
- text: sector-{001..040}

## Category: Sectors B | view: Explorer | write: Explorer | description: generate: short, atmospheric, about $zone
- text: sector-{041..080}

## Category: Sectors C | view: Explorer | write: Explorer | description: generate: short, atmospheric, about $zone
- text: sector-{081..120}
```

That gives **120 text channels** (`sector-001` … `sector-120`), all visible and
writable only by `Explorer`. Each one gets its **own generated description**
from the category's hint.

ARGUS compiles the structure locally (no AI) and then makes the AI calls only to
write the 120 descriptions: the pending ones go in **batches of 40**, so it is 3
calls to the agent, each without tools. If the agent fails or leaves some out, the
proposal still arrives with those descriptions empty and a warning.

If a category would go over 50 channels (counting the ones it already has),
ARGUS creates the first 50 and warns how many it skipped, so Apply never fails
halfway. It also warns when the server would go over 500.

If you prefer no AI at all, use a literal description with the number:

```
- text: sector-{001..040} | description: "Sector {n} of the station."
```

## Build from plan

1. In **Execute** mode, with a structured plan, press **Build from plan**.
2. ARGUS analyses and compiles the plan **locally**, skipping what already exists
   (roles by name; channels by name within their category).
3. If there are `generate:` descriptions, **one** call writes them (in batches of
   40 if there are many).
4. The proposal card shows how many actions come from the structure and how many
   descriptions the AI wrote. If there is free text, it says so and suggests
   asking the assistant in the chat.
5. You review and press **Apply**; ARGUS runs it in order (roles → categories →
   channels → permissions).

You can also use **Plan** mode: the assistant writes or improves the structured
plan (always starting from what is in the editor now) and shows a diff with
**Apply to plan** / **Discard**. Building still happens without AI.

With order commands enabled (see [ASISTENTE.md](ASISTENTE.md#orders-from-discord)),
a member allowed to give orders can attach a `.md` or `.txt` plan (max 200 KB) in
a message to the bot: ARGUS compiles it without AI, answers with the summary and
warnings, and applies it after the author confirms.

The **Manual** button next to the editor shows this syntax, and **Insert
template** adds a commented example plan.

## Saving tokens

- **Use structure instead of prose.** A line with a range is compiled by ARGUS
  for free; asking the chat to “create 120 channels” spends tokens and tool
  calls.
- **Literal descriptions cost nothing.** Use `generate:` only where you want
  variety, and give a short hint.
- **Put the description on the category** so every channel without its own
  inherits the same generated base, instead of one hint per line.
- **Write the context once** in the server context rather than repeating theme and
  tone in each request or hint.
- **Use variables** (`@var`) so the theme is written one time.
- **Use Plan mode for changes** (“add 3 voice channels to Tavern”): it sends the
  editor, not a long conversation, and building stays free.
- **Pick a cheap or free model** (see [ASISTENTE.md](ASISTENTE.md#2-choose-a-model));
  descriptions don't need the strongest one.
- **Start a new conversation** when a long chat is no longer needed: a long
  history is sent again every turn.
- **Check the warnings** before building so you don't pay for a second pass.

## Plan warnings

Below the editor, **Plan warnings** lists each problem as “Line N: reason”: an
invalid range, a badly written colour, an undefined variable. Fix them before
pressing **Build from plan**.

See also: [ASISTENTE.md](ASISTENTE.md) · [MCP.md](MCP.md).
