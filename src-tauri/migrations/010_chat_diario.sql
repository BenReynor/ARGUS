-- Cupo independiente de los ajustes: guardarlos no puede reiniciar el contador.
CREATE TABLE chat_diario (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    dia TEXT NOT NULL,
    respuestas INTEGER NOT NULL DEFAULT 0,
    avisado INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (bot_id, guild_id, dia)
);
