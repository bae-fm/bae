//! Turning item references into items, and reading one candidate whole.
//!
//! Everything expensive about the import tab lives here: a candidate's
//! resolved boundaries, a boundary's tree, and the archived documents behind a
//! pick. All three are read for the entries inside a requested window and for
//! the one key a selection names — never for the queue.

use super::super::folder_scans::{load_item_by_key, load_resolved_boundaries};
use super::super::identity::check_releases_in_library_on;
use super::super::import_state::{load_pane_rows_on, load_states_on};
use super::super::payloads::load_release_payloads_on;
use super::*;
use crate::identify::{classify, TerminalVerdict, VerdictSummary};
use crate::import::cover_art::{CoverChoice, RemoteCover};
use crate::import::folder_scanner::{
    CategorizedFiles, FolderCandidate, InvalidCandidate, ResolvedFolderReleaseBoundary, ScanItem,
};
use crate::import::list::{window_refs, Flattened, ImportListItem, ItemRef};
use crate::import::mapping::MappingTable;
use crate::import::probe::ProbedDurations;
use crate::import::search::{ImportSearchReleaseDetail, MetadataResult};
use crate::import::triage::MatchedRelease;
use crate::import::MetadataRef;
use crate::import::{AudioFile, CoverSelection, RawReleaseEdit};
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
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<Option<ImportCandidateDetailProjection>, DbError> {
    let Some((_, stored)) = load_item_by_key(sql, key)? else {
        return Ok(None);
    };
    let (candidate, actionable) = match stored.item {
        ScanItem::Valid(candidate) => (candidate, true),
        ScanItem::Discovered(candidate) => (candidate, false),
        ScanItem::Invalid(_) | ScanItem::Decided { .. } => return Ok(None),
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
    let durations = current
        .as_ref()
        .map(|state| state.durations.clone())
        .unwrap_or_default();
    let signals = current.as_ref().and_then(|state| state.signals.clone());
    let pane_rows = load_pane_rows_on(sql, &content_hash)?;

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

    let pane = pane_of(
        sql,
        &candidate,
        picked.as_ref(),
        &durations,
        &pane_rows,
        clock,
        ids,
    )?;
    let picked_library_status = match pane.release.as_ref() {
        Some(release) => check_releases_in_library_on(
            sql,
            &[LibraryCheck {
                source: release.source,
                release_id: release.release_id.clone(),
                source_group_id: release.source_group_id.clone(),
            }],
        )?
        .into_iter()
        .next(),
        None => None,
    };
    let remote_covers = pane
        .release
        .as_ref()
        .map(|release| release.cover_art.clone())
        .unwrap_or_default();
    let cover = chosen_cover(
        &candidate.files,
        pane_rows.cover.as_ref(),
        pane.release.as_ref(),
    );
    let unprobed = unprobed_units(&candidate.files, picked.is_some(), &durations);

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
        release: pane.release,
        picked_library_status,
        edit: pane.edit,
        mapping: pane.mapping,
        unprobed,
        cover,
        remote_covers,
        signals,
        failure: pane_rows.failure,
    }))
}

/// What the pick produces for the pane. A folder with no pick still gets its
/// table — the roles say what every file becomes, and only the tracks are the
/// open question.
struct PaneValue {
    release: Option<ImportSearchReleaseDetail>,
    edit: Option<RawReleaseEdit>,
    mapping: MappingTable,
}

#[allow(clippy::too_many_arguments)]
fn pane_of(
    sql: &SqlReadContext<'_>,
    candidate: &FolderCandidate,
    picked: Option<&IdentityPick>,
    durations: &ProbedDurations,
    rows: &DbCandidatePaneRows,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<PaneValue, DbError> {
    let message = |error: crate::import::ImportError| DbError::Message(error.to_string());
    let Some(picked) = picked else {
        return Ok(PaneValue {
            release: None,
            edit: None,
            mapping: crate::import::pane::unpicked_mapping(&candidate.files, durations),
        });
    };
    let pick = match picked {
        IdentityPick::Release {
            source, release_id, ..
        } => {
            let release = MetadataRef::new(release_id.clone(), *source);
            // A stored pick always has readable documents: the pick write
            // archives them first. Serving half a pane instead would hide the
            // break rather than state it.
            let payloads = load_release_payloads_on(sql, &release)
                .map_err(message)?
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "{} is picked for {} but nothing stored its lookups",
                        release.id,
                        candidate.path.display()
                    ))
                })?;
            crate::import::pane::release_pane(
                &payloads,
                &candidate.files,
                durations,
                &rows.edit,
                &rows.track_edits,
                clock,
                ids,
            )
            .map_err(message)?
        }
        IdentityPick::Unknown => crate::import::pane::unknown_pane(
            &candidate.files,
            Some(&candidate.name),
            durations,
            &rows.edit,
            &rows.track_edits,
            clock,
            ids,
        )
        .map_err(message)?,
    };
    Ok(PaneValue {
        release: pick.release,
        edit: Some(pick.edit),
        mapping: pick.mapping,
    })
}

/// The cover the candidate commits with: the one chosen, else the picked
/// release's default. A choice naming an image the folder no longer holds
/// falls back the same way — the folder moved under the choice.
fn chosen_cover(
    files: &CategorizedFiles,
    chosen: Option<&CoverSelection>,
    release: Option<&ImportSearchReleaseDetail>,
) -> Option<CoverChoice> {
    let default = || {
        release
            .and_then(|release| release.default_cover())
            .map(CoverChoice::remote)
    };
    match chosen {
        None => default(),
        Some(CoverSelection::Local(file_id)) => files
            .artwork()
            .find(|image| &image.relative_path == file_id)
            .map(|image| CoverChoice::local(file_id.clone(), image.path.clone()))
            .or_else(default),
        Some(CoverSelection::Remote(url, source)) => {
            let matching = release
                .into_iter()
                .flat_map(|release| release.cover_art.iter())
                .find(|cover| &cover.url == url);
            Some(match matching {
                Some(cover) => CoverChoice::remote(cover),
                // The chosen address is no longer one the release offers, but
                // it is still the address the user picked, so it is still what
                // this import commits with.
                None => CoverChoice::remote(&RemoteCover {
                    url: url.clone(),
                    thumbnail_url: url.clone(),
                    label: source.cover_source_label().to_string(),
                    source: *source,
                }),
            })
        }
    }
}

/// The folder's audio units nothing has measured. Asked only under a pick: a
/// table with no tracks shows no lengths and so asks for none.
fn unprobed_units(
    files: &CategorizedFiles,
    picked: bool,
    durations: &ProbedDurations,
) -> Vec<AudioFile> {
    if !picked {
        return Vec::new();
    }
    crate::import::track_slots::audio_units(files)
        .into_iter()
        .filter(|unit| durations.duration_of(unit).is_none())
        .collect()
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
