//! Release pins restore their previous state if downloading fails or is cancelled.
use super::*;

/// Of the files `sizes` names, by release file id with its plaintext size,
/// the bytes a pin holds: each file kept whole, and of each file downloading
/// to be kept, the share of its size its object has moved as `moved` tells
/// it. Counting each file's own share keeps the bytes moving as a file
/// arrives, where the cache's coverage holds still until a file is kept.
fn pinned_bytes(
    sizes: &std::collections::BTreeMap<String, u64>,
    pins: &std::collections::BTreeMap<String, crate::db::DbFilePin>,
    moved: &std::collections::BTreeMap<coven::FileId, coven::TransferProgress>,
) -> u64 {
    sizes
        .iter()
        .map(|(id, size)| match pins.get(id) {
            Some(pin) if pin.kept => *size,
            Some(crate::db::DbFilePin {
                downloading: Some(file),
                ..
            }) => moved.get(file).map_or(0, |progress| {
                (*size as f64 * crate::library::file_transfers::fraction(*progress)) as u64
            }),
            _ => 0,
        })
        .try_fold(0u64, |total, bytes| total.checked_add(bytes))
        .expect("a release's pinned bytes fit its total")
}

impl LibraryManager {
    pub(crate) async fn pin_release_files_with_progress(
        &self,
        release: &str,
        mut on_progress: impl FnMut(crate::library::DownloadTransferProgress) + Send,
    ) -> Result<(), LibraryError> {
        let pinnable = self.release_pinnable_files(release).await?;
        let total = super::coven_files::total_bytes(release, &pinnable)?;
        let ids: Vec<String> = pinnable.iter().map(|entry| entry.row_id.clone()).collect();
        // Each file's plaintext size, by release file id, which its share of
        // the download counts against.
        let sizes: std::collections::BTreeMap<String, u64> = ids
            .iter()
            .cloned()
            .zip(pinnable.iter().map(|entry| entry.bytes))
            .collect();
        let files = pinnable
            .into_iter()
            .map(|entry| entry.file)
            .collect::<Vec<_>>();
        let previous = self
            .database
            .files_pinned(&files.iter().map(coven::FileRef::id).collect::<Vec<_>>())
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
                // Says when the release's pins change. Its value can predate
                // what the download watch has told since: joined with the
                // watch, a file whose download just ended would count as
                // neither kept nor downloading, and the bytes would fall
                // back. So each count reads the pins itself, after the
                // watch's state it joins.
                let pins = manager.database.subscribe_release_file_pins(&release);
                #[cfg(test)]
                let pins = {
                    let hold = manager.pin_reads_hold.clone();
                    pins.process(move |read| {
                        hold.wait();
                        Ok(read)
                    })
                };
                let mut pins = pins;
                let mut transfers = manager.database.subscribe_download_file_transfers();
                // How far each file being downloaded has got, as the watch
                // last told it.
                let mut moved = std::collections::BTreeMap::new();
                // Where each file stands, read after the watch's first word
                // and after each download it told ended; `None` until first
                // read.
                let mut read = None;
                let pin = manager.database.pin(&files);
                tokio::pin!(pin);
                let mut rate: Option<crate::library::download_snapshot::DownloadRate> = None;
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
                            // coven ends a download only after keeping its
                            // file, so a read after the end counts it kept.
                            // A share that only started or moved needs no
                            // read: until the pins say the file downloads it
                            // counts nothing, and after, its share only grows.
                            let ended = changes
                                .iter()
                                .any(|change| !matches!(change, coven::TransferChange::Updated { .. }));
                            crate::library::file_transfers::apply(&mut moved, changes);
                            (false, ended)
                        }
                    };
                    if reread {
                        read = Some(manager.database.release_file_pins(&release).await?);
                    }
                    let Some(read) = &read else {
                        continue;
                    };
                    let next = crate::library::DownloadTransferProgress::new(
                        &release,
                        pinned_bytes(&sizes, read, &moved),
                        total,
                    )?;
                    let now = manager.clock.now();
                    let next = rate
                        .get_or_insert_with(|| crate::library::download_snapshot::DownloadRate::begin(now, next.bytes_done))
                        .measure(next, now);
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
                        let error = LibraryError::Cleanup { operation: Box::new(operation), cleanup: Box::new(LibraryError::file_read("restore release pins", error)) };
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

#[cfg(test)]
mod pinned_bytes_tests {
    use super::pinned_bytes;
    use crate::db::DbFilePin;
    use std::collections::BTreeMap;

    fn file(n: u128) -> coven::FileId {
        coven::FileId(uuid::Uuid::from_u128(n))
    }

    /// A kept file counts its whole size, a file downloading the share of its
    /// size its object has moved, and a file waiting, or downloading but not
    /// yet told, counts none.
    #[test]
    fn a_pin_holds_its_kept_files_and_each_downloads_share() {
        let sizes: BTreeMap<String, u64> = [
            ("kept".to_string(), 1_000),
            ("downloading".to_string(), 400),
            ("untold".to_string(), 300),
            ("waiting".to_string(), 200),
        ]
        .into_iter()
        .collect();
        let pin = |kept, downloading| DbFilePin { kept, downloading };
        let pins: BTreeMap<String, DbFilePin> = [
            ("kept".to_string(), pin(true, None)),
            ("downloading".to_string(), pin(false, Some(file(1)))),
            ("untold".to_string(), pin(false, Some(file(2)))),
            ("waiting".to_string(), pin(false, None)),
        ]
        .into_iter()
        .collect();
        let moved = [(
            file(1),
            coven::TransferProgress {
                bytes_transferred: 110,
                bytes_total: 440,
            },
        )]
        .into_iter()
        .collect();

        assert_eq!(pinned_bytes(&sizes, &pins, &moved), 1_000 + 100);
    }
}
