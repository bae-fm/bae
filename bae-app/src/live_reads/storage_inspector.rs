//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;
use bae_views::storage_inspector::StorageInspector;

/// What the reader was last told of the release shown.
enum Told {
    /// Nothing since the release was last set.
    Nothing,
    /// That no such release exists.
    Gone,
    /// The release as it was shown.
    Shown(StorageInspector),
}

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
    // What the reader was told, and the revision of the id it was told for:
    // a new revision is a new `set`, which starts telling over.
    let mut told = Told::Nothing;
    let mut told_for: Option<u64> = None;
    bae_library::LiveDetail::new(
        library.until_closed(library_views.subscribe_storage_inspector(None)),
        move |read: bae_store::live_query::ReadFor<'_, Option<String>>,
              rows: Option<bae_views::DbStorageInspector>| {
            if told_for != Some(read.revision) {
                told_for = Some(read.revision);
                told = Told::Nothing;
            }
            let release_id = read.request.clone()?;
            // `rows` is the shown release as read: `None` once no such
            // release exists.
            let value = match rows {
                None => {
                    if matches!(told, Told::Gone) {
                        return None;
                    }
                    told = Told::Gone;
                    None
                }
                Some(rows) => {
                    let inspector = StorageInspector::read(&release_id, rows);
                    if matches!(&told, Told::Shown(shown) if *shown == inspector) {
                        return None;
                    }
                    told = Told::Shown(inspector.clone());
                    Some(inspector)
                }
            };
            Some(bae_library::DetailSnapshot { id: Some(release_id), value })
        },
    )
}
