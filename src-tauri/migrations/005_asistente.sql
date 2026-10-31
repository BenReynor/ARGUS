-- plan del servidor (Markdown) que el usuario escribe para el asistente IA, por bot y servidor
CREATE TABLE IF NOT EXISTS plan_servidor (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    contenido TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (bot_id, guild_id)
);

-- conversación con el agente CLI, por bot y servidor (mensajes = JSON);
-- session_id permite reanudarla con `--resume` (claude) o `--session` (opencode)
CREATE TABLE IF NOT EXISTS conversacion_asistente (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    agente TEXT NOT NULL,
    modelo TEXT NOT NULL DEFAULT '',
    session_id TEXT,
    mensajes TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (bot_id, guild_id)
);
