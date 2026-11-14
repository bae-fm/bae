//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;
use bae_views::storage_inspector::StorageInspector;

/// The Storage Manager inspector's release as it changes, read for the id
/// the subscription is set to: its files, each beside its pin and its
/// upload while one waits, and its entry in the download, export and
/// upload queues, all read for this release alone and at once, with the
/// error behind each upload's record when coven keeps it. A
/// change that leaves what it shows as it was sends nothing; how fast its
/// uploads and downloads go are their transfers. Another release is a
/// new id on the same read.
pub fn subscribe_storage_inspector(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> bae_library::LiveDetail<StorageInspector> {
    detail_read(library, library_views.subscribe_storage_inspector(None), |inspector| inspector)
}
