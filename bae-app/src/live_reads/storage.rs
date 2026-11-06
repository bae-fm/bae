//! The Storage Manager's live read on [`AppServices`].

use super::*;

/// The Storage Manager list as it changes, read in the view the
/// subscription is asked for: its rows, whose pin markers the query reads
/// from the listing's pinned counts, whose uploads from coven's upload
/// queue and whose transitions from `release_storage_action` and the
/// download queue, and whether a cloud home is connected — the state the
/// rows are resolved against. One query serves every window: a new view
/// points it at what that reads.
pub fn subscribe_storage_browse(
    library: &LibraryManager,
    library_views: &LibraryViews,
    initial: bae_views::StorageBrowseRequest,
) -> crate::live_reads::LiveStorageBrowse {
    crate::live_reads::LiveStorageBrowse::new(
        library.until_closed(library_views.subscribe_storage_browse(initial)),
        |_, projection| Some(bae_views::storage::resolve_storage_browse(projection)),
    )
}
