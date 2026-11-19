-- Roles pendientes por bot/servidor, con IDs resueltos tras Apply.
CREATE TABLE snapshot_return_roles (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    members TEXT NOT NULL,
    PRIMARY KEY(bot_id, guild_id)
);
