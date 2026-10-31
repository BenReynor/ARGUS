-- contexto del servidor (temática, tono, historia, estilo de nombres), por bot y servidor
CREATE TABLE IF NOT EXISTS contexto_servidor (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    contenido TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (bot_id, guild_id)
);
