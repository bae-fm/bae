//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;
use crate::album_detail::FileDetail;
use crate::library::storage_inspector::{
    selected_upload_group, storage_inspector_transfers, StorageInspector, StorageInspectorTransfer,
};
use crate::library::{
    LibraryError, LiveReadError, OutboxPauseState, OutboxState, ReleaseQueueEntries,
    UploadReleaseGroup,
};

type OutboxValues = tokio::sync::watch::Receiver<Option<Result<OutboxState, String>>>;

/// The shown release's upload group and whether the upload queue is paused,
/// as the queue at `revision` holds them.
struct ShownUploads {
    revision: u64,
    group: Option<UploadReleaseGroup>,
    paused: bool,
}

impl ShownUploads {
    fn of(state: &OutboxState, release_id: &str) -> Self {
        Self {
            revision: state.revision,
            group: selected_upload_group(release_id, &state.queue.upload_groups),
            paused: state.queue.pause_state == OutboxPauseState::Paused,
        }
    }
}

/// What the reader was last told of the release shown.
enum Told {
    /// Nothing since the release or its files were last read.
    Nothing,
    /// That no such release exists.
    Gone,
    /// The release with these transfers.
    Shown(Vec<StorageInspectorTransfer>),
}

impl AppServices {
    /// The Storage Manager inspector's release as it changes, read for the id
    /// the subscription is set to: its files, each beside the upload coven
    /// holds for it, and its entry in the download, export and upload
    /// queues, the first two read for this release alone. A queue change
    /// that leaves the release's entries as they were sends nothing; the
    /// bytes its uploads and downloads move are their transfers. Another
    /// release is a new id on the same read.
    pub fn subscribe_storage_inspector(
        &self,
    ) -> crate::library::DetailSubscription<StorageInspector> {
        let (id_tx, mut ids) = tokio::sync::watch::channel(None::<String>);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let manager = self.inner.manager.clone();
        let release_detail = self.subscribe_release_detail();
        let mut outbox = self.subscribe_outbox_values();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task("storage inspector", task.clone(), async move {
            let mut id: Option<String> = None;
            // The shown release's files as last read: `None` until they are,
            // `Some(None)` once no such release exists.
            let mut files: Option<Option<Vec<FileDetail>>> = None;
            let mut queues = reconfigurable_live_query_events(
                &manager,
                manager.subscribe_release_queue_entries(None),
            );
            // The release's download and export entries as last read: `None`
            // until they are read for the release shown.
            let mut entries: Option<ReleaseQueueEntries> = None;
            // `None` until read for the release shown, and after the upload
            // queue moves.
            let mut uploads: Option<ShownUploads> = None;
            let mut told = Told::Nothing;
            let fail = |error: LibraryError| tx.send(Err(error)).is_ok();
            loop {
                tokio::select! {
                    changed = ids.changed() => {
                        if changed.is_err() { return; }
                        id = ids.borrow_and_update().clone();
                        files = None;
                        entries = None;
                        queues.set(id.clone());
                        uploads = None;
                        told = Told::Nothing;
                        if release_detail.set(id.clone()).is_err() { return; }
                        continue;
                    }
                    delivered = release_detail.next() => match delivered {
                        // A value read for the release shown before it moved
                        // shows nothing now.
                        Ok(snapshot) if snapshot.id != id => continue,
                        Ok(snapshot) => {
                            files = Some(snapshot.value.map(|detail| detail.files));
                            told = Told::Nothing;
                        }
                        Err(LiveReadError::Cancelled) => return,
                        Err(LiveReadError::Query(error)) => {
                            files = None;
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
                    changed = outbox.changed() => {
                        if changed.is_err() { return; }
                        // A transfer that moves on the same queue changes
                        // nothing shown here.
                        let moved = match &*outbox.borrow() {
                            Some(Ok(state)) => {
                                uploads.as_ref().map(|shown| shown.revision) != Some(state.revision)
                            }
                            Some(Err(_)) | None => true,
                        };
                        if !moved { continue; }
                        uploads = None;
                    }
                }
                let Some(release_id) = id.clone() else {
                    continue;
                };
                if uploads.is_none() {
                    match read_uploads(&mut outbox, &services, &release_id).await {
                        Ok(shown) => uploads = Some(shown),
                        Err(error) => {
                            if !fail(error) {
                                return;
                            }
                            continue;
                        }
                    }
                }
                let (Some(read_files), Some(entries), Some(uploads)) = (&files, &entries, &uploads)
                else {
                    continue;
                };
                let value = match read_files {
                    None => {
                        if matches!(told, Told::Gone) {
                            continue;
                        }
                        told = Told::Gone;
                        None
                    }
                    Some(release_files) => {
                        let transfers = storage_inspector_transfers(
                            entries,
                            uploads.group.clone(),
                            uploads.paused,
                        );
                        if matches!(&told, Told::Shown(shown) if *shown == transfers) {
                            continue;
                        }
                        told = Told::Shown(transfers.clone());
                        Some(StorageInspector::of(release_files.clone(), transfers))
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

/// The release's upload group as the outbox holds the queue now, read from
/// the durable queue when nothing has been published yet.
async fn read_uploads(
    outbox: &mut OutboxValues,
    services: &AppServices,
    release_id: &str,
) -> Result<ShownUploads, LibraryError> {
    let published = match &*outbox.borrow_and_update() {
        Some(Ok(state)) => Some(Ok(ShownUploads::of(state, release_id))),
        Some(Err(error)) => Some(Err(LibraryError::Internal(error.clone()))),
        None => None,
    };
    match published {
        Some(shown) => shown,
        None => services
            .outbox_state()
            .await
            .map(|state| ShownUploads::of(&state, release_id)),
    }
}
