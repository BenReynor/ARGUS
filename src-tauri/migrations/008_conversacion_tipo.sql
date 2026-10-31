-- Cada servidor tiene una conversación por tipo (construccion o moderacion).
-- Las existentes pasan a `construccion`.
CREATE TABLE conversacion_asistente_nueva (
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    tipo TEXT NOT NULL DEFAULT 'construccion',
    agente TEXT NOT NULL,
    modelo TEXT NOT NULL DEFAULT '',
    session_id TEXT,
    mensajes TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (bot_id, guild_id, tipo)
);
INSERT INTO conversacion_asistente_nueva (bot_id, guild_id, tipo, agente, modelo, session_id, mensajes, updated_at)
    SELECT bot_id, guild_id, 'construccion', agente, modelo, session_id, mensajes, updated_at
    FROM conversacion_asistente;
DROP TABLE conversacion_asistente;
ALTER TABLE conversacion_asistente_nueva RENAME TO conversacion_asistente;
