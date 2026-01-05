use crate::sodium_ffi;
use std::ptr;
use std::sync::Once;
use thiserror::Error;
use tracing::info;

/// 64KB plaintext chunks
pub const CHUNK_SIZE: usize = 65536;
/// Each encrypted chunk: plaintext + 16-byte auth tag
pub const ENCRYPTED_CHUNK_SIZE: usize = CHUNK_SIZE + sodium_ffi::ABYTES;

static SODIUM_INIT: Once = Once::new();

/// Ensure libsodium is initialized. Safe to call multiple times.
pub fn ensure_sodium_init() {
    SODIUM_INIT.call_once(|| {
        let result = unsafe { sodium_ffi::sodium_init() };
        if result < 0 {
            panic!("Failed to initialize libsodium");
        }
    });
}

/// Generate a random 32-byte key using libsodium's secure random.
pub fn generate_random_key() -> [u8; 32] {
    ensure_sodium_init();
    let mut key = [0u8; 32];
    unsafe { sodium_ffi::randombytes_buf(key.as_mut_ptr(), 32) };
    key
}

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
/// Manages encryption keys and provides XChaCha20-Poly1305 encryption/decryption
///
/// This implements the security model described in the README:
/// - Files are encrypted using XChaCha20-Poly1305 for authenticated encryption
/// - Chunked format enables random-access decryption for efficient range reads
#[derive(Clone)]
pub struct EncryptionService {
    key: [u8; 32],
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
        let key: [u8; 32] = key_bytes.try_into().map_err(|_| {
            EncryptionError::KeyManagement("Failed to convert key bytes to array".to_string())
        })?;
        Ok(EncryptionService { key })
    }

    /// Create an encryption service with a raw key (for testing)
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_with_key(key_bytes: &[u8]) -> Self {
        if key_bytes.len() != 32 {
            panic!("Invalid key length, expected 32 bytes");
        }
        let key: [u8; 32] = key_bytes.try_into().unwrap();
        EncryptionService { key }
    }

    /// Encrypt data using chunked XChaCha20-Poly1305 format.
    /// Returns: [base_nonce: 24 bytes][ciphertext with auth tags]
    /// For small data (single chunk), this is equivalent to standard AEAD.
    /// For large data, each chunk is independently encrypted for random-access.
    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        self.encrypt_chunked(plaintext)
    }

    /// Decrypt data in chunked format: [nonce (24 bytes)][ciphertext chunks...]
    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        self.decrypt_chunked(encrypted_data)
    }

    /// Encrypt data using chunked XChaCha20-Poly1305 format.
    /// Returns: [base_nonce: 24 bytes][chunk_0][chunk_1]...
    /// Each chunk is independently encrypted, enabling random-access decryption.
    pub fn encrypt_chunked(&self, plaintext: &[u8]) -> Vec<u8> {
        ensure_sodium_init();

        // Generate random base nonce
        let mut base_nonce = [0u8; sodium_ffi::NPUBBYTES];
        unsafe {
            sodium_ffi::randombytes_buf(base_nonce.as_mut_ptr(), sodium_ffi::NPUBBYTES);
        }

        let mut output = base_nonce.to_vec();

        // Handle empty plaintext - still produce one chunk with just auth tag
        if plaintext.is_empty() {
            let nonce = chunk_nonce(&base_nonce, 0);
            let mut ciphertext = vec![0u8; sodium_ffi::ABYTES];
            let mut ciphertext_len: u64 = 0;

            unsafe {
                sodium_ffi::crypto_aead_xchacha20poly1305_ietf_encrypt(
                    ciphertext.as_mut_ptr(),
                    &mut ciphertext_len,
                    ptr::null(),
                    0,
                    ptr::null(),
                    0,
                    ptr::null(),
                    nonce.as_ptr(),
                    self.key.as_ptr(),
                );
            }

            output.extend(&ciphertext[..ciphertext_len as usize]);
            return output;
        }

        for (i, chunk) in plaintext.chunks(CHUNK_SIZE).enumerate() {
            let nonce = chunk_nonce(&base_nonce, i as u64);
            let mut ciphertext = vec![0u8; chunk.len() + sodium_ffi::ABYTES];
            let mut ciphertext_len: u64 = 0;

            unsafe {
                sodium_ffi::crypto_aead_xchacha20poly1305_ietf_encrypt(
                    ciphertext.as_mut_ptr(),
                    &mut ciphertext_len,
                    chunk.as_ptr(),
                    chunk.len() as u64,
                    ptr::null(),
                    0,
                    ptr::null(),
                    nonce.as_ptr(),
                    self.key.as_ptr(),
                );
            }

            output.extend(&ciphertext[..ciphertext_len as usize]);
        }

        output
    }

    /// Decrypt a specific chunk from chunked encrypted data.
    /// Enables random-access decryption without reading preceding chunks.
    pub fn decrypt_chunk(
        &self,
        ciphertext: &[u8],
        chunk_index: usize,
    ) -> Result<Vec<u8>, EncryptionError> {
        ensure_sodium_init();

        if ciphertext.len() < sodium_ffi::NPUBBYTES {
            return Err(EncryptionError::Decryption(
                "Ciphertext too short for nonce".to_string(),
            ));
        }

        let base_nonce: [u8; sodium_ffi::NPUBBYTES] = ciphertext[..sodium_ffi::NPUBBYTES]
            .try_into()
            .map_err(|_| EncryptionError::Decryption("Invalid nonce".to_string()))?;

        let data_start = sodium_ffi::NPUBBYTES;
        let total_data_len = ciphertext.len() - data_start;

        // Calculate chunk boundaries
        let num_full_chunks = total_data_len / ENCRYPTED_CHUNK_SIZE;
        let has_partial = !total_data_len.is_multiple_of(ENCRYPTED_CHUNK_SIZE);
        let total_chunks = num_full_chunks + if has_partial { 1 } else { 0 };

        if chunk_index >= total_chunks {
            return Err(EncryptionError::Decryption(format!(
                "Chunk index {} out of range (total chunks: {})",
                chunk_index, total_chunks
            )));
        }

        let chunk_start = data_start + chunk_index * ENCRYPTED_CHUNK_SIZE;
        let chunk_end = if chunk_index == total_chunks - 1 && has_partial {
            ciphertext.len()
        } else {
            chunk_start + ENCRYPTED_CHUNK_SIZE
        };

        let chunk_data = &ciphertext[chunk_start..chunk_end];
        let nonce = chunk_nonce(&base_nonce, chunk_index as u64);

        let mut plaintext = vec![0u8; chunk_data.len() - sodium_ffi::ABYTES];
        let mut plaintext_len: u64 = 0;

        let result = unsafe {
            sodium_ffi::crypto_aead_xchacha20poly1305_ietf_decrypt(
                plaintext.as_mut_ptr(),
                &mut plaintext_len,
                ptr::null_mut(),
                chunk_data.as_ptr(),
                chunk_data.len() as u64,
                ptr::null(),
                0,
                nonce.as_ptr(),
                self.key.as_ptr(),
            )
        };

        if result != 0 {
            return Err(EncryptionError::Decryption(
                "Authentication failed".to_string(),
            ));
        }

        plaintext.truncate(plaintext_len as usize);
        Ok(plaintext)
    }

    /// Decrypt all chunks from chunked encrypted data.
    pub fn decrypt_chunked(&self, ciphertext: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        ensure_sodium_init();

        if ciphertext.len() < sodium_ffi::NPUBBYTES {
            return Err(EncryptionError::Decryption(
                "Ciphertext too short for nonce".to_string(),
            ));
        }

        let data_start = sodium_ffi::NPUBBYTES;
        let total_data_len = ciphertext.len() - data_start;

        let num_full_chunks = total_data_len / ENCRYPTED_CHUNK_SIZE;
        let has_partial = !total_data_len.is_multiple_of(ENCRYPTED_CHUNK_SIZE);
        let total_chunks = num_full_chunks + if has_partial { 1 } else { 0 };

        let mut result = Vec::new();
        for i in 0..total_chunks {
            let chunk = self.decrypt_chunk(ciphertext, i)?;
            result.extend(chunk);
        }

        Ok(result)
    }

    /// Decrypt a specific plaintext byte range from encrypted data.
    ///
    /// The ciphertext must start with the nonce (first 24 bytes) but may be
    /// truncated after the chunks needed for the requested range.
    ///
    /// Returns exactly the plaintext bytes from `plaintext_start` to `plaintext_end`.
    pub fn decrypt_range(
        &self,
        ciphertext: &[u8],
        plaintext_start: u64,
        plaintext_end: u64,
    ) -> Result<Vec<u8>, EncryptionError> {
        ensure_sodium_init();

        if plaintext_start >= plaintext_end {
            return Err(EncryptionError::Decryption(format!(
                "Invalid range: start ({}) >= end ({})",
                plaintext_start, plaintext_end
            )));
        }

        let start_chunk = plaintext_start / CHUNK_SIZE as u64;
        let end_chunk = (plaintext_end.saturating_sub(1)) / CHUNK_SIZE as u64;

        let mut plaintext = Vec::new();
        for chunk_idx in start_chunk..=end_chunk {
            let chunk = self.decrypt_chunk(ciphertext, chunk_idx as usize)?;
            plaintext.extend(chunk);
        }

        // Slice to exact range within the decrypted chunks
        let offset_in_first_chunk = (plaintext_start % CHUNK_SIZE as u64) as usize;
        let len = (plaintext_end - plaintext_start) as usize;
        let end = offset_in_first_chunk + len;

        if end > plaintext.len() {
            return Err(EncryptionError::Decryption(format!(
                "Decrypted data too short: need {} bytes, got {}",
                end,
                plaintext.len()
            )));
        }

        Ok(plaintext[offset_in_first_chunk..end].to_vec())
    }
}

/// Derive nonce for chunk i: base_nonce XOR i (little-endian)
fn chunk_nonce(
    base_nonce: &[u8; sodium_ffi::NPUBBYTES],
    chunk_index: u64,
) -> [u8; sodium_ffi::NPUBBYTES] {
    let mut nonce = *base_nonce;
    let index_bytes = chunk_index.to_le_bytes();
    for i in 0..8 {
        nonce[i] ^= index_bytes[i];
    }
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calculate the encrypted byte range needed for a plaintext byte range.
    /// Returns (encrypted_start, encrypted_end) including the nonce header.
    fn encrypted_range_for_plaintext(start: u64, end: u64) -> (u64, u64) {
        let start_chunk = start / CHUNK_SIZE as u64;
        let end_chunk = (end.saturating_sub(1)) / CHUNK_SIZE as u64;

        let enc_start = sodium_ffi::NPUBBYTES as u64 + start_chunk * ENCRYPTED_CHUNK_SIZE as u64;
        let enc_end = sodium_ffi::NPUBBYTES as u64 + (end_chunk + 1) * ENCRYPTED_CHUNK_SIZE as u64;

        // Always need the nonce header
        (0, enc_end.max(enc_start))
    }

    fn test_key() -> [u8; 32] {
        // Fixed test key for reproducibility
        [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ]
    }

    fn create_test_service() -> EncryptionService {
        EncryptionService::new_with_key(&test_key())
    }

    #[test]
    fn test_roundtrip_small() {
        let service = create_test_service();
        let plaintext = b"Hello, world!";

        let ciphertext = service.encrypt(plaintext);
        let decrypted = service.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_roundtrip_exact_chunk() {
        let service = create_test_service();
        let plaintext = vec![0x42u8; CHUNK_SIZE];

        let ciphertext = service.encrypt(&plaintext);
        let decrypted = service.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_roundtrip_multiple_chunks() {
        let service = create_test_service();
        // 2.5 chunks worth of data
        let plaintext: Vec<u8> = (0..CHUNK_SIZE * 2 + CHUNK_SIZE / 2)
            .map(|i| (i % 256) as u8)
            .collect();

        let ciphertext = service.encrypt(&plaintext);
        let decrypted = service.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_random_access_chunk() {
        let service = create_test_service();
        // 3 chunks: chunk 0 = 0x00, chunk 1 = 0x11, chunk 2 = 0x22
        let mut plaintext = vec![0x00u8; CHUNK_SIZE];
        plaintext.extend(vec![0x11u8; CHUNK_SIZE]);
        plaintext.extend(vec![0x22u8; CHUNK_SIZE]);

        let ciphertext = service.encrypt(&plaintext);

        // Decrypt only chunk 1 (middle chunk)
        let chunk1 = service.decrypt_chunk(&ciphertext, 1).unwrap();
        assert_eq!(chunk1, vec![0x11u8; CHUNK_SIZE]);

        // Decrypt chunk 0
        let chunk0 = service.decrypt_chunk(&ciphertext, 0).unwrap();
        assert_eq!(chunk0, vec![0x00u8; CHUNK_SIZE]);

        // Decrypt chunk 2
        let chunk2 = service.decrypt_chunk(&ciphertext, 2).unwrap();
        assert_eq!(chunk2, vec![0x22u8; CHUNK_SIZE]);
    }

    #[test]
    fn test_random_access_partial_last_chunk() {
        let service = create_test_service();
        // 1 full chunk + partial chunk
        let mut plaintext = vec![0xAAu8; CHUNK_SIZE];
        plaintext.extend(vec![0xBBu8; 100]);

        let ciphertext = service.encrypt(&plaintext);

        let chunk0 = service.decrypt_chunk(&ciphertext, 0).unwrap();
        assert_eq!(chunk0, vec![0xAAu8; CHUNK_SIZE]);

        let chunk1 = service.decrypt_chunk(&ciphertext, 1).unwrap();
        assert_eq!(chunk1, vec![0xBBu8; 100]);
    }

    #[test]
    fn test_tamper_detection() {
        let service = create_test_service();
        let plaintext = b"Secret data";

        let mut ciphertext = service.encrypt(plaintext);

        // Tamper with the ciphertext (after nonce)
        let tamper_pos = sodium_ffi::NPUBBYTES + 5;
        ciphertext[tamper_pos] ^= 0xFF;

        let result = service.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let service = create_test_service();
        let plaintext = b"";

        let ciphertext = service.encrypt(plaintext);

        // Should just be nonce + auth tag
        assert_eq!(ciphertext.len(), sodium_ffi::NPUBBYTES + sodium_ffi::ABYTES);

        let decrypted = service.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_single_byte() {
        let service = create_test_service();
        let plaintext = b"x";

        let ciphertext = service.encrypt(plaintext);
        let decrypted = service.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypted_range_single_chunk() {
        // Plaintext bytes 0-100 are in chunk 0
        let (start, end) = encrypted_range_for_plaintext(0, 100);

        assert_eq!(start, 0); // Always need nonce
        assert_eq!(
            end,
            sodium_ffi::NPUBBYTES as u64 + ENCRYPTED_CHUNK_SIZE as u64
        );
    }

    #[test]
    fn test_encrypted_range_spans_chunks() {
        // Plaintext bytes spanning chunk 0 and chunk 1
        let (start, end) =
            encrypted_range_for_plaintext(CHUNK_SIZE as u64 - 10, CHUNK_SIZE as u64 + 10);

        assert_eq!(start, 0); // Always need nonce
        assert_eq!(
            end,
            sodium_ffi::NPUBBYTES as u64 + 2 * ENCRYPTED_CHUNK_SIZE as u64
        );
    }

    #[test]
    fn test_encrypted_range_middle_chunk() {
        // Plaintext bytes entirely within chunk 2
        let chunk2_start = CHUNK_SIZE as u64 * 2;
        let (start, end) = encrypted_range_for_plaintext(chunk2_start + 10, chunk2_start + 100);

        assert_eq!(start, 0); // Always need nonce
        assert_eq!(
            end,
            sodium_ffi::NPUBBYTES as u64 + 3 * ENCRYPTED_CHUNK_SIZE as u64
        );
    }

    #[test]
    fn test_different_encryptions_different_ciphertext() {
        let service = create_test_service();
        let plaintext = b"Same message";

        let ciphertext1 = service.encrypt(plaintext);
        let ciphertext2 = service.encrypt(plaintext);

        // Different nonces = different ciphertext
        assert_ne!(ciphertext1, ciphertext2);

        // Both decrypt to same plaintext
        assert_eq!(service.decrypt(&ciphertext1).unwrap(), plaintext);
        assert_eq!(service.decrypt(&ciphertext2).unwrap(), plaintext);
    }

    #[test]
    fn test_chunk_index_out_of_range() {
        let service = create_test_service();
        let plaintext = vec![0u8; CHUNK_SIZE]; // Exactly 1 chunk

        let ciphertext = service.encrypt(&plaintext);

        // Chunk 0 should work
        assert!(service.decrypt_chunk(&ciphertext, 0).is_ok());

        // Chunk 1 should fail
        assert!(service.decrypt_chunk(&ciphertext, 1).is_err());
    }

    #[test]
    fn test_decrypt_range_within_single_chunk() {
        let service = create_test_service();
        // Create plaintext with recognizable pattern
        let plaintext: Vec<u8> = (0..CHUNK_SIZE).map(|i| (i % 256) as u8).collect();

        let ciphertext = service.encrypt(&plaintext);

        // Decrypt range [100, 200) within first chunk
        let decrypted = service.decrypt_range(&ciphertext, 100, 200).unwrap();

        assert_eq!(decrypted.len(), 100);
        assert_eq!(decrypted, plaintext[100..200]);
    }

    #[test]
    fn test_decrypt_range_spanning_chunks() {
        let service = create_test_service();
        // 3 chunks of data
        let plaintext: Vec<u8> = (0..CHUNK_SIZE * 3).map(|i| (i % 256) as u8).collect();

        let ciphertext = service.encrypt(&plaintext);

        // Range spanning from end of chunk 0 into chunk 1
        let start = CHUNK_SIZE as u64 - 100;
        let end = CHUNK_SIZE as u64 + 100;
        let decrypted = service.decrypt_range(&ciphertext, start, end).unwrap();

        assert_eq!(decrypted.len(), 200);
        assert_eq!(decrypted, &plaintext[start as usize..end as usize]);
    }

    #[test]
    fn test_decrypt_range_entire_middle_chunk() {
        let service = create_test_service();
        // 3 chunks, middle chunk filled with 0xBB
        let mut plaintext = vec![0xAAu8; CHUNK_SIZE];
        plaintext.extend(vec![0xBBu8; CHUNK_SIZE]);
        plaintext.extend(vec![0xCCu8; CHUNK_SIZE]);

        let ciphertext = service.encrypt(&plaintext);

        // Decrypt just the middle chunk
        let start = CHUNK_SIZE as u64;
        let end = (CHUNK_SIZE * 2) as u64;
        let decrypted = service.decrypt_range(&ciphertext, start, end).unwrap();

        assert_eq!(decrypted, vec![0xBBu8; CHUNK_SIZE]);
    }

    #[test]
    fn test_decrypt_range_with_partial_encrypted_data() {
        let service = create_test_service();
        // Create 3-chunk plaintext
        let plaintext: Vec<u8> = (0..CHUNK_SIZE * 3).map(|i| (i % 256) as u8).collect();
        let full_ciphertext = service.encrypt(&plaintext);

        // Calculate encrypted range for plaintext bytes in chunk 1
        let plaintext_start = CHUNK_SIZE as u64 + 100;
        let plaintext_end = CHUNK_SIZE as u64 + 200;
        let (enc_start, enc_end) = encrypted_range_for_plaintext(plaintext_start, plaintext_end);

        // Fetch only the needed encrypted bytes (simulating range read)
        let partial_ciphertext = full_ciphertext[enc_start as usize..enc_end as usize].to_vec();

        // Decrypt range from partial data
        let decrypted = service
            .decrypt_range(&partial_ciphertext, plaintext_start, plaintext_end)
            .unwrap();

        assert_eq!(decrypted.len(), 100);
        assert_eq!(
            decrypted,
            &plaintext[plaintext_start as usize..plaintext_end as usize]
        );
    }
}
