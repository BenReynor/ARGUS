# ARGUS AI Assistant

The **AI Assistant** is ARGUS's main feature: build huge Discord servers in minutes.
You write the **server plan** (a Markdown document, one per server) and chat with an
AI agent that studies the server and the plan and proposes the changes. You review
the proposal, press **Apply**, and ARGUS itself creates everything.

It lives in the **AI Assistant** section of the sidebar and has two panels:

- **Server plan:** a Markdown editor with autosave, a character counter and
  **Import .md** / **Export .md** buttons.
- **Chat:** agent and model selectors, a **Read / Execute** switch, the
  conversation, **Stop** and **New conversation** buttons.

Plan and conversation are saved per server and per bot, so they are still there
when you close and reopen ARGUS.

> ⚠️ **Privacy.** The agent is an external program backed by an AI provider. See
> [What leaves your computer](#what-leaves-your-computer) before you use it with a
> server you care about.

## 1. Install an AI agent

ARGUS does not include a model. It drives a command-line agent that you install
and sign in to once. Install at least one of them:

| Agent | Install | Sign in |
|---|---|---|
| [Claude Code](https://docs.anthropic.com/en/docs/claude-code) (`claude`) | `npm install -g @anthropic-ai/claude-code` | run `claude` and follow the prompts (Claude subscription or API key) |
| [opencode](https://opencode.ai) (`opencode`) | `npm install -g opencode-ai` | run `opencode auth login` and pick a provider |

Notes:

- ARGUS looks for the agents in your `PATH` and in the usual places
  (`~/.local/bin`, `~/.claude/local`, `~/.opencode/bin`, `~/.npm-global/bin`; on
  Windows `%APPDATA%\npm` and `%USERPROFILE%\.local\bin`), so it works even when
  you open it from a desktop shortcut.
- Only the agents that are installed appear in the selector. If there are none,
  the assistant shows how to install them.
- If an agent is installed but not signed in, ARGUS tells you and shows the
  command to run. Sign in from a terminal and send your message again.

## 2. Choose a model

Pick the agent first, then one of its models:

- **claude:** the aliases `opus` (most capable, slowest), `sonnet` (a good balance;
  the best default for building servers) and `haiku` (fastest and cheapest).
- **opencode:** every model in its catalog, listed as `provider/model`. ARGUS
  reads them from `opencode models`. Run `opencode auth login` to add providers;
  the list updates after that.

### Free models with opencode

opencode's catalog includes models that cost nothing, such as the free ones offered
through the **opencode** provider (their names usually end in `-free`). Choose one
of those in the model selector to use the assistant without paying. Which models
are free, and for how long, is decided by their providers and changes often: run
`opencode models` to see what is available to you today.

Free models are great for **Read** mode and for small servers. For a large build
in **Execute** mode, a stronger model (for example `claude/sonnet` or `opus`)
produces more reliable proposals.

## 3. Write a good server plan

The plan is plain Markdown. The agent reads it in full on every message, so write
it as if briefing a human administrator:

- **Be explicit and structured.** One heading per category, one list item per
  channel, one heading per role.
- **Give every channel a type and a description** (the description becomes the
  channel topic).
- **Give every role its permissions** by their official Discord names
  (`Manage Messages`, `Send Messages`, `View Channel`…), and say what makes it
  different from the others.
- **State the channel permissions** (who can see, read or write where), e.g.
  “private to Staff”, “read-only for Member”.
- **Say which existing members get which role**, if you want that.
- **State what to keep.** The agent compares the plan with the real server; say
  whether anything extra should be left alone or removed.
- Write names exactly as they should appear in Discord, including emoji.

### Complete example: a themed server

```markdown
# SCP Foundation RP — server plan

Theme: a secret organization that contains anomalies. Serious but welcoming
tone. Names in English, lowercase with dashes for channels.

## Roles (highest to lowest)

1. **O5 Council** — owners. Color gold. Administrator.
2. **Site Director** — head moderators. Color red, shown separately.
   Manage Channels, Manage Roles, Manage Messages, Kick Members, Ban Members,
   Moderate Members, View Audit Log, Mention Everyone.
3. **Researcher** — trusted roleplayers. Color blue, shown separately.
   Create Public Threads, Embed Links, Attach Files, Add Reactions,
   Use External Emoji, Manage Threads.
4. **Security Officer** — moderators of the in-character side. Color green.
   Manage Messages, Moderate Members, Mute Members, Move Members.
5. **Class-D** — new members. Color orange. Only the basics: View Channel,
   Send Messages, Read Message History, Add Reactions, Connect, Speak.
6. **Visitor** — read-only guests. Color grey. View Channel and
   Read Message History only; cannot send messages anywhere.

## Categories and channels

### 📋 INFORMATION
- #welcome (text) — "Read this first: what the server is and how to join."
- #rules (text) — "The server rules. Breaking them has consequences."
- #announcements (announcement) — "News and events from the O5 Council."
- #scp-database (text) — "Entries for catalogued anomalies. One thread per SCP."

### 🧪 ROLEPLAY
- #site-lobby (text) — "Out-of-character chat while you wait for a scene."
- #containment-wing (text) — "Main in-character channel: the containment wing."
- #research-lab (text) — "In-character: experiments and reports."
- #security-checkpoint (text) — "In-character: patrols and checkpoint scenes."
- #incident-reports (forum) — "Post a report after every notable scene."
- Voice: #briefing-room (voice) — "Voice scenes and briefings."

### 🔒 STAFF
- #staff-chat (text) — "Private discussion for staff."
- #mod-log (text) — "Moderation actions and decisions."
- #staff-voice (voice) — "Staff meetings."

## Channel permissions

- **INFORMATION** (category): @everyone can view and read but not send messages
  or add reactions; Site Director and O5 Council can send. #announcements:
  only O5 Council and Site Director can send.
- **ROLEPLAY** (category): Visitor can view but not send; Class-D, Researcher and
  Security Officer can send. #site-lobby: Visitor can also send.
  #incident-reports: only Researcher and above can create posts.
- **STAFF** (category): hidden from @everyone, Visitor, Class-D and Researcher.
  Visible and writable for Security Officer, Site Director and O5 Council.
  #mod-log: Security Officer can read but not write.

## Members

- Everyone already on the server who has no role: give them **Class-D**.
- Leave all existing roles that are not in this plan untouched.
```

A plan like this is enough for the agent to propose every role, category, channel,
topic and permission overwrite in one go.

## 4. Read and Execute modes

The switch above the chat chooses what the agent is asked to do:

| | **Read** | **Execute** |
|---|---|---|
| Purpose | Talk and consult | Build what the plan describes |
| The agent | Answers questions, explains, reviews your plan | Studies the server and the plan and returns a **proposal** |
| Changes Discord | Never | Never — only after you press **Apply** |

**Read** examples: “What channels and roles exist?”, “Which roles can ban members?”,
“Does my plan clash with what is already there?”, “Improve the channel
descriptions in my plan”.

**Execute** examples: “Build the plan.”, “Only create the STAFF category and its
permissions.”, “Add a #suggestions channel to ROLEPLAY.”

In both modes the agent connects only to ARGUS's MCP server and is given **read
tools only** (server state, channels, roles, members, permissions, audit log,
messages…). Shell, file editing, web access and every write tool are disabled, so
the agent cannot change anything by itself, whatever it is told.

## 5. Approval: the proposal and Apply

In Execute mode the agent finishes with a structured **proposal**: a summary and a
list of actions. ARGUS checks it without touching Discord and shows a card with:

- the summary,
- the changes grouped by type (roles, categories, channels, permissions, member
  roles…),
- any validation problem (unknown action, invalid arguments, a reference to a name
  that neither exists nor is created).

You then choose:

- **Apply:** ARGUS executes the actions itself, in a safe order — roles →
  categories → channels → channel permissions → member roles — and marks each one
  live as pending, done, failed or skipped, with a progress bar. A failed action
  does not stop the rest, except actions that depend on it (they are skipped).
  **Stop** cancels the run midway.
- **Cancel:** nothing happens. You can ask the agent to adjust the proposal.

What is executed is exactly what you approved, using the same permission and
role-hierarchy checks as the rest of ARGUS: Discord's limits still apply (the bot
cannot touch a role at or above its own highest role, for example), and each
failure is reported with a clear message. The proposal card is not saved: if you
close ARGUS before applying, it disappears.

Tip: try a plan on a **test server** first, and apply large builds in stages.

## What leaves your computer

ARGUS has no telemetry and never sends your bot's token to the agent: the token is
stored encrypted and `argus-mcp` reads it from the local database. But the agent is
a program that talks to an **AI provider** (Anthropic for `claude`; whichever
provider you chose for `opencode`), and that provider receives:

- **Your server plan,** in full, with every message.
- **Everything the AI reads from your server** through the read tools: server
  name, channels, roles and permissions, member names and roles, audit-log entries
  and message contents it decides to look at.
- **The conversation** (your messages and its answers).

Consequences:

- Do not put secrets in the plan, and avoid pointing the assistant at servers with
  private conversations unless you accept that provider's data policy.
- With **free models**, check the provider's terms: free tiers often use prompts
  to improve their models.
- Each provider's own retention and privacy rules apply; read them before use.
- The plan and the conversation are stored only on your computer
  (`~/.config/argus/`; on Windows `%APPDATA%\argus\`).

## Troubleshooting

- **No agents found:** install one (section 1) and reopen ARGUS.
- **“The AI agent is not signed in”:** run `claude`, or `opencode auth login`, in
  a terminal.
- **Invalid model or no quota:** pick another model or check your plan with the
  provider.
- **The proposal is missing or invalid:** ask the agent to answer again with the
  proposal; in Execute mode it must end with the structured block.
- **An action fails with a permission message:** the bot lacks that permission or
  its role is below the target. Fix it in Discord (Server Settings → Roles) and
  apply again.

See also [MCP.md](MCP.md) for the tools the agent can read with.
