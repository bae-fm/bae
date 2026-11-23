//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;
use bae_views::storage_inspector::StorageInspector;

/// The Storage Manager inspector's release as it changes, read for the id
/// the subscription is set to: its files, each beside its download and its
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

/// Returns the release file whose id the subscription is set to, as the
/// Storage Manager inspector's file details show it, and sends it again each
/// time it changes. The read sends none while the library holds no file with
/// that id. To read another file, the caller sets a new id on the same
/// subscription.
pub fn subscribe_storage_file(
    library: &LibraryManager,
    library_views: &LibraryViews,
) -> bae_library::LiveDetail<bae_views::album_detail::FileDetail> {
    detail_read(library, library_views.subscribe_storage_file(None), |file| file)
}
