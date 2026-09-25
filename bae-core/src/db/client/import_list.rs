//! The import tab's list, read as columns.
//!
//! The whole queue is read on every rerun — a few short columns per scanned
//! folder, per boundary and per stored verdict, plus each verdict's match rows,
//! which is what says how many pressings it named — and nothing else: no files,
//! no cue sheets, no boundary trees, no fetched releases. Ordering the list
//! uses folder dates or natural-order paths, keeping each folder group's rows
//! together. The list interleaves group headers with three kinds of entry, so
//! the ordering and the offsets are worked out in Rust by
//! [`crate::import::list::flatten`]. Only the entries inside the requested
//! windows are then loaded whole.

mod window;

use super::import_state::{load_matches_on, load_provenance_on};
use super::*;
use crate::identify::{LeadMatch, VerdictKind, VerdictSummary};
use crate::import::folder_scanner::InvalidReason;
use crate::import::folder_scanner::ScanItem;
use crate::import::list::{
    flatten, ImportCandidateDetailProjection, ImportListProjection, ImportListRequest,
    ImportListWindow,
};
use crate::import::watched_folder::WatchedFolder;
use crate::import::{ImportedRelease, MetadataProvenance};
use folder_scans::columns::{invalid_reason_of, to_u32, to_u64, unreadable};

/// What the scan made of one folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanCandidateKind {
    /// A release approximation found before its enclosing boundary was known.
    Tentative,
    Valid,
    Invalid,
}

/// One scanned folder, as the list places it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanCandidateListRow {
    pub source: CandidateListSource,
    pub watched_folder_path: String,
    pub path: String,
    pub kind: ScanCandidateKind,
    pub name: String,
    pub display_path: String,
    /// Filesystem date, or first observation when the filesystem has none.
    /// Absent for a pre-date-tracking candidate that has not been rescanned.
    pub discovered_at: Option<i64>,
    /// `None` only for an invalid folder, which carries no files.
    pub content_hash: Option<String>,
    pub file_edit_revision: u64,
    pub combine_ancestor_relative_path: Option<String>,
    /// Set exactly when `kind` is [`ScanCandidateKind::Invalid`].
    pub invalid_reason: Option<InvalidReason>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CandidateListSource {
    Folder,
    Combination {
        skipped: bool,
        error: Option<String>,
    },
}

impl CandidateListSource {
    pub(crate) fn error(&self) -> Option<&str> {
        match self {
            Self::Folder => None,
            Self::Combination { error, .. } => error.as_deref(),
        }
    }
}

/// One candidate, as the list reads it: the revision it describes, what
/// identification concluded, and what was decided.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateStateListRow {
    pub edit_revision: u64,
    /// `None` when nothing has identified this candidate.
    pub verdict: Option<VerdictSummary>,
    pub metadata_provenance: Option<MetadataProvenance>,
    /// Who wrote the draft, which decides whether a valid one is the answer.
    pub metadata_author: crate::import::MetadataAuthor,
    pub metadata_draft_valid: bool,
    pub metadata_summary: Option<crate::import::TriageMetadataSummary>,
    pub selected_cover: Option<crate::import::CoverSelection>,
}

/// Every column the queue is placed from, in one read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportQueueRows {
    /// The watched roots in their stored order — the list's outer ordering.
    pub watched_folders: Vec<WatchedFolder>,
    pub candidates: Vec<ScanCandidateListRow>,
    /// `(watched_folder_path, relative_candidate_path)` of every skipped row.
    pub skipped: HashSet<(String, String)>,
    /// The library release each imported content hash became.
    pub imported: HashMap<String, ImportedRelease>,
    /// When each imported content hash's release was written, as Unix epoch
    /// milliseconds — the Done tab's within-section order. Kept beside
    /// `imported` rather than inside `ImportedRelease`: a row carries the
    /// release its import became, not when the import happened.
    pub imported_at: HashMap<String, i64>,
    /// The error the last import attempt left behind, by content hash. Read
    /// here rather than only in the pane because it is what a row's placement
    /// says on the next launch: without it a candidate whose import failed
    /// before the app quit comes back looking untouched.
    pub failures: HashMap<String, String>,
    pub states: HashMap<String, CandidateStateListRow>,
    /// Every folder whose reading is settled as several releases, keyed by
    /// `(watched_folder_path, relative_folder_path)`. The rows below such a
    /// folder are its releases; the folder is where the choice to read them as
    /// one is offered, so the list has to know which folders those are.
    pub separated_folders: HashSet<(String, String)>,
}

pub(super) fn load_import_queue_on(sql: &SqlReadContext<'_>) -> Result<ImportQueueRows, DbError> {
    let watched_folders: Vec<WatchedFolder> = sql
        .query(
            "SELECT path FROM watched_import_folders ORDER BY position",
            [],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .map(WatchedFolder::from_path)
        .collect();

    let candidates = candidate_rows(sql)?;

    let skipped: HashSet<(String, String)> = sql
        .query(
            "SELECT watched_folder_path, relative_candidate_path FROM skipped_import_candidates",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .into_iter()
        .collect();

    let mut imported: HashMap<String, ImportedRelease> = HashMap::new();
    let mut imported_at: HashMap<String, i64> = HashMap::new();
    for (content_hash, release, created_at) in sql.query(
        "SELECT content_hash, id, album_id, created_at \
         FROM releases WHERE content_hash IS NOT NULL",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                ImportedRelease {
                    release_id: row.get(1)?,
                    album_id: row.get(2)?,
                },
                super::read::rfc3339_column(row, "created_at")?,
            ))
        },
    )? {
        if imported.insert(content_hash.clone(), release).is_some() {
            return Err(DbError::Message(format!(
                "content hash {content_hash} names more than one imported release"
            )));
        }
        imported_at.insert(content_hash, created_at.timestamp_millis());
    }

    let separated_folders: HashSet<(String, String)> = sql
        .query(
            "SELECT watched_folder_path, relative_folder_path \
             FROM folder_release_decisions WHERE decision = 'keep_as_separate_releases'",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .into_iter()
        .collect();

    let failures: HashMap<String, String> = sql
        .query(
            "SELECT content_hash, error FROM import_candidate_failure",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .into_iter()
        .collect();

    let states = state_rows(sql)?;
    Ok(ImportQueueRows {
        watched_folders,
        candidates,
        skipped,
        imported,
        imported_at,
        failures,
        states,
        separated_folders,
    })
}

fn candidate_rows(sql: &SqlReadContext<'_>) -> Result<Vec<ScanCandidateListRow>, DbError> {
    sql.query(
        "SELECT watched_folder_path, path, kind, name, display_path, content_hash, \
                file_edit_revision, combine_ancestor_relative_path, invalid_reason, \
                invalid_reason_path, COALESCE(source_date, first_seen_at), source_kind, \
                (SELECT skipped FROM candidate_combination WHERE candidate_key = path), \
                (SELECT error FROM candidate_combination WHERE candidate_key = path) \
         FROM scan_candidate WHERE NOT EXISTS \
             (SELECT 1 FROM candidate_combination_member WHERE candidate_key = path)",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, Option<bool>>(12)?,
                row.get::<_, Option<String>>(13)?,
            ))
        },
    )?
    .into_iter()
    .map(
        |(
            watched_folder_path,
            path,
            kind,
            name,
            display_path,
            content_hash,
            file_edit_revision,
            combine_ancestor_relative_path,
            invalid_reason,
            invalid_reason_path,
            discovered_at,
            source_kind,
            combined_skipped,
            combined_error,
        )| {
            let kind = match kind.as_str() {
                "tentative" => ScanCandidateKind::Tentative,
                "valid" => ScanCandidateKind::Valid,
                "invalid" => ScanCandidateKind::Invalid,
                other => return Err(unreadable("kind", other)),
            };
            Ok(ScanCandidateListRow {
                source: match source_kind.as_str() {
                    "folder" => CandidateListSource::Folder,
                    "combination" => CandidateListSource::Combination {
                        skipped: combined_skipped.ok_or_else(|| {
                            DbError::Message(format!("combination {path} has no membership record"))
                        })?,
                        error: combined_error,
                    },
                    other => return Err(unreadable("source_kind", other)),
                },
                watched_folder_path,
                path,
                kind,
                name,
                display_path,
                discovered_at,
                content_hash,
                file_edit_revision: to_u64(
                    file_edit_revision,
                    "a scan candidate's file edit revision",
                )?,
                combine_ancestor_relative_path,
                invalid_reason: invalid_reason
                    .map(|reason| invalid_reason_of(&reason, invalid_reason_path))
                    .transpose()?,
            })
        },
    )
    .collect()
}

fn state_rows(sql: &SqlReadContext<'_>) -> Result<HashMap<String, CandidateStateListRow>, DbError> {
    let mut drafts = super::import_state::load_drafts_on(sql, None)?;
    let mut covers = super::import_state::load_covers_on(sql, None)?;
    // Every match row, not a count and a lead row: how many *pressings* a
    // verdict named is what the Ready rule asks, and which row each match
    // belongs to is what its own run decided.
    let mut matches = load_matches_on(sql, None)?;
    let mut provenances = load_provenance_on(sql, None)?;
    let mut authors = super::import_state::load_authors_on(sql, None)?;
    let mut verdicts: HashMap<String, VerdictSummary> = HashMap::new();
    for row in sql.query(
        "SELECT content_hash, kind, track_count FROM import_candidate_verdict",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        },
    )? {
        let (content_hash, kind, track_count) = row;
        // Read the lead off the first row, then spend the rest on the count:
        // both come from the one read of this candidate's matches.
        // The releases agreement narrowed out are not what the verdict
        // settled on: the row leads with a match and counts pressings among
        // the matches alone.
        let found = matches.remove(&content_hash).unwrap_or_default().found;
        let lead = found
            .first()
            .map(|stored| LeadMatch::of(&stored.result, Some(&stored.provenance)));
        // The rows the run built, read off the row each match names. Nothing
        // re-forms them: a list of a run's answers does not hold what it
        // decided those rows against.
        let pressing_count = crate::import::release_group::row_count(
            &found.iter().map(|stored| stored.pressing).collect::<Vec<_>>(),
        ) as u32;
        let summary = VerdictSummary {
            kind: match kind.as_str() {
                "found" => VerdictKind::Found,
                "not_found" => VerdictKind::NotFound,
                "manual_only" => VerdictKind::ManualOnly,
                "failed" => VerdictKind::Failed,
                other => return Err(unreadable("verdict kind", other)),
            },
            track_count: track_count
                .map(|count| to_u32(count, "a verdict's track count"))
                .transpose()?,
            pressing_count,
            lead,
        };
        verdicts.insert(content_hash, summary);
    }

    let mut states = HashMap::new();
    for (content_hash, edit_revision) in sql.query(
        "SELECT content_hash, edit_revision FROM import_candidate_state",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )? {
        let verdict = verdicts.remove(&content_hash);
        let metadata_provenance = provenances.remove(&content_hash);
        let metadata_draft = drafts.remove(&content_hash).ok_or_else(|| {
            DbError::Message(format!(
                "candidate {content_hash} has no editable metadata draft"
            ))
        })?;
        let metadata_author = authors.remove(&content_hash).ok_or_else(|| {
            DbError::Message(format!(
                "candidate {content_hash} has no editable metadata draft"
            ))
        })?;
        metadata_author
            .check_provenance(metadata_provenance.as_ref())
            .map_err(|error| DbError::Message(format!("candidate {content_hash}: {error}")))?;
        let release_edit = metadata_draft.release_edit();
        let metadata_draft_valid = release_edit.shape().is_ok();
        let metadata_summary =
            crate::import::TriageMetadataSummary::of(&release_edit, metadata_provenance.clone());
        let selected_cover = covers.remove(&content_hash);
        states.insert(
            content_hash,
            CandidateStateListRow {
                edit_revision: to_u64(edit_revision, "a candidate's edit revision")?,
                verdict,
                metadata_provenance,
                metadata_author,
                metadata_draft_valid,
                metadata_summary,
                selected_cover,
            },
        );
    }
    Ok(states)
}

/// One read of the list for `request`.
fn load_import_list_on(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
) -> Result<impl FnOnce() -> Result<ImportListProjection, DbError> + Send + 'static, DbError> {
    let rows = load_import_queue_on(sql)?;
    let flat = flatten(&rows, request).map_err(|error| DbError::Message(error.to_string()))?;
    let windows = request
        .windows
        .iter()
        .map(|window| {
            Ok((
                window.clone(),
                window::materialise(sql, window, &flat, &rows)?,
            ))
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(move || {
        let windows = windows
            .into_iter()
            .map(|(window, items)| {
                Ok(ImportListWindow {
                    window,
                    items: items
                        .into_iter()
                        .map(window::WindowItemRows::process)
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(ImportListProjection {
            total_count: flat.items.len() as u64,
            windows,
            summary: flat.summary,
        })
    })
}

impl Database {
    /// The import tab as a reconfigurable live query: the view and the windows
    /// travel in the request, so changing either reruns the read without
    /// rebuilding the subscription, and a commit that changes nothing the
    /// request asked for is withheld.
    pub(crate) fn subscribe_import_list(
        &self,
        initial: ImportListRequest,
    ) -> coven::ReconfigurableLiveQuery<ImportListRequest, ImportListProjection> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, move |request, sql| {
                load_import_list_on(&sql, request).map_err(CovenError::from)
            })
            .process(|_, process| process().map_err(CovenError::from))
    }

    /// One read of the list, for a caller with no subscription.
    pub(crate) async fn load_import_list(
        &self,
        request: ImportListRequest,
    ) -> Result<ImportListProjection, DbError> {
        self.read(move |sql| load_import_list_on(&sql, &request))
            .process(|process| process())
            .await
    }

    pub(crate) async fn locate_import_candidate(
        &self,
        request: ImportListRequest,
        candidate_key: &str,
    ) -> Result<Option<crate::import::ImportCandidateListLocation>, DbError> {
        let candidate_key = candidate_key.to_string();
        self.read(move |sql| load_import_queue_on(&sql))
            .process(move |rows| {
                crate::import::list::locate_candidate(&rows, &request, &candidate_key)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .await
    }

    /// The first of `keys` in the queue's own order under `request`'s view,
    /// read from the tables. `None` when the queue holds none of them.
    pub(crate) async fn first_import_candidate_among(
        &self,
        request: ImportListRequest,
        keys: HashSet<String>,
    ) -> Result<Option<String>, DbError> {
        self.read(move |sql| load_import_queue_on(&sql))
            .process(move |rows| {
                crate::import::list::first_candidate_among(&rows, &request, &keys)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .await
    }

    /// One candidate as the pane reads it, live. `None` once the key names no
    /// scanned folder, which is what clears a selection.
    pub(crate) fn subscribe_import_candidate(
        &self,
        key: &str,
    ) -> coven::LiveQuery<Option<ImportCandidateDetailProjection>> {
        let key = key.to_string();
        self.inner
            .handle
            .subscribe(move |sql| {
                window::load_candidate_detail_on(&sql, &key).map_err(CovenError::from)
            })
            .process(|process| {
                process
                    .map(|process| process())
                    .transpose()
                    .map_err(CovenError::from)
            })
    }

    /// Every candidate the queue sweep is responsible for: settled folders,
    /// with their files, that are neither skipped nor already in the library.
    ///
    /// Read from the tables rather than from the list, which is a query that
    /// lands after the commit it reflects — the sweep plans a pass right after
    /// the event that changed the answer.
    pub(crate) async fn load_sweepable_candidates(
        &self,
    ) -> Result<Vec<crate::import::FolderCandidate>, DbError> {
        self.read(move |sql| load_sweepable_candidates_on(&sql))
            .process(|process| process())
            .await
    }

    /// One candidate as the pane reads it, once.
    pub(crate) async fn load_import_candidate(
        &self,
        key: &str,
    ) -> Result<Option<ImportCandidateDetailProjection>, DbError> {
        let key = key.to_string();
        self.read(move |sql| window::load_candidate_detail_on(&sql, &key))
            .process(|process| process.map(|process| process()).transpose())
            .await
    }
}

fn load_sweepable_candidates_on(
    sql: &SqlReadContext<'_>,
) -> Result<
    impl FnOnce() -> Result<Vec<crate::import::FolderCandidate>, DbError> + Send + 'static,
    DbError,
> {
    let sweepable: HashSet<(String, String)> = sql
        .query(
            "SELECT watched_folder_path, path FROM scan_candidate \
             WHERE kind = 'valid' \
               AND NOT EXISTS (SELECT 1 FROM candidate_combination_member WHERE candidate_key = path)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .into_iter()
        .collect();
    let skipped: HashSet<(String, String)> = sql
        .query(
            "SELECT watched_folder_path, relative_candidate_path FROM skipped_import_candidates",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .into_iter()
        .collect();
    let imported: HashSet<String> = sql
        .query(
            "SELECT DISTINCT content_hash FROM releases WHERE content_hash IS NOT NULL",
            [],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .collect();
    let roots = sql.query(
        "SELECT watched_folder_path FROM folder_scan_roots ORDER BY watched_folder_path",
        [],
        |row| row.get::<_, String>(0),
    )?;
    let roots = roots
        .into_iter()
        .map(|root| folder_scans::read::load_candidate_items_rows(sql, &root, None))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(move || {
        let mut candidates = Vec::new();
        for root in roots {
            for stored in root()? {
                let ScanItem::Valid(candidate) = stored.item else {
                    continue;
                };
                let candidate_key = candidate.path.to_string_lossy().into_owned();
                if !sweepable.contains(&(candidate.watched_folder_path.clone(), candidate_key)) {
                    continue;
                }
                let relative = crate::import::watched_folder::candidate_relative_path(
                    &candidate.watched_folder_path,
                    &candidate.path,
                )
                .map_err(|error| DbError::Message(error.to_string()))?;
                if skipped.contains(&(candidate.watched_folder_path.clone(), relative))
                    || imported.contains(&candidate.files.content_hash())
                {
                    continue;
                }
                candidates.push(candidate);
            }
        }
        Ok(candidates)
    })
}
