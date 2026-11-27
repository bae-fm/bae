//! What the store knows that a pass over a watched root takes with it: what
//! was decided about the root's folders and files and what the scans stored of
//! each file they read, so the pass reads no file again that is unchanged (see
//! [`FileReads`]); and, for each batch of items the pass is about to write,
//! what seeds each one.

use super::read::{CandidateRow, CoveredFolders};
use super::sidecar::sidecar_of;
use super::tag_snapshot::{track_readings, StoredSnapshot};
use super::*;
use crate::file_tag_reader::FileTagReader;
use crate::file_tag_snapshot::{FileObservation, StoredTagReading};
use crate::folder_scanner::{
    FileReads, FolderEntry, FolderReleaseDecisions, ScanItem, StoredCandidateEdits,
};
use bae_store::statement_rows;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

sqlive_compose::read!(
    KnownOfRootRows(root: Vec<u8>, below_first: Vec<u8>, below_past: Vec<u8>) {
        root_generation: Option<i64>
    },
    r#"
    SELECT item.*,
        @crate::tables::folder_scans::fragments::SnapshotFileTags(parent: reading)
            AS file_tags,
        sidecar.*, decided.*, anchored.*,
        (SELECT scanned.generation FROM folder_scan_roots AS scanned
         WHERE scanned.watched_folder_path = :root) AS root_generation
    FROM @crate::tables::folder_scans::row_fragments::RootKeys(
        root: :root,
        decided: @crate::tables::import_state::file_decision_rows::DecidedFileHashes,
        anchored: @crate::tables::release_groupings::row_fragments::AnchoredGroupingKeys(
            root: :root, below_first: :below_first, below_past: :below_past)) AS known
    LEFT JOIN @crate::tables::folder_scans::fragments::ScanItems(keys: (
        SELECT entry.path AS candidate_key
        FROM scan_candidate AS entry
        WHERE entry.watched_folder_path = :root)) AS item
        ON item.path = known.item_key AND item.source_kind = 'folder'
    LEFT JOIN scan_candidate_tag_snapshot AS reading
        ON reading.watched_folder_path = :root AND reading.candidate_path = known.item_key
    LEFT JOIN @crate::tables::folder_scans::fragments::Sidecars AS sidecar
        ON sidecar.sidecar_folder = known.sidecar_key
    LEFT JOIN @crate::tables::import_state::file_decision_rows::FileEdits(
        keys: @crate::tables::import_state::file_decision_rows::DecidedFileHashes) AS decided
        ON decided.content_hash = known.decided_hash
    LEFT JOIN @crate::tables::release_groupings::fragments::Groupings(
        keys: @crate::tables::release_groupings::row_fragments::AnchoredGroupingKeys(
            root: :root, below_first: :below_first, below_past: :below_past)) AS anchored
        ON anchored.stored_key = known.anchored_key
    ORDER BY known.item_key, known.sidecar_key
"#
);

impl ImportTables {
    /// What the store knows of `watched_folder_path` as a pass over it begins,
    /// in one statement: the file decisions of every candidate one was made
    /// for, which the pass applies to whatever files it finds; each stored
    /// folder reading below the root, by path below it; and what the root's
    /// scans stored about each file they read — its entries, its folders'
    /// sidecars and the tags read from its releases' tracks — for a pass that
    /// reads any other track's tags through `tags` and opens each file in a
    /// place of `files`. A store that says two things about one file fails
    /// the read.
    pub(crate) async fn load_known_of_root(
        &self,
        watched_folder_path: &Path,
        tags: Arc<dyn FileTagReader>,
        files: coven::FileBudget,
    ) -> Result<(StoredCandidateEdits, FolderReleaseDecisions, FileReads), crate::ImportError> {
        let root = watched_folder_path.to_path_buf();
        let (below_first, below_past) = crate::tables::watched_path::paths_below(&root);
        let request =
            KnownOfRootRowsParams { root: coven::path_bytes(&root), below_first, below_past };
        self.store
            .read(move |sql| Ok(statement_rows(&sql, &request)?))
            .process(move |rows| known_of_root(&root, rows, tags, files))
            .await
    }

    /// What the store holds that seeds each of `items`, in their order. An
    /// item stored at a key in `moved` is read for its seed whatever its
    /// entry says: that entry changed after a read found it unchanged.
    ///
    /// One statement reads them all. For an item whose key holds an entry
    /// that may be the item, it reads that entry and nothing of its tags, so
    /// a pass over unchanged releases loads no picture; where the entry turns
    /// out to be another, the statement runs once more, in the same
    /// snapshot, for what seeds those items.
    pub(crate) async fn load_scan_batch_readings(
        &self,
        items: &[ScanItem],
        moved: &HashSet<CandidateKey>,
    ) -> Result<Vec<ScanItemReadings>, DbError> {
        let wanted: Vec<Option<CandidateWanted>> =
            items.iter().map(|item| CandidateWanted::of(item, moved)).collect();
        self.store
            .read(move |sql| {
                let asked: Vec<Asked<'_>> = wanted
                    .iter()
                    .enumerate()
                    .filter_map(|(at, wanted)| {
                        let candidate = wanted.as_ref()?;
                        Some(Asked { at, candidate, may_be_unchanged: candidate.may_be_unchanged })
                    })
                    .collect();
                let first = read_batch(&sql, &asked)?;
                let again: Vec<Asked<'_>> = asked
                    .iter()
                    .filter(|asked| matches!(first.get(&asked.at), Some(None)))
                    .map(|asked| Asked { may_be_unchanged: false, ..*asked })
                    .collect();
                let mut read = first;
                read.extend(read_batch(&sql, &again)?);
                Ok((0..wanted.len())
                    .map(|at| match read.remove(&at) {
                        // An item with no files to read tags from.
                        None => ScanItemReadings::Stored(None),
                        Some(reading) => reading
                            .expect("an item read for its seed has one after the second read"),
                    })
                    .collect())
            })
            .await
    }
}

/// What `rows`, read of the watched root `root`, say a pass knows of it.
fn known_of_root(
    root: &Path,
    rows: Vec<KnownOfRootRowsRow>,
    tags: Arc<dyn FileTagReader>,
    files: coven::FileBudget,
) -> Result<(StoredCandidateEdits, FolderReleaseDecisions, FileReads), crate::ImportError> {
    let mut items = Vec::new();
    let mut sidecars = Vec::new();
    let mut read_tags = Vec::new();
    let mut edits = Vec::new();
    let mut groupings = Vec::new();
    for row in rows {
        if let Some(item) = row.item {
            let item = CandidateRow::of(item)?.item()?;
            read::check_generations(std::slice::from_ref(&item), row.root_generation)?;
            read_tags.extend(track_readings(&item.key, &row.file_tags.tag_files)?);
            items.push(item);
        }
        if let Some(sidecar) = row.sidecar {
            sidecars.push(sidecar_of(sidecar)?);
        }
        edits.extend(row.decided);
        groupings.extend(row.anchored);
    }
    let entries: Vec<FolderEntry> = items
        .into_iter()
        .map(|stored| FolderEntry::Release(stored.item))
        .chain(sidecars.into_iter().map(FolderEntry::Sidecar))
        .collect();
    Ok((
        StoredCandidateEdits::new(crate::tables::import_state::file_decision_rows::edits_by_hash(
            edits,
        )?),
        crate::tables::release_groupings::folder_release_decisions(root, groupings)?,
        FileReads::stored(&entries, &read_tags, tags, files)?,
    ))
}

/// What the store holds that seeds one scan item.
pub(crate) enum ScanItemReadings {
    /// A tentative item whose files a settled entry already reads.
    Settled,
    /// The entry stored at the item's key is exactly the item: its write
    /// keeps that entry and takes no seed.
    Unchanged,
    /// The tag reading stored for the item's candidate, when it reads the
    /// same files: none for an item with no tracks or no such reading. Its
    /// picture is loaded only when the file it came from is unchanged.
    Stored(Option<StoredTagReading>),
}

/// One candidate [`ImportTables::load_scan_batch_readings`] asks after.
struct CandidateWanted {
    item: ScanItem,
    watched_folder_path: PathBuf,
    key: CandidateKey,
    file_root: PathBuf,
    /// The candidate's audio files as the scan read them.
    audio_files: Vec<FileObservation>,
    coverage: crate::folder_scanner::Coverage,
    /// Whether the item is tentative, so a settled entry may already read it.
    tentative: bool,
    /// Whether the entry stored at its key may be the item: false once that
    /// entry proved another.
    may_be_unchanged: bool,
}

impl CandidateWanted {
    /// None for an item with no files to read tags from.
    fn of(item: &ScanItem, moved: &HashSet<CandidateKey>) -> Option<Self> {
        let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = item else {
            return None;
        };
        let key = candidate.key();
        Some(Self {
            item: item.clone(),
            may_be_unchanged: !moved.contains(&key),
            watched_folder_path: candidate.watched_folder_path.clone(),
            key,
            file_root: candidate.file_root.clone(),
            audio_files: candidate.files.audio().map(FileObservation::scanned).collect(),
            coverage: item.coverage(),
            tentative: matches!(item, ScanItem::Discovered(_)),
        })
    }
}

/// One candidate a run of [`ScanBatchReadings`] asks after.
#[derive(Clone, Copy)]
struct Asked<'a> {
    /// The item's place in the batch.
    at: usize,
    candidate: &'a CandidateWanted,
    /// Whether this run reads the entry at its key, to tell whether it is the
    /// item, rather than what seeds it.
    may_be_unchanged: bool,
}

/// One candidate as an element of [`ScanBatchReadings`]' `wanted` array, its
/// paths and key in hex.
#[derive(serde::Serialize)]
struct WantedElement<'a> {
    /// The item's place in the batch.
    at: usize,
    root: String,
    key: String,
    file_root: String,
    folder: String,
    above: String,
    below_first: Option<String>,
    below_past: Option<String>,
    tentative: bool,
    may_be_unchanged: bool,
    audio: &'a [FileObservation],
}

impl<'a> WantedElement<'a> {
    fn of(asked: &Asked<'a>) -> Self {
        let wanted = asked.candidate;
        let covered = CoveredFolders::of(&wanted.watched_folder_path, &wanted.coverage);
        let (below_first, below_past) = covered.below.unzip();
        Self {
            at: asked.at,
            root: hex::encode(covered.root),
            key: hex::encode(wanted.key.as_bytes()),
            file_root: hex::encode(coven::path_bytes(&wanted.file_root)),
            folder: hex::encode(covered.folder),
            above: covered.above,
            below_first: below_first.map(hex::encode),
            below_past: below_past.map(hex::encode),
            tentative: wanted.tentative,
            may_be_unchanged: asked.may_be_unchanged,
            audio: &wanted.audio_files,
        }
    }
}

sqlive_compose::read!(
    ScanBatchReadings(wanted: String) { at: i64, settled: bool },
    r#"
    SELECT asked.value ->> 'at' AS at,
        (asked.value ->> 'at') IN (
            SELECT reader.reader_at
            FROM @crate::tables::folder_scans::fragments::FolderReaders(covered: (
                SELECT covered.value ->> 'at' AS at, unhex(covered.value ->> 'root') AS root,
                    unhex(covered.value ->> 'folder') AS folder,
                    covered.value ->> 'above' AS above,
                    unhex(covered.value ->> 'below_first') AS below_first,
                    unhex(covered.value ->> 'below_past') AS below_past, NULL AS besides
                FROM json_each(:wanted) AS covered
                WHERE covered.value ->> 'tentative')) AS reader
        ) AS settled,
        stored.*, tags.*
    FROM json_each(:wanted) AS asked
    LEFT JOIN @crate::tables::folder_scans::fragments::ScanItems(keys: (
        SELECT DISTINCT unhex(unchanged.value ->> 'key') AS candidate_key
        FROM json_each(:wanted) AS unchanged
        WHERE unchanged.value ->> 'may_be_unchanged')) AS stored
        ON stored.path = unhex(asked.value ->> 'key')
        AND (asked.value ->> 'may_be_unchanged') AND stored.source_kind = 'folder'
    LEFT JOIN @crate::tables::folder_scans::fragments::TagSnapshot(wanted: (
        SELECT seeded.value ->> 'at' AS at, entry.path AS candidate_key,
            seeded.value ->> 'audio' AS found
        FROM json_each(:wanted) AS seeded
        JOIN scan_candidate AS entry
            ON entry.watched_folder_path = unhex(seeded.value ->> 'root')
            AND entry.path = unhex(seeded.value ->> 'key')
            AND entry.file_root = unhex(seeded.value ->> 'file_root')
        WHERE NOT seeded.value ->> 'may_be_unchanged')) AS tags
        ON tags.snapshot_at = asked.value ->> 'at'
"#
);

/// Read the candidates of `asked` in one statement, by their places in the
/// batch: `None` for a candidate whose entry may have been the item and is
/// another.
fn read_batch(
    sql: &SqlReadContext<'_>,
    asked: &[Asked<'_>],
) -> Result<HashMap<usize, Option<ScanItemReadings>>, DbError> {
    if asked.is_empty() {
        return Ok(HashMap::new());
    }
    let elements: Vec<WantedElement<'_>> = asked.iter().map(WantedElement::of).collect();
    let request = ScanBatchReadingsParams {
        wanted: serde_json::to_string(&elements)
            .map_err(|error| DbError::encode("the candidates to read seeds of", error))?,
    };
    let by_place: HashMap<usize, &Asked<'_>> =
        asked.iter().map(|asked| (asked.at, asked)).collect();
    statement_rows(sql, &request)?
        .into_iter()
        .map(|row| {
            let ScanBatchReadingsRow { at, settled, stored, tags } = row;
            let asked = usize::try_from(at)
                .ok()
                .and_then(|at| by_place.get(&at))
                .expect("each row is of a candidate the statement asked after");
            let candidate = asked.candidate;
            let reading = if candidate.tentative && settled {
                Some(ScanItemReadings::Settled)
            } else if asked.may_be_unchanged {
                match stored {
                    None => Some(ScanItemReadings::Stored(None)),
                    Some(stored) => (CandidateRow::of(stored)?.item()?.item == candidate.item)
                        .then_some(ScanItemReadings::Unchanged),
                }
            } else {
                let reading = tags
                    .map(|tags| StoredSnapshot::of(&candidate.key, tags))
                    .transpose()?
                    .map(StoredSnapshot::reading);
                Some(ScanItemReadings::Stored(reading))
            };
            Ok((asked.at, reading))
        })
        .collect()
}

#[cfg(test)]
#[path = "file_reads_tests.rs"]
mod tests;
