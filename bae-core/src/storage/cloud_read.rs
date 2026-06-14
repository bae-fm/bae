//! Verified ranged reads of cloud-stored audio blobs, shared by playback
//! (streaming a track in chunk-sized windows) and pin (downloading a release
//! file to local storage one window at a time).
//!
//! A managed cloud blob is `[nonce: 24 bytes][encrypted chunks…]` (see
//! `encryption::encrypt_chunked`). Serving an arbitrary plaintext range needs
//! the nonce plus only the encrypted chunks covering that range — never the
//! whole object. `CloudBlobReader` fetches the nonce ONCE per blob and reuses
//! it across every range read, so a track streamed in N windows issues one
//! nonce read, not N.

use crate::encryption::EncryptionService;
use crate::storage::cloud::CloudHome;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// Reads plaintext ranges from a single cloud blob, decrypting on the fly. Every
/// cloud blob is master-key-encrypted, so the library key is always required.
/// The nonce header is fetched lazily on the first read and cached for the
/// reader's lifetime.
pub struct CloudBlobReader {
    cloud_home: Arc<dyn CloudHome>,
    /// The library master key used to decrypt the chunks.
    encryption: EncryptionService,
    key: String,
    /// Plaintext length of the blob. Range requests are validated against it,
    /// and the encrypted chunk range is clamped to the matching blob length.
    source_size: u64,
    /// The 24-byte base nonce, read once from the blob header on the first
    /// range read.
    nonce: OnceCell<Vec<u8>>,
}

impl CloudBlobReader {
    pub fn new(
        cloud_home: Arc<dyn CloudHome>,
        encryption: EncryptionService,
        key: String,
        source_size: u64,
    ) -> Self {
        Self {
            cloud_home,
            encryption,
            key,
            source_size,
            nonce: OnceCell::new(),
        }
    }

    /// Read exactly `len` plaintext bytes starting at `offset`. An out-of-range
    /// request errors rather than truncating. Fetches the cached nonce,
    /// range-reads only the chunks covering `offset..offset+len`, and decrypts
    /// with the chunk offset.
    pub async fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        if len == 0 {
            return Ok(Vec::new());
        }

        let end = offset
            .checked_add(len)
            .ok_or_else(|| format!("Cloud read range overflow: offset={offset}, len={len}"))?;

        if end > self.source_size {
            return Err(format!(
                "Cloud read range {offset}..{end} exceeds source size {}",
                self.source_size
            ));
        }

        use crate::encryption::{
            encrypted_blob_len_for_plaintext, encrypted_chunk_range, CHUNK_SIZE,
        };

        let nonce = self.nonce().await?;

        let (chunk_start, mut chunk_end) = encrypted_chunk_range(offset, end);
        chunk_end = chunk_end.min(encrypted_blob_len_for_plaintext(self.source_size));
        let encrypted_chunks = self
            .cloud_home
            .read_range(&self.key, chunk_start, chunk_end)
            .await
            .map_err(|e| format!("Cloud range read failed: {e}"))?;

        let first_chunk_index = offset / CHUNK_SIZE as u64;

        self.encryption
            .decrypt_range_with_offset(nonce, &encrypted_chunks, first_chunk_index, offset, end)
            .map_err(|e| format!("Decryption failed: {e}"))
    }

    /// The cached 24-byte base nonce, fetched from the blob header on first use.
    async fn nonce(&self) -> Result<&[u8], String> {
        use crate::encryption::NONCE_SIZE;
        let nonce = self
            .nonce
            .get_or_try_init(|| async {
                self.cloud_home
                    .read_range(&self.key, 0, NONCE_SIZE as u64)
                    .await
                    .map_err(|e| format!("Cloud nonce-header read failed: {e}"))
            })
            .await?;
        Ok(nonce)
    }
}
