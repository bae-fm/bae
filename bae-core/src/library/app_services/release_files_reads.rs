//! The Storage sheet's live read of a release's files on [`AppServices`].

use super::*;
use crate::library::{ReleaseFilesRequest, ReleaseFilesSnapshot, ReleaseFilesSubscription};

impl AppServices {
    /// The files of the release the subscription is set to, in the order it
    /// asks for, as they change: each value answers the request set last.
    /// Another release or order is a new request on the same read.
    pub fn subscribe_release_files(&self) -> ReleaseFilesSubscription {
        let (request_tx, mut requests) = tokio::sync::watch::channel(None::<ReleaseFilesRequest>);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = self.inner.manager.clone();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task(
            "release files",
            task.clone(),
            async move {
                let mut request = None;
                let mut query = reconfigurable_live_query_events(
                    &manager,
                    manager.subscribe_release_files(None),
                );
                loop {
                    tokio::select! {
                        changed = requests.changed() => {
                            if changed.is_err() { return; }
                            request = requests.borrow_and_update().clone();
                            query.set(request.clone());
                        }
                        value = query.recv() => {
                            let Some(value) = value else { return };
                            let snapshot = value.map(|files| ReleaseFilesSnapshot {
                                request: request.clone(),
                                files: files.map(|files| {
                                    files
                                        .into_iter()
                                        .map(crate::album_detail::FileDetail::from_raw)
                                        .collect()
                                }),
                            });
                            if tx.send(snapshot).is_err() { return; }
                        }
                    }
                }
            },
        );
        ReleaseFilesSubscription::new(request_tx, rx, task)
    }
}
