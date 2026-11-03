//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;
use bae_library::library::LibraryError;
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
) -> bae_library::library::LiveDetail<StorageInspector> {
    let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
    let (tx, rx) = bae_library::library::live_read_channel();
    let manager = library.clone();
    let views = library_views.clone();
    let task = bae_library::library::CancellationToken::new();
    library.spawn_stoppable_subscription_task(
        "storage inspector",
        task.clone(),
        tx.clone(),
        async move {
            let mut id: Option<String> = None;
            let mut query = reconfigurable_live_query_events(
                &manager,
                views.subscribe_storage_inspector(None),
                tx.clone(),
            );
            let mut told = Told::Nothing;
            let fail = |error: LibraryError| tx.send(Err(error)).is_ok();
            loop {
                // The shown release as read: `None` once no such release
                // exists.
                let rows: Option<bae_views::DbStorageInspector> = tokio::select! {
                    changed = ids.changed() => {
                        if changed.is_err() { return; }
                        id = ids.borrow_and_update().clone();
                        query.set(id.clone());
                        told = Told::Nothing;
                        continue;
                    }
                    value = query.recv() => match value {
                        None => return,
                        Some(Ok(rows)) => rows,
                        Some(Err(error)) => {
                            if !fail(error) { return; }
                            continue;
                        }
                    },
                };
                let Some(release_id) = id.clone() else {
                    continue;
                };
                let value = match rows {
                    None => {
                        if matches!(told, Told::Gone) {
                            continue;
                        }
                        told = Told::Gone;
                        None
                    }
                    Some(rows) => {
                        let inspector = StorageInspector::read(&release_id, rows);
                        if matches!(&told, Told::Shown(shown) if *shown == inspector) {
                            continue;
                        }
                        told = Told::Shown(inspector.clone());
                        Some(inspector)
                    }
                };
                let snapshot = bae_library::library::DetailSnapshot { id: Some(release_id), value };
                if tx.send(Ok(snapshot)).is_err() {
                    return;
                }
            }
        },
    );
    bae_library::library::LiveDetail::new(id_tx, rx, task)
}
