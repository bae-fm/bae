//! Release pins restore their previous state if downloading fails or is cancelled.
use super::*;

/// What a pin has heard of its release's files, from coven's download watch
/// and from its own reads of the release's pins, which together say how many
/// of the release's bytes the pin holds.
#[derive(Default)]
struct PinCount {
    /// How far each file being downloaded has got, as the watch last told it.
    moved: std::collections::BTreeMap<coven::FileId, coven::TransferProgress>,
    /// The release's files whose download for this pin has ended whole, as
    /// the watch says a download finished or `read` infers from the pins.
    /// coven commits the pin's files together once every download is, so
    /// each waits whole for that write.
    fetched: std::collections::BTreeSet<coven::FileId>,
    /// Where each file stood at the last read of the pins; `None` until the
    /// first.
    read: Option<std::collections::BTreeMap<String, bae_records::DbFilePin>>,
}

impl PinCount {
    /// Take what the watch told, and say whether a download ended or the
    /// watch started over, after which the pins are read again. A share that
    /// only started or moved needs no read: until the pins say the file
    /// downloads it counts nothing, and after, its share only grows. Only a
    /// download that finished brought its file whole; one that failed, was
    /// held or was dropped brought none of it.
    fn heard(&mut self, changes: Vec<coven::TransferChange>) -> bool {
        let ended =
            changes.iter().any(|change| !matches!(change, coven::TransferChange::Updated { .. }));
        self.fetched.extend(changes.iter().filter_map(|change| match change {
            coven::TransferChange::Removed { file, end: coven::TransferEnd::Finished } => {
                Some(*file)
            }
            _ => None,
        }));
        crate::file_transfers::apply(&mut self.moved, changes);
        ended
    }

    /// Take a read of the release's pins. The pins and the watch each say
    /// when a download ends, in either order: a file the last read showed
    /// downloading that this one shows neither downloading nor kept has
    /// ended its download whole, whether or not the watch has said so yet.
    fn read(&mut self, pins: std::collections::BTreeMap<String, bae_records::DbFilePin>) {
        if let Some(last) = &self.read {
            for (id, pin) in last {
                let Some(file) = pin.downloading else { continue };
                if pins.get(id).is_some_and(|now| !now.kept && now.downloading.is_none()) {
                    self.fetched.insert(file);
                }
            }
        }
        self.read = Some(pins);
    }

    /// Of the files `sizes` names, by release file id with its coven file
    /// and plaintext size, the bytes the pin holds: each file kept or
    /// fetched whole, and of each file downloading to be kept, the share of
    /// its size its object has moved. Counting each file's own share keeps
    /// the bytes moving as a file arrives, where the cache's coverage holds
    /// still until a file is kept. `None` before the pins were first read.
    fn bytes(
        &self,
        sizes: &std::collections::BTreeMap<String, (coven::FileId, u64)>,
    ) -> Option<u64> {
        let pins = self.read.as_ref()?;
        Some(
            sizes
                .iter()
                .map(|(id, (file, size))| match pins.get(id) {
                    Some(pin) if pin.kept => *size,
                    _ if self.fetched.contains(file) => *size,
                    Some(bae_records::DbFilePin { downloading: Some(file), .. }) => {
                        self.moved.get(file).map_or(0, |progress| {
                            (*size as f64 * crate::file_transfers::fraction(*progress)) as u64
                        })
                    }
                    _ => 0,
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
                // Says when the release's pins change. Its value can predate
                // what the download watch has told since: joined with the
                // watch, a file whose download just ended would count as
                // neither kept nor downloading, and the bytes would fall
                // back. So each count reads the pins itself, after the
                // watch's state it joins.
                let pins = manager.records.subscribe_release_file_pins(&release);
                #[cfg(test)]
                let pins = {
                    let hold = manager.pin_reads_hold.clone();
                    pins.process(move |read| {
                        hold.wait();
                        Ok(read)
                    })
                };
                let mut pins = pins;
                let mut transfers = manager.store.subscribe_download_file_transfers();
                let mut count = PinCount::default();
                let pin = manager.store.pin(&files);
                tokio::pin!(pin);
                let mut rate: Option<bae_transfers::DownloadRate> = None;
                loop {
                    // Whether the pin finished, and whether the pins must be
                    // read again before counting.
                    let (finished, reread) = tokio::select! {
                        _ = cancel.cancelled() => return Err(LibraryError::Storage("pinning release was cancelled".into())),
                        _ = closing.cancelled() => return Err(LibraryError::Storage("pinning release was cancelled because the library is closing".into())),
                        result = &mut pin => {
                            result.map_err(|error| LibraryError::file_read(format!("pin release {release}"), error))?;
                            (true, true)
                        }
                        next = pins.next() => {
                            next.expect("a pin query reports why it ended, which returns first")?;
                            (false, true)
                        }
                        changes = transfers.next() => {
                            let changes = changes.ok_or_else(|| LibraryError::Storage(format!("pin release {release}: the store closed")))?;
                            (false, count.heard(changes))
                        }
                    };
                    if reread {
                        count.read(manager.records.release_file_pins(&release).await?);
                    }
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

/// Where a test holds each run of a pin's query of its release's pins, whose
/// timing it doesn't otherwise control: while held, each run waits; dropping
/// the returned guard lets them through.
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
