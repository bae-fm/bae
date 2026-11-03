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
    writes: Vec<(ScanItem, crate::tables::ScanItemWrite)>,
}

impl ImportService {
    /// Store `items` in one write, in the order the walk yielded them, each
    /// with its folder's date. `None`, with nothing stored, when
    /// `generation` is no longer the root's.
    async fn persist_scan_items(
        root: &Path,
        generation: u64,
        items: Vec<ScanItem>,
        reads: &Arc<crate::folder_scanner::FileReads>,
        services: &crate::ImportServices,
    ) -> Result<Option<PersistedScanItems>, crate::ImportError> {
        let tables = &services.tables;
        let mut dated = Vec::with_capacity(items.len());
        for item in items {
            let folder_date = Self::folder_date(&item, &services.diagnostics).await?;
            dated.push((item, folder_date));
        }
        loop {
            // Each item as the file decisions stored right now describe it, and
            // what its own tags seed it with, both read before the commit lock:
            // reading a folder's tags off a network share takes as long as the
            // share does, and every pane control waits on this lock.
            let mut prepared = Vec::with_capacity(dated.len());
            for (item, folder_date) in &dated {
                let (item, edits_read) = Self::with_stored_file_edits(item, tables).await?;
                let seed = crate::file_metadata_seed::scan_item_seed(
                    tables,
                    &services.diagnostics,
                    &item,
                    generation,
                    reads.clone(),
                )
                .await?;
                prepared.push((
                    crate::tables::ScanItemToWrite { item, seed, folder_date: *folder_date },
                    edits_read,
                ));
            }
            let commit = services.folder_state_commit.lock("store scan items").await;
            // A file decision stored while the tags were being read describes
            // other files than the ones read: read them again, as decided now.
            let mut edited = false;
            for (_, edits_read) in &prepared {
                let Some((content_hash, read_revision)) = edits_read else {
                    continue;
                };
                let stored = tables.load_candidate_file_edits(content_hash).await?;
                if stored.revision != *read_revision {
                    edited = true;
                    break;
                }
            }
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

    /// The date of the folder `item` was read from; `None` for a folder's
    /// sidecar files, which are no release.
    async fn folder_date(
        item: &ScanItem,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) -> Result<Option<crate::folder_scanner::FolderDate>, crate::ImportError> {
        let path = match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => candidate.path.clone(),
            ScanItem::Invalid(candidate) => candidate.path.clone(),
            ScanItem::Sidecar(_) => return Ok(None),
            ScanItem::Decided { .. } => {
                return Err(crate::ImportError::Internal {
                    detail: "a folder reading is stored as a decision, not as a scan entry".into(),
                })
            }
        };
        bae_util::task_failure::blocking("read a folder's date", move || {
            crate::folder_scanner::FolderDate::read(&path)
        })
        .await
        .map_err(|failed| diagnostics.bug(failed))?
        .map_err(Into::into)
    }

    /// `item` with the file decisions stored for its files applied, and the
    /// content hash they are stored under with the revision they stood at —
    /// `None` for an item with no files to decide.
    async fn with_stored_file_edits(
        item: &ScanItem,
        tables: &crate::tables::ImportTables,
    ) -> Result<(ScanItem, Option<(String, u64)>), crate::ImportError> {
        let mut item = item.clone();
        let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = &mut item else {
            return Ok((item, None));
        };
        let content_hash = candidate.files.content_hash();
        let edits = tables.load_candidate_file_edits(&content_hash).await?;
        candidate.files.apply_candidate_file_edits(&edits)?;
        candidate.file_edit_revision = edits.revision;
        let revision = edits.revision;
        Ok((item, Some((content_hash, revision))))
    }

    /// Announce what one stored scan item changed: the entries it displaced,
    /// then the item itself. An unchanged row is announced to nobody.
    pub(super) async fn announce_scan_write(
        item: ScanItem,
        write: &crate::tables::ScanItemWrite,
        services: &crate::ImportServices,
    ) -> Result<(), crate::ImportError> {
        if !write.changed() {
            return Ok(());
        }
        let event_tx = &services.event_tx;
        for candidate_key in write.superseded_keys() {
            event_tx.send(crate::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                candidate_key: candidate_key.clone(),
            }));
        }
        Self::announce_item(item, write.found(), services).await?;
        if let Some(regrouped) = write.regrouped() {
            Self::announce_regrouped(regrouped, services).await?;
        }
        Ok(())
    }

    /// Tell the runtime and the list about the releases groupings rebuilt.
    pub(super) async fn announce_regrouped(
        regrouped: &crate::tables::GroupingChanges,
        services: &crate::ImportServices,
    ) -> Result<(), crate::ImportError> {
        for candidate_key in &regrouped.removed {
            services.event_tx.send(crate::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                candidate_key: candidate_key.clone(),
            }));
        }
        for item in &regrouped.written {
            let found = item.persisted_key().is_some_and(|key| regrouped.found.contains(&key));
            Self::announce_item(item.clone(), found, services).await?;
        }
        Ok(())
    }

    /// Announce one stored entry as what it now is, and whether its write
    /// `found` the release while identification runs on its own.
    async fn announce_item(
        item: ScanItem,
        found: bool,
        services: &crate::ImportServices,
    ) -> Result<(), crate::ImportError> {
        let event_tx = &services.event_tx;
        // The write stores the item as it stands and never changes its
        // variant, so the variant still says which announcement it earns.
        let actionable = matches!(item, ScanItem::Valid(_));
        let automatic = found
            && actionable
            && bae_preferences::preferences(&services.database).await?.identification.automatic;
        match item {
            ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                event_tx.send(crate::handle::ImportEvent::Scan(if actionable {
                    ScanEvent::FolderCandidate { candidate, found_while_automatic: automatic }
                } else {
                    ScanEvent::CandidateDiscovered { candidate }
                }));
            }
            ScanItem::Invalid(candidate) => {
                event_tx
                    .send(crate::handle::ImportEvent::Scan(ScanEvent::InvalidCandidate(candidate)));
            }
            // A folder's sidecar files are no row of the list; what their
            // write displaced and rebuilt is announced on its own.
            ScanItem::Sidecar(_) => {}
            ScanItem::Decided { .. } => {
                return Err(crate::ImportError::Internal {
                    detail: "a folder reading is stored as a decision, not as a scan entry".into(),
                })
            }
        }
        Ok(())
    }

    /// Store `items` and announce what each write changed, adding what it
    /// wrote and displaced to the pass's tallies. `false` when the root has
    /// moved past `generation`, and nothing was stored.
    pub(super) async fn store_scan_items(
        root: &Path,
        generation: u64,
        items: Vec<ScanItem>,
        reads: &Arc<crate::folder_scanner::FileReads>,
        services: &crate::ImportServices,
        written_keys: &mut Vec<bae_db::RelativeFolder>,
        displaced_keys: &mut Vec<crate::CandidateKey>,
    ) -> Result<bool, crate::ImportError> {
        if items.is_empty() {
            return Ok(true);
        }
        let Some(PersistedScanItems { commit: _commit, writes }) =
            Self::persist_scan_items(root, generation, items, reads, services).await?
        else {
            return Ok(false);
        };
        for (item, write) in writes {
            if !write.changed() {
                continue;
            }
            displaced_keys.extend(write.superseded_keys().iter().cloned());
            if let Some(relative_folder) = item.relative_folder() {
                written_keys.push(relative_folder.clone());
            }
            Self::announce_scan_write(item, &write, services).await?;
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "scan_items_tests.rs"]
mod tests;
