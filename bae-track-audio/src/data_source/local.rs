//! A local file as the source a buffer's fill reads from.

use super::*;
use coven::LeasedFile;

/// Reads a file its caller opened: a file being previewed, or an import's
/// sources before they are the library's. The fill holds the open file, and
/// with it its place of the open-files budget, until the fill ends.
pub struct LocalReader {
    opened: std::io::Result<LeasedFile<tokio::fs::File>>,
    path: std::path::PathBuf,
    diagnostics: Diagnostics,
}

impl LocalReader {
    /// A reader of the file at `path`, which the caller opened as `opened`. A
    /// failed open fails the buffer when the fill starts, like a failed read
    /// of the file, so the caller hears of it through the fill's error
    /// handler. `path` names the file in errors and is never opened.
    pub fn new(
        opened: std::io::Result<LeasedFile<tokio::fs::File>>,
        path: impl Into<std::path::PathBuf>,
        diagnostics: Diagnostics,
    ) -> Self {
        Self { opened, path: path.into(), diagnostics }
    }
}

impl AudioDataReader for LocalReader {
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        on_error: FillErrorHandler,
    ) -> FillTask {
        let LocalReader { opened, path, diagnostics } = *self;
        let fill_buffer = buffer.clone();

        spawn_fill(diagnostics, &buffer, on_error, async move {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};

            let file = opened.map_err(|e| {
                FillFailure::Open(TrackReadError::io(
                    format!("Failed to open {}", path.display()),
                    e,
                ))
            })?;
            // One handle, seeked per fetch. The fill loop awaits each fetch in
            // turn, so the lock is never actually contended.
            let file = tokio::sync::Mutex::new(file);

            fill_buffer
                .fill_on_demand(FILL_WINDOW_SIZE, |src_off, len| {
                    let file = &file;
                    let path = &path;
                    async move {
                        let mut f = file.lock().await;
                        f.seek(std::io::SeekFrom::Start(src_off)).await.map_err(|e| {
                            TrackReadError::io(
                                format!("Failed to seek {} to {src_off}", path.display()),
                                e,
                            )
                        })?;
                        let mut buf = vec![0u8; len as usize];
                        f.read_exact(&mut buf).await.map_err(|e| {
                            TrackReadError::io(
                                format!(
                                    "Failed to read {len} bytes at {src_off} from {}",
                                    path.display()
                                ),
                                e,
                            )
                        })?;
                        Ok(buf)
                    }
                })
                .await
                .map_err(FillFailure::Read)
        })
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
