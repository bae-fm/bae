//! Turning item references into items, and reading one candidate whole.
//!
//! Everything expensive about the import tab lives here: a candidate's
//! resolved boundaries, a boundary's tree, and the archived documents behind a
//! pick. All three are read for entries inside a requested window or the visible
//! album editor. Bulk selection uses its own facts query. Provider parsing and
//! audio projection run over these owned inputs after the read returns.

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
use crate::import::mapping::MappingTable;
use crate::import::probe::SourceDurations;
use crate::import::release_candidate::ReleaseCandidate;
use crate::import::search::{ImportSearchReleaseDetail, MetadataResult};
use crate::import::triage::MatchedRelease;
use crate::import::MetadataRef;
use crate::import::{CoverSelection, RawReleaseEdit};
use crate::library::LibraryPageWindow;
use std::path::PathBuf;

/// Owned window input; provider models and audio projections are constructed on
/// the processing worker after the transaction has returned.
pub(super) enum WindowItemRows {
    Projected(ImportListItem),
    Candidate {
        row: crate::import::TriageRow,
        is_group_member: bool,
        files: Option<CategorizedFiles>,
        payloads: Option<crate::import::payloads::ReleasePayloads>,
        selected_cover: Option<crate::import::CoverImageSource>,
    },
}

pub(super) fn materialise(
    sql: &SqlReadContext<'_>,
    window: &LibraryPageWindow,
    flat: &Flattened,
    rows: &ImportQueueRows,
) -> Result<Vec<WindowItemRows>, DbError> {
    window_refs(&flat.items, window)
        .iter()
        .map(|item| match item {
            ItemRef::Header(index) => Ok(WindowItemRows::Projected(flat.headers[*index].item())),
            ItemRef::Candidate {
                index,
                is_group_member,
            } => {
                let placed = &flat.rows[*index];
                let scanned = &rows.candidates[placed.index];
                let mut row = placed.row.clone();
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
                let selected_cover = selected
                    .map(|cover| row_cover_source(sql, scanned, cover))
                    .transpose()?;
                let payloads = match row.metadata_provenance.as_ref() {
                    Some(MetadataProvenance::ExternalRelease {
                        source, release_id, ..
                    }) => load_release_payloads_on(
                        sql,
                        &MetadataRef::new(release_id.clone(), *source),
                    )
                    .map_err(|error| DbError::Message(error.to_string()))?,
                    Some(MetadataProvenance::FileTags) | None => None,
                };
                let needs_files = row.metadata_provenance.is_some()
                    || (selected.is_none()
                        && row
                            .matched
                            .as_ref()
                            .and_then(|matched| matched.cover_thumbnail_url.as_ref())
                            .is_none());
                let files = if needs_files {
                    Some(
                        load_candidate_on(sql, &row.candidate_key)?
                            .ok_or_else(|| {
                                DbError::Message(format!(
                                    "candidate {} vanished while reading its list files",
                                    row.candidate_key
                                ))
                            })?
                            .candidate
                            .files()
                            .clone(),
                    )
                } else {
                    None
                };
                Ok(WindowItemRows::Candidate {
                    row,
                    is_group_member: *is_group_member,
                    files,
                    payloads,
                    selected_cover,
                })
            }
            ItemRef::Invalid {
                index,
                is_group_member,
            } => {
                let scanned = &rows.candidates[*index];
                Ok(WindowItemRows::Projected(ImportListItem::Invalid {
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

pub(super) fn process_window(
    rows: Vec<WindowItemRows>,
    parsed: &mut HashMap<MetadataRef, crate::import::payloads::ParsedReleasePayloads>,
) -> Result<Vec<ImportListItem>, DbError> {
    rows.into_iter()
        .map(|item| {
            let WindowItemRows::Candidate {
                mut row,
                is_group_member,
                files,
                payloads,
                selected_cover,
            } = item
            else {
                let WindowItemRows::Projected(item) = item else {
                    unreachable!()
                };
                return Ok(item);
            };
            if let Some(pick) = &row.metadata_provenance {
                let files = files
                    .as_ref()
                    .expect("a picked row fetches its audio files");
                let durations = crate::import::probe::source_durations(files)
                    .map_err(|error| DbError::Message(error.to_string()))?;
                let audio = crate::import::track_slots::audio_durations(files, &durations)
                    .map_err(|error| DbError::Message(error.to_string()))?;
                row.matched = match (pick, payloads) {
                    (
                        MetadataProvenance::ExternalRelease {
                            source, release_id, ..
                        },
                        Some(raw),
                    ) => {
                        let key = MetadataRef::new(release_id.clone(), *source);
                        let documents = match parsed.entry(key) {
                            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(
                                raw.parse()
                                    .map_err(|error| DbError::Message(error.to_string()))?,
                            ),
                        };
                        let detail = documents
                            .detail_for_audio(&audio)
                            .map_err(|error| DbError::Message(error.to_string()))?;
                        Some(MatchedRelease::of_pick(*source, &detail))
                    }
                    _ => None,
                };
            }
            row.cover_thumbnail = selected_cover
                .or_else(|| {
                    row.matched
                        .as_ref()
                        .and_then(|matched| matched.cover_thumbnail_url.as_ref())
                        .map(|url| crate::import::CoverImageSource::Remote { url: url.clone() })
                })
                .or_else(|| {
                    files
                        .as_ref()
                        .and_then(crate::import::local_artwork::default_local_cover_choice)
                        .map(|cover| cover.thumbnail)
                });
            Ok(ImportListItem::Candidate {
                row,
                is_group_member,
            })
        })
        .collect()
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

/// Owned input from one tracked SQLite transaction. Provider projection and
/// mapping execute after releasing the read worker and its transaction.
pub(super) struct CandidateDetailRows {
    stored: super::super::import_combinations::StoredReleaseCandidate,
    current: Option<DbImportCandidateState>,
    pane_rows: DbCandidatePaneRows,
    skipped: bool,
    imported_release: Option<ImportedRelease>,
    initial_metadata_source: crate::config::DefaultImportMetadataSource,
    metadata_revision: u64,
    payloads: Option<crate::import::payloads::ReleasePayloads>,
    statuses: Vec<LibraryStatus>,
    picked_library_status: Option<LibraryStatus>,
    embedded_cover: Option<crate::import::file_tag_snapshot::EmbeddedCoverFact>,
}

pub(super) fn load_candidate_detail_rows_on(
    sql: &SqlReadContext<'_>,
    key: &str,
) -> Result<Option<CandidateDetailRows>, DbError> {
    let Some(stored) = load_candidate_on(sql, key)? else {
        return Ok(None);
    };
    let candidate = &stored.candidate;
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

    let statuses = match identify {
        Some(identify) => library_statuses(sql, &identify.verdict)?,
        None => Vec::new(),
    };
    let (payloads, picked_library_status) = match picked.as_ref() {
        Some(MetadataProvenance::ExternalRelease {
            source, release_id, ..
        }) => {
            let release = MetadataRef::new(release_id.clone(), *source);
            let payloads = load_release_payloads_on(sql, &release)
                .map_err(|error| DbError::Message(error.to_string()))?
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "{} is picked for {} but nothing stored its lookups",
                        release.id,
                        candidate.key()
                    ))
                })?;
            let status = check_releases_in_library_on(
                sql,
                &[LibraryCheck {
                    source: *source,
                    release_id: payloads.source_release_id().to_string(),
                    source_group_id: payloads.source_group_id().map(str::to_string),
                }],
            )?
            .into_iter()
            .next();
            (Some(payloads), status)
        }
        Some(MetadataProvenance::FileTags) | None => (None, None),
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
    Ok(Some(CandidateDetailRows {
        stored,
        current,
        pane_rows,
        skipped,
        imported_release,
        initial_metadata_source,
        metadata_revision,
        payloads,
        statuses,
        picked_library_status,
        embedded_cover,
    }))
}

pub(super) fn process_candidate_detail(
    rows: Option<CandidateDetailRows>,
) -> Result<Option<ImportCandidateDetailProjection>, DbError> {
    let Some(CandidateDetailRows {
        stored,
        current,
        pane_rows,
        skipped,
        imported_release,
        initial_metadata_source,
        metadata_revision,
        payloads,
        statuses,
        picked_library_status,
        embedded_cover,
    }) = rows
    else {
        return Ok(None);
    };
    let actionable = stored.actionable;
    let source_error = stored.error;
    let candidate = stored.candidate;
    let identify = current.as_ref().and_then(|state| state.identify.as_ref());
    let picked = current
        .as_ref()
        .and_then(|state| state.metadata_provenance.clone());
    let signals = current.as_ref().and_then(|state| state.signals.clone());
    let durations = crate::import::probe::source_durations(candidate.files())
        .map_err(|error| DbError::Message(error.to_string()))?;
    let mut answer = None;
    let mut resumed_identify_state = crate::identify::IdentifyState::Idle;
    let mut matched = None;
    if let Some(identify) = identify {
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
    let pane = pane_of(
        payloads.as_ref(),
        &candidate,
        picked.as_ref(),
        &durations,
        &pane_rows,
    )?;
    if picked.is_some() {
        matched = pane
            .release
            .as_ref()
            .map(|release| MatchedRelease::of_pick(release.source, release));
    }
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
    Ok(Some(ImportCandidateDetailProjection {
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
        metadata_draft_valid: pane_rows.draft.release_edit().shape().is_ok(),
        metadata_draft: pane.edit,
        mapping: pane.mapping,
        cover,
        remote_covers,
        signals,
        failure: pane_rows.failure,
        session: pane_rows.session,
    }))
}

/// What the pick produces for the pane. A folder with no pick still gets its
/// table — the roles say what every file becomes, and only the tracks are the
/// open question.
struct PaneValue {
    release: Option<ImportSearchReleaseDetail>,
    edit: RawReleaseEdit,
    mapping: MappingTable,
}

fn pane_of(
    payloads: Option<&crate::import::payloads::ReleasePayloads>,
    candidate: &ReleaseCandidate,
    picked: Option<&MetadataProvenance>,
    durations: &SourceDurations,
    rows: &DbCandidatePaneRows,
) -> Result<PaneValue, DbError> {
    let release = payloads
        .map(|payloads| {
            let parsed = payloads
                .parse()
                .map_err(|error| DbError::Message(error.to_string()))?;
            parsed
                .detail_for_audio(
                    &crate::import::track_slots::audio_durations(candidate.files(), durations)
                        .map_err(|error| DbError::Message(error.to_string()))?,
                )
                .map_err(|error| DbError::Message(error.to_string()))
        })
        .transpose()?;
    let pick =
        crate::import::pane::draft_pane(release, candidate.files(), durations, &rows.draft, picked);
    Ok(PaneValue {
        release: pick.release,
        edit: pick.edit,
        mapping: pick.mapping,
    })
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
