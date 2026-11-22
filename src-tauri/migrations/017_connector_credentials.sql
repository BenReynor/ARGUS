CREATE TABLE IF NOT EXISTS connector_credentials (
    service TEXT NOT NULL,
    account TEXT NOT NULL,
    guild_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    credential_ciphertext TEXT NOT NULL,
    expires_at INTEGER,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (service, account, guild_id)
);
