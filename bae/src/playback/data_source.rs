//! Unified data source abstraction for audio playback.
//!
//! Provides a common interface for reading audio bytes from:
//! - Local files (non-storage releases, or storage releases with local backend)
//! - Cloud storage (storage releases with cloud backend)

use crate::encryption::EncryptionService;
use crate::playback::sparse_buffer::SharedSparseBuffer;
use std::sync::Arc;
use tracing::{debug, error};

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
    encryption_service: Arc<EncryptionService>,
    encrypted: bool,
}

impl CloudStorageReader {
    pub fn new(
        config: AudioReadConfig,
        storage: Arc<dyn crate::cloud_storage::CloudStorage>,
        encryption_service: Arc<EncryptionService>,
        encrypted: bool,
    ) -> Self {
        Self {
            config,
            storage,
            encryption_service,
            encrypted,
        }
    }
}

impl AudioDataReader for CloudStorageReader {
    fn start_reading(self: Box<Self>, buffer: SharedSparseBuffer) {
        let config = self.config;
        let storage = self.storage;
        let encryption_service = self.encryption_service;
        let encrypted = self.encrypted;

        tokio::spawn(async move {
            let result = if encrypted {
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
    encryption_service: &EncryptionService,
    start: u64,
    end: Option<u64>,
    flac_headers: Option<&[u8]>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Download encrypted data
    let encrypted_data = if let Some(end) = end {
        storage.download_range(path, start, end - start).await?
    } else {
        storage.download(path).await?
    };

    // Decrypt
    let decrypted = encryption_service
        .decrypt(&encrypted_data)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    let mut buffer_pos: u64 = 0;

    if let Some(headers) = flac_headers {
        buffer.append_at(buffer_pos, headers);
        buffer_pos += headers.len() as u64;
    }

    buffer.append_at(buffer_pos, &decrypted);
    buffer_pos += decrypted.len() as u64;

    debug!(
        "CloudStorageReader: decrypted {} bytes -> {} bytes",
        encrypted_data.len(),
        buffer_pos
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
}
