-- memoria breve por persona del chat de Discord (opcional): una nota por bot, servidor y usuario
CREATE TABLE IF NOT EXISTS chat_memoria (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    nota TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (bot_id, guild_id, user_id)
);
