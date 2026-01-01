use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use thiserror::Error;
use tracing::info;
#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("Encryption failed: {0}")]
    Encryption(String),
    #[error("Decryption failed: {0}")]
    Decryption(String),
    #[error("Key management error: {0}")]
    KeyManagement(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
/// Manages encryption keys and provides AES-256-GCM encryption/decryption
///
/// This implements the security model described in the README:
/// - Files are encrypted using AES-256-GCM for authenticated encryption
/// - Each file gets a unique nonce for security
#[derive(Clone)]
pub struct EncryptionService {
    cipher: Aes256Gcm,
}
impl std::fmt::Debug for EncryptionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionService")
            .field("cipher", &"<initialized>")
            .finish()
    }
}
impl EncryptionService {
    /// Create a new encryption service, loading the key from config
    pub fn new(config: &crate::config::Config) -> Result<Self, EncryptionError> {
        info!("Loading master key...");
        let key_bytes = hex::decode(&config.encryption_key)
            .map_err(|e| EncryptionError::KeyManagement(format!("Invalid key format: {}", e)))?;
        if key_bytes.len() != 32 {
            return Err(EncryptionError::KeyManagement(
                "Invalid key length, expected 32 bytes".to_string(),
            ));
        }
        let key_array: [u8; 32] = key_bytes.try_into().map_err(|_| {
            EncryptionError::KeyManagement("Failed to convert key bytes to array".to_string())
        })?;
        let key = Key::<Aes256Gcm>::from_slice(&key_array);
        let cipher = Aes256Gcm::new(key);
        Ok(EncryptionService { cipher })
    }
    /// Create an encryption service with a raw key (for testing)
    #[cfg(feature = "test-utils")]
    #[allow(unused)]
    pub fn new_with_key(key_bytes: Vec<u8>) -> Self {
        if key_bytes.len() != 32 {
            panic!("Invalid key length, expected 32 bytes");
        }
        let key_array: [u8; 32] = key_bytes.try_into().unwrap();
        let key = Key::<Aes256Gcm>::from_slice(&key_array);
        let cipher = Aes256Gcm::new(key);
        EncryptionService { cipher }
    }
    /// Encrypt data with AES-256-GCM
    /// Returns (encrypted_data, nonce) - both needed for decryption
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), EncryptionError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self.cipher.encrypt(&nonce, plaintext).map_err(|e| {
            EncryptionError::Encryption(format!("AES-GCM encryption failed: {}", e))
        })?;
        Ok((ciphertext, nonce.to_vec()))
    }
    /// Decrypt data with AES-256-GCM
    /// Requires both the encrypted data and the nonce used during encryption
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if nonce.len() != 12 {
            return Err(EncryptionError::Decryption(
                "Invalid nonce length, expected 12 bytes".to_string(),
            ));
        }
        let nonce_array: [u8; 12] = nonce.try_into().map_err(|_| {
            EncryptionError::Decryption("Failed to convert nonce bytes to array".to_string())
        })?;
        let nonce = Nonce::from_slice(&nonce_array);
        let plaintext = self.cipher.decrypt(nonce, ciphertext).map_err(|e| {
            EncryptionError::Decryption(format!("AES-GCM decryption failed: {}", e))
        })?;
        Ok(plaintext)
    }
    /// Decrypt data in simple format: [nonce (12 bytes)][ciphertext]
    /// This is used by ReleaseStorageImpl which prepends nonce to ciphertext
    pub fn decrypt_simple(&self, encrypted_data: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        if encrypted_data.len() < 12 {
            return Err(EncryptionError::Decryption(
                "Invalid encrypted data: too short for nonce".to_string(),
            ));
        }
        let (nonce, ciphertext) = encrypted_data.split_at(12);
        self.decrypt(ciphertext, nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Create a test encryption service with a pre-populated test key (avoids keyring)
    fn create_test_encryption_service() -> EncryptionService {
        let test_key = Aes256Gcm::generate_key(OsRng);
        let test_key_hex = hex::encode(test_key.as_ref() as &[u8]);
        let test_config = crate::config::Config {
            library_id: "test-library".to_string(),
            discogs_api_key: Some("test-key".to_string()),
            encryption_key: test_key_hex,
            torrent_bind_interface: None,
        };
        EncryptionService::new(&test_config).expect("Failed to create test encryption service")
    }

    #[test]
    fn test_encryption_roundtrip() {
        let encryption_service = create_test_encryption_service();
        let plaintext = b"Hello, world! This is a test message for encryption.";
        let (ciphertext, nonce) = encryption_service.encrypt(plaintext).unwrap();
        assert_ne!(ciphertext, plaintext);
        assert_eq!(nonce.len(), 12);
        let decrypted = encryption_service.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces() {
        let encryption_service = create_test_encryption_service();
        let plaintext = b"Same message";
        let (ciphertext1, nonce1) = encryption_service.encrypt(plaintext).unwrap();
        let (ciphertext2, nonce2) = encryption_service.encrypt(plaintext).unwrap();
        assert_ne!(nonce1, nonce2);
        assert_ne!(ciphertext1, ciphertext2);
        let decrypted1 = encryption_service.decrypt(&ciphertext1, &nonce1).unwrap();
        let decrypted2 = encryption_service.decrypt(&ciphertext2, &nonce2).unwrap();
        assert_eq!(decrypted1, plaintext);
        assert_eq!(decrypted2, plaintext);
    }
}
