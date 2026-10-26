//! What the tests' fills fetch by and fail with.

use crate::sparse_buffer::{ShortRead, SourceReadError};

/// The window the tests fill by: the 4 MiB bae-core's playback fills by.
pub(crate) const TEST_WINDOW: u64 = 4 * 1024 * 1024;

/// A test source's read failure: an OS error with what was being read, or a
/// read that came back short.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TestReadError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Short(#[from] ShortRead),
}

impl TestReadError {
    pub(crate) fn io(context: impl Into<String>, kind: std::io::ErrorKind) -> Self {
        Self::Io {
            context: context.into(),
            source: std::io::Error::from(kind),
        }
    }
}

/// The OS error kind behind a buffer's source-read failure, when an OS read
/// is what failed.
pub(crate) fn io_kind(error: &SourceReadError) -> Option<std::io::ErrorKind> {
    std::error::Error::source(error)
        .and_then(|source| source.downcast_ref::<std::io::Error>())
        .map(std::io::Error::kind)
}
