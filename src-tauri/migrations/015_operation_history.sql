CREATE TABLE operations (
 id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, guild_id TEXT NOT NULL,
 destination TEXT, status TEXT NOT NULL, reason TEXT,
 started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 finished_at TEXT
);
CREATE INDEX operations_scope ON operations(bot_id, guild_id, started_at);
CREATE TABLE operation_steps (
 operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
 step_index INTEGER NOT NULL, action TEXT NOT NULL, destination TEXT, status TEXT NOT NULL,
 reason TEXT, started_at TEXT, finished_at TEXT,
 PRIMARY KEY(operation_id, step_index)
);
