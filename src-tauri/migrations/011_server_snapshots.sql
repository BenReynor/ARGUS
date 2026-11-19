-- Snapshots independientes por bot y servidor; el JSON incluye las imágenes.
CREATE TABLE server_snapshots (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    summary TEXT NOT NULL,
    data TEXT NOT NULL,
    automatic INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX server_snapshots_scope ON server_snapshots(bot_id, guild_id, created_at);
CREATE TABLE server_snapshot_schedules (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    keep_last INTEGER NOT NULL DEFAULT 7 CHECK(keep_last BETWEEN 1 AND 365),
    last_success TEXT,
    last_error TEXT,
    PRIMARY KEY(bot_id, guild_id)
);
