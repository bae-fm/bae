//! Blocking IO run off the async workers.
use super::*;

/// Run blocking IO (files, the keychain, the config file) on a blocking thread.
pub(super) async fn blocking_io<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, LibraryError> + Send + 'static,
) -> Result<T, LibraryError> {
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}
