//! Storing what a pass over a watched root read, each write with the file
//! decisions stored for its folder and the tags its files seed it with, and
//! announcing what each write changed.

use super::*;

/// Scan items after their durable write, with the commit lock still held so
/// the events announcing them go out before anything else writes.
struct PersistedScanItems {
    commit: crate::FolderStateCommitGuard,
    /// Each item with what its write changed, in the order they were written;
    /// rows a write left as they were are not announced.
    writes: Vec<(FolderEntry, crate::tables::ScanItemWrite)>,
}

/// Store `items` in one write, in the order the walk yielded them, each
/// with its folder's date. `None`, with nothing stored, when
/// `generation` is no longer the root's.
async fn persist_scan_items(
    root: &Path,
    generation: u64,
    items: Vec<FolderEntry>,
    reads: &Arc<crate::folder_scanner::FileReads>,
    services: &crate::ImportServices,
) -> Result<Option<PersistedScanItems>, crate::ImportError> {
    let tables = &services.tables;
    let mut dated = Vec::with_capacity(items.len());
    for entry in items {
        // A folder's sidecar files are no release: no date, no tags.
        let folder_date = match &entry {
            FolderEntry::Release(item) => folder_date(item, &services.diagnostics).await?,
            FolderEntry::Sidecar(_) => None,
        };
        dated.push((entry, folder_date));
    }
    loop {
        // Each item as the file decisions stored right now describe it, and
        // what its own tags seed it with, both read before the commit lock:
        // reading a folder's tags off a network share takes as long as the
        // share does, and every pane control waits on this lock.
        let releases: Vec<&ScanItem> = dated
            .iter()
            .filter_map(|(entry, _)| match entry {
                FolderEntry::Release(item) => Some(item),
                FolderEntry::Sidecar(_) => None,
            })
            .collect();
        let (releases, edits_read): (Vec<ScanItem>, Vec<Option<(String, u64)>>) =
            with_stored_file_edits(&releases, tables).await?.into_iter().unzip();
        let seeds =
            scan_item_seeds(tables, &services.diagnostics, &releases, generation, reads.clone())
                .await?;
        let mut releases = releases.into_iter().zip(seeds).zip(edits_read);
        let mut prepared = Vec::with_capacity(dated.len());
        for (entry, folder_date) in &dated {
            match entry {
                FolderEntry::Release(_) => {
                    let ((item, seed), edits_read) =
                        releases.next().expect("each release was read for its edits and seed");
                    prepared.push((
                        crate::tables::FolderEntryToWrite::Release(
                            crate::tables::ScanItemToWrite {
                                item,
                                seed,
                                folder_date: *folder_date,
                            },
                        ),
                        edits_read,
                    ));
                }
                FolderEntry::Sidecar(sidecar) => prepared
                    .push((crate::tables::FolderEntryToWrite::Sidecar(sidecar.clone()), None)),
            }
        }
        let commit = services.folder_state_commit.lock("store scan items").await;
        // A file decision stored while the tags were being read describes
        // other files than the ones read: read them again, as decided now.
        let read_at: Vec<(String, u64)> =
            prepared.iter().filter_map(|(_, edits_read)| edits_read.clone()).collect();
        let hashes: Vec<String> = read_at.iter().map(|(hash, _)| hash.clone()).collect();
        let stored = tables.load_candidate_file_edit_revisions(&hashes).await?;
        let edited = read_at
            .iter()
            .any(|(hash, read_revision)| stored.get(hash).copied().unwrap_or(0) != *read_revision);
        if edited {
            drop(commit);
            continue;
        }
        let items = prepared.into_iter().map(|(to_write, _)| to_write).collect();
        let Some(writes) = tables.save_folder_scan_items(root, generation, items).await? else {
            return Ok(None);
        };
        return Ok(Some(PersistedScanItems { commit, writes }));
    }
}

/// The date of the folder `item` was read from.
pub(super) async fn folder_date(
    item: &ScanItem,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Option<crate::folder_scanner::FolderDate>, crate::ImportError> {
    let path = match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => candidate.path.clone(),
        ScanItem::Invalid(candidate) => candidate.path.clone(),
    };
    diagnostics
        .blocking("read a folder's date", move || crate::folder_scanner::FolderDate::read(&path))
        .await?
        .map_err(Into::into)
}

/// Each of `items` with the file decisions stored for its files applied, and
/// the content hash they are stored under with the revision they stood at —
/// `None` for an item with no files to decide — all read at once.
async fn with_stored_file_edits(
    items: &[&ScanItem],
    tables: &crate::tables::ImportTables,
) -> Result<Vec<(ScanItem, Option<(String, u64)>)>, crate::ImportError> {
    let hashes: Vec<String> = items
        .iter()
        .filter_map(|item| match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                Some(candidate.files.content_hash())
            }
            ScanItem::Invalid(_) => None,
        })
        .collect();
    let stored = tables.load_candidate_file_edits_of(&hashes).await?;
    items
        .iter()
        .map(|&item| {
            let mut item = item.clone();
            let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = &mut item else {
                return Ok((item, None));
            };
            let content_hash = candidate.files.content_hash();
            let edits = stored.get(&content_hash).cloned().unwrap_or_default();
            candidate.files.apply_candidate_file_edits(&edits)?;
            candidate.file_edit_revision = edits.revision;
            let revision = edits.revision;
            Ok((item, Some((content_hash, revision))))
        })
        .collect()
}

/// Announce what one stored entry changed: the entries it displaced, then
/// the release it is. A folder's sidecar files are no row of the list; what
/// their write displaced and rebuilt is announced on its own. An unchanged
/// row is announced to nobody.
pub(super) async fn announce_scan_write(
    entry: FolderEntry,
    write: &crate::tables::ScanItemWrite,
    automatic: bool,
    services: &crate::ImportServices,
) -> Result<(), crate::ImportError> {
    if !write.changed() {
        return Ok(());
    }
    let event_tx = &services.event_tx;
    for candidate_key in write.superseded_keys() {
        event_tx.send(crate::ImportEvent::Scan(ScanEvent::CandidateRemoved {
            candidate_key: candidate_key.clone(),
        }));
    }
    if let FolderEntry::Release(item) = entry {
        announce_item(item, write.found(), automatic, services).await?;
    }
    if let Some(regrouped) = write.regrouped() {
        announce_regrouped(regrouped, automatic, services).await?;
    }
    Ok(())
}

/// Tell the runtime and the list about the releases groupings rebuilt.
pub(super) async fn announce_regrouped(
    regrouped: &crate::tables::GroupingChanges,
    automatic: bool,
    services: &crate::ImportServices,
) -> Result<(), crate::ImportError> {
    for candidate_key in &regrouped.removed {
        services.event_tx.send(crate::ImportEvent::Scan(ScanEvent::CandidateRemoved {
            candidate_key: candidate_key.clone(),
        }));
    }
    for item in &regrouped.written {
        let found = regrouped.found.contains(&item.key());
        announce_item(item.clone(), found, automatic, services).await?;
    }
    Ok(())
}

/// Announce one stored entry as what it now is, and whether its write
/// `found` the release while identification runs on its own (`automatic`).
async fn announce_item(
    item: ScanItem,
    found: bool,
    automatic: bool,
    services: &crate::ImportServices,
) -> Result<(), crate::ImportError> {
    let event_tx = &services.event_tx;
    // The write stores the item as it stands and never changes its
    // variant, so the variant still says which announcement it earns.
    let actionable = matches!(item, ScanItem::Valid(_));
    let automatic = found && actionable && automatic;
    match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            event_tx.send(crate::ImportEvent::Scan(if actionable {
                ScanEvent::FolderCandidate { candidate, found_while_automatic: automatic }
            } else {
                ScanEvent::CandidateDiscovered { candidate }
            }));
        }
        ScanItem::Invalid(candidate) => {
            event_tx.send(crate::ImportEvent::Scan(ScanEvent::InvalidCandidate(candidate)));
        }
    }
    Ok(())
}

/// Whether a release found now is identified on its own.
pub(super) async fn identifies_automatically(
    services: &crate::ImportServices,
) -> Result<bool, crate::ImportError> {
    Ok(bae_preferences::preferences(&services.store).await?.identification.automatic)
}

/// Store `items` and announce what each write changed, adding what it
/// wrote and displaced to the pass's tallies. `false` when the root has
/// moved past `generation`, and nothing was stored.
pub(super) async fn store_scan_items(
    root: &Path,
    generation: u64,
    items: Vec<FolderEntry>,
    reads: &Arc<crate::folder_scanner::FileReads>,
    services: &crate::ImportServices,
    written_keys: &mut Vec<crate::RelativeFolder>,
    displaced_keys: &mut Vec<crate::CandidateKey>,
) -> Result<bool, crate::ImportError> {
    if items.is_empty() {
        return Ok(true);
    }
    let Some(PersistedScanItems { commit: _commit, writes }) =
        persist_scan_items(root, generation, items, reads, services).await?
    else {
        return Ok(false);
    };
    // Read once for the batch, and only when a write has something to announce.
    let automatic = match writes.iter().any(|(_, write)| write.changed()) {
        true => identifies_automatically(services).await?,
        false => false,
    };
    for (entry, write) in writes {
        if !write.changed() {
            continue;
        }
        displaced_keys.extend(write.superseded_keys().iter().cloned());
        if let FolderEntry::Release(item) = &entry {
            written_keys.push(item.relative_folder().clone());
        }
        announce_scan_write(entry, &write, automatic, services).await?;
    }
    Ok(true)
}

/// Each of `items` about to be stored as its own file tags describe it: the
/// reading of its tracks' tags its scan row keeps, and the draft and cover
/// they seed a candidate that has none with. A reading is taken from what
/// `reads` — the pass that found the item — and the reading stored for it
/// hold, and read where neither knows. A track whose tags do not parse states
/// nothing, and the others seed the draft. A folder one of whose files can't
/// be read at all keeps no reading and, when it has no draft, starts on a
/// blank one; it is logged rather than kept out of the queue. Reading audio
/// files can be slow on a network share, so each runs on a blocking thread,
/// and its caller takes them outside the folder-state commit lock every pane
/// control waits on. What the store holds for all of them is read once.
///
/// A tentative folder whose files a settled entry already reads takes no seed
/// and opens no file: the store keeps that entry and writes nothing for the
/// folder.
async fn scan_item_seeds(
    tables: &crate::tables::ImportTables,
    diagnostics: &bae_diagnostics::Diagnostics,
    items: &[crate::folder_scanner::ScanItem],
    generation: u64,
    reads: std::sync::Arc<crate::folder_scanner::FileReads>,
) -> Result<Vec<crate::tables::ScanItemSeed>, crate::ImportError> {
    use crate::folder_scanner::ScanItem;
    use crate::tables::ScanItemSeed;
    let readings = tables.load_scan_batch_readings(items).await?;
    let mut seeds = Vec::with_capacity(items.len());
    for (item, readings) in items.iter().zip(readings) {
        let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = item else {
            seeds.push(ScanItemSeed::Tags(None));
            continue;
        };
        if readings.settled {
            seeds.push(ScanItemSeed::AlreadySettled);
            continue;
        }
        let folder = candidate.path.display().to_string();
        let candidate = candidate.clone();
        let reads = reads.clone();
        let read = diagnostics
            .blocking("read a folder's file tags", move || {
                crate::file_metadata_seed::FileMetadataSeed::read(
                    &candidate,
                    generation,
                    &reads,
                    readings.stored.as_ref(),
                )
            })
            .await?;
        seeds.push(match read {
            Ok(seed) => ScanItemSeed::Tags(Some(seed)),
            Err(error) => {
                tracing::warn!(
                    "{folder} keeps no reading of its file tags, and starts on a blank draft \
                     if it has none: a file could not be read: {error}"
                );
                ScanItemSeed::Tags(None)
            }
        });
    }
    Ok(seeds)
}

/// [`scan_item_seeds`] of one item.
pub(crate) async fn scan_item_seed(
    tables: &crate::tables::ImportTables,
    diagnostics: &bae_diagnostics::Diagnostics,
    item: &crate::folder_scanner::ScanItem,
    generation: u64,
    reads: std::sync::Arc<crate::folder_scanner::FileReads>,
) -> Result<crate::tables::ScanItemSeed, crate::ImportError> {
    let mut seeds =
        scan_item_seeds(tables, diagnostics, std::slice::from_ref(item), generation, reads).await?;
    Ok(seeds.pop().expect("one item has one seed"))
}

/// Store `item` under `generation` of `watched_folder_path` as a scan stores
/// it, its draft seeded from its files' tags: the write of a scan a test
/// makes without walking a folder. `None`, with nothing written, when
/// `generation` is no longer the folder's.
#[cfg(any(test, feature = "test-utils"))]
pub async fn store_scan_item(
    tables: &crate::tables::ImportTables,
    diagnostics: &bae_diagnostics::Diagnostics,
    watched_folder_path: &std::path::Path,
    generation: u64,
    item: &crate::folder_scanner::ScanItem,
) -> Result<Option<crate::tables::ScanItemWrite>, crate::ImportError> {
    let seed = scan_item_seed(
        tables,
        diagnostics,
        item,
        generation,
        std::sync::Arc::new(crate::folder_scanner::FileReads::new(std::sync::Arc::new(
            crate::file_tag_reader::LoftyFileTagReader,
        ))),
    )
    .await?;
    let writes = tables
        .save_folder_scan_items(
            watched_folder_path,
            generation,
            vec![crate::tables::FolderEntryToWrite::Release(crate::tables::ScanItemToWrite {
                item: item.clone(),
                seed,
                folder_date: None,
            })],
        )
        .await?;
    Ok(writes.map(|mut writes| writes.pop().expect("one item stores one write").1))
}

#[cfg(test)]
#[path = "scan_items_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scan_items_reads_tests.rs"]
mod reads_tests;
