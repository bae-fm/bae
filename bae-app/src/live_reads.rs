//! The live reads the screens pull: each read follows one of the library's
//! views or tables on a task of the library's, resolves what it reads into
//! the shape a screen shows, and moves to another id or window in place. The
//! library closing ends every one.

mod detail;
#[cfg(desktop)]
mod release_files;
mod storage;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod storage_inspector;

pub use detail::*;
#[cfg(desktop)]
pub use release_files::*;
pub use storage::*;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use storage_inspector::*;

use bae_library::live_query_events::reconfigurable_live_query_events;
use bae_library::LibraryManager;
use bae_views::LibraryViews;

/// A live read that follows the request set on it: `open` makes the query for
/// the request set first, a new request points the same query at it, and each
/// value the query reads becomes a snapshot of the request it was read for.
pub(crate) fn follow_request<Request, Projection, Snapshot>(
    library: &LibraryManager,
    task_name: &'static str,
    open: impl FnOnce(Option<Request>) -> coven::ReconfigurableLiveQuery<Option<Request>, Projection>
        + Send
        + 'static,
    snapshot: impl Fn(&Option<Request>, Projection) -> Snapshot + Send + 'static,
) -> bae_library::LiveRead<Option<Request>, Snapshot>
where
    Projection: Clone + PartialEq + Send + 'static,
    Snapshot: Send + 'static,
    Request: Clone + PartialEq + std::fmt::Debug + Send + Sync + 'static,
{
    let (request_tx, mut requests) = tokio::sync::watch::channel(None::<Request>);
    let (tx, rx) = bae_library::live_read_channel();
    let task = bae_library::CancellationToken::new();
    let manager = library.clone();
    library.spawn_stoppable_subscription_task(task_name, task.clone(), tx.clone(), async move {
        let mut request = requests.borrow_and_update().clone();
        let mut query =
            reconfigurable_live_query_events(&manager, open(request.clone()), tx.clone());
        loop {
            tokio::select! {
                event = query.recv() => {
                    let Some(result) = event else { return };
                    let value = result.map(|projection| snapshot(&request, projection));
                    if tx.send(value).is_err() {
                        return;
                    }
                }
                changed = requests.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    request = requests.borrow_and_update().clone();
                    query.set(request.clone());
                }
            }
        }
    });
    bae_library::LiveRead::new(request_tx, rx, task)
}

/// A live read of the Storage Manager list. The view changes in place
/// through [`LiveRead::set`](bae_library::LiveRead::set); the upload
/// queue, pin markers, and sync state the rows are resolved against move the
/// same read.
pub type LiveStorageBrowse =
    bae_library::LiveRead<bae_views::StorageBrowseView, bae_views::StorageBrowseSnapshot>;

/// A live read of one release's files in one order. The release and order
/// change in place through
/// [`LiveRead::set`](bae_library::LiveRead::set); `None` reads
/// nothing.
#[cfg(desktop)]
pub type LiveReleaseFiles =
    bae_library::LiveRead<Option<bae_views::ReleaseFilesRequest>, bae_views::ReleaseFilesSnapshot>;
