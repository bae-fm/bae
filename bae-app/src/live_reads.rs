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

use bae_library::library::live_query_events::reconfigurable_live_query_events;
use bae_library::library::LibraryManager;
use bae_views::LibraryViews;

/// A live read of the Storage Manager list. The view changes in place
/// through [`LiveRead::set`](bae_library::library::LiveRead::set); the upload
/// queue, pin markers, and sync state the rows are resolved against move the
/// same read.
pub type LiveStorageBrowse =
    bae_library::library::LiveRead<bae_views::StorageBrowseView, bae_views::StorageBrowseSnapshot>;

/// A live read of one release's files in one order. The release and order
/// change in place through
/// [`LiveRead::set`](bae_library::library::LiveRead::set); `None` reads
/// nothing.
#[cfg(desktop)]
pub type LiveReleaseFiles = bae_library::library::LiveRead<
    Option<bae_views::ReleaseFilesRequest>,
    bae_views::ReleaseFilesSnapshot,
>;
