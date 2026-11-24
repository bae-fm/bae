//! A stored file read through the store.

/// coven's stream over one file, opened through the store. Each read reports
/// a bug it carries back to the store's bug sink as it returns, as the
/// store's own calls do, so a reader above shows the failure and doesn't
/// report it again.
pub struct FileStream {
    stream: coven::FileStream,
    bugs: coven::task::BugSinkRef,
}

impl FileStream {
    pub(crate) fn new(stream: coven::FileStream, bugs: coven::task::BugSinkRef) -> Self {
        Self { stream, bugs }
    }

    /// The file's whole size in bytes.
    pub fn plaintext_size(&self) -> u64 {
        self.stream.plaintext_size()
    }

    /// Read `len` bytes at `offset`. A range past the end is an error, never
    /// a short read.
    pub async fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>, coven::FileReadError> {
        self.stream
            .read_at(offset, len)
            .await
            .map_err(|error| coven::task::reported(self.bugs.as_ref(), error))
    }

    /// The stream as `Read` and `Seek` for a thread that may block, such as a
    /// decoder's: each read waits on `runtime` for the bytes it asks for, and
    /// nothing is mapped into memory.
    pub fn into_blocking_reader(self, runtime: tokio::runtime::Handle) -> BlockingFileReader {
        std::sync::Arc::new(self).blocking_reader(runtime)
    }

    /// A reader of the stream like [`into_blocking_reader`](Self::into_blocking_reader)
    /// that shares it: each reader keeps its own position, and the stream
    /// lives until the last of them drops.
    pub fn blocking_reader(
        self: &std::sync::Arc<Self>,
        runtime: tokio::runtime::Handle,
    ) -> BlockingFileReader {
        BlockingFileReader { stream: self.clone(), runtime, position: 0 }
    }

    /// `read_at`, telling `progress` how many of the range's bytes are here
    /// each time that grows while the read fetches from storage.
    pub async fn read_at_reporting(
        &self,
        offset: u64,
        len: u64,
        progress: &coven::ReadProgress,
    ) -> Result<Vec<u8>, coven::FileReadError> {
        self.stream
            .read_at_reporting(offset, len, progress)
            .await
            .map_err(|error| coven::task::reported(self.bugs.as_ref(), error))
    }
}

/// A [`FileStream`] read with `Read` and `Seek` on a thread that may block:
/// each read asks the stream for the range it fills, and waits for it.
pub struct BlockingFileReader {
    stream: std::sync::Arc<FileStream>,
    runtime: tokio::runtime::Handle,
    position: u64,
}

impl std::io::Read for BlockingFileReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let left = self.stream.plaintext_size().saturating_sub(self.position);
        let length = left.min(u64::try_from(buffer.len()).map_err(std::io::Error::other)?);
        if length == 0 {
            return Ok(0);
        }
        let bytes = self
            .runtime
            .block_on(self.stream.read_at(self.position, length))
            .map_err(std::io::Error::other)?;
        buffer[..bytes.len()].copy_from_slice(&bytes);
        self.position += length;
        Ok(bytes.len())
    }
}

impl std::io::Seek for BlockingFileReader {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        let target = match position {
            std::io::SeekFrom::Start(offset) => Some(offset),
            std::io::SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            std::io::SeekFrom::End(offset) => {
                self.stream.plaintext_size().checked_add_signed(offset)
            }
        };
        let Some(target) = target else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "a seek before the start of a stored file",
            ));
        };
        self.position = target;
        Ok(target)
    }
}
