//! Storage reader utilities for creating storage clients from profiles
use crate::cloud_storage::{CloudStorage, CloudStorageError, S3CloudStorage};
use crate::db::{DbStorageProfile, StorageLocation};
use crate::encryption::{encrypted_range_for_plaintext, EncryptionService, CHUNK_SIZE};
use crate::playback::SharedStreamingBuffer;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tracing::{debug, info};

/// Chunk size for streaming downloads (64KB)
const STREAMING_CHUNK_SIZE: usize = 65536;

/// Create a storage reader from a profile.
///
/// For cloud profiles: creates S3CloudStorage from profile credentials
/// For local profiles: returns LocalFileStorage that reads from disk
pub async fn create_storage_reader(
    profile: &DbStorageProfile,
) -> Result<Arc<dyn CloudStorage>, CloudStorageError> {
    debug!(
        "Creating storage reader for profile '{}' (id={}, location={:?})",
        profile.name, profile.id, profile.location
    );

    match profile.location {
        StorageLocation::Cloud => {
            let s3_config = profile.to_s3_config().ok_or_else(|| {
                CloudStorageError::Config("Missing S3 credentials in profile".into())
            })?;
            let client = S3CloudStorage::new(s3_config).await?;
            Ok(Arc::new(client))
        }
        StorageLocation::Local => Ok(Arc::new(LocalFileStorage)),
    }
}

/// Local file storage that reads files from disk paths.
pub struct LocalFileStorage;

#[async_trait::async_trait]
impl CloudStorage for LocalFileStorage {
    async fn upload(&self, path: &str, data: &[u8]) -> Result<String, CloudStorageError> {
        tokio::fs::write(path, data).await?;
        Ok(path.to_string())
    }

    async fn download(&self, path: &str) -> Result<Vec<u8>, CloudStorageError> {
        tokio::fs::read(path).await.map_err(CloudStorageError::Io)
    }

    async fn download_range(
        &self,
        path: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<u8>, CloudStorageError> {
        if start >= end {
            return Err(CloudStorageError::Download(format!(
                "Invalid range: start ({}) >= end ({})",
                start, end
            )));
        }

        let mut file = tokio::fs::File::open(path).await?;
        file.seek(std::io::SeekFrom::Start(start)).await?;

        let len = (end - start) as usize;
        let mut buffer = vec![0u8; len];
        file.read_exact(&mut buffer).await?;

        Ok(buffer)
    }

    async fn delete(&self, path: &str) -> Result<(), CloudStorageError> {
        tokio::fs::remove_file(path)
            .await
            .map_err(CloudStorageError::Io)
    }
}

/// Download a file to a streaming buffer in chunks.
///
/// Data is appended to the buffer as it arrives, enabling streaming playback.
pub async fn download_to_streaming_buffer(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedStreamingBuffer,
) -> Result<(), CloudStorageError> {
    info!("Starting streaming download: {}", path);

    // Download the entire file in chunks using range requests
    let mut offset = 0u64;

    loop {
        if buffer.is_cancelled() {
            debug!("Streaming download cancelled");
            return Err(CloudStorageError::Download("Cancelled".into()));
        }

        let end = offset + STREAMING_CHUNK_SIZE as u64;
        let chunk = storage.download_range(path, offset, end).await;

        match chunk {
            Ok(data) if data.is_empty() => {
                // End of file
                break;
            }
            Ok(data) => {
                let len = data.len();
                buffer.append(&data);
                offset += len as u64;

                // If we got less than requested, we've reached EOF
                if len < STREAMING_CHUNK_SIZE {
                    break;
                }
            }
            Err(e) => {
                // Some storage backends return an error at EOF, treat as EOF
                debug!("Download range returned error (may be EOF): {:?}", e);
                break;
            }
        }
    }

    buffer.mark_eof();

    info!("Streaming download complete: {} bytes", offset);
    Ok(())
}

/// Download a byte range to a streaming buffer in chunks.
///
/// Used for CUE/FLAC tracks where only a portion of the file is needed.
pub async fn download_to_streaming_buffer_with_range(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedStreamingBuffer,
    start_byte: u64,
    end_byte: u64,
) -> Result<(), CloudStorageError> {
    info!(
        "Starting streaming download with range: {} bytes {}-{}",
        path, start_byte, end_byte
    );

    let mut offset = start_byte;

    while offset < end_byte {
        if buffer.is_cancelled() {
            debug!("Streaming download cancelled");
            return Err(CloudStorageError::Download("Cancelled".into()));
        }

        let chunk_end = (offset + STREAMING_CHUNK_SIZE as u64).min(end_byte);
        let chunk = storage.download_range(path, offset, chunk_end).await?;

        if chunk.is_empty() {
            break;
        }

        let len = chunk.len();
        buffer.append(&chunk);
        offset += len as u64;
    }

    buffer.mark_eof();

    info!(
        "Streaming range download complete: {} bytes",
        offset - start_byte
    );
    Ok(())
}

/// Download an encrypted file to a streaming buffer, decrypting on the fly.
///
/// Downloads encrypted chunks aligned with encryption boundaries, decrypts,
/// and appends plaintext to the buffer.
pub async fn download_encrypted_to_streaming_buffer(
    storage: Arc<dyn CloudStorage>,
    path: &str,
    buffer: SharedStreamingBuffer,
    encryption: &EncryptionService,
    plaintext_start: u64,
    plaintext_end: Option<u64>,
) -> Result<(), CloudStorageError> {
    const NONCE_SIZE: u64 = 24;
    // Process 4 encryption chunks at a time (256KB plaintext)
    let plaintext_chunk_size = CHUNK_SIZE as u64 * 4;

    info!(
        "Starting encrypted streaming download: {} (plaintext start: {})",
        path, plaintext_start
    );

    // First, download the nonce (always needed)
    let nonce_data = storage.download_range(path, 0, NONCE_SIZE).await?;
    if nonce_data.len() < NONCE_SIZE as usize {
        return Err(CloudStorageError::Download("File too short for nonce".into()));
    }

    let mut plaintext_pos = plaintext_start;

    loop {
        if buffer.is_cancelled() {
            debug!("Encrypted streaming download cancelled");
            return Err(CloudStorageError::Download("Cancelled".into()));
        }

        // Calculate the plaintext range for this iteration
        let chunk_plaintext_end = if let Some(end) = plaintext_end {
            (plaintext_pos + plaintext_chunk_size).min(end)
        } else {
            plaintext_pos + plaintext_chunk_size
        };

        // Calculate the encrypted byte range needed
        let (enc_start, enc_end) = encrypted_range_for_plaintext(plaintext_pos, chunk_plaintext_end);

        // Download encrypted data (including nonce at start)
        let encrypted_data = storage.download_range(path, enc_start, enc_end).await?;

        if encrypted_data.is_empty() {
            break;
        }

        // Decrypt the range
        let plaintext = encryption
            .decrypt_range(&encrypted_data, plaintext_pos, chunk_plaintext_end)
            .map_err(|e| CloudStorageError::Download(format!("Decryption failed: {}", e)))?;

        if plaintext.is_empty() {
            break;
        }

        buffer.append(&plaintext);
        plaintext_pos += plaintext.len() as u64;

        // Check if we've reached the requested end
        if let Some(end) = plaintext_end {
            if plaintext_pos >= end {
                break;
            }
        }

        // If we got less than expected, we've hit EOF
        if plaintext.len() < plaintext_chunk_size as usize {
            break;
        }
    }

    buffer.mark_eof();

    info!(
        "Encrypted streaming download complete: {} plaintext bytes",
        plaintext_pos - plaintext_start
    );
    Ok(())
}
