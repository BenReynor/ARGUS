use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Error as AeadError, Nonce,
};
use anyhow::{anyhow, Result};
use machine_uid;
use std::sync::OnceLock;

static MASTER_KEY: OnceLock<[u8; 32]> = OnceLock::new();

fn derive_master_key() -> [u8; 32] {
    *MASTER_KEY.get_or_init(|| {
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

pub fn encrypt(plaintext: &str) -> Result<String> {
    let key = derive_master_key();
    let cipher = Aes256Gcm::new(&key.into());

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

pub fn decrypt(ciphertext_b64: &str) -> Result<String> {
    let key = derive_master_key();
    let cipher = Aes256Gcm::new(&key.into());

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

#[cfg(test)]
mod tests {
    use super::*;

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
