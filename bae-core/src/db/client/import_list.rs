//! The import tab's list, read as columns.
//!
//! Every rerun reads the whole queue as a few short columns per folder, draft
//! and verdict (never files, cue sheets, fetched releases, draft tracks or
//! covers), places it with [`crate::import::list::flatten`], and loads whole
//! only the entries inside the requested windows.

mod window;

use super::import_state::{load_matches_on, load_provenance_on};
use super::*;
use crate::identify::{LeadMatch, VerdictKind, VerdictSummary};
use crate::import::folder_scanner::InvalidReason;
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
    /// A release found before the folder around it was known.
    Tentative,
    Valid,
    Invalid,
}

/// One stored release, as the list places it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanCandidateListRow {
    /// The grouping this release reads several folders as, when it is one.
    pub grouping: Option<CandidateListGrouping>,
    pub watched_folder_path: String,
    /// The release's key: its folder's path, or its grouping's key.
    pub path: String,
    /// The folder the release is shown as.
    pub folder: String,
    pub kind: ScanCandidateKind,
    pub name: String,
    pub display_path: String,
    /// Filesystem date, or first observation when the filesystem has none;
    /// `None` when neither is stored.
    pub discovered_at: Option<i64>,
    /// `None` only for an invalid folder, which carries no files.
    pub content_hash: Option<String>,
    pub file_edit_revision: u64,
    /// Set exactly when `kind` is [`ScanCandidateKind::Invalid`].
    pub invalid_reason: Option<InvalidReason>,
}

/// What a release read from several folders carries on its grouping.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateListGrouping {
    pub skipped: bool,
    /// Why a grouping of releases picked together cannot be worked on as it
    /// stands.
    pub error: Option<crate::import::GroupingBlock>,
}

impl ScanCandidateListRow {
    /// Why this release cannot be worked on as it stands, if anything says so.
    pub(crate) fn error(&self) -> Option<&crate::import::GroupingBlock> {
        self.grouping
            .as_ref()
            .and_then(|grouping| grouping.error.as_ref())
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
}

/// Every column the queue is placed from, in one read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportQueueRows {
    /// In stored order, which is the list's outer order.
    pub watched_folders: Vec<WatchedFolder>,
    pub candidates: Vec<ScanCandidateListRow>,
    /// Every skipped candidate as the list addresses it: the watched folder
    /// covering it, and its path below that.
    pub skipped: HashSet<(String, String)>,
    /// The library release each imported content hash became.
    pub imported: HashMap<String, ImportedRelease>,
    /// When each imported content hash's release was written, in Unix epoch
    /// milliseconds; orders rows within a Done section.
    pub imported_at: HashMap<String, i64>,
    /// The error the last import attempt left, by content hash, so a failed
    /// import still places as failed after a relaunch.
    pub failures: HashMap<String, String>,
    pub states: HashMap<String, CandidateStateListRow>,
    /// Each grouping's anchor folder as (watched folder, path below it):
    /// `true` when its releases are read as one, `false` when kept apart.
    pub folder_readings: HashMap<(String, String), bool>,
    /// Each Done row's library text by release id, which the filter tests;
    /// `None` when the view does not filter, so an unfiltered list neither
    /// reads nor reruns on it.
    pub imported_text: Option<HashMap<String, crate::import::ImportedReleaseText>>,
}

/// Whether a queue read loads the Done rows' text: only when the view filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DoneRowText {
    Read,
    Skip,
}

impl DoneRowText {
    fn of(view: &crate::import::list::ImportListView) -> Self {
        if view.filters() {
            Self::Read
        } else {
            Self::Skip
        }
    }
}

/// Each imported release's title, credited artists after merges, and year, by
/// release id; only `only`'s when given.
pub(super) fn load_imported_release_text_on(
    sql: &SqlReadContext<'_>,
    only: Option<&str>,
) -> Result<HashMap<String, crate::import::ImportedReleaseText>, DbError> {
    Ok(sql
        .query(
            &format!(
                "SELECT r.id, a.title, NULLIF({artist_names}, ''), a.year \
                 FROM releases r JOIN albums a ON a.id = r.album_id \
                 WHERE r.content_hash IS NOT NULL AND (?1 IS NULL OR r.id = ?1)",
                artist_names = super::query::album_artist_names_sql(),
            ),
            params![only],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    crate::import::ImportedReleaseText {
                        title: row.get(1)?,
                        artist: row.get(2)?,
                        year: row.get(3)?,
                    },
                ))
            },
        )?
        .into_iter()
        .collect())
}

pub(super) fn load_import_queue_on(
    sql: &SqlReadContext<'_>,
    done_row_text: DoneRowText,
) -> Result<ImportQueueRows, DbError> {
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

    let roots: Vec<String> = watched_folders
        .iter()
        .map(|folder| folder.path.clone())
        .collect();
    let skipped: HashSet<(String, String)> = sql
        .query(
            "SELECT candidate_path FROM skipped_import_candidates",
            [],
            |row| row.get::<_, String>(0),
        )?
        .iter()
        .map(|path| listed_below(&roots, path))
        .collect::<Result<_, _>>()?;

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

    let folder_readings: HashMap<(String, String), bool> = sql
        .query(
            "SELECT anchor_folder, combined FROM release_grouping WHERE anchor_folder IS NOT NULL",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )?
        .into_iter()
        .map(|(folder, combined)| Ok((listed_below(&roots, &folder)?, combined)))
        .collect::<Result<_, DbError>>()?;

    let failures: HashMap<String, String> = sql
        .query(
            "SELECT content_hash, error FROM import_candidate_failure",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .into_iter()
        .collect();

    let states = state_rows(sql)?;
    let imported_text = match done_row_text {
        DoneRowText::Read => Some(load_imported_release_text_on(sql, None)?),
        DoneRowText::Skip => None,
    };
    Ok(ImportQueueRows {
        watched_folders,
        candidates,
        skipped,
        imported,
        imported_at,
        failures,
        states,
        folder_readings,
        imported_text,
    })
}

/// A stored folder as the list addresses it: the watched folder covering it
/// and its path below that. A stored folder outside every watched folder is an
/// error.
fn listed_below(roots: &[String], folder: &str) -> Result<(String, String), DbError> {
    crate::import::watched_folder::validate_stored_folder(folder)?;
    let root = crate::import::watched_folder::covering_root(roots, folder)
        .ok_or_else(|| DbError::Message(format!("{folder} is under no watched folder")))?;
    Ok((
        root.to_string(),
        super::import_state::relative_below(root, folder)?,
    ))
}

fn candidate_rows(sql: &SqlReadContext<'_>) -> Result<Vec<ScanCandidateListRow>, DbError> {
    // A release a grouping takes in stays stored but is left out of the queue.
    sql.query(
        "SELECT c.watched_folder_path, c.path, c.folder, c.kind, c.name, c.display_path, \
                c.content_hash, c.file_edit_revision, c.invalid_reason, c.invalid_reason_path, \
                COALESCE(d.source_date, d.first_seen_at), c.grouping_key, g.skipped, \
                g.blocked, g.blocked_subject, g.blocked_holder \
         FROM scan_candidate AS c \
         LEFT JOIN folder_discovery AS d ON d.folder = c.folder \
         LEFT JOIN release_grouping AS g ON g.key = c.grouping_key \
         WHERE NOT EXISTS \
             (SELECT 1 FROM release_grouping_member WHERE member_key = c.path)",
        [],
        |row| {
            Ok((
                (
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ),
                (
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<i64>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<bool>>(12)?,
                    (
                        row.get::<_, Option<String>>(13)?,
                        row.get::<_, Option<String>>(14)?,
                        row.get::<_, Option<String>>(15)?,
                    ),
                ),
            ))
        },
    )?
    .into_iter()
    .map(
        |(
            (watched_folder_path, path, folder, kind, name, display_path),
            (
                content_hash,
                file_edit_revision,
                invalid_reason,
                invalid_reason_path,
                discovered_at,
                grouping_key,
                grouping_skipped,
                (blocked, blocked_subject, blocked_holder),
            ),
        )| {
            let kind = match kind.as_str() {
                "tentative" => ScanCandidateKind::Tentative,
                "valid" => ScanCandidateKind::Valid,
                "invalid" => ScanCandidateKind::Invalid,
                other => return Err(unreadable("kind", other)),
            };
            let grouping = match grouping_key {
                Some(grouping_key) => Some(CandidateListGrouping {
                    skipped: grouping_skipped.ok_or_else(|| {
                        DbError::Message(format!("release {path} names no stored grouping {grouping_key}"))
                    })?,
                    error: super::release_groupings::block_of(
                        blocked,
                        blocked_subject,
                        blocked_holder,
                    )?,
                }),
                None => None,
            };
            Ok(ScanCandidateListRow {
                grouping,
                watched_folder_path,
                path,
                folder,
                kind,
                name,
                display_path,
                discovered_at,
                content_hash,
                file_edit_revision: to_u64(
                    file_edit_revision,
                    "a scan candidate's file edit revision",
                )?,
                invalid_reason: invalid_reason
                    .map(|reason| invalid_reason_of(&reason, invalid_reason_path))
                    .transpose()?,
            })
        },
    )
    .collect()
}

fn state_rows(sql: &SqlReadContext<'_>) -> Result<HashMap<String, CandidateStateListRow>, DbError> {
    // Only the draft's list columns; its tracks are the pane's to read.
    let mut drafts: HashMap<String, (String, bool, bool, crate::import::MetadataAuthor)> = sql
        .query(
            "SELECT content_hash, album_title, draft_blank, draft_valid, author \
             FROM import_candidate_edit",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )?
        .into_iter()
        .map(|(content_hash, album_title, blank, valid, author)| {
            Ok((
                content_hash,
                (
                    album_title,
                    blank,
                    valid,
                    super::import_state::author_of(&author)?,
                ),
            ))
        })
        .collect::<Result<_, DbError>>()?;
    let mut album_artists = super::import_state::load_album_artist_assignments_on(sql, None)?;
    // Every match row, not a count: pressings are counted by the row each
    // match's run stored for it.
    let mut matches = load_matches_on(sql, None)?;
    let mut provenances = load_provenance_on(sql, None)?;
    let mut verdicts: HashMap<String, VerdictSummary> = HashMap::new();
    for row in sql.query(
        "SELECT content_hash, kind, track_count, medium_conflict \
         FROM import_candidate_verdict",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )? {
        let (content_hash, kind, track_count, medium_conflict) = row;
        // The releases agreement narrowed out are not what the verdict settled
        // on, so the lead and the count come from `found` alone.
        let found = matches.remove(&content_hash).unwrap_or_default().found;
        let lead = found
            .first()
            .map(|stored| LeadMatch::of(&stored.result, Some(&stored.provenance)));
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
            medium_conflict: super::import_state::medium_conflict_of(medium_conflict)?,
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
        let (album_title, draft_blank, metadata_draft_valid, metadata_author) =
            drafts.remove(&content_hash).ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {content_hash} has no editable metadata draft"
                ))
            })?;
        metadata_author
            .check_provenance(metadata_provenance.as_ref())
            .map_err(|error| DbError::Message(format!("candidate {content_hash}: {error}")))?;
        let album_artist_assignments = album_artists.remove(&content_hash).unwrap_or_default();
        let metadata_summary = crate::import::TriageMetadataSummary::of_columns(
            album_title,
            album_artist_assignments,
            draft_blank,
            metadata_provenance.as_ref(),
        );
        states.insert(
            content_hash,
            CandidateStateListRow {
                edit_revision: to_u64(edit_revision, "a candidate's edit revision")?,
                verdict,
                metadata_provenance,
                metadata_author,
                metadata_draft_valid,
                metadata_summary,
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
    let rows = load_import_queue_on(sql, DoneRowText::of(&request.view))?;
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
    /// The import tab as a live query whose request carries the view and the
    /// windows, so changing either reruns the read on the same subscription.
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
        // Locating clears the filter, so nothing tests a Done row's text.
        self.read(move |sql| load_import_queue_on(&sql, DoneRowText::Skip))
            .process(move |rows| {
                crate::import::list::locate_candidate(&rows, &request, &candidate_key)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .await
    }

    /// The first of `keys` in the queue's order under `request`'s view; `None`
    /// when the queue holds none of them.
    pub(crate) async fn first_import_candidate_among(
        &self,
        request: ImportListRequest,
        keys: HashSet<String>,
    ) -> Result<Option<String>, DbError> {
        // The queue's own order ignores the filter, so nothing tests a Done
        // row's text.
        self.read(move |sql| load_import_queue_on(&sql, DoneRowText::Skip))
            .process(move |rows| {
                crate::import::list::first_candidate_among(&rows, &request, &keys)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .await
    }

    /// The candidate the request names, as the pane reads it, live; `None` for
    /// no key or a key naming no scanned folder, which clears a selection.
    pub(crate) fn subscribe_import_candidate(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<ImportCandidateDetailProjection>>
    {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |key, sql| match key {
                None => Ok(None),
                Some(key) => window::load_candidate_detail_on(&sql, key).map_err(CovenError::from),
            })
            .process(|_, process| {
                process
                    .map(|process| process())
                    .transpose()
                    .map_err(CovenError::from)
            })
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
