-- config key-value (JSON)
CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP);

-- bots configurados
CREATE TABLE bots (id TEXT PRIMARY KEY, name TEXT NOT NULL, token_ciphertext TEXT NOT NULL, allowed_guilds TEXT NOT NULL DEFAULT '[]', created_at DATETIME DEFAULT CURRENT_TIMESTAMP, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP);

-- guilds cache
CREATE TABLE guilds (id TEXT PRIMARY KEY, name TEXT, icon TEXT, member_count INTEGER, joined_at DATETIME);

-- server_maps por guild
CREATE TABLE server_maps (guild_id TEXT PRIMARY KEY, data TEXT NOT NULL, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP);

-- plantillas
CREATE TABLE templates (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT, data TEXT NOT NULL, version INTEGER DEFAULT 1, created_at DATETIME DEFAULT CURRENT_TIMESTAMP, updated_at DATETIME DEFAULT CURRENT_TIMESTAMP);

-- agentes CLI
CREATE TABLE cli_agents (id TEXT PRIMARY KEY, name TEXT NOT NULL, mode TEXT NOT NULL DEFAULT 'auto', binary_path TEXT, model TEXT, config TEXT NOT NULL DEFAULT '{}', enabled INTEGER DEFAULT 1);

-- vigilancia
CREATE TABLE vigilance (id INTEGER PRIMARY KEY, preset TEXT NOT NULL DEFAULT 'standard', enabled_events TEXT NOT NULL DEFAULT '[]', filters TEXT NOT NULL DEFAULT '{}');

-- chat history (contexto activo)
CREATE TABLE messages (id INTEGER PRIMARY KEY, guild_id TEXT NOT NULL, channel_id TEXT NOT NULL, user_id TEXT NOT NULL, username TEXT, content TEXT NOT NULL, role TEXT NOT NULL, provider TEXT, timestamp DATETIME NOT NULL, is_archived INTEGER DEFAULT 0);
CREATE INDEX idx_msg_guild_user ON messages(guild_id, user_id);
CREATE INDEX idx_msg_channel_time ON messages(channel_id, timestamp);
CREATE INDEX idx_msg_guild_channel_time ON messages(guild_id, channel_id, timestamp);

-- FTS5 búsqueda
CREATE VIRTUAL TABLE messages_fts USING fts5(content, username, guild_id, channel_id, user_id, content='messages', content_rowid='id');
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN INSERT INTO messages_fts(rowid, content, username, guild_id, channel_id, user_id) VALUES (new.id, new.content, new.username, new.guild_id, new.channel_id, new.user_id); END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN INSERT INTO messages_fts(messages_fts, rowid, content, username, guild_id, channel_id, user_id) VALUES ('delete', old.id, old.content, old.username, old.guild_id, old.channel_id, old.user_id); END;
CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN INSERT INTO messages_fts(messages_fts, rowid, content, username, guild_id, channel_id, user_id) VALUES ('delete', old.id, old.content, old.username, old.guild_id, old.channel_id, old.user_id); INSERT INTO messages_fts(rowid, content, username, guild_id, channel_id, user_id) VALUES (new.id, new.content, new.username, new.guild_id, new.channel_id, new.user_id); END;

-- incidents (estilo Dédalo)
CREATE TABLE incidents (id INTEGER PRIMARY KEY, guild_id TEXT, type TEXT NOT NULL, payload TEXT NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
CREATE INDEX idx_incidents_guild_type ON incidents(guild_id, type);
CREATE INDEX idx_incidents_created ON incidents(created_at);

-- settings globales
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);