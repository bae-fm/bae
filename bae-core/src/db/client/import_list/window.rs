//! Turning item references into items, and reading one candidate whole.
//!
//! Everything expensive about the import tab lives here: a candidate's
//! resolved boundaries, a boundary's tree, and the archived documents behind a
//! pick. All three are read for the entries inside a requested window and for
//! the one key a selection names — never for the queue.

use super::super::folder_scans::{load_boundary_items, load_item_by_key, load_resolved_boundaries};
use super::super::identity::check_releases_in_library_on;
use super::super::import_state::load_states_on;
use super::super::payloads::load_release_payloads_on;
use super::*;
use crate::identify::{classify, TerminalVerdict, VerdictSummary};
use crate::import::folder_scanner::{InvalidCandidate, ResolvedFolderReleaseBoundary, ScanItem};
use crate::import::list::{window_refs, Flattened, ImportListItem, ItemRef};
use crate::import::search::MetadataResult;
use crate::import::triage::MatchedRelease;
use crate::import::MetadataRef;
use crate::library::LibraryPageWindow;
use std::path::PathBuf;

/// The items one window holds, loaded whole.
pub(super) fn materialise(
    sql: &SqlReadContext<'_>,
    window: &LibraryPageWindow,
    flat: &Flattened,
    rows: &ImportQueueRows,
) -> Result<Vec<ImportListItem>, DbError> {
    window_refs(&flat.items, window)
        .iter()
        .map(|item| match item {
            ItemRef::Header(index) => Ok(flat.headers[*index].item()),
            ItemRef::Candidate(index) => {
                let placed = &flat.rows[*index];
                let scanned = &rows.candidates[placed.index];
                let mut row = placed.row.clone();
                row.resolved_boundaries =
                    resolved_boundaries(sql, &scanned.watched_folder_path, &scanned.path)?;
                // A decided identity outranks the verdict's lead: a manual
                // search settles a folder on a release the verdict never
                // named. With nothing archived behind the pick the row leads
                // with its folder name rather than someone else's release.
                if let Some(pick) = row.picked.clone() {
                    row.matched = picked_release(sql, &pick)?;
                }
                Ok(ImportListItem::Candidate(row))
            }
            ItemRef::Boundary(index) => {
                let boundary = &rows.boundaries[*index];
                let mut items = load_boundary_items(
                    sql,
                    &boundary.watched_folder_path,
                    Some(&boundary.relative_folder_path),
                )?;
                match items.pop().map(|stored| stored.item) {
                    Some(ScanItem::Boundary(boundary)) => Ok(ImportListItem::Boundary(boundary)),
                    _ => Err(DbError::Message(format!(
                        "folder scan boundary {} vanished between its two reads",
                        boundary.display_path
                    ))),
                }
            }
            ItemRef::Invalid(index) => {
                let scanned = &rows.candidates[*index];
                Ok(ImportListItem::Invalid(InvalidCandidate {
                    path: PathBuf::from(&scanned.path),
                    name: scanned.name.clone(),
                    watched_folder_path: scanned.watched_folder_path.clone(),
                    display_path: scanned.display_path.clone(),
                    resolved_boundaries: resolved_boundaries(
                        sql,
                        &scanned.watched_folder_path,
                        &scanned.path,
                    )?,
                    reason: scanned.invalid_reason.clone().ok_or_else(|| {
                        DbError::Message(format!("scan candidate {} has no reason", scanned.path))
                    })?,
                }))
            }
        })
        .collect()
}

fn resolved_boundaries(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &str,
    candidate_path: &str,
) -> Result<Vec<ResolvedFolderReleaseBoundary>, DbError> {
    Ok(
        load_resolved_boundaries(sql, watched_folder_path, Some(candidate_path))?
            .remove(candidate_path)
            .unwrap_or_default(),
    )
}

/// The picked release as its own archived documents describe it. `None` when
/// the folder is read as its own tags, and when nothing archived the documents
/// behind a release pick.
fn picked_release(
    sql: &SqlReadContext<'_>,
    pick: &IdentityPick,
) -> Result<Option<MatchedRelease>, DbError> {
    let IdentityPick::Release {
        source, release_id, ..
    } = pick
    else {
        return Ok(None);
    };
    let message = |error: crate::import::ImportError| DbError::Message(error.to_string());
    let Some(payloads) =
        load_release_payloads_on(sql, &MetadataRef::new(release_id.clone(), *source))
            .map_err(message)?
    else {
        return Ok(None);
    };
    let detail = payloads.detail().map_err(message)?;
    Ok(Some(MatchedRelease::of_pick(*source, &detail)))
}

/// One candidate, whole, before its runtime is folded in. `None` when the key
/// names no scanned folder — which is what clears a selection.
pub(super) fn load_candidate_detail_on(
    sql: &SqlReadContext<'_>,
    key: &str,
) -> Result<Option<ImportCandidateDetailProjection>, DbError> {
    let Some((_, stored)) = load_item_by_key(sql, key)? else {
        return Ok(None);
    };
    let (candidate, actionable) = match stored.item {
        ScanItem::Valid(candidate) => (candidate, true),
        ScanItem::Discovered(candidate) => (candidate, false),
        ScanItem::Invalid(_) | ScanItem::Boundary(_) => return Ok(None),
    };
    let content_hash = candidate.files.content_hash();

    let relative = crate::import::folder_registry::candidate_relative_path(
        &candidate.watched_folder_path,
        &candidate.path,
    )
    .map_err(|error| DbError::Message(error.to_string()))?;
    let skipped = sql
        .query_row(
            "SELECT 1 FROM skipped_import_candidates \
             WHERE watched_folder_path = ? AND relative_candidate_path = ?",
            params![candidate.watched_folder_path, relative],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    let imported_release = sql
        .query_row(
            "SELECT id, album_id FROM releases WHERE content_hash = ? LIMIT 1",
            params![content_hash],
            |row| {
                Ok(ImportedRelease {
                    release_id: row.get(0)?,
                    album_id: row.get(1)?,
                })
            },
        )
        .optional()?;

    let state = load_states_on(sql, Some(&content_hash))?.remove(&content_hash);
    let current = state.filter(|state| state.file_edits.revision == candidate.file_edit_revision);
    let identify = current.as_ref().and_then(|state| state.identify.as_ref());
    let picked = current
        .as_ref()
        .and_then(|state| state.identity_pick.clone());

    let mut answer = None;
    let mut resumed_identify_state = crate::identify::IdentifyState::Idle;
    let mut matched = None;
    if let Some(identify) = identify {
        let statuses = library_statuses(sql, &identify.verdict)?;
        // The check answers for every release the verdict names, so a lookup
        // cannot miss: `check_releases_in_library_on` returns one status per
        // check and the checks are exactly those releases.
        let status_of = |result: &MetadataResult| {
            statuses
                .iter()
                .find(|status| status.release_id == result.release_id)
                .expect("the library check covers every release the verdict names")
                .clone()
        };
        answer = Some(classify(
            &identify.verdict,
            identify.probed_total_duration_ms,
            &statuses,
        ));
        matched = MatchedRelease::of_summary(&VerdictSummary::of(&identify.verdict));
        resumed_identify_state = identify.verdict.clone().resume_state(&status_of);
    }
    if let Some(pick) = picked.as_ref() {
        matched = picked_release(sql, pick)?;
    }

    Ok(Some(ImportCandidateDetailProjection {
        is_added: imported_release.is_some(),
        candidate,
        actionable,
        skipped,
        resumed_identify_state,
        answer,
        matched,
        picked,
        imported_release,
    }))
}

/// The live library status of every release the verdict names. A release the
/// check does not answer for is a read that must fail rather than a release
/// silently resumed as "not in the library".
fn library_statuses(
    sql: &SqlReadContext<'_>,
    verdict: &TerminalVerdict,
) -> Result<Vec<LibraryStatus>, DbError> {
    let mut seen = HashSet::new();
    let checks: Vec<LibraryCheck> = verdict
        .named_releases()
        .into_iter()
        .filter(|result| seen.insert(result.release_id.clone()))
        .map(LibraryCheck::from)
        .collect();
    check_releases_in_library_on(sql, &checks)
}
