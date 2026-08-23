//! The import tab's list, read as columns.
//!
//! The whole queue is read on every rerun — a few short columns per scanned
//! folder, per boundary and per stored verdict — and nothing else: no files,
//! no cue sheets, no boundary trees, no archived documents. Ordering the list
//! is natural-order over the folder's display path, which SQLite has no
//! collation for, and the list interleaves group headers with three kinds of
//! entry, so the ordering and the offsets are worked out in Rust by
//! [`crate::import::list::flatten`]. Only the entries inside the requested
//! windows are then loaded whole.

mod window;

use super::identity::check_releases_in_library_on;
use super::import_state::pick_of;
use super::*;
use crate::identify::{LeadMatch, VerdictKind, VerdictSummary};
use crate::import::folder_registry::WatchedFolder;
use crate::import::folder_scanner::InvalidReason;
use crate::import::folder_scanner::ScanItem;
use crate::import::list::{
    flatten, ImportCandidateDetailProjection, ImportListProjection, ImportListRequest,
    ImportListWindow,
};
use crate::import::search::SourceTracks;
use crate::import::{
    FolderScanStatus, IdentityPick, ImportedRelease, MetadataSource, WatchedFolderScanStatus,
};
use folder_scans::columns::{invalid_reason_of, to_u32, to_u64, unreadable};
use std::str::FromStr;

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
    pub watched_folder_path: String,
    pub path: String,
    pub kind: ScanCandidateKind,
    pub name: String,
    pub display_path: String,
    /// `None` only for an invalid folder, which carries no files.
    pub content_hash: Option<String>,
    pub file_edit_revision: u64,
    pub combine_ancestor_relative_path: Option<String>,
    /// Set exactly when `kind` is [`ScanCandidateKind::Invalid`].
    pub invalid_reason: Option<InvalidReason>,
}

/// One boundary, as the list places it. The tree itself is loaded only for the
/// boundaries inside a window; the rows' display paths are here because the
/// filter matches them.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanBoundaryListRow {
    pub watched_folder_path: String,
    pub relative_folder_path: String,
    pub name: String,
    pub display_path: String,
    pub tree_row_display_paths: Vec<String>,
}

/// One `import_candidate_state` row, as the list reads it: the revision it
/// describes, what identification concluded, and what was decided.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateStateListRow {
    pub edit_revision: u64,
    /// `None` when the identify columns are clear.
    pub verdict: Option<VerdictSummary>,
    pub probed_total_duration_ms: u64,
    pub pick: Option<IdentityPick>,
}

/// Every column the queue is placed from, in one read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportQueueRows {
    /// The watched roots in their stored order — the list's outer ordering.
    pub watched_folders: Vec<WatchedFolder>,
    pub folder_scan_statuses: Vec<WatchedFolderScanStatus>,
    pub candidates: Vec<ScanCandidateListRow>,
    pub boundaries: Vec<ScanBoundaryListRow>,
    /// `(watched_folder_path, relative_candidate_path)` of every skipped row.
    pub skipped: HashSet<(String, String)>,
    /// The library release each imported content hash became.
    pub imported: HashMap<String, ImportedRelease>,
    /// The error the last import attempt left behind, by content hash. Read
    /// here rather than only in the pane because it is what a row's placement
    /// says on the next launch: without it a candidate whose import failed
    /// before the app quit comes back looking untouched.
    pub failures: HashMap<String, String>,
    pub states: HashMap<String, CandidateStateListRow>,
    /// The live library check of every lead match, by release id. Only the
    /// leads of single-match verdicts are checked: those are the only ones the
    /// Ready rule asks about.
    pub lead_statuses: HashMap<String, LibraryStatus>,
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

    let folder_scan_statuses = scan_statuses(sql, &watched_folders)?;
    let candidates = candidate_rows(sql)?;
    let boundaries = boundary_rows(sql)?;

    let skipped: HashSet<(String, String)> = sql
        .query(
            "SELECT watched_folder_path, relative_candidate_path FROM skipped_import_candidates",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .into_iter()
        .collect();

    let mut imported: HashMap<String, ImportedRelease> = HashMap::new();
    for (content_hash, release) in sql.query(
        "SELECT content_hash, id, album_id FROM releases WHERE content_hash IS NOT NULL",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                ImportedRelease {
                    release_id: row.get(1)?,
                    album_id: row.get(2)?,
                },
            ))
        },
    )? {
        if imported.insert(content_hash.clone(), release).is_some() {
            return Err(DbError::Message(format!(
                "content hash {content_hash} names more than one imported release"
            )));
        }
    }

    let failures: HashMap<String, String> = sql
        .query(
            "SELECT content_hash, error FROM import_candidate_failure",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .into_iter()
        .collect();

    let states = state_rows(sql)?;
    let mut checks = Vec::new();
    for state in states.values() {
        let Some(verdict) = state.verdict.as_ref() else {
            continue;
        };
        // The Ready rule consults the lead and only when it is the one match;
        // every other shape is answered before the library is asked.
        if verdict.match_count != 1 {
            continue;
        }
        if let Some(lead) = verdict.lead.as_ref() {
            checks.push(LibraryCheck {
                release_id: lead.release_id.clone(),
                source: lead.source,
                source_group_id: lead.source_group_id.clone(),
            });
        }
    }
    checks.sort_by(|left, right| left.release_id.cmp(&right.release_id));
    checks.dedup_by(|left, right| left.release_id == right.release_id);
    let lead_statuses = check_releases_in_library_on(sql, &checks)?
        .into_iter()
        .map(|status| (status.release_id.clone(), status))
        .collect();

    Ok(ImportQueueRows {
        watched_folders,
        folder_scan_statuses,
        candidates,
        boundaries,
        skipped,
        imported,
        failures,
        states,
        lead_statuses,
    })
}

fn scan_statuses(
    sql: &SqlReadContext<'_>,
    watched_folders: &[WatchedFolder],
) -> Result<Vec<WatchedFolderScanStatus>, DbError> {
    let order: HashMap<&str, usize> = watched_folders
        .iter()
        .enumerate()
        .map(|(index, folder)| (folder.path.as_str(), index))
        .collect();
    let mut statuses = Vec::new();
    for (watched_folder_path, status, error) in sql.query(
        "SELECT watched_folder_path, status, error FROM folder_scan_roots",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )? {
        let watched_folder = watched_folders
            .iter()
            .find(|folder| folder.path == watched_folder_path)
            .ok_or_else(|| {
                DbError::Message(format!(
                    "folder scan root {watched_folder_path} is not a watched folder"
                ))
            })?;
        let status = match (status.as_str(), error) {
            ("scanning", None) => FolderScanStatus::Scanning,
            ("complete", None) => FolderScanStatus::Complete,
            ("failed", Some(error)) => FolderScanStatus::Failed { error },
            (status, error) => {
                return Err(DbError::Message(format!(
                    "folder scan root {watched_folder_path} has invalid status {status:?} \
                     and error {error:?}"
                )))
            }
        };
        statuses.push(WatchedFolderScanStatus {
            watched_folder_path,
            watched_folder_name: watched_folder.name.clone(),
            status,
        });
    }
    statuses.sort_by(|left, right| {
        order
            .get(left.watched_folder_path.as_str())
            .cmp(&order.get(right.watched_folder_path.as_str()))
            .then_with(|| left.watched_folder_path.cmp(&right.watched_folder_path))
    });
    Ok(statuses)
}

fn candidate_rows(sql: &SqlReadContext<'_>) -> Result<Vec<ScanCandidateListRow>, DbError> {
    sql.query(
        "SELECT watched_folder_path, path, kind, name, display_path, content_hash, \
                file_edit_revision, combine_ancestor_relative_path, invalid_reason, \
                invalid_reason_path \
         FROM scan_candidate",
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
        )| {
            let kind = match kind.as_str() {
                "tentative" => ScanCandidateKind::Tentative,
                "valid" => ScanCandidateKind::Valid,
                "invalid" => ScanCandidateKind::Invalid,
                other => return Err(unreadable("kind", other)),
            };
            Ok(ScanCandidateListRow {
                watched_folder_path,
                path,
                kind,
                name,
                display_path,
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

fn boundary_rows(sql: &SqlReadContext<'_>) -> Result<Vec<ScanBoundaryListRow>, DbError> {
    let mut tree_rows: HashMap<(String, String), Vec<String>> = HashMap::new();
    for (watched_folder_path, boundary, display_path) in sql.query(
        "SELECT watched_folder_path, boundary_relative_folder_path, display_path \
         FROM scan_boundary_tree_row ORDER BY watched_folder_path, \
              boundary_relative_folder_path, position",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )? {
        tree_rows
            .entry((watched_folder_path, boundary))
            .or_default()
            .push(display_path);
    }
    Ok(sql
        .query(
            "SELECT watched_folder_path, relative_folder_path, name, display_path \
             FROM scan_boundary",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?
        .into_iter()
        .map(
            |(watched_folder_path, relative_folder_path, name, display_path)| {
                let tree_row_display_paths = tree_rows
                    .remove(&(watched_folder_path.clone(), relative_folder_path.clone()))
                    .unwrap_or_default();
                ScanBoundaryListRow {
                    watched_folder_path,
                    relative_folder_path,
                    name,
                    display_path,
                    tree_row_display_paths,
                }
            },
        )
        .collect())
}

fn state_rows(sql: &SqlReadContext<'_>) -> Result<HashMap<String, CandidateStateListRow>, DbError> {
    let mut match_counts: HashMap<String, u32> = HashMap::new();
    for (content_hash, count) in sql.query(
        "SELECT content_hash, COUNT(*) FROM import_candidate_match \
         GROUP BY content_hash",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )? {
        match_counts.insert(content_hash, to_u32(count, "a verdict's match count")?);
    }
    let mut leads: HashMap<String, LeadMatch> = HashMap::new();
    for (content_hash, lead) in sql.query(
        "SELECT content_hash, source, release_id, source_group_id, title, artist, year, \
                format, cover_thumbnail_url, source_tracks_kind, source_tracks_count, \
                source_tracks_total_ms, by_disc_id, by_barcode \
         FROM import_candidate_match WHERE position = 0",
        [],
        |row| Ok((row.get::<_, String>(0)?, read_lead(row))),
    )? {
        leads.insert(content_hash, lead?);
    }

    let mut states = HashMap::new();
    for row in sql.query(
        "SELECT content_hash, edit_revision, verdict_kind, verdict_track_count, \
                probed_total_duration_ms, pick_kind, pick_source, pick_release_id \
         FROM import_candidate_state",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                pick_of(row.get(5)?, row.get(6)?, row.get(7)?),
            ))
        },
    )? {
        let (content_hash, edit_revision, verdict_kind, track_count, probed, pick) = row;
        let verdict = verdict_kind
            .map(|kind| -> Result<VerdictSummary, DbError> {
                Ok(VerdictSummary {
                    kind: match kind.as_str() {
                        "found" => VerdictKind::Found,
                        "not_found" => VerdictKind::NotFound,
                        "manual_only" => VerdictKind::ManualOnly,
                        other => return Err(unreadable("verdict_kind", other)),
                    },
                    track_count: track_count
                        .map(|count| to_u32(count, "a verdict's track count"))
                        .transpose()?,
                    match_count: match_counts.get(&content_hash).copied().unwrap_or_default(),
                    lead: leads.get(&content_hash).cloned(),
                })
            })
            .transpose()?;
        states.insert(
            content_hash,
            CandidateStateListRow {
                edit_revision: to_u64(edit_revision, "a candidate's edit revision")?,
                verdict,
                probed_total_duration_ms: probed
                    .map(|probed| to_u64(probed, "a candidate's probed total"))
                    .transpose()?
                    .unwrap_or_default(),
                pick: pick?,
            },
        );
    }
    Ok(states)
}

fn read_lead(row: &Row<'_>) -> Result<LeadMatch, DbError> {
    let source: String = row.get("source")?;
    let tracks_kind: Option<String> = row.get("source_tracks_kind")?;
    let source_tracks = match tracks_kind.as_deref() {
        None => None,
        Some("nothing") => Some(SourceTracks::Nothing),
        Some("listed") => {
            let count: i64 = row
                .get::<_, Option<i64>>("source_tracks_count")?
                .ok_or_else(|| {
                    DbError::Message("a listed source tracklist states no count".to_string())
                })?;
            Some(SourceTracks::Listed {
                count: to_u32(count, "a source tracklist's count")?,
                total_duration_ms: row
                    .get::<_, Option<i64>>("source_tracks_total_ms")?
                    .map(|total| to_u64(total, "a source tracklist's total"))
                    .transpose()?,
            })
        }
        Some(other) => return Err(unreadable("source_tracks_kind", other)),
    };
    Ok(LeadMatch {
        release_id: row.get("release_id")?,
        source: MetadataSource::from_str(&source).map_err(DbError::Message)?,
        source_group_id: row.get("source_group_id")?,
        title: row.get("title")?,
        artist: row.get("artist")?,
        year: row.get("year")?,
        format: row.get("format")?,
        cover_thumbnail_url: row.get("cover_thumbnail_url")?,
        source_tracks,
        by_disc_id: row
            .get::<_, Option<bool>>("by_disc_id")?
            .unwrap_or_default(),
        by_barcode: row
            .get::<_, Option<bool>>("by_barcode")?
            .unwrap_or_default(),
    })
}

/// One read of the list for `request`.
fn load_import_list_on(
    sql: &SqlReadContext<'_>,
    request: &ImportListRequest,
) -> Result<ImportListProjection, DbError> {
    let rows = load_import_queue_on(sql)?;
    let flat = flatten(&rows, &request.view, &request.runtime_facts)
        .map_err(|error| DbError::Message(error.to_string()))?;
    let windows = request
        .windows
        .iter()
        .map(|window| {
            Ok(ImportListWindow {
                window: window.clone(),
                items: window::materialise(sql, window, &flat, &rows)?,
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(ImportListProjection {
        total_count: flat.items.len() as u64,
        windows,
        summary: flat.summary,
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
    }

    /// One read of the list, for a caller with no subscription.
    pub(crate) async fn load_import_list(
        &self,
        request: ImportListRequest,
    ) -> Result<ImportListProjection, DbError> {
        self.read(move |sql| load_import_list_on(&sql, &request))
            .await
    }

    /// One candidate as the pane reads it, live. `None` once the key names no
    /// scanned folder, which is what clears a selection.
    pub(crate) fn subscribe_import_candidate(
        &self,
        key: &str,
    ) -> coven::LiveQuery<Option<ImportCandidateDetailProjection>> {
        let key = key.to_string();
        let clock = self.inner.clock.clone();
        let ids = self.inner.ids.clone();
        self.inner.handle.subscribe(move |sql| {
            window::load_candidate_detail_on(&sql, &key, clock.as_ref(), ids.as_ref())
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
            .await
    }

    /// One candidate as the pane reads it, once.
    pub(crate) async fn load_import_candidate(
        &self,
        key: &str,
    ) -> Result<Option<ImportCandidateDetailProjection>, DbError> {
        let key = key.to_string();
        let clock = self.inner.clock.clone();
        let ids = self.inner.ids.clone();
        self.read(move |sql| {
            window::load_candidate_detail_on(&sql, &key, clock.as_ref(), ids.as_ref())
        })
        .await
    }
}

fn load_sweepable_candidates_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<crate::import::FolderCandidate>, DbError> {
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
    let mut candidates = Vec::new();
    for root in roots {
        for stored in folder_scans::load_candidate_items(sql, &root, None)? {
            let ScanItem::Valid(candidate) = stored.item else {
                continue;
            };
            let relative = crate::import::folder_registry::candidate_relative_path(
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
}
