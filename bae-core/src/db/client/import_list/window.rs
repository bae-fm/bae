//! Turning item references into items, and reading one candidate whole.
//!
//! Everything expensive about the import tab lives here: a candidate's
//! resolved boundaries, a boundary's tree, and the archived documents behind a
//! pick. All three are read for the entries inside a requested window and for
//! the one key a selection names — never for the queue.

use super::super::folder_scans::load_resolved_boundaries;
use super::super::identity::check_releases_in_library_on;
use super::super::import_combinations::{load_candidate_on, skipped_on};
use super::super::import_state::{load_pane_rows_on, load_states_on};
use super::super::payloads::load_release_payloads_on;
use super::*;
use crate::identify::{classify, TerminalVerdict, VerdictSummary};
use crate::import::cover_art::{CoverChoice, RemoteCover};
use crate::import::folder_scanner::{
    CategorizedFiles, InvalidCandidate, ResolvedFolderReleaseBoundary,
};
use crate::import::list::{window_refs, Flattened, ImportListItem, ItemRef};
use crate::import::release_candidate::ReleaseCandidate;
use crate::import::search::{ImportSearchReleaseDetail, MetadataResult};
use crate::import::triage::MatchedRelease;
use crate::import::CoverSelection;
use crate::import::MetadataRef;
use crate::library::LibraryPageWindow;
use std::path::PathBuf;

/// The items one window holds, loaded whole.
pub(super) fn materialise(
    sql: &SqlReadContext<'_>,
    window: &LibraryPageWindow,
    flat: &Flattened,
    rows: &ImportQueueRows,
) -> Result<Vec<WindowItemRows>, DbError> {
    window_refs(&flat.items, window)
        .iter()
        .map(|item| match item {
            ItemRef::Header(index) => Ok(WindowItemRows::Ready(flat.headers[*index].item())),
            ItemRef::Candidate {
                index,
                is_group_member,
            } => {
                let placed = &flat.rows[*index];
                let scanned = &rows.candidates[placed.index];
                let mut row = placed.row.clone();
                // A resolved boundary is the row's offer to read its folder
                // the other way, which is a question about a folder nobody has
                // imported yet. Past that point the reading is settled and the
                // row is flat, so the read is not made at all.
                if row.placement.tab() == crate::import::TriageTab::Pending {
                    row.resolved_boundaries =
                        resolved_boundaries(sql, &scanned.watched_folder_path, &scanned.path)?;
                }
                let content_hash = scanned.content_hash.as_deref().ok_or_else(|| {
                    DbError::Message(format!("candidate {} has no content hash", scanned.path))
                })?;
                let selected = rows
                    .states
                    .get(content_hash)
                    .filter(|state| state.edit_revision == scanned.file_edit_revision)
                    .and_then(|state| state.selected_cover.as_ref());
                let files = if row.metadata_provenance.is_some() || selected.is_none() {
                    Some(
                        load_candidate_on(sql, &scanned.path)?
                            .ok_or_else(|| {
                                DbError::Message(format!(
                                    "candidate {} vanished while reading its presentation",
                                    scanned.path
                                ))
                            })?
                            .candidate
                            .files()
                            .clone(),
                    )
                } else {
                    None
                };
                // A decided identity outranks the verdict's lead. Its payloads
                // are fetched in this snapshot and interpreted after release.
                let picked = match row.metadata_provenance.as_ref() {
                    Some(seed) => picked_release(
                        sql,
                        seed,
                        files
                            .as_ref()
                            .expect("a picked candidate has fetched files"),
                    )?,
                    None => None,
                };
                let cover = match selected {
                    Some(selected) => RowCover::Selected(row_cover_source(sql, scanned, selected)?),
                    None => RowCover::Default(files.expect("a default cover has fetched files")),
                };
                Ok(WindowItemRows::Candidate {
                    row,
                    picked,
                    cover,
                    is_group_member: *is_group_member,
                })
            }
            ItemRef::Invalid {
                index,
                is_group_member,
            } => {
                let scanned = &rows.candidates[*index];
                Ok(WindowItemRows::Ready(ImportListItem::Invalid {
                    candidate: InvalidCandidate {
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
                            DbError::Message(format!(
                                "scan candidate {} has no reason",
                                scanned.path
                            ))
                        })?,
                    },
                    is_group_member: *is_group_member,
                }))
            }
        })
        .collect()
}

pub(super) enum WindowItemRows {
    Ready(ImportListItem),
    Candidate {
        row: crate::import::triage::TriageRow,
        picked: Option<PickedReleaseRows>,
        cover: RowCover,
        is_group_member: bool,
    },
}

impl WindowItemRows {
    pub(super) fn process(self) -> Result<ImportListItem, DbError> {
        match self {
            Self::Ready(item) => Ok(item),
            Self::Candidate {
                mut row,
                picked,
                cover,
                is_group_member,
            } => {
                if row.metadata_provenance.is_some() {
                    row.matched = picked.map(PickedReleaseRows::process).transpose()?;
                }
                row.cover_thumbnail = match cover {
                    RowCover::Selected(cover) => Some(cover),
                    RowCover::Default(files) => match row
                        .matched
                        .as_ref()
                        .and_then(|matched| matched.cover_thumbnail_url.as_ref())
                    {
                        Some(url) => {
                            Some(crate::import::CoverImageSource::Remote { url: url.clone() })
                        }
                        None => crate::import::local_artwork::default_local_cover_choice(&files)
                            .map(|choice| choice.thumbnail),
                    },
                };
                Ok(ImportListItem::Candidate {
                    row,
                    is_group_member,
                })
            }
        }
    }
}

pub(super) enum RowCover {
    Selected(crate::import::CoverImageSource),
    Default(CategorizedFiles),
}

pub(super) struct PickedReleaseRows {
    source: MetadataSource,
    payloads: crate::import::payloads::ReleasePayloads,
    files: CategorizedFiles,
}

impl PickedReleaseRows {
    fn process(self) -> Result<MatchedRelease, DbError> {
        let durations = crate::import::probe::source_durations(&self.files)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let audio_durations = crate::import::track_slots::audio_durations(&self.files, &durations)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let detail = self
            .payloads
            .detail_for_audio(&audio_durations)
            .map_err(|error| DbError::Message(error.to_string()))?;
        Ok(MatchedRelease::of_pick(self.source, &detail))
    }
}

fn row_cover_source(
    sql: &SqlReadContext<'_>,
    candidate: &ScanCandidateListRow,
    cover: &CoverSelection,
) -> Result<crate::import::CoverImageSource, DbError> {
    match cover {
        CoverSelection::Remote(url, _) => {
            Ok(crate::import::CoverImageSource::Remote { url: url.clone() })
        }
        CoverSelection::Local(file_id) => {
            let path = sql
                .query_row(
                    "SELECT absolute_path FROM scan_candidate_file \
                     WHERE watched_folder_path = ? AND candidate_path = ? \
                       AND relative_path = ?",
                    params![candidate.watched_folder_path, candidate.path, file_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "candidate {} selects missing cover file {file_id}",
                        candidate.path
                    ))
                })?;
            Ok(crate::import::CoverImageSource::Local {
                path: PathBuf::from(path),
            })
        }
        CoverSelection::Embedded(source_file_id) => {
            let snapshot = super::super::folder_scans::load_candidate_file_tag_snapshot(
                sql,
                &candidate.watched_folder_path,
                &candidate.path,
            )?
            .and_then(|stored| stored.snapshot)
            .ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {} selects embedded cover without a File Tags snapshot",
                    candidate.path
                ))
            })?;
            let cover = snapshot.embedded_cover.ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {} selects embedded cover without stored artwork",
                    candidate.path
                ))
            })?;
            if cover.source_relative_path != *source_file_id {
                return Err(DbError::Message(format!(
                    "candidate {} selects embedded cover from {source_file_id}, but its snapshot stores {}",
                    candidate.path, cover.source_relative_path
                )));
            }
            Ok(crate::import::CoverImageSource::Bytes { data: cover.data })
        }
    }
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
    pick: &MetadataProvenance,
    files: &CategorizedFiles,
) -> Result<Option<PickedReleaseRows>, DbError> {
    let MetadataProvenance::ExternalRelease {
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
    Ok(Some(PickedReleaseRows {
        source: *source,
        payloads,
        files: files.clone(),
    }))
}

/// One candidate, whole, before its runtime is folded in. `None` when the key
/// names no scanned folder — which is what clears a selection.
pub(super) fn load_candidate_detail_on(
    sql: &SqlReadContext<'_>,
    key: &str,
) -> Result<
    Option<impl FnOnce() -> Result<ImportCandidateDetailProjection, DbError> + Send + 'static>,
    DbError,
> {
    let Some(stored) = load_candidate_on(sql, key)? else {
        return Ok(None);
    };
    let actionable = stored.actionable;
    let source_error = stored.error;
    let candidate = stored.candidate;
    let content_hash = candidate.files().content_hash();

    let skipped = skipped_on(sql, &candidate)?;

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
    let current = state.filter(|state| state.file_edits.revision == candidate.file_edit_revision());
    let identify = current.as_ref().and_then(|state| state.identify.as_ref());
    let picked = current
        .as_ref()
        .and_then(|state| state.metadata_provenance.clone());
    let signals = current.as_ref().and_then(|state| state.signals.clone());
    let lookup_choices = current
        .as_ref()
        .map(|state| state.lookup_choices.clone())
        .unwrap_or_default();
    let pane_rows = load_pane_rows_on(sql, &content_hash)?;
    let (initial_metadata_source, metadata_revision) = sql.query_row(
        "SELECT c.initial_metadata_source, s.metadata_revision \
             FROM scan_candidate c JOIN import_candidate_state s \
               ON s.content_hash = c.content_hash \
             WHERE c.watched_folder_path = ? AND c.path = ?",
        params![candidate.watched_folder_path(), candidate.key()],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let initial_metadata_source = initial_metadata_source.parse().map_err(DbError::Message)?;
    let metadata_revision = u64::try_from(metadata_revision)
        .map_err(|_| DbError::Message("candidate metadata revision is negative".to_string()))?;

    let statuses = identify
        .map(|identify| library_statuses(sql, &identify.verdict))
        .transpose()?;
    // Only identity keys are needed for the next SQL query. Track and artwork
    // processing runs after the snapshot ends.
    let payloads = payloads_for_pane_on(sql, &candidate, picked.as_ref())?;
    let picked_library_status = match payloads.as_ref() {
        Some(payloads) => {
            let check = payloads
                .library_check()
                .map_err(|error| DbError::Message(error.to_string()))?;
            check_releases_in_library_on(sql, &[check])?
                .into_iter()
                .next()
        }
        None => None,
    };
    let embedded_cover = match pane_rows.cover.as_ref() {
        Some(CoverSelection::Embedded(source_file_id)) => {
            let snapshot = super::super::folder_scans::load_candidate_file_tag_snapshot(
                sql,
                candidate.watched_folder_path(),
                &candidate.key(),
            )?
            .and_then(|stored| stored.snapshot)
            .ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {} selects embedded cover without a File Tags snapshot",
                    candidate.key()
                ))
            })?;
            let cover = snapshot.embedded_cover.ok_or_else(|| {
                DbError::Message(format!(
                    "candidate {} selects embedded cover without stored artwork",
                    candidate.key()
                ))
            })?;
            if cover.source_relative_path != *source_file_id {
                return Err(DbError::Message(format!(
                    "candidate {} selects embedded cover from {source_file_id}, but its snapshot stores {}",
                    candidate.key(), cover.source_relative_path
                )));
            }
            Some(cover)
        }
        _ => None,
    };
    Ok(Some(move || {
        let durations = crate::import::probe::source_durations(candidate.files())
            .map_err(|error| DbError::Message(error.to_string()))?;
        let release = payloads
            .map(|payloads| {
                let audio_durations =
                    crate::import::track_slots::audio_durations(candidate.files(), &durations)
                        .map_err(|error| DbError::Message(error.to_string()))?;
                payloads
                    .detail_for_audio(&audio_durations)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .transpose()?;
        let mut answer = None;
        let mut resumed_identify_state = crate::identify::IdentifyState::Idle;
        let mut matched = None;
        let identify = current.as_ref().and_then(|state| state.identify.as_ref());
        if let Some(identify) = identify {
            let statuses = statuses
                .as_ref()
                .expect("identification has fetched library statuses");
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
                statuses,
            ));
            matched = MatchedRelease::of_summary(&VerdictSummary::of(&identify.verdict));
            resumed_identify_state = identify
                .verdict
                .clone()
                .resume_state(signals.as_ref(), &lookup_choices, &status_of);
        }
        if picked.is_some() {
            matched = release
                .as_ref()
                .map(|release| MatchedRelease::of_pick(release.source, release));
        }
        let pane = crate::import::pane::draft_pane(
            release,
            candidate.files(),
            &durations,
            &pane_rows.draft,
            picked.as_ref(),
        );
        let remote_covers = pane
            .release
            .as_ref()
            .map(|release| release.cover_art.clone())
            .unwrap_or_default();
        let cover = chosen_cover(
            candidate.files(),
            pane_rows.cover.as_ref(),
            pane.release.as_ref(),
            embedded_cover.as_ref(),
        );
        Ok(ImportCandidateDetailProjection {
            is_added: imported_release.is_some(),
            candidate,
            source_error,
            actionable,
            skipped,
            resumed_identify_state,
            answer,
            matched,
            metadata_provenance: picked,
            metadata_revision,
            initial_metadata_source,
            imported_release,
            release: pane.release,
            picked_library_status,
            metadata_draft: pane.edit,
            mapping: pane.mapping,
            cover,
            remote_covers,
            signals,
            lookup_choices,
            failure: pane_rows.failure,
            session: pane_rows.session,
        })
    }))
}

/// Read the picked release documents in the pane's SQL snapshot.
fn payloads_for_pane_on(
    sql: &SqlReadContext<'_>,
    candidate: &ReleaseCandidate,
    picked: Option<&MetadataProvenance>,
) -> Result<Option<crate::import::payloads::ReleasePayloads>, DbError> {
    let release = match picked {
        Some(MetadataProvenance::ExternalRelease {
            source, release_id, ..
        }) => {
            let release = MetadataRef::new(release_id.clone(), *source);
            // A stored pick always has readable documents: the pick write
            // archives them first. Serving half a pane instead would hide the
            // break rather than state it.
            let payloads = load_release_payloads_on(sql, &release)
                .map_err(|error| DbError::Message(error.to_string()))?
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "{} is picked for {} but nothing stored its lookups",
                        release.id,
                        candidate.key()
                    ))
                })?;
            Some(payloads)
        }
        Some(MetadataProvenance::FileTags) | None => None,
    };
    Ok(release)
}

/// The cover the candidate commits with: its selection, the picked release's
/// default, or the folder's default image. A selection naming an image the
/// folder no longer holds falls back through the same source-neutral order.
fn chosen_cover(
    files: &CategorizedFiles,
    chosen: Option<&CoverSelection>,
    release: Option<&ImportSearchReleaseDetail>,
    embedded_cover: Option<&crate::import::file_tag_snapshot::EmbeddedCoverFact>,
) -> Option<CoverChoice> {
    let default = || {
        release
            .and_then(|release| release.default_cover())
            .map(CoverChoice::remote)
            .or_else(|| crate::import::local_artwork::default_local_cover_choice(files))
    };
    match chosen {
        None => default(),
        Some(CoverSelection::Local(file_id)) => files
            .artwork()
            .find(|image| &image.relative_path == file_id)
            .map(|image| CoverChoice::local(file_id.clone(), image.path.clone()))
            .or_else(default),
        Some(CoverSelection::Embedded(source_file_id)) => embedded_cover
            .filter(|cover| &cover.source_relative_path == source_file_id)
            .map(|cover| CoverChoice::embedded(source_file_id.clone(), cover.data.clone())),
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
