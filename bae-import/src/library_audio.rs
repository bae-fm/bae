//! The audio of a library release, read where coven has it: the bytes of a
//! file as a [`bae_audio::ReadableFile`], with no copy of the file anywhere.

use crate::service::CueImageOpener;
use crate::types::CueAnalyzedAudioFile;
use crate::ImportError;
use bae_audio::ReadableFile;
use bae_records::Records;
use bae_store::FileStream;
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;

/// The bytes one read of coven's stream fetches, so the demuxer's many small
/// reads of a file's neighbouring bytes make one read of the stream.
const WINDOW: u64 = 1024 * 1024;

/// A coven stream read as a file, from a thread of the blocking pool.
struct StreamFile {
    stream: Arc<FileStream>,
    runtime: tokio::runtime::Handle,
    position: u64,
    /// The bytes last fetched, and the offset they start at.
    window: Option<(u64, Vec<u8>)>,
}

impl StreamFile {
    fn new(stream: Arc<FileStream>, runtime: tokio::runtime::Handle) -> Self {
        Self { stream, runtime, position: 0, window: None }
    }
}

impl ReadableFile for StreamFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let size = self.stream.plaintext_size();
        if self.position >= size || buf.is_empty() {
            return Ok(0);
        }
        let held = self
            .window
            .as_ref()
            .filter(|(start, bytes)| {
                *start <= self.position && self.position < start + bytes.len() as u64
            })
            .is_some();
        if !held {
            let len = WINDOW.min(size - self.position);
            let bytes = self
                .runtime
                .block_on(self.stream.read_at(self.position, len))
                .map_err(std::io::Error::other)?;
            self.window = Some((self.position, bytes));
        }
        let (start, bytes) = self.window.as_ref().expect("a window was fetched above");
        let from = (self.position - start) as usize;
        let count = buf.len().min(bytes.len() - from);
        buf[..count].copy_from_slice(&bytes[from..from + count]);
        self.position += count as u64;
        Ok(count)
    }

    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let target = match to {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
            SeekFrom::End(delta) => self.stream.plaintext_size().checked_add_signed(delta),
        };
        self.position = target.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek to a negative offset")
        })?;
        Ok(self.position)
    }

    fn size(&self) -> std::io::Result<u64> {
        Ok(self.stream.plaintext_size())
    }
}

/// The audio files of a library release, opened through coven for the search
/// of each CUE image for where its tracks start.
pub(crate) struct LibraryImages {
    records: Records,
    /// One place, which each image's stream holds while the search reads it.
    /// The searches run one after the other.
    lease: coven::FileLease,
    runtime: tokio::runtime::Handle,
    /// The release file each image path names.
    file_ids: HashMap<PathBuf, String>,
}

impl LibraryImages {
    pub(crate) fn new(
        records: Records,
        lease: coven::FileLease,
        runtime: tokio::runtime::Handle,
        file_ids: HashMap<PathBuf, String>,
    ) -> Self {
        Self { records, lease, runtime, file_ids }
    }
}

impl CueImageOpener for LibraryImages {
    fn open_for_search(
        &self,
        image: &CueAnalyzedAudioFile,
    ) -> Result<[Box<dyn ReadableFile>; bae_audio::SEEK_LANDING_PLACES], ImportError> {
        let file_id = self.file_ids.get(&image.path).ok_or_else(|| ImportError::Internal {
            detail: format!(
                "a CUE sheet's audio at {} is none of the release's files",
                image.path.display()
            ),
        })?;
        // The stream keeps the lease's one place until it drops, and both
        // openings of the search read through it.
        let stream = self.runtime.block_on(self.records.open_release_file(file_id, &self.lease))?;
        let stream = Arc::new(stream);
        Ok([
            Box::new(StreamFile::new(stream.clone(), self.runtime.clone())),
            Box::new(StreamFile::new(stream, self.runtime.clone())),
        ])
    }
}
