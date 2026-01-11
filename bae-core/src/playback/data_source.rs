//! Unified data source abstraction for audio playback.
//!
//! Provides a common interface for reading audio bytes from:
//! - Local files (non-storage releases, or storage releases with local backend)
//! - Cloud storage (storage releases with cloud backend)

use crate::encryption::EncryptionService;
use crate::playback::sparse_buffer::SharedSparseBuffer;
use std::sync::Arc;
use tracing::{debug, error, info};

/// Reads audio data into a sparse buffer for streaming playback.
///
/// Implementations handle the specifics of local vs cloud reads,
/// byte range extraction, and optional decryption.
pub trait AudioDataReader: Send + 'static {
    /// Start reading data into the buffer.
    ///
    /// This spawns an async task that fills the buffer. The reader handles:
    /// - Optional FLAC headers prepending
    /// - Byte range extraction (for CUE/FLAC tracks)
    /// - Decryption (for encrypted storage)
    fn start_reading(self: Box<Self>, buffer: SharedSparseBuffer);
}

/// Configuration for reading audio data.
#[derive(Debug, Clone)]
pub struct AudioReadConfig {
    /// Path to the audio file (local path or cloud key)
    pub path: String,
    /// FLAC headers to prepend (for CUE/FLAC tracks)
    pub flac_headers: Option<Vec<u8>>,
    /// Start byte offset (for CUE/FLAC byte range)
    pub start_byte: Option<u64>,
    /// End byte offset (for CUE/FLAC byte range)
    pub end_byte: Option<u64>,
}

/// Reads from local filesystem.
///
/// Used for:
/// - Non-storage releases (files at original import location)
/// - Storage releases with local backend
pub struct LocalFileReader {
    config: AudioReadConfig,
}

impl LocalFileReader {
    pub fn new(config: AudioReadConfig) -> Self {
        Self { config }
    }
}

impl AudioDataReader for LocalFileReader {
    fn start_reading(self: Box<Self>, buffer: SharedSparseBuffer) {
        let config = self.config;

        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};

            let mut file = match tokio::fs::File::open(&config.path).await {
                Ok(f) => f,
                Err(e) => {
                    error!("Failed to open file {}: {}", config.path, e);
                    buffer.cancel();
                    return;
                }
            };

            let mut buffer_pos: u64 = 0;

            // Prepend FLAC headers if provided
            if let Some(headers) = &config.flac_headers {
                buffer.append_at(buffer_pos, headers);
                buffer_pos += headers.len() as u64;
            }

            // Seek to start position if needed
            let start = config.start_byte.unwrap_or(0);
            if start > 0 {
                if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
                    error!("Failed to seek: {}", e);
                    buffer.cancel();
                    return;
                }
            }

            let end = config.end_byte;
            let mut file_pos = start;
            let mut chunk = vec![0u8; 65536];

            loop {
                if buffer.is_cancelled() {
                    return;
                }

                let to_read = if let Some(end) = end {
                    chunk.len().min((end - file_pos) as usize)
                } else {
                    chunk.len()
                };

                if to_read == 0 {
                    break;
                }

                match file.read(&mut chunk[..to_read]).await {
                    Ok(0) => break,
                    Ok(n) => {
                        buffer.append_at(buffer_pos, &chunk[..n]);
                        buffer_pos += n as u64;
                        file_pos += n as u64;
                    }
                    Err(e) => {
                        error!("Read error: {}", e);
                        break;
                    }
                }
            }

            debug!("LocalFileReader: read {} bytes total", buffer_pos);
            buffer.set_total_size(buffer_pos);
            buffer.mark_eof();
        });
    }
}

/// Reads from cloud storage with optional decryption.
pub struct CloudStorageReader {
    config: AudioReadConfig,
    storage: Arc<dyn crate::cloud_storage::CloudStorage>,
    encryption_service: Option<Arc<EncryptionService>>,
    encrypted: bool,
    /// Encryption nonce from DB for efficient range requests.
    /// When set with start/end byte range, uses chunked decryption
    /// to avoid downloading entire file.
    encryption_nonce: Option<Vec<u8>>,
}

impl CloudStorageReader {
    pub fn new(
        config: AudioReadConfig,
        storage: Arc<dyn crate::cloud_storage::CloudStorage>,
        encryption_service: Option<Arc<EncryptionService>>,
        encrypted: bool,
    ) -> Self {
        Self {
            config,
            storage,
            encryption_service,
            encrypted,
            encryption_nonce: None,
        }
    }

    /// Create reader with encryption nonce for efficient encrypted seeks.
    /// Use this when seeking in encrypted files to avoid downloading entire file.
    pub fn with_encryption_nonce(mut self, nonce: Option<Vec<u8>>) -> Self {
        self.encryption_nonce = nonce;
        self
    }
}

impl AudioDataReader for CloudStorageReader {
    fn start_reading(self: Box<Self>, buffer: SharedSparseBuffer) {
        let config = self.config;
        let storage = self.storage;
        let encryption_service = self.encryption_service;
        let encrypted = self.encrypted;
        let encryption_nonce = self.encryption_nonce;

        tokio::spawn(async move {
            info!(
                "CloudStorageReader: encrypted={}, start={:?}, end={:?}, headers_len={}, has_nonce={}",
                encrypted,
                config.start_byte,
                config.end_byte,
                config.flac_headers.as_ref().map(|h| h.len()).unwrap_or(0),
                encryption_nonce.is_some()
            );

            let result = if encrypted {
                // Check if we can use efficient range request (nonce + byte range)
                if let (Some(nonce), Some(start), Some(end)) =
                    (&encryption_nonce, config.start_byte, config.end_byte)
                {
                    use crate::encryption::encrypted_chunk_range;

                    // Calculate encrypted chunk range for efficient download
                    let (chunk_start, chunk_end) = encrypted_chunk_range(start, end);

                    info!(
                        "CloudStorageReader: using efficient range request, plaintext [{}, {}) -> encrypted [{}, {})",
                        start, end, chunk_start, chunk_end
                    );

                    download_encrypted_range_to_buffer(
                        storage,
                        &config.path,
                        buffer.clone(),
                        &encryption_service,
                        nonce,
                        start,
                        end,
                        chunk_start,
                        chunk_end,
                        config.flac_headers.as_deref(),
                    )
                    .await
                } else {
                    // Fall back to full download (initial playback, no nonce available)
                    download_encrypted_to_buffer(
                        storage,
                        &config.path,
                        buffer.clone(),
                        &encryption_service,
                        config.start_byte.unwrap_or(0),
                        config.end_byte,
                        config.flac_headers.as_deref(),
                    )
                    .await
                }
            } else if let (Some(start), Some(end)) = (config.start_byte, config.end_byte) {
                download_range_to_buffer(
                    storage,
                    &config.path,
                    buffer.clone(),
                    start,
                    end,
                    config.flac_headers.as_deref(),
                )
                .await
            } else {
                download_full_to_buffer(
                    storage,
                    &config.path,
                    buffer.clone(),
                    config.flac_headers.as_deref(),
                )
                .await
            };

            if let Err(e) = result {
                error!("Cloud download failed: {:?}", e);
                buffer.cancel();
            }
        });
    }
}

// Helper functions for cloud downloads

async fn download_full_to_buffer(
    storage: Arc<dyn crate::cloud_storage::CloudStorage>,
    path: &str,
    buffer: SharedSparseBuffer,
    flac_headers: Option<&[u8]>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data = storage.download(path).await?;

    let mut buffer_pos: u64 = 0;

    if let Some(headers) = flac_headers {
        buffer.append_at(buffer_pos, headers);
        buffer_pos += headers.len() as u64;
    }

    buffer.append_at(buffer_pos, &data);
    buffer_pos += data.len() as u64;

    debug!("CloudStorageReader: downloaded {} bytes", buffer_pos);
    buffer.set_total_size(buffer_pos);
    buffer.mark_eof();

    Ok(())
}

async fn download_range_to_buffer(
    storage: Arc<dyn crate::cloud_storage::CloudStorage>,
    path: &str,
    buffer: SharedSparseBuffer,
    start: u64,
    end: u64,
    flac_headers: Option<&[u8]>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data = storage.download_range(path, start, end - start).await?;

    let mut buffer_pos: u64 = 0;

    if let Some(headers) = flac_headers {
        buffer.append_at(buffer_pos, headers);
        buffer_pos += headers.len() as u64;
    }

    buffer.append_at(buffer_pos, &data);
    buffer_pos += data.len() as u64;

    debug!(
        "CloudStorageReader: downloaded range {}-{} ({} bytes)",
        start, end, buffer_pos
    );
    buffer.set_total_size(buffer_pos);
    buffer.mark_eof();

    Ok(())
}

async fn download_encrypted_to_buffer(
    storage: Arc<dyn crate::cloud_storage::CloudStorage>,
    path: &str,
    buffer: SharedSparseBuffer,
    encryption_service: &Option<Arc<EncryptionService>>,
    start: u64,
    end: Option<u64>,
    flac_headers: Option<&[u8]>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let enc = encryption_service
        .as_ref()
        .ok_or("Cannot play encrypted files: encryption not configured")?;

    // For encrypted files, we must download and decrypt the entire file
    // since we can't decrypt partial data. The start/end offsets are applied
    // to the decrypted data.
    let encrypted_data = storage.download(path).await?;

    // Decrypt
    let decrypted = enc
        .decrypt(&encrypted_data)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    // Apply start/end offsets to the decrypted data
    let start_offset = start as usize;
    let end_offset = end.map(|e| e as usize).unwrap_or(decrypted.len());
    let slice = &decrypted[start_offset.min(decrypted.len())..end_offset.min(decrypted.len())];

    let mut buffer_pos: u64 = 0;

    if let Some(headers) = flac_headers {
        buffer.append_at(buffer_pos, headers);
        buffer_pos += headers.len() as u64;
    }

    buffer.append_at(buffer_pos, slice);
    buffer_pos += slice.len() as u64;

    info!(
        "CloudStorageReader: decrypted {} bytes, sliced [{}, {}) -> {} bytes (headers prepended: {})",
        encrypted_data.len(),
        start_offset,
        end_offset,
        buffer_pos,
        flac_headers.is_some()
    );
    buffer.set_total_size(buffer_pos);
    buffer.mark_eof();

    Ok(())
}

/// Download encrypted data using range request with nonce from DB.
///
/// This is the efficient path for encrypted cloud seeks:
/// - `nonce`: 24-byte nonce stored in DB at import time
/// - `plaintext_start`, `plaintext_end`: Byte range we want in decrypted file
/// - `chunk_start`, `chunk_end`: Encrypted byte range (from `encrypted_chunk_range`)
///
/// Downloads only the needed encrypted chunks, not the entire file.
pub async fn download_encrypted_range_to_buffer(
    storage: Arc<dyn crate::cloud_storage::CloudStorage>,
    path: &str,
    buffer: SharedSparseBuffer,
    encryption_service: &Option<Arc<EncryptionService>>,
    nonce: &[u8],
    plaintext_start: u64,
    plaintext_end: u64,
    chunk_start: u64,
    chunk_end: u64,
    flac_headers: Option<&[u8]>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let enc = encryption_service
        .as_ref()
        .ok_or("Cannot play encrypted files: encryption not configured")?;

    use crate::encryption::CHUNK_SIZE;

    // Download only the needed encrypted chunks via range request
    let encrypted_chunks = storage.download_range(path, chunk_start, chunk_end).await?;

    // Calculate first chunk index for decrypt_range_with_offset
    let first_chunk_index = plaintext_start / CHUNK_SIZE as u64;

    // Decrypt using nonce from DB + partial chunks
    let decrypted = enc
        .decrypt_range_with_offset(
            nonce,
            &encrypted_chunks,
            first_chunk_index,
            plaintext_start,
            plaintext_end,
        )
        .map_err(|e| format!("Decryption failed: {}", e))?;

    let mut buffer_pos: u64 = 0;

    if let Some(headers) = flac_headers {
        buffer.append_at(buffer_pos, headers);
        buffer_pos += headers.len() as u64;
    }

    buffer.append_at(buffer_pos, &decrypted);
    buffer_pos += decrypted.len() as u64;

    info!(
        "CloudStorageReader: range request [{}, {}) -> {} encrypted bytes -> {} decrypted bytes",
        chunk_start,
        chunk_end,
        encrypted_chunks.len(),
        decrypted.len()
    );

    buffer.set_total_size(buffer_pos);
    buffer.mark_eof();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::sparse_buffer::create_sparse_buffer;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_local_file_reader_full_file() {
        // Create temp file with test data
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, this is test audio data for streaming!";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let config = AudioReadConfig {
            path: temp_file.path().to_string_lossy().to_string(),
            flac_headers: None,
            start_byte: None,
            end_byte: None,
        };

        let reader = Box::new(LocalFileReader::new(config));
        let buffer = create_sparse_buffer();

        reader.start_reading(buffer.clone());

        // Wait for read to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Read from buffer
        let mut read_buf = vec![0u8; 1024];
        let mut result = Vec::new();
        loop {
            match buffer.read(&mut read_buf) {
                Some(0) => break,
                Some(n) => result.extend_from_slice(&read_buf[..n]),
                None => break,
            }
        }

        assert_eq!(result, test_data);
    }

    #[tokio::test]
    async fn test_local_file_reader_with_byte_range() {
        // Create temp file with test data
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"0123456789ABCDEFGHIJ";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let config = AudioReadConfig {
            path: temp_file.path().to_string_lossy().to_string(),
            flac_headers: None,
            start_byte: Some(5),
            end_byte: Some(15),
        };

        let reader = Box::new(LocalFileReader::new(config));
        let buffer = create_sparse_buffer();

        reader.start_reading(buffer.clone());

        // Wait for read to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Read from buffer
        let mut read_buf = vec![0u8; 1024];
        let mut result = Vec::new();
        loop {
            match buffer.read(&mut read_buf) {
                Some(0) => break,
                Some(n) => result.extend_from_slice(&read_buf[..n]),
                None => break,
            }
        }

        // Should only have bytes 5-14 (10 bytes)
        assert_eq!(result, b"56789ABCDE");
    }

    #[tokio::test]
    async fn test_local_file_reader_with_headers_prepend() {
        // Create temp file with "audio data"
        let mut temp_file = NamedTempFile::new().unwrap();
        let audio_data = b"AUDIO_DATA_HERE";
        temp_file.write_all(audio_data).unwrap();
        temp_file.flush().unwrap();

        let headers = b"fLaC_HEADERS".to_vec();

        let config = AudioReadConfig {
            path: temp_file.path().to_string_lossy().to_string(),
            flac_headers: Some(headers.clone()),
            start_byte: None,
            end_byte: None,
        };

        let reader = Box::new(LocalFileReader::new(config));
        let buffer = create_sparse_buffer();

        reader.start_reading(buffer.clone());

        // Wait for read to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Read from buffer
        let mut read_buf = vec![0u8; 1024];
        let mut result = Vec::new();
        loop {
            match buffer.read(&mut read_buf) {
                Some(0) => break,
                Some(n) => result.extend_from_slice(&read_buf[..n]),
                None => break,
            }
        }

        // Should have headers + audio data
        let mut expected = headers;
        expected.extend_from_slice(audio_data);
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_local_file_reader_with_headers_and_range() {
        // Create temp file with "audio data"
        let mut temp_file = NamedTempFile::new().unwrap();
        let audio_data = b"0123456789ABCDEFGHIJ";
        temp_file.write_all(audio_data).unwrap();
        temp_file.flush().unwrap();

        let headers = b"HDR".to_vec();

        let config = AudioReadConfig {
            path: temp_file.path().to_string_lossy().to_string(),
            flac_headers: Some(headers.clone()),
            start_byte: Some(10),
            end_byte: Some(15),
        };

        let reader = Box::new(LocalFileReader::new(config));
        let buffer = create_sparse_buffer();

        reader.start_reading(buffer.clone());

        // Wait for read to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Read from buffer
        let mut read_buf = vec![0u8; 1024];
        let mut result = Vec::new();
        loop {
            match buffer.read(&mut read_buf) {
                Some(0) => break,
                Some(n) => result.extend_from_slice(&read_buf[..n]),
                None => break,
            }
        }

        // Should have headers + byte range (ABCDE)
        assert_eq!(result, b"HDRABCDE");
    }

    #[tokio::test]
    async fn test_local_file_reader_nonexistent_file() {
        let config = AudioReadConfig {
            path: "/nonexistent/path/to/file.flac".to_string(),
            flac_headers: None,
            start_byte: None,
            end_byte: None,
        };

        let reader = Box::new(LocalFileReader::new(config));
        let buffer = create_sparse_buffer();

        reader.start_reading(buffer.clone());

        // Wait a bit
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Buffer should be cancelled (error case)
        assert!(
            buffer.is_cancelled(),
            "Buffer should be cancelled for nonexistent file"
        );
    }

    #[tokio::test]
    async fn test_encrypted_seek_uses_range_request() {
        use crate::cloud_storage::{CloudStorage, CloudStorageError};
        use crate::encryption::{encrypted_chunk_range, EncryptionService, CHUNK_SIZE};
        use async_trait::async_trait;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Mock storage that tracks what was downloaded
        struct RangeTrackingStorage {
            encrypted_data: Vec<u8>,
            full_downloads: AtomicUsize,
            range_downloads: AtomicUsize,
            last_range: std::sync::Mutex<Option<(u64, u64)>>,
        }

        #[async_trait]
        impl CloudStorage for RangeTrackingStorage {
            async fn upload(&self, _: &str, _: &[u8]) -> Result<String, CloudStorageError> {
                unimplemented!()
            }

            async fn download(&self, _: &str) -> Result<Vec<u8>, CloudStorageError> {
                self.full_downloads.fetch_add(1, Ordering::SeqCst);
                Ok(self.encrypted_data.clone())
            }

            async fn download_range(
                &self,
                _: &str,
                start: u64,
                end: u64,
            ) -> Result<Vec<u8>, CloudStorageError> {
                self.range_downloads.fetch_add(1, Ordering::SeqCst);
                *self.last_range.lock().unwrap() = Some((start, end));
                Ok(self.encrypted_data[start as usize..end as usize].to_vec())
            }

            async fn delete(&self, _: &str) -> Result<(), CloudStorageError> {
                unimplemented!()
            }
        }

        // Create test data: 1MB plaintext (16 chunks of 64KB each)
        let plaintext: Vec<u8> = (0..CHUNK_SIZE * 16).map(|i| (i % 256) as u8).collect();
        let encryption_service = EncryptionService::new_with_key(&[0x42; 32]);
        let encrypted_data = encryption_service.encrypt(&plaintext);
        let nonce = encrypted_data[..24].to_vec();
        let encryption_service = Some(std::sync::Arc::new(encryption_service));

        let storage = std::sync::Arc::new(RangeTrackingStorage {
            encrypted_data: encrypted_data.clone(),
            full_downloads: AtomicUsize::new(0),
            range_downloads: AtomicUsize::new(0),
            last_range: std::sync::Mutex::new(None),
        });

        // Request plaintext bytes from middle of the file (chunk 8)
        let plaintext_start = CHUNK_SIZE as u64 * 8;
        let plaintext_end = plaintext_start + 1000;

        let buffer = create_sparse_buffer();

        // Calculate chunk range and call the function with nonce
        let (chunk_start, chunk_end) = encrypted_chunk_range(plaintext_start, plaintext_end);

        super::download_encrypted_range_to_buffer(
            storage.clone(),
            "test/file.enc",
            buffer.clone(),
            &encryption_service,
            &nonce,
            plaintext_start,
            plaintext_end,
            chunk_start,
            chunk_end,
            None,
        )
        .await
        .expect("download should succeed");

        // Verify: should NOT have downloaded the full file
        assert_eq!(
            storage.full_downloads.load(Ordering::SeqCst),
            0,
            "Should not download entire file for encrypted seek"
        );

        // Verify: should have used range request
        assert_eq!(
            storage.range_downloads.load(Ordering::SeqCst),
            1,
            "Should use range request for encrypted seek"
        );

        // Verify: range should be much smaller than full file
        let (start, end) = storage.last_range.lock().unwrap().unwrap();
        let downloaded_bytes = end - start;
        let full_size = encrypted_data.len() as u64;

        assert!(
            downloaded_bytes < full_size / 4,
            "Should download <25% of file for single-chunk seek, got {}/{} bytes",
            downloaded_bytes,
            full_size
        );

        // Verify: decrypted data is correct
        // Wait for buffer to be ready
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut read_buf = vec![0u8; 2000];
        let mut result = Vec::new();
        loop {
            match buffer.read(&mut read_buf) {
                Some(0) => break,
                Some(n) => result.extend_from_slice(&read_buf[..n]),
                None => break,
            }
        }

        assert_eq!(
            &result[..],
            &plaintext[plaintext_start as usize..plaintext_end as usize],
            "Decrypted data should match original plaintext at seek position"
        );
    }
}
