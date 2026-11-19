-- Propuestas compartidas entre argus-mcp y la app; nunca ejecutadas al crearlas.
CREATE TABLE snapshot_restore_proposals (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    data TEXT NOT NULL
);
