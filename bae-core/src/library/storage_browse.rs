//! The Storage Manager list, read through one live query whose sort, filter,
//! and windows change in place.

use super::{LibraryBrowseWindow, LibraryError, LibraryPageWindows};
use tokio_util::sync::CancellationToken;

/// What the Storage Manager shows: the list under one sort and filter, and
/// the windows of it on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBrowseView {
    pub sort: crate::db::StorageSortCriterion,
    pub filter: crate::db::StorageFilter,
    pub windows: LibraryPageWindows,
}

/// Every window of the list read under one sort and filter — the ones named
/// here, which a UI that has since moved on drops — with the filtered set's
/// row count and total size.
#[derive(Debug, Clone)]
pub struct StorageBrowseSnapshot {
    pub sort: crate::db::StorageSortCriterion,
    pub filter: crate::db::StorageFilter,
    pub windows: Vec<LibraryBrowseWindow<crate::album_detail::StorageRow>>,
    pub total_count: u64,
    pub total_size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageBrowseSubscriptionError {
    #[error("storage browse subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] LibraryError),
}

/// A live read of the Storage Manager list. The view changes in place
/// through [`set_view`](Self::set_view); the upload queue, pin markers, and
/// sync state the rows are resolved against move the same subscription.
pub struct StorageBrowseSubscription {
    view: tokio::sync::watch::Sender<StorageBrowseView>,
    values: tokio::sync::Mutex<
        tokio::sync::mpsc::UnboundedReceiver<Result<StorageBrowseSnapshot, LibraryError>>,
    >,
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl StorageBrowseSubscription {
    /// Owns `task`, which reads the view `view` holds and sends each value on
    /// `values`.
    pub(crate) fn new(
        view: tokio::sync::watch::Sender<StorageBrowseView>,
        values: tokio::sync::mpsc::UnboundedReceiver<Result<StorageBrowseSnapshot, LibraryError>>,
        task: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            view,
            values: tokio::sync::Mutex::new(values),
            cancellation: CancellationToken::new(),
            task,
        }
    }

    /// Read `view` from now on. Repeating the standing view reads nothing
    /// again.
    pub fn set_view(&self, view: StorageBrowseView) -> Result<(), StorageBrowseSubscriptionError> {
        if self.cancellation.is_cancelled() {
            return Err(StorageBrowseSubscriptionError::Cancelled);
        }
        self.view.send_if_modified(|current| {
            if *current == view {
                return false;
            }
            *current = view;
            true
        });
        Ok(())
    }

    /// The next value, or the end of the subscription. Query errors are
    /// values, not the end.
    pub async fn next(&self) -> Result<StorageBrowseSnapshot, StorageBrowseSubscriptionError> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(StorageBrowseSubscriptionError::Cancelled),
            value = async { self.values.lock().await.recv().await } => match value {
                Some(value) => value.map_err(StorageBrowseSubscriptionError::Query),
                None => Err(StorageBrowseSubscriptionError::Cancelled),
            },
        }
    }

    /// Stop reading and settle a waiting [`next`](Self::next).
    pub fn cancel(&self) {
        self.cancellation.cancel();
        self.task.abort();
    }
}

impl Drop for StorageBrowseSubscription {
    fn drop(&mut self) {
        self.cancel();
    }
}
