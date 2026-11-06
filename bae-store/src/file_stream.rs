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
