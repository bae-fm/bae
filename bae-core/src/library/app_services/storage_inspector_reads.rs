//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;
use crate::album_detail::{FileDetail, ReleaseUpload};
use crate::library::storage_inspector::{storage_inspector_transfers, StorageInspector};
use crate::library::{LibraryError, LiveReadError, ReleaseQueueEntries};

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
    /// the subscription is set to: its files, each beside its upload while
    /// one waits, and its entry in the download, export and upload queues,
    /// all read for this release alone, with whether each queue is paused and
    /// the error behind each upload's record while this process holds it. A
    /// change that leaves what it shows as it was sends nothing; how fast its
    /// uploads and downloads go are their transfers. Another release is a
    /// new id on the same read.
    pub fn subscribe_storage_inspector(
        &self,
    ) -> crate::library::DetailSubscription<StorageInspector> {
        let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = self.inner.manager.clone();
        let release_detail = self.subscribe_release_detail();
        let mut paused_reads = manager.subscribe_uploads_paused();
        let mut causes = manager.subscribe_pending_causes();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task("storage inspector", task.clone(), async move {
            let mut id: Option<String> = None;
            // The shown release's files and its files waiting to upload as
            // last read: `None` until they are, `Some(None)` once no such
            // release exists.
            let mut release: Option<Option<(Vec<FileDetail>, Option<ReleaseUpload>)>> = None;
            let mut queues = reconfigurable_live_query_events(
                &manager,
                manager.subscribe_release_queue_entries(None),
            );
            // The release's download and export entries as last read: `None`
            // until they are read for the release shown.
            let mut entries: Option<ReleaseQueueEntries> = None;
            // Whether uploads are paused, as last read.
            let mut uploads_paused: Option<bool> = None;
            let mut told = Told::Nothing;
            let fail = |error: LibraryError| tx.send(Err(error)).is_ok();
            loop {
                tokio::select! {
                    changed = ids.changed() => {
                        if changed.is_err() { return; }
                        id = ids.borrow_and_update().clone();
                        release = None;
                        entries = None;
                        queues.set(id.clone());
                        told = Told::Nothing;
                        if release_detail.set(id.clone()).is_err() { return; }
                        continue;
                    }
                    delivered = release_detail.next() => match delivered {
                        // A value read for the release shown before it moved
                        // shows nothing now.
                        Ok(snapshot) if snapshot.id != id => continue,
                        Ok(snapshot) => {
                            release = Some(snapshot.value.map(|detail| (detail.files, detail.summary.upload)));
                        }
                        Err(LiveReadError::Cancelled) => return,
                        Err(LiveReadError::Query(error)) => {
                            release = None;
                            if !fail(error) { return; }
                            continue;
                        }
                    },
                    value = queues.recv() => match value {
                        None => return,
                        Some(Ok(read)) => entries = Some(read),
                        Some(Err(error)) => {
                            entries = None;
                            if !fail(error) { return; }
                            continue;
                        }
                    },
                    read = paused_reads.next() => match read {
                        None => return,
                        Some(Ok(paused)) => uploads_paused = Some(paused),
                        Some(Err(error)) => {
                            uploads_paused = None;
                            if !fail(LibraryError::from(error)) { return; }
                            continue;
                        }
                    },
                    changed = causes.changed() => {
                        if changed.is_err() { return; }
                    }
                }
                let Some(release_id) = id.clone() else {
                    continue;
                };
                let (Some(read), Some(entries), Some(uploads_paused)) = (&release, &entries, uploads_paused)
                else {
                    continue;
                };
                let value = match read {
                    None => {
                        if matches!(told, Told::Gone) {
                            continue;
                        }
                        told = Told::Gone;
                        None
                    }
                    Some((files, upload)) => {
                        let inspector = StorageInspector::of(
                            files.clone(),
                            storage_inspector_transfers(&release_id, entries, upload.clone(), uploads_paused),
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
        });
        crate::library::DetailSubscription::new(id_tx, rx, task)
    }
}
