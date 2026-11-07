//! Release pins restore their previous state if downloading fails or is cancelled.
use super::*;

/// What a pin has heard of its release's files, from coven's download watch
/// and from its reads of which files this device keeps, which together say
/// how many of the release's bytes the pin holds.
#[derive(Default)]
struct PinCount {
    /// How far each file being downloaded has got, as the watch last told it.
    moved: std::collections::BTreeMap<coven::FileId, coven::TransferProgress>,
    /// The release's files whose download for this pin finished, as the watch
    /// told it: each is whole and waits for the write that keeps the pin's
    /// files together once every download has finished.
    fetched: std::collections::BTreeSet<coven::FileId>,
    /// Whether this device keeps each file whole, by release file id, as the
    /// last read told it; `None` until the first.
    kept: Option<std::collections::BTreeMap<String, bool>>,
}

impl PinCount {
    /// Take what the watch told. Only a download that finished brought its
    /// file whole; one that failed, was held or was dropped brought none of
    /// it.
    fn heard(&mut self, changes: Vec<coven::TransferChange>) {
        self.fetched.extend(changes.iter().filter_map(|change| match change {
            coven::TransferChange::Removed { file, end: coven::TransferEnd::Finished } => {
                Some(*file)
            }
            _ => None,
        }));
        crate::file_transfers::apply(&mut self.moved, changes);
    }

    /// Take a read of which of the release's files this device keeps.
    fn read(&mut self, kept: std::collections::BTreeMap<String, bool>) {
        self.kept = Some(kept);
    }

    /// Of the files `sizes` names, by release file id with its coven file
    /// and plaintext size, the bytes the pin holds: each file kept or fetched
    /// whole, and of each file the watch has in flight, the share of its size
    /// its object has moved. The watch's files in flight are the ones
    /// `coven_downloading` lists, and the watch says when each download ends
    /// and how, so a file counts its share until then and its whole size
    /// after only if it finished: the bytes never go back as a file lands,
    /// and never count a file whose download failed. Counting each file's own
    /// share keeps the bytes moving as a file arrives, where the cache's
    /// coverage holds still until a file is kept. `None` before the first
    /// read of which files are kept.
    fn bytes(
        &self,
        sizes: &std::collections::BTreeMap<String, (coven::FileId, u64)>,
    ) -> Option<u64> {
        let kept = self.kept.as_ref()?;
        Some(
            sizes
                .iter()
                .map(|(id, (file, size))| {
                    if kept.get(id) == Some(&true) || self.fetched.contains(file) {
                        return *size;
                    }
                    self.moved.get(file).map_or(0, |progress| {
                        (*size as f64 * crate::file_transfers::fraction(*progress)) as u64
                    })
                })
                .try_fold(0u64, |total, bytes| total.checked_add(bytes))
                .expect("a release's pinned bytes fit its total"),
        )
    }
}

impl LibraryManager {
    pub(crate) async fn pin_release_files_with_progress(
        &self,
        release: &str,
        mut on_progress: impl FnMut(bae_transfers::DownloadTransferProgress) + Send,
    ) -> Result<(), LibraryError> {
        let pinnable = self.release_pinnable_files(release).await?;
        let total = super::coven_files::total_bytes(release, &pinnable)?;
        let ids: Vec<String> = pinnable.iter().map(|entry| entry.row_id.clone()).collect();
        // Each file's plaintext size, by release file id, which its share of
        // the download counts against.
        let sizes: std::collections::BTreeMap<String, (coven::FileId, u64)> = ids
            .iter()
            .cloned()
            .zip(pinnable.iter().map(|entry| (entry.file.id(), entry.bytes)))
            .collect();
        let files = pinnable.into_iter().map(|entry| entry.file).collect::<Vec<_>>();
        let previous = self
            .store
            .files_pinned(&files.iter().map(coven::FileRef::id).collect::<Vec<_>>())
            .await?;
        let changed = files
            .iter()
            .zip(previous)
            .filter_map(|(file, pinned)| (pinned != Some(true)).then_some(file.id()))
            .collect::<Vec<_>>();
        let cancel = crate::CancellationToken::new();
        let cancel_on_drop = cancel.clone().drop_guard();
        let manager = self.clone();
        let release = release.to_string();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let task = self.run_file_operation("pin release", move |closing| async move {
            // Progress reads belong to the operation: if one fails, finish
            // rolling back its pins before reporting the failure. A panic in
            // the pin is caught here, reported, and rolled back the same way.
            let result = caught("pin a release", async {
                // Which of the release's files this device keeps, as each is
                // kept. Its runs can lag the download watch: a file whose
                // download finished counts whole from the watch's word, so
                // the lag never sets the bytes back.
                let kept = manager.records.subscribe_release_files_kept(&release);
                #[cfg(test)]
                let kept = {
                    let hold = manager.pin_reads_hold.clone();
                    kept.process(move |read| {
                        hold.wait();
                        Ok(read)
                    })
                };
                let mut kept = kept;
                let mut transfers = manager.store.subscribe_download_file_transfers();
                let mut count = PinCount::default();
                let pin = manager.store.pin(&files);
                tokio::pin!(pin);
                let mut rate: Option<bae_transfers::DownloadRate> = None;
                loop {
                    // Whether the pin finished.
                    let finished = tokio::select! {
                        _ = cancel.cancelled() => return Err(LibraryError::Storage("pinning release was cancelled".into())),
                        _ = closing.cancelled() => return Err(LibraryError::Storage("pinning release was cancelled because the library is closing".into())),
                        result = &mut pin => {
                            result.map_err(|error| LibraryError::file_read(format!("pin release {release}"), error))?;
                            // The write keeps every file; read that now
                            // rather than wait for the live query's next run.
                            count.read(manager.records.release_files_kept(&release).await?);
                            true
                        }
                        next = kept.next() => {
                            count.read(next.expect("a query of kept files reports why it ended, which returns first")?);
                            false
                        }
                        changes = transfers.next() => {
                            count.heard(changes.ok_or_else(|| LibraryError::Storage(format!("pin release {release}: the store closed")))?);
                            false
                        }
                    };
                    let Some(bytes) = count.bytes(&sizes) else {
                        continue;
                    };
                    let next = bae_transfers::DownloadTransferProgress::new(&release, bytes, total)?;
                    let now = manager.clock.now();
                    let next = rate
                        .get_or_insert_with(|| bae_transfers::DownloadRate::begin(now, next.bytes_done))
                        .measure(next, now);
                    progress_tx.send(next).map_err(|error| LibraryError::Storage(format!("pinning release was cancelled: {error}")))?;
                    if finished {
                        return Ok(());
                    }
                }
            })
            .await
            .unwrap_or_else(|bug| Err(manager.panicked(bug).into()));
            if let Err(operation) = result {
                return match manager.store.unpin(&changed).await {
                    Ok(()) => Err(operation),
                    Err(error) => {
                        let error = LibraryError::Cleanup { operation: Box::new(operation), cleanup: Box::new(error.into()) };
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

/// Where a test holds each run of a pin's query of which of its release's
/// files are kept, whose timing it doesn't otherwise control: while held,
/// each run waits; dropping the returned guard lets them through.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Hold {
    held: std::sync::Mutex<bool>,
    released: std::sync::Condvar,
}

#[cfg(test)]
impl Hold {
    /// Wait, blocking this thread, until no test holds the work.
    pub(crate) fn wait(&self) {
        let _released = self.released.wait_while(self.held.lock().unwrap(), |held| *held).unwrap();
    }

    /// Hold the work until the returned guard drops.
    pub(crate) fn hold(self: &Arc<Self>) -> Held {
        *self.held.lock().unwrap() = true;
        Held(self.clone())
    }
}

/// Lets the held work through when dropped.
#[cfg(test)]
pub(crate) struct Held(Arc<Hold>);

#[cfg(test)]
impl Drop for Held {
    fn drop(&mut self) {
        *self.0.held.lock().unwrap() = false;
        self.0.released.notify_all();
    }
}

#[cfg(test)]
#[path = "pin_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pin_progress_tests.rs"]
mod progress_tests;
