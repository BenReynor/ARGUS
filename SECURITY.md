# ARGUS security

[Español](docs/SEGURIDAD.md)

ARGUS manages your own bots through Discord's official API. Its safeguards
reduce the risk of mistaken or unauthorized orders. They cannot guarantee that
a model's decisions are correct or protect a compromised computer.

## What ARGUS protects

### Bot tokens and local encryption

Tokens are encrypted with AES-256-GCM in the local SQLite database, under
`~/.config/argus/` on Linux or `%APPDATA%\argus\` on Windows. They are not stored
in browser localStorage or included in logs or MCP responses. The agent does
not receive the bot token: the backend and `argus-mcp` decrypt it when needed.

The random 32-byte master key is stored in the system keyring: Secret Service
on Linux and Credential Manager on Windows. The app and MCP share that key.
Legacy tokens encrypted with a machine-id-derived key migrate in a transaction
without losing bots. If the keyring is unavailable, new tokens retain legacy
encryption and Settings shows a warning. This fallback provides weaker
protection: a machine identifier is not a secret. Unlock the keyring and restart
to migrate. Already migrated tokens require their original key; copying only
the database does not restore access.

Token encryption does not encrypt the entire database, plans, conversations or
logs. A process with access to your session, keyring or memory may compromise
them. Protect local files and backups too.

### Orders from Discord

Orders are disabled by default. When enabled, only users in the dedicated order
allowlist (`chat_ordenes_usuarios`) are authorized; an empty list authorizes
nobody. Permission to chat with the bot does not grant permission to issue
orders. Chat channels and users are configured per server.

The builder receives the authorized author's original message, with the
`ORDEN[...]` marker as a hint, without other users' previous messages. The chat
prompt treats that history as untrusted data rather than instructions. These
controls reduce prompt injection exposure but do not eliminate it.

**Discord replies → Allow from Discord** selects the allowed action groups.
Defaults allow everything except deleting channels/roles and banning. A
disallowed action rejects the proposal with a reason; action permissions are
checked again on confirmation. These controls apply to Discord orders, not to
app actions or external MCP access.

### Confirmation and execution

The integrated assistant exposes only read MCP tools to its agent. For build
or moderation work, the agent returns a structured proposal. The user reviews
it and clicks **Apply**, then ARGUS executes the approved actions in order,
with validation and Discord permission and role hierarchy checks.

Discord order confirmation is enabled by default. Only the proposal's author
can confirm it in the same server and channel. Expired proposals are not
applied, and confirming does not execute the same proposal twice. Reply with
“no” or “cancel” to discard it.

Even when confirmation is disabled, irreversible actions still require it:
deleting channels, roles, messages, invites or webhooks; banning or kicking;
and sending messages, DMs or warnings that recipients may already have seen.
The proposal identifies these actions. Other actions can run directly if you
turn confirmation off.

### Limits and scope

Text and file orders have a default limit of **50 actions**, configurable from
**1 to 500** (`ordenes_max_acciones`). Proposals exceeding it are rejected before
publication or execution. This cap does not apply to Build/Moderation in the
app. Per-user cooldowns and concurrency controls also exist; they are not a
provider usage or cost budget.

Discord's effective permissions and role hierarchy limit the bot's reach.
Execution can fail partway through: approval does not provide a transaction
with rollback and cannot recover deleted content.

## Remaining risks

- **Prompt injection:** messages, names, descriptions, plans and files may
  contain malicious instructions. Isolation and validation do not make models
  infallible. Review the actions and arguments, not just the summary.
- **Model provider access:** the agent/model provider receives the context
  sent to it, including data the agent reads through its tools. ARGUS has no
  telemetry of its own; AI features can transmit messages, member data, plans
  and conversations to your chosen provider. Review the context and provider's
  terms before using sensitive data. Local token encryption does not hide
  that context from the model.
- **External MCP with everything allowed on your PC:** `argus-mcp` works with
  the app closed and exposes write tools. A client with automatic approval can
  modify or delete everything the bot's permissions and hierarchy allow,
  without the integrated assistant's Apply step. Discord order allowlists,
  Allow from Discord, confirmation and action caps do not protect this access.
  MCP does not itself grant general PC access, but an external agent with
  shell or file tools may also exercise those local permissions.
- **Compromised account or computer:** a stolen authorized account can issue
  orders; malware under your session can access data and credentials. ARGUS
  is not a boundary against your session owner or a trusted MCP client that
  becomes malicious.

## Recommended configuration

1. Run the bot **without Administrator**. Grant only permissions needed for the
   features you use, and place its role above only the roles it must manage.
   Fewer permissions reduce potential damage.
2. Keep the order user allowlist short. Restrict chat channels and disable
   orders when unnecessary.
3. Keep **confirmation enabled** and review the complete proposal before
   approval. Restrict Allow from Discord and lower the action cap as needed.
4. Connect MCP only to trusted clients and require approval for writes. Avoid
   unrestricted modes and review the agent's local tools. Test on a test server
   first.
5. Keep the keyring available, protect your session, and review and redact
   tokens, databases, conversations and logs before sharing anything.

See also the [assistant guide](ASISTENTE.md) and [MCP guide](MCP.md).

## Reporting a security problem

Do not publish tokens, keys, private data or an exploitable demonstration in a
public issue. Use **Security → Advisories → Report a vulnerability** in the
[ARGUS repository](https://github.com/BenReynor/ARGUS) if enabled. If no private
reporting channel is available, open an
[issue](https://github.com/BenReynor/ARGUS/issues) asking only for a private
contact channel, without vulnerability details or sensitive attachments.

In the private report, include the version or commit, operating system, affected
feature (app, orders or MCP), relevant configuration without secrets, steps
using fictitious data, expected and observed results, and impact. Do not test
on other people's servers. If a token was exposed, stop the affected bot and
MCP clients, regenerate the token in the Developer Portal and update it in
ARGUS. Deleting a message or log does not revoke a credential.
