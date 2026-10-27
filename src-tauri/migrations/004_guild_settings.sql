-- ajustes propios de cada servidor, por bot (data = JSON de GuildSettings)
CREATE TABLE IF NOT EXISTS guild_settings (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    data TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (bot_id, guild_id)
);
