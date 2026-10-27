-- id de aplicación del bot, leído de GET /applications/@me (para el enlace de invitación)
ALTER TABLE bots ADD COLUMN application_id TEXT;
