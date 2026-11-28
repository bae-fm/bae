//! A release pin keeps nothing until every file is whole, then pins them all
//! and ends its download queue entry in one write.
use super::*;

/// What a pin has heard of its release's files, from coven's download watch
/// and from its reads of which files this device keeps, which together say
/// how many of the release's bytes the pin holds.
struct PinCount {
    /// Each file's coven file and plaintext size, by release file id, which
    /// its share of the download counts against.
    sizes: std::collections::BTreeMap<String, (coven::FileId, u64)>,
    /// The release file id of each coven file in `sizes`.
    release_files: std::collections::HashMap<coven::FileId, String>,
    /// How far each file being downloaded has got, as the watch last told it.
    moved: std::collections::BTreeMap<coven::FileId, coven::TransferProgress>,
    /// The release's files whose download for this pin finished, as the watch
    /// told it: each is whole and waits for the write that keeps the pin's
    /// files together once every download has finished.
    fetched: std::collections::BTreeSet<coven::FileId>,
    /// Whether this device keeps each file whole, by release file id, as the
    /// last read told it; `None` until the first.
    kept: Option<std::collections::BTreeMap<String, bool>>,
    /// The bytes each file counts now, by release file id, once `kept` is
    /// read.
    counted: std::collections::BTreeMap<String, u64>,
    /// The sum of `counted`, kept as files change so a transfer wake counts
    /// only the files it names.
    held: u64,
}

impl PinCount {
    /// A count of the files `sizes` names, by release file id with its coven
    /// file and plaintext size, that has heard nothing.
    fn new(sizes: std::collections::BTreeMap<String, (coven::FileId, u64)>) -> Self {
        Self {
            release_files: sizes.iter().map(|(id, (file, _))| (*file, id.clone())).collect(),
            sizes,
            moved: Default::default(),
            fetched: Default::default(),
            kept: None,
            counted: Default::default(),
            held: 0,
        }
    }

    /// Take what the watch told. Only a download that finished brought its
    /// file whole; one that failed, was held or was dropped brought none of
    /// it.
    fn heard(&mut self, changes: Vec<coven::TransferChange>) {
        let mut touched = std::collections::BTreeSet::new();
        let mut all = false;
        for change in &changes {
            match change {
                coven::TransferChange::Reset(_) => all = true,
                coven::TransferChange::Updated { file, .. } => {
                    touched.insert(*file);
                }
                coven::TransferChange::Removed { file, end } => {
                    touched.insert(*file);
                    if *end == coven::TransferEnd::Finished {
                        self.fetched.insert(*file);
                    }
                }
            }
        }
        crate::file_transfers::apply(&mut self.moved, changes);
        if all {
            self.recount_all();
        } else {
            for file in touched {
                if let Some(id) = self.release_files.get(&file) {
                    self.recount(&id.clone());
                }
            }
        }
    }

    /// Take a read of which of the release's files this device keeps.
    fn read(&mut self, kept: std::collections::BTreeMap<String, bool>) {
        self.kept = Some(kept);
        self.recount_all();
    }

    fn recount_all(&mut self) {
        let ids: Vec<String> = self.sizes.keys().cloned().collect();
        for id in ids {
            self.recount(&id);
        }
    }

    /// Count the release file `id` again: it counts its whole size if kept or
    /// fetched whole, else the share of its size its object has moved. The
    /// watch says when each download ends and how, so a file counts its share
    /// until then and its whole size after only if it finished: the bytes
    /// never go back as a file lands, and never count a file whose download
    /// failed. Counting each file's own share keeps the bytes moving as a
    /// file arrives, where the cache's coverage holds still until a file is
    /// kept. Nothing counts before the first read of which files are kept.
    fn recount(&mut self, id: &str) {
        let Some(kept) = &self.kept else {
            return;
        };
        let (file, size) = self.sizes[id];
        let now = if kept.get(id) == Some(&true) || self.fetched.contains(&file) {
            size
        } else {
            self.moved.get(&file).map_or(0, |progress| {
                (size as f64 * crate::file_transfers::fraction(*progress)) as u64
            })
        };
        let before = self.counted.insert(id.to_string(), now).unwrap_or(0);
        self.held =
            (self.held - before).checked_add(now).expect("a release's pinned bytes fit its total");
    }

    /// The bytes the pin holds; `None` before the first read of which files
    /// are kept.
    fn bytes(&self) -> Option<u64> {
        self.kept.as_ref().map(|_| self.held)
    }
}

impl LibraryManager {
    pub(crate) async fn pin_release_files_with_progress(
        &self,
        release: &str,
        plan: &crate::storage::transfer::PinPlan,
        entry: bae_transfers::ActiveEntry<bae_transfers::DownloadRequest>,
        mut on_progress: impl FnMut(bae_transfers::DownloadTransferProgress) + Send,
    ) -> Result<bae_transfers::Finished, LibraryError> {
        let total = plan.initial.bytes_total;
        let pinnable = self.release_pinnable_files(&plan.files).await?;
        let ids: Vec<String> = pinnable.iter().map(|entry| entry.row_id.clone()).collect();
        // Each file's plaintext size, by release file id, which its share of
        // the download counts against.
        let sizes: std::collections::BTreeMap<String, (coven::FileId, u64)> = ids
            .iter()
            .cloned()
            .zip(pinnable.iter().map(|entry| (entry.file.id(), entry.bytes)))
            .collect();
        let files = pinnable.into_iter().map(|entry| entry.file).collect::<Vec<_>>();
        let cancel = crate::CancellationToken::new();
        let cancel_on_drop = cancel.clone().drop_guard();
        let manager = self.clone();
        let release = release.to_string();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let task = self.run_file_operation("pin release", move |closing| async move {
            // Which of the release's files this device keeps, as each is
            // kept. Its runs can lag the download watch: a file whose
            // download finished counts whole from the watch's word, so the
            // lag never sets the bytes back.
            let kept = manager.records.subscribe_release_files_kept(&release);
            #[cfg(test)]
            let kept = {
                let hold = manager.pin_reads_hold.clone();
                kept.process(move |read| {
                    hold.wait();
                    Ok(read)
                })
            };
            let mut kept = manager.store.results(kept);
            let mut transfers = manager.store.subscribe_download_file_transfers();
            let mut count = PinCount::new(sizes);
            let prepare = manager.store.prepare_pin(&files);
            tokio::pin!(prepare);
            let mut rate: Option<bae_transfers::DownloadRate> = None;
            // Tell `on_progress` what the pin holds now, once a read has said.
            let mut report = |count: &PinCount| -> Result<(), LibraryError> {
                let Some(bytes) = count.bytes() else {
                    return Ok(());
                };
                let next = bae_transfers::DownloadTransferProgress::new(&release, bytes, total)?;
                let now = manager.clock.elapsed();
                let next = rate
                    .get_or_insert_with(|| bae_transfers::DownloadRate::begin(now, next.bytes_done))
                    .measure(next, now);
                progress_tx.send(next).map_err(|error| LibraryError::Storage(format!("pinning release was cancelled: {error}")))
            };
            // Nothing is pinned until every file is whole.
            let prepared = loop {
                tokio::select! {
                    _ = cancel.cancelled() => return Err(LibraryError::Storage("pinning release was cancelled".into())),
                    _ = closing.cancelled() => return Err(LibraryError::Storage("pinning release was cancelled because the library is closing".into())),
                    prepared = &mut prepare => {
                        break prepared.map_err(|error| LibraryError::file_read(format!("pin release {release}"), error))?;
                    }
                    next = kept.next() => {
                        count.read(next.expect("a query of kept files reports why it ended, which returns first")?);
                    }
                    changes = transfers.next() => {
                        count.heard(changes.ok_or_else(|| LibraryError::Storage(format!("pin release {release}: the store closed")))?);
                    }
                }
                report(&count)?;
            };
            // One write pins every file and ends the release's queue entry,
            // so no reader sees the release pinned with its entry still
            // active, and a cancel that took the entry out meanwhile fails
            // the write and pins nothing.
            let finished = manager
                .store
                .call(move |sql| {
                    sql.pin(prepared)?;
                    entry.finish_on(sql)
                })
                .await?;
            // The write keeps every file; read that now rather than wait for
            // the live query's next run.
            count.read(manager.records.release_files_kept(&release).await?);
            report(&count)?;
            Ok(finished)
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

// Each release whose storage row's pinned or file count differs from the one
// probed from its files, or that has no storage row, and each whose listing
// row is missing or holds a pinned or file count other than its storage
// row's.
#[cfg(test)]
coven::read!(
    writer PinDisagreements { disagreement: String },
    r#"
    SELECT release.release_id || ': stored '
               || COALESCE(stored.pinned_count || ' of ' || stored.file_count, 'nothing')
               || ', listed '
               || COALESCE(listed.pinned_count || ' of ' || listed.file_count, 'nothing')
               || ', probed ' || probe.pinned || ' of ' || probe.files AS disagreement
    FROM @bae_records::records::fragments::ReleaseAlbum AS release
    LEFT JOIN @bae_records::release_storage::ReleaseStorageCounts AS stored
        ON stored.release_id = release.release_id
    LEFT JOIN @bae_views::storage::ListedStorageCounts AS listed
        ON listed.release_id = release.release_id
    JOIN (SELECT each.release_id,
                 (SELECT COUNT(*)
                  FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                  WHERE file.release_id = each.release_id) AS files,
                 (SELECT COUNT(*)
                  FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                  WHERE file.release_id = each.release_id
                    AND EXISTS (SELECT 1 FROM coven_pins AS pin
                                WHERE pin.file = file.stored_file)) AS pinned
          FROM @bae_records::records::fragments::ReleaseAlbum AS each) AS probe
        ON probe.release_id = release.release_id
    WHERE stored.release_id IS NULL OR stored.pinned_count <> probe.pinned
       OR stored.file_count <> probe.files
       OR listed.release_id IS NULL OR listed.pinned_count <> stored.pinned_count
       OR listed.file_count <> stored.file_count
"#
);

// The releases with files whose every file `coven_pins` lists, probed file by
// file.
#[cfg(test)]
coven::read!(
    writer ProbedPinnedReleases,
    r#"
    SELECT release.release_id
    FROM @bae_records::records::fragments::ReleaseAlbum AS release
    WHERE EXISTS (SELECT 1 FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                  WHERE file.release_id = release.release_id)
      AND NOT EXISTS (SELECT 1
                      FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                      WHERE file.release_id = release.release_id
                        AND NOT EXISTS (SELECT 1 FROM coven_pins AS pin
                                        WHERE pin.file = file.stored_file))
"#
);

// The release `release`'s storage row: its file count, total size, whether it
// is stored, its pinned and queued counts, and whether its first queued place
// is the first place coven's queue holds for its files.
#[cfg(test)]
coven::read!(
    writer StorageRowText(release: String) { row: String },
    r#"
    SELECT stored.file_count || ' files, ' || stored.total_size || ' bytes, stored '
               || stored.stored || ', ' || stored.pinned_count || ' pinned, '
               || stored.queued_count || ' queued, '
               || CASE WHEN stored.first_queued_position IS (
                      SELECT MIN(queued.position)
                      FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                      JOIN coven_upload_queue AS queued ON queued.file = file.stored_file
                      WHERE file.release_id = stored.release_id)
                  THEN 'first place as queued' ELSE 'first place not as queued' END AS row
    FROM @bae_records::release_storage::ReleaseStorageCounts AS stored
    WHERE stored.release_id = :release
"#
);

#[cfg(test)]
#[path = "pin_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pin_progress_tests.rs"]
mod progress_tests;
