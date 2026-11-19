-- Conservar las elecciones existentes, incluida la frecuencia diaria anterior.
CREATE TABLE server_snapshot_schedules_new (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    keep_last INTEGER NOT NULL DEFAULT 14 CHECK(keep_last BETWEEN 1 AND 365),
    interval_hours INTEGER NOT NULL DEFAULT 12 CHECK(interval_hours IN (6, 12, 24)),
    last_success TEXT,
    last_error TEXT,
    PRIMARY KEY(bot_id, guild_id)
);
INSERT INTO server_snapshot_schedules_new
    (bot_id, guild_id, enabled, keep_last, interval_hours, last_success, last_error)
SELECT bot_id, guild_id, enabled, keep_last, 24,
    CASE WHEN length(last_success) = 10 THEN last_success || 'T00:00:00Z' ELSE last_success END,
    last_error
FROM server_snapshot_schedules;
DROP TABLE server_snapshot_schedules;
ALTER TABLE server_snapshot_schedules_new RENAME TO server_snapshot_schedules;
