//! What a folder's own file tags make of it.
//!
//! Discovery and Reset initialize a candidate from this projection, and
//! applying file metadata uses it to replace metadata
//! while retaining the current rows' identities. The projected draft, its
//! source reading, and embedded cover are produced together.

use crate::import::file_tag_snapshot::FileTagSnapshot;
use crate::import::folder_scanner::FolderCandidate;
use crate::import::{CandidateDraft, CandidateTrack, CoverSelection, ImportError};

/// A folder read as its own files describe it.
pub(crate) struct FileMetadataSeed {
    /// The exact reading the draft was projected from.
    pub snapshot: FileTagSnapshot,
    pub draft: CandidateDraft,
    /// The cover applying these tags selects, where the tags embed artwork:
    /// that artwork, or a folder image named as the front cover, which ranks
    /// ahead of it (`local_artwork::file_tags_cover`).
    pub cover: Option<CoverSelection>,
}

impl FileMetadataSeed {
    /// Project what `candidate`'s audio files' tags say into the draft a
    /// candidate starts from: what `reads` holds of them and `stored` — the
    /// reading stored for the candidate, taken under the same folder — says,
    /// and a read of whatever neither knows.
    pub(crate) fn read(
        candidate: &crate::import::folder_scanner::FolderCandidate,
        scan_generation: u64,
        reads: &crate::import::folder_scanner::FileReads,
        stored: Option<&FileTagSnapshot>,
    ) -> Result<Self, ImportError> {
        let audio_files = candidate.files.audio().cloned().collect::<Vec<_>>();
        let snapshot = reads.tag_snapshot(
            &audio_files,
            scan_generation,
            candidate.file_edit_revision,
            stored,
        )?;
        Self::project(candidate, snapshot, None)
    }

    /// Project `snapshot` into the draft a candidate stores for it.
    ///
    /// `keeping` are the draft's current track rows, whose identities the
    /// tracks read afresh take. A candidate with no draft yet passes none.
    ///
    /// Applying tags replaces previously entered metadata values.
    pub(crate) fn project(
        candidate: &FolderCandidate,
        snapshot: FileTagSnapshot,
        keeping: Option<&[CandidateTrack]>,
    ) -> Result<Self, ImportError> {
        let mut draft = crate::import::pane::file_metadata_draft(candidate, &snapshot)?;
        if let Some(keeping) = keeping {
            crate::import::pane::keep_row_identities(&mut draft.tracks, keeping)?;
        }
        let cover = crate::import::local_artwork::file_tags_cover(
            crate::import::file_tag_snapshot::embedded_cover_selection(&snapshot),
            candidate.files.artwork(),
        );
        Ok(Self { snapshot, draft, cover })
    }
}

/// The folder `item` about to be stored as its own file tags describe it: the
/// reading of its tracks' tags its scan row keeps, and the draft and cover
/// they seed a candidate that has none with. It is taken from what `reads` —
/// the pass that found it — and the reading stored for it hold, and read where
/// neither knows. A folder whose tags cannot be read keeps no reading and,
/// when it has no draft, starts on a blank one; it is logged rather than kept
/// out of the queue. Reading audio files can be slow on a network share, so it
/// runs on a blocking thread, and its caller takes it outside the folder-state
/// commit lock every pane control waits on.
///
/// A tentative folder whose files a settled entry already reads takes no seed
/// and opens no file: the store keeps that entry and writes nothing for the
/// folder.
pub(crate) async fn scan_item_seed(
    tables: &crate::import::tables::ImportTables,
    diagnostics: &bae_diagnostics::Diagnostics,
    item: &crate::import::folder_scanner::ScanItem,
    generation: u64,
    reads: std::sync::Arc<crate::import::folder_scanner::FileReads>,
) -> Result<crate::import::tables::ScanItemSeed, ImportError> {
    use crate::import::folder_scanner::ScanItem;
    use crate::import::tables::ScanItemSeed;
    let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = item else {
        return Ok(ScanItemSeed::Tags(None));
    };
    if let (ScanItem::Discovered(_), Some(coverage)) = (item, item.coverage()) {
        if tables.settled_entry_reads(&candidate.watched_folder_path, &coverage).await? {
            return Ok(ScanItemSeed::AlreadySettled);
        }
    }
    let folder = candidate.path.display().to_string();
    let stored = tables
        .load_stored_file_tag_reading(
            &candidate.watched_folder_path,
            &candidate.key(),
            &candidate.file_root,
        )
        .await?;
    let candidate = candidate.clone();
    let read = bae_util::task_failure::blocking("read a folder's file tags", move || {
        FileMetadataSeed::read(&candidate, generation, &reads, stored.as_ref())
    })
    .await
    .map_err(|failed| diagnostics.bug(failed))?;
    match read {
        Ok(seed) => Ok(ScanItemSeed::Tags(Some(seed))),
        Err(error) => {
            tracing::warn!(
                "{folder} keeps no reading of its file tags, and starts on a blank draft if it \
                 has none: they could not be read: {error}"
            );
            Ok(ScanItemSeed::Tags(None))
        }
    }
}

/// Store `item` under `generation` of `watched_folder_path` as a scan stores
/// it, its draft seeded from its files' tags: the write of a scan a test
/// makes without walking a folder. `None`, with nothing written, when
/// `generation` is no longer the folder's.
#[cfg(any(test, feature = "test-utils"))]
pub async fn store_scan_item(
    tables: &crate::import::tables::ImportTables,
    diagnostics: &bae_diagnostics::Diagnostics,
    watched_folder_path: &std::path::Path,
    generation: u64,
    item: &crate::import::folder_scanner::ScanItem,
) -> Result<Option<crate::import::tables::ScanItemWrite>, ImportError> {
    let seed = scan_item_seed(
        tables,
        diagnostics,
        item,
        generation,
        std::sync::Arc::new(crate::import::folder_scanner::FileReads::new(std::sync::Arc::new(
            crate::import::file_tag_snapshot::LoftyFileTagReader,
        ))),
    )
    .await?;
    let writes = tables
        .save_folder_scan_items(
            watched_folder_path,
            generation,
            vec![crate::import::tables::ScanItemToWrite {
                item: item.clone(),
                seed,
                folder_date: None,
            }],
        )
        .await?;
    Ok(writes.map(|mut writes| writes.pop().expect("one item stores one write").1))
}
