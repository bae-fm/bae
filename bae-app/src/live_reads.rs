//! The live reads the screens pull: each read follows one of the library's
//! views or tables through one live query of the library's, resolves what it
//! reads into the shape a screen shows, and moves to another id or window in
//! place. The library closing ends every one.

mod detail;
#[cfg(desktop)]
mod import_candidate;
#[cfg(desktop)]
mod release_files;
mod storage;
#[cfg(desktop)]
mod storage_inspector;

pub use detail::*;
#[cfg(desktop)]
pub use import_candidate::LiveImportCandidate;
#[cfg(desktop)]
pub use release_files::*;
pub use storage::*;
#[cfg(desktop)]
pub use storage_inspector::*;

use bae_library::LibraryManager;
use bae_store::live_query::LiveRead;
use bae_views::LibraryViews;

/// A live read that follows the request set on it: `query` starts with no
/// request, a new request points it at that, and each value it reads becomes
/// a snapshot of the request it was read for.
pub(crate) fn follow_request<Request, Projection, Snapshot>(
    library: &LibraryManager,
    query: coven::ReconfigurableLiveQuery<Option<Request>, Projection, coven::CovenError>,
    snapshot: impl Fn(&Option<Request>, Projection) -> Snapshot + Send + 'static,
) -> LiveRead<Option<Request>, Snapshot>
where
    Projection: Clone + PartialEq + Send + 'static,
    Snapshot: Send + 'static,
    Request: Clone + PartialEq + Send + Sync + 'static,
{
    LiveRead::new(library.until_closed(query), move |read, projection| {
        Some(snapshot(read.request, projection))
    })
}

/// A live read of the Storage Manager list. The view changes in place
/// through [`LiveRead::set`]; the upload
/// queue, pin markers, and sync state the rows are resolved against move the
/// same read.
pub type LiveStorageBrowse =
    LiveRead<bae_views::StorageBrowseRequest, bae_views::StorageBrowseSnapshot>;

/// A live read of one release's files in one order. The release and order
/// change in place through
/// [`LiveRead::set`]; `None` reads
/// nothing.
#[cfg(desktop)]
pub type LiveReleaseFiles =
    LiveRead<Option<bae_views::ReleaseFilesRequest>, bae_views::ReleaseFilesSnapshot>;
