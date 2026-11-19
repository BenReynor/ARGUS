use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, Result};
use machine_uid;
use std::sync::OnceLock;

static LEGACY_KEY: OnceLock<[u8; 32]> = OnceLock::new();
static MASTER_KEY: OnceLock<CryptoContext> = OnceLock::new();

fn derive_master_key() -> [u8; 32] {
    *LEGACY_KEY.get_or_init(|| {
        let machine_id = machine_uid::get().unwrap_or_else(|_| "default-machine-id".to_string());
        let salt = b"argus-salt-v1";
        let mut key = [0u8; 32];

        // Simple key derivation using SHA256
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(salt);
        hasher.update(machine_id.as_bytes());
        hasher.update(b"argus-master-key-v1");
        let result = hasher.finalize();
        key.copy_from_slice(&result[..32]);
        key
    })
}

fn encrypt_with(plaintext: &str, key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new(key.into());

    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes)?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Combine nonce + ciphertext and encode as base64
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(base64::encode(combined))
}

fn decrypt_with(ciphertext_b64: &str, key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new(key.into());

    let combined = base64::decode(ciphertext_b64)
        .map_err(|e| anyhow::anyhow!("Base64 decode failed: {}", e))?;
    if combined.len() < 12 {
        return Err(anyhow::anyhow!("Invalid ciphertext: too short"));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;
    Ok(String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("UTF-8 decode failed: {}", e))?)
}

/// Interfaz inyectable: los tests nunca consultan el llavero del usuario.
pub trait KeyStore: Send + Sync {
    fn read(&self) -> Result<Option<Vec<u8>>>;
    fn write(&self, key: &[u8]) -> Result<()>;
}

struct SystemKeyStore;
impl KeyStore for SystemKeyStore {
    fn read(&self) -> Result<Option<Vec<u8>>> {
        let entry = keyring::Entry::new("argus", "master-key-v2")?;
        match entry.get_secret() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn write(&self, key: &[u8]) -> Result<()> {
        keyring::Entry::new("argus", "master-key-v2")?.set_secret(key)?;
        Ok(())
    }
}

// Sin Debug: una clave nunca debe aparecer en logs.
pub struct CryptoContext {
    key: Option<[u8; 32]>,
}
impl CryptoContext {
    fn from_store(store: &dyn KeyStore) -> Result<Self> {
        let key = match store.read() {
            Ok(Some(bytes)) => Some(
                bytes
                    .try_into()
                    .map_err(|_| anyhow!("Invalid keyring key length"))?,
            ),
            Ok(None) => {
                let mut key = [0; 32];
                getrandom::getrandom(&mut key)?;
                if store.write(&key).is_ok() {
                    // No usar una clave que el llavero no pueda devolver.
                    match store.read() {
                        Ok(Some(bytes)) if bytes == key => Some(key),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Err(_) => None,
        };
        Ok(Self { key })
    }

    pub fn keyring_available(&self) -> bool {
        self.key.is_some()
    }

    fn encrypt(&self, text: &str) -> Result<String> {
        match self.key {
            Some(key) => Ok(format!("keyring:v2:{}", encrypt_with(text, &key)?)),
            None => encrypt_with(text, &derive_master_key()),
        }
    }

    fn decrypt(&self, text: &str) -> Result<String> {
        if let Some(ciphertext) = text.strip_prefix("keyring:v2:") {
            let key = self.key.as_ref().ok_or_else(|| {
                anyhow!("Unlock the system keyring and restart ARGUS to access migrated tokens")
            })?;
            decrypt_with(ciphertext, key)
        } else {
            decrypt_with(text, &derive_master_key())
        }
    }
}

/// Serializa también dos arranques simultáneos (app y MCP) antes de crear la
/// clave. SQLite revierte todos los cambios si algún token no se puede migrar.
pub async fn initialize_with_store(
    pool: &sqlx::SqlitePool,
    store: std::sync::Arc<dyn KeyStore>,
) -> Result<CryptoContext> {
    use sqlx::Row;
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE config SET value = value WHERE key = 'config'")
        .execute(&mut *tx)
        .await?;
    let context =
        tokio::task::spawn_blocking(move || CryptoContext::from_store(store.as_ref())).await??;
    if context.keyring_available() {
        let rows = sqlx::query("SELECT id, token_ciphertext FROM bots")
            .fetch_all(&mut *tx)
            .await?;
        for row in rows {
            let old: String = row.get("token_ciphertext");
            if !old.starts_with("keyring:v2:") {
                let new = context.encrypt(&context.decrypt(&old)?)?;
                sqlx::query("UPDATE bots SET token_ciphertext = ? WHERE id = ?")
                    .bind(new)
                    .bind(row.get::<String, _>("id"))
                    .execute(&mut *tx)
                    .await?;
            } else {
                // Detectar una clave perdida/sustituida sin sobrescribir tokens.
                context.decrypt(&old)?;
            }
        }
        // Config puede contener copias de los bots y otras credenciales cifradas.
        if let Some(row) = sqlx::query("SELECT value FROM config WHERE key = 'config'")
            .fetch_optional(&mut *tx)
            .await?
        {
            let mut config: serde_json::Value = serde_json::from_str(row.get("value"))?;
            migrate_config(&mut config, &context)?;
            sqlx::query("UPDATE config SET value = ? WHERE key = 'config'")
                .bind(serde_json::to_string(&config)?)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(context)
}

fn migrate_config(value: &mut serde_json::Value, context: &CryptoContext) -> Result<()> {
    match value {
        serde_json::Value::Object(fields) => {
            for (name, value) in fields {
                if name.ends_with("_ciphertext") {
                    if let Some(text) = value.as_str().filter(|s| !s.is_empty()) {
                        let plain = context.decrypt(text)?;
                        if !text.starts_with("keyring:v2:") {
                            *value = context.encrypt(&plain)?.into();
                        }
                    }
                } else {
                    migrate_config(value, context)?;
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                migrate_config(value, context)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub async fn initialize(pool: &sqlx::SqlitePool) -> Result<()> {
    let context = initialize_with_store(pool, std::sync::Arc::new(SystemKeyStore)).await?;
    MASTER_KEY
        .set(context)
        .map_err(|_| anyhow!("Encryption is already initialized"))
}

fn context() -> Result<&'static CryptoContext> {
    // Los tests de comandos existentes usan solo cifrado local de prueba.
    #[cfg(test)]
    return Ok(MASTER_KEY.get_or_init(|| CryptoContext { key: None }));
    #[cfg(not(test))]
    MASTER_KEY
        .get()
        .ok_or_else(|| anyhow!("Encryption is not initialized"))
}

#[tauri::command]
pub fn keyring_available() -> bool {
    MASTER_KEY
        .get()
        .is_some_and(CryptoContext::keyring_available)
}

pub fn encrypt(plaintext: &str) -> Result<String> {
    context()?.encrypt(plaintext)
}
pub fn decrypt(ciphertext: &str) -> Result<String> {
    context()?.decrypt(ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeKeyStore {
        key: Mutex<Option<Vec<u8>>>,
        writes: AtomicUsize,
        fail_read: bool,
        fail_write: bool,
    }
    impl KeyStore for FakeKeyStore {
        fn read(&self) -> Result<Option<Vec<u8>>> {
            if self.fail_read {
                return Err(anyhow!("fake unavailable"));
            }
            Ok(self.key.lock().unwrap().clone())
        }
        fn write(&self, key: &[u8]) -> Result<()> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            if self.fail_write {
                return Err(anyhow!("fake locked"));
            }
            *self.key.lock().unwrap() = Some(key.to_vec());
            Ok(())
        }
    }

    async fn legacy_database() -> (sqlx::SqlitePool, Vec<String>) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let mut ciphertexts = Vec::new();
        for (id, token) in [
            ("fake-one", "fake-token-one"),
            ("fake-two", "fake-token-two"),
        ] {
            let ciphertext = encrypt_with(token, &derive_master_key()).unwrap();
            sqlx::query("INSERT INTO bots (id, name, token_ciphertext) VALUES (?, ?, ?)")
                .bind(id)
                .bind(id)
                .bind(&ciphertext)
                .execute(&pool)
                .await
                .unwrap();
            ciphertexts.push(ciphertext);
        }
        let config = serde_json::json!({
            "active_bot_id": "fake-two", "bots": [{"token_ciphertext": ciphertexts[0]}],
            "llm_chat_api_key_ciphertext": encrypt_with("fake-api-key", &derive_master_key()).unwrap(),
            "language": "es"
        });
        crate::db::set_config(&pool, "config", &config.to_string())
            .await
            .unwrap();
        (pool, ciphertexts)
    }

    async fn tokens(pool: &sqlx::SqlitePool) -> Vec<String> {
        sqlx::query_scalar("SELECT token_ciphertext FROM bots ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn migrates_all_bots_once_and_mcp_reuses_random_key() {
        let (pool, old) = legacy_database().await;
        let store = Arc::new(FakeKeyStore::default());
        let app = initialize_with_store(&pool, store.clone()).await.unwrap();
        assert!(app.keyring_available());
        let key = store.read().unwrap().unwrap();
        assert_eq!(key.len(), 32);
        assert_ne!(key, derive_master_key());
        let migrated = tokens(&pool).await;
        assert_ne!(migrated, old);
        for (ciphertext, token) in migrated.iter().zip(["fake-token-one", "fake-token-two"]) {
            assert!(ciphertext.starts_with("keyring:v2:"));
            assert_eq!(app.decrypt(ciphertext).unwrap(), token);
            assert!(decrypt_with(
                ciphertext.strip_prefix("keyring:v2:").unwrap(),
                &derive_master_key()
            )
            .is_err());
        }
        let config: serde_json::Value = serde_json::from_str(
            &crate::db::get_config(&pool, "config")
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(config["active_bot_id"], "fake-two");
        assert_eq!(config["language"], "es");
        assert_eq!(
            app.decrypt(config["bots"][0]["token_ciphertext"].as_str().unwrap())
                .unwrap(),
            "fake-token-one"
        );
        assert_eq!(
            app.decrypt(config["llm_chat_api_key_ciphertext"].as_str().unwrap())
                .unwrap(),
            "fake-api-key"
        );
        // Un segundo consumidor (MCP) obtiene la misma clave, sin volver a cifrar.
        let mcp = initialize_with_store(&pool, store.clone()).await.unwrap();
        assert_eq!(tokens(&pool).await, migrated);
        assert_eq!(store.writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            mcp.decrypt(&app.encrypt("shared-token").unwrap()).unwrap(),
            "shared-token"
        );
        assert_eq!(
            app.decrypt(&mcp.encrypt("shared-token").unwrap()).unwrap(),
            "shared-token"
        );
        let other = FakeKeyStore::default();
        let independent = CryptoContext::from_store(&other).unwrap();
        assert_ne!(app.key, independent.key);
    }

    #[tokio::test]
    async fn existing_key_is_never_replaced() {
        let (pool, _) = legacy_database().await;
        let store = Arc::new(FakeKeyStore {
            key: Mutex::new(Some(vec![17; 32])),
            ..Default::default()
        });
        let context = initialize_with_store(&pool, store.clone()).await.unwrap();
        assert_eq!(context.key, Some([17; 32]));
        assert_eq!(store.writes.load(Ordering::SeqCst), 0);
        assert_eq!(
            context.decrypt(&tokens(&pool).await[1]).unwrap(),
            "fake-token-two"
        );
    }

    #[tokio::test]
    async fn unavailable_or_unwritable_keyring_preserves_legacy_and_can_retry() {
        for (fail_read, fail_write) in [(true, false), (false, true)] {
            let (pool, old) = legacy_database().await;
            let store = Arc::new(FakeKeyStore {
                fail_read,
                fail_write,
                ..Default::default()
            });
            let context = initialize_with_store(&pool, store).await.unwrap();
            assert!(!context.keyring_available());
            assert_eq!(tokens(&pool).await, old);
            assert_eq!(context.decrypt(&old[0]).unwrap(), "fake-token-one");
            let new = context.encrypt("new-token").unwrap();
            assert_eq!(
                decrypt_with(&new, &derive_master_key()).unwrap(),
                "new-token"
            );
            let recovered = initialize_with_store(&pool, Arc::new(FakeKeyStore::default()))
                .await
                .unwrap();
            assert!(recovered.keyring_available());
            assert_eq!(
                recovered.decrypt(&tokens(&pool).await[0]).unwrap(),
                "fake-token-one"
            );
        }
    }

    #[tokio::test]
    async fn unavailable_keyring_does_not_modify_migrated_tokens() {
        let (pool, _) = legacy_database().await;
        let store = Arc::new(FakeKeyStore::default());
        initialize_with_store(&pool, store.clone()).await.unwrap();
        let saved = tokens(&pool).await;
        let fallback = initialize_with_store(
            &pool,
            Arc::new(FakeKeyStore {
                fail_read: true,
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        assert!(!fallback.keyring_available());
        assert_eq!(tokens(&pool).await, saved);
        assert!(fallback.decrypt(&saved[0]).is_err());
        let restored = initialize_with_store(&pool, store).await.unwrap();
        assert_eq!(restored.decrypt(&saved[0]).unwrap(), "fake-token-one");
    }

    #[tokio::test]
    async fn migration_failure_rolls_back_every_bot() {
        let (pool, old) = legacy_database().await;
        sqlx::query("UPDATE bots SET token_ciphertext = 'corrupt' WHERE id = 'fake-two'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            initialize_with_store(&pool, Arc::new(FakeKeyStore::default()))
                .await
                .is_err()
        );
        let saved = tokens(&pool).await;
        assert_eq!(saved[0], old[0]);
        assert_eq!(saved[1], "corrupt");
        assert_eq!(crate::db::list_bots(&pool).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn invalid_or_replaced_key_never_overwrites_tokens() {
        let (pool, old) = legacy_database().await;
        let malformed = Arc::new(FakeKeyStore {
            key: Mutex::new(Some(vec![1; 5])),
            ..Default::default()
        });
        assert!(initialize_with_store(&pool, malformed.clone())
            .await
            .is_err());
        assert_eq!(malformed.writes.load(Ordering::SeqCst), 0);
        assert_eq!(tokens(&pool).await, old);
        initialize_with_store(&pool, Arc::new(FakeKeyStore::default()))
            .await
            .unwrap();
        let migrated = tokens(&pool).await;
        let replaced = Arc::new(FakeKeyStore {
            key: Mutex::new(Some(vec![2; 32])),
            ..Default::default()
        });
        assert!(initialize_with_store(&pool, replaced).await.is_err());
        assert_eq!(tokens(&pool).await, migrated);
    }

    #[test]
    fn test_encrypt_decrypt() {
        let plaintext = "test-token-12345";
        let encrypted = encrypt(plaintext).unwrap();
        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_different_outputs() {
        let plaintext = "test";
        let e1 = encrypt(plaintext).unwrap();
        let e2 = encrypt(plaintext).unwrap();
        // Different nonces should produce different ciphertexts
        assert_ne!(e1, e2);
        // But both decrypt to same plaintext
        assert_eq!(decrypt(&e1).unwrap(), plaintext);
        assert_eq!(decrypt(&e2).unwrap(), plaintext);
    }
}

/// Cifrado portátil: Argon2id (19 MiB, 2 pasadas) + AES-256-GCM.
/// La sal y el nonce son aleatorios; ninguna clave de esta máquina participa.
pub fn encrypt_backup(text: &str, password: &str) -> Result<Vec<u8>> {
    if password.len() < 8 {
        anyhow::bail!("Use a backup password with at least 8 characters");
    }
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt)?;
    let key = backup_key(password, &salt)?;
    let encrypted = encrypt_with(text, &key)?;
    let mut file = b"ARGUSBAK\x01".to_vec();
    file.extend_from_slice(&salt);
    file.extend_from_slice(encrypted.as_bytes());
    Ok(file)
}

pub fn decrypt_backup(file: &[u8], password: &str) -> Result<String> {
    if file.len() < 26 || !file.starts_with(b"ARGUSBAK\x01") {
        anyhow::bail!("Invalid or unsupported backup file");
    }
    let key = backup_key(password, &file[9..25])?;
    let encoded = std::str::from_utf8(&file[25..])
        .map_err(|_| anyhow!("Invalid or unsupported backup file"))?;
    decrypt_with(encoded, &key).map_err(|_| anyhow!("Incorrect backup password or damaged file"))
}

fn backup_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|_| anyhow!("Could not derive the backup key"))?;
    Ok(key)
}

#[cfg(test)]
mod portable_backup_tests {
    use super::*;

    #[test]
    fn password_backup_restores_between_independent_machine_keys() {
        let source = CryptoContext {
            key: Some([11; 32]),
        };
        let destination = CryptoContext {
            key: Some([29; 32]),
        };
        let local = source.encrypt("fake-portable-secret").unwrap();
        assert!(destination.decrypt(&local).is_err());
        let file = encrypt_backup(&source.decrypt(&local).unwrap(), "fixture password").unwrap();
        let recovered = decrypt_backup(&file, "fixture password").unwrap();
        let restored = destination.encrypt(&recovered).unwrap();
        assert_eq!(
            destination.decrypt(&restored).unwrap(),
            "fake-portable-secret"
        );
        assert!(source.decrypt(&restored).is_err());
        assert!(decrypt_backup(&file, "wrong password").is_err());
        assert_ne!(
            file,
            encrypt_backup(&recovered, "fixture password").unwrap()
        );
        assert!(encrypt_backup(&recovered, "").is_err());
    }
}
