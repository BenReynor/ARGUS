-- avisos (warnings) a miembros de un servidor
CREATE TABLE IF NOT EXISTS avisos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    motivo TEXT NOT NULL,
    mensaje_id TEXT,
    autor TEXT NOT NULL DEFAULT '',
    fecha TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_avisos_guild_user ON avisos (guild_id, user_id);
