//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;
use crate::library::storage_inspector::StorageInspector;
use crate::library::LibraryError;

/// What the reader was last told of the release shown.
enum Told {
    /// Nothing since the release was last set.
    Nothing,
    /// That no such release exists.
    Gone,
    /// The release as it was shown.
    Shown(StorageInspector),
}

impl AppServices {
    /// The Storage Manager inspector's release as it changes, read for the id
    /// the subscription is set to: its files, each beside its pin and its
    /// upload while one waits, and its entry in the download, export and
    /// upload queues, all read for this release alone and at once, with the
    /// error behind each upload's record while this process holds it. A
    /// change that leaves what it shows as it was sends nothing; how fast its
    /// uploads and downloads go are their transfers. Another release is a
    /// new id on the same read.
    pub fn subscribe_storage_inspector(
        &self,
    ) -> crate::library::DetailSubscription<StorageInspector> {
        let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = self.inner.manager.clone();
        let mut causes = manager.subscribe_pending_causes();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task(
            "storage inspector",
            task.clone(),
            async move {
                let mut id: Option<String> = None;
                let mut query = reconfigurable_live_query_events(
                    &manager,
                    manager.subscribe_storage_inspector(None),
                );
                // The shown release as last read: `None` until it is read,
                // `Some(None)` once no such release exists.
                let mut read: Option<Option<crate::db::DbStorageInspector>> = None;
                let mut told = Told::Nothing;
                let fail = |error: LibraryError| tx.send(Err(error)).is_ok();
                loop {
                    tokio::select! {
                        changed = ids.changed() => {
                            if changed.is_err() { return; }
                            id = ids.borrow_and_update().clone();
                            read = None;
                            query.set(id.clone());
                            told = Told::Nothing;
                            continue;
                        }
                        value = query.recv() => match value {
                            None => return,
                            Some(Ok(rows)) => read = Some(rows),
                            Some(Err(error)) => {
                                read = None;
                                if !fail(error) { return; }
                                continue;
                            }
                        },
                        changed = causes.changed() => {
                            if changed.is_err() { return; }
                        }
                    }
                    let (Some(release_id), Some(rows)) = (id.clone(), &read) else {
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
                            let inspector = StorageInspector::read(
                                &release_id,
                                rows.clone(),
                                &causes.borrow_and_update(),
                            );
                            if matches!(&told, Told::Shown(shown) if *shown == inspector) {
                                continue;
                            }
                            told = Told::Shown(inspector.clone());
                            Some(inspector)
                        }
                    };
                    let snapshot = crate::library::DetailSnapshot {
                        id: Some(release_id),
                        value,
                    };
                    if tx.send(Ok(snapshot)).is_err() {
                        return;
                    }
                }
            },
        );
        crate::library::DetailSubscription::new(id_tx, rx, task)
    }
}
