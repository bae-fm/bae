//! The live reads the screens pull: each read follows one of the library's
//! views or tables through one live query of the library's, resolves what it
//! reads into the shape a screen shows, and moves to another id or window in
//! place. The library closing ends every one.

mod detail;
#[cfg(desktop)]
mod import_candidate;
#[cfg(desktop)]
mod release_files;
#[cfg(desktop)]
mod storage_inspector;

pub use detail::*;
#[cfg(desktop)]
pub use import_candidate::LiveImportCandidate;
#[cfg(desktop)]
pub use release_files::*;
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
    query: coven::ReconfigurableLiveQuery<Option<Request>, Projection, bae_store::DbError>,
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

/// A live read of the rows of the selected releases the Storage Manager list
/// holds. The selection changes in place through [`LiveRead::set`].
pub type LiveStorageSelection =
    LiveRead<std::collections::BTreeSet<String>, bae_views::StorageSelection>;

/// A live read of the covers a library release's picker shows. The release
/// and source change in place through [`LiveRead::set`]; `None` reads
/// nothing.
#[cfg(desktop)]
pub type LiveCoverGallery =
    LiveRead<Option<bae_import::CoverGalleryRequest>, bae_import::CoverGallerySnapshot>;

/// A live read of one release's files in one order. The release and order
/// change in place through
/// [`LiveRead::set`]; `None` reads
/// nothing.
#[cfg(desktop)]
pub type LiveReleaseFiles =
    LiveRead<Option<bae_views::ReleaseFilesRequest>, bae_views::ReleaseFilesSnapshot>;
