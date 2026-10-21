//! Release pins restore their previous state if downloading fails or is cancelled.
use super::*;

impl LibraryManager {
    pub(crate) async fn pin_release_blobs_with_progress(
        &self,
        release: &str,
        mut on_progress: impl FnMut(crate::library::DownloadTransferProgress) + Send,
    ) -> Result<(), LibraryError> {
        let files = self
            .release_pinnable_blobs(release)
            .await?
            .into_iter()
            .map(|entry| entry.blob)
            .collect::<Vec<_>>();
        let ids = files
            .iter()
            .map(|file| match file.key().values() {
                [rusqlite::types::Value::Text(id)] => Ok(id.clone()),
                _ => Err(LibraryError::Storage(
                    "release file has a non-text key".into(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let previous = self
            .database
            .rows_pinned(crate::sync::RELEASE_FILES_NAMESPACE, ids)
            .await?;
        let changed = files
            .iter()
            .zip(previous)
            .filter_map(|(file, pinned)| (pinned != Some(true)).then_some(file.clone()))
            .collect::<Vec<_>>();
        let cancel = crate::library::CancellationToken::new();
        let cancel_on_drop = cancel.clone().drop_guard();
        let manager = self.clone();
        let release = release.to_string();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let task = self.run_file_operation("pin release", move |closing| async move {
            // Progress reads belong to the operation: if one fails, finish
            // rolling back its pins before reporting the failure.
            let result = async {
                let mut progress = manager.database.subscribe_pin_progress(files.clone());
                let pin = manager.database.pin(&files);
                tokio::pin!(pin);
                loop {
                    let (next, finished) = tokio::select! {
                        _ = cancel.cancelled() => return Err(LibraryError::Storage("pinning release was cancelled".into())),
                        _ = closing.cancelled() => return Err(LibraryError::Storage("pinning release was cancelled because the library is closing".into())),
                        result = &mut pin => {
                            result.map_err(|error| LibraryError::blob(format!("pin release {release}"), error))?;
                            (manager.database.pin_progress(files.clone()).await?, true)
                        }
                        next = progress.next() => (next?, false),
                    };
                    let next = crate::library::DownloadTransferProgress::new(
                        &release, next.bytes_cached, next.bytes_total,
                    )?;
                    progress_tx.send(next).map_err(|error| LibraryError::Storage(format!("pinning release was cancelled: {error}")))?;
                    if finished {
                        return Ok(());
                    }
                }
            }.await;
            if let Err(operation) = result {
                return match manager.database.unpin(&changed).await {
                    Ok(()) => Err(operation),
                    Err(error) => {
                        let error = LibraryError::Cleanup { operation: Box::new(operation), cleanup: Box::new(LibraryError::blob("restore release pins", error)) };
                        tracing::error!(%error, "could not roll back release pins");
                        Err(error)
                    }
                };
            }
            Ok(())
        });
        tokio::pin!(task);
        let result = loop {
            tokio::select! {
                result = &mut task => {
                    while let Ok(progress) = progress_rx.try_recv() {
                        on_progress(progress);
                    }
                    break result;
                },
                progress = progress_rx.recv() => match progress {
                    Some(progress) => on_progress(progress),
                    None => break task.await,
                },
            }
        };
        cancel_on_drop.disarm();
        result
    }
}
