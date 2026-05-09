//! Simple embedded encryption for Kubernetes secrets
//!
//! Uses AES-256-GCM for encrypting secret data at rest

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;
use std::path::Path;

/// Encryption key size (32 bytes for AES-256)
const KEY_SIZE: usize = 32;

/// Nonce size for GCM (12 bytes)
const NONCE_SIZE: usize = 12;

/// Simple encryption provider for secrets
pub struct SecretEncryption {
    cipher: Aes256Gcm,
}

impl SecretEncryption {
    /// Load or generate encryption key from file
    pub fn load_or_generate(key_path: &Path) -> std::io::Result<Self> {
        let key_bytes = if key_path.exists() {
            // Load existing key
            std::fs::read(key_path)?
        } else {
            // Generate new key
            let mut key = vec![0u8; KEY_SIZE];
            OsRng.fill_bytes(&mut key);

            // Ensure parent directory exists
            if let Some(parent) = key_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Save key with restricted permissions
            std::fs::write(key_path, &key)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600))?;
            }

            tracing::info!("Generated new encryption key at {:?}", key_path);
            key
        };

        if key_bytes.len() != KEY_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid key size: expected {}, got {}", KEY_SIZE, key_bytes.len())
            ));
        }

        let key: &Key<Aes256Gcm> = key_bytes.as_slice().into();
        let cipher = Aes256Gcm::new(key);

        Ok(Self { cipher })
    }

    /// Encrypt data
    /// Returns: base64(nonce || ciphertext)
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<String, String> {
        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("Encryption failed: {e}"))?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);

        // Base64 encode
        Ok(BASE64.encode(&result))
    }

    /// Decrypt data
    /// Expects: base64(nonce || ciphertext)
    pub fn decrypt(&self, encrypted: &str) -> Result<Vec<u8>, String> {
        // Base64 decode
        let data = BASE64.decode(encrypted)
            .map_err(|e| format!("Failed to decode base64: {e}"))?;

        if data.len() < NONCE_SIZE {
            return Err("Invalid encrypted data: too short".to_string());
        }

        // Extract nonce and ciphertext
        let (nonce_bytes, ciphertext) = data.split_at(NONCE_SIZE);
        let nonce = Nonce::from_slice(nonce_bytes);

        // Decrypt
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_encryption_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("key");

        let enc = SecretEncryption::load_or_generate(&key_path).unwrap();

        let plaintext = b"my-secret-password";
        let encrypted = enc.encrypt(plaintext).unwrap();
        let decrypted = enc.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_key_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("key");

        // Generate key
        let enc1 = SecretEncryption::load_or_generate(&key_path).unwrap();
        let encrypted = enc1.encrypt(b"test").unwrap();

        // Load same key
        let enc2 = SecretEncryption::load_or_generate(&key_path).unwrap();
        let decrypted = enc2.decrypt(&encrypted).unwrap();

        assert_eq!(b"test".to_vec(), decrypted);
    }
}
