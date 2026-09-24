//! Turning item references into items, and reading one candidate whole.
//!
//! Everything expensive about the import tab lives here: a candidate's
//! resolved boundaries, a boundary's tree, and the archived documents behind a
//! pick. All three are read for the entries inside a requested window and for
//! the one key a selection names — never for the queue.

use super::super::folder_scans::load_resolved_boundaries;
use super::super::import_combinations::{load_candidate_on, skipped_on};
use super::super::import_state::{load_pane_rows_on, load_states_on};
use super::super::payloads::load_release_payloads_on;
use super::super::records::check_releases_in_library_on;
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
                let files = if row.metadata_provenance.is_some() {
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
                let cover = selected
                    .map(|selected| row_cover_source(sql, scanned, selected))
                    .transpose()?;
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
        /// The candidate's stored cover selection, as the row draws it.
        /// Nothing stands in for an empty one: the row shows what the
        /// candidate would commit with.
        cover: Option<crate::import::CoverImageSource>,
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
                match picked {
                    Some(picked) => {
                        let PickedRelease { matched, records } = picked.process()?;
                        row.matched = matched;
                        // The reading the queue placed the row with named no
                        // records; this is where the documents are read.
                        row.reading = crate::import::triage::TriageReading::of(
                            row.metadata_summary.as_ref(),
                            row.metadata_provenance.as_ref(),
                            records,
                        );
                    }
                    // File metadata names no external release, so nothing leads
                    // the row: the verdict's lead does not stand in for a pick.
                    None if row.metadata_provenance.is_some() => row.matched = None,
                    None => {}
                }
                row.cover_thumbnail = cover;
                Ok(ImportListItem::Candidate {
                    row,
                    is_group_member,
                })
            }
        }
    }
}

pub(super) struct PickedReleaseRows {
    /// Every release the pick claims, the primary first and then its partners,
    /// each with the documents archived for it. A release nothing archived
    /// documents for is still listed: the pick claims it either way, and the
    /// row says so.
    claimed: Vec<(
        MetadataRef,
        Option<crate::import::payloads::ReleasePayloads>,
    )>,
    files: CategorizedFiles,
}

/// What the picked documents say: the release the row leads with, and every
/// catalog that describes it.
pub(super) struct PickedRelease {
    matched: Option<MatchedRelease>,
    records: Vec<crate::import::ReleaseRecord>,
}

impl PickedReleaseRows {
    fn process(self) -> Result<PickedRelease, DbError> {
        let durations = crate::import::probe::source_durations(&self.files)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let audio_durations = crate::import::track_slots::audio_durations(&self.files, &durations)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let records = crate::import::payloads::claimed_records(&self.claimed)
            .map_err(|error| DbError::Message(error.to_string()))?;
        // Only the release the draft was read from states the row's facts; a
        // partner's own document is not a second set of them. Its artwork is
        // another matter: a row's cover is the pick's, so the partners go to
        // the detail that carries the cover options.
        let mut claimed = self.claimed.into_iter();
        let (primary, payloads) = claimed.next().expect("a pick claims at least its primary");
        let partners: Vec<_> = claimed.filter_map(|(_, payloads)| payloads).collect();
        let matched = payloads
            .map(|payloads| payloads.detail_for_audio(&audio_durations, &partners))
            .transpose()
            .map_err(|error| DbError::Message(error.to_string()))?
            .map(|detail| MatchedRelease::of_pick(primary.catalog, &detail));
        Ok(PickedRelease { matched, records })
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
                    "candidate {} selects embedded cover without a file-tag snapshot",
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

/// Every release a pick claims, with the documents archived for each. `None`
/// when the folder is read as its own tags, which claims no release at all.
fn picked_release(
    sql: &SqlReadContext<'_>,
    pick: &MetadataProvenance,
    files: &CategorizedFiles,
) -> Result<Option<PickedReleaseRows>, DbError> {
    let MetadataProvenance::ExternalRelease { record, partners } = pick else {
        return Ok(None);
    };
    let message = |error: crate::import::ImportError| DbError::Message(error.to_string());
    let claimed = std::iter::once(record.clone())
        .chain(partners.iter().cloned())
        .map(|release| {
            let payloads = load_release_payloads_on(sql, &release).map_err(message)?;
            Ok((release, payloads))
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(Some(PickedReleaseRows {
        claimed,
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
    let metadata_author = current
        .as_ref()
        .map_or(crate::import::MetadataAuthor::Nobody, |state| {
            state.metadata_author
        });
    let signals = current.as_ref().and_then(|state| state.signals.clone());
    let lookup_choices = current
        .as_ref()
        .map(|state| state.lookup_choices.clone())
        .unwrap_or_default();
    let pane_rows = load_pane_rows_on(sql, &content_hash)?;
    let metadata_revision = sql.query_row(
        "SELECT s.metadata_revision \
             FROM scan_candidate c JOIN import_candidate_state s \
               ON s.content_hash = c.content_hash \
             WHERE c.watched_folder_path = ? AND c.path = ?",
        params![candidate.watched_folder_path(), candidate.key()],
        |row| row.get::<_, i64>(0),
    )?;
    let metadata_revision = u64::try_from(metadata_revision)
        .map_err(|_| DbError::Message("candidate metadata revision is negative".to_string()))?;

    let statuses = identify
        .map(|identify| library_statuses(sql, &identify.verdict))
        .transpose()?;
    // Only identity keys are needed for the next SQL query. Track and artwork
    // processing runs after the snapshot ends.
    let claimed = claimed_payloads_on(sql, &candidate, picked.as_ref())?;
    // Every catalog record named by the picked source documents.
    let records = crate::import::payloads::claimed_records(
        &claimed
            .iter()
            .map(|payloads| (payloads.release().clone(), Some(payloads.clone())))
            .collect::<Vec<_>>(),
    )
    .map_err(|error| DbError::Message(error.to_string()))?;
    let picked_library_status = match claimed.first() {
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
                    "candidate {} selects embedded cover without a file-tag snapshot",
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
        let audio_durations =
            crate::import::track_slots::audio_durations(candidate.files(), &durations)
                .map_err(|error| DbError::Message(error.to_string()))?;
        let release = claimed
            .split_first()
            .map(|(primary, partners)| {
                primary
                    .detail_for_audio(&audio_durations, partners)
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
            answer = Some(classify(&identify.verdict));
            matched = MatchedRelease::of_summary(&VerdictSummary::of(&identify.verdict));
            // The candidate's own text is what the rows are judged and ordered
            // against, live or resumed, with the numbers the person struck out
            // of it. Both are the candidate's rather than the run's, so the
            // ranking is this read's, not the run's: striking a number out
            // re-orders the rows the next time they are read, with nothing
            // asked again. A candidate whose extraction never stored any text
            // offers its rows unranked rather than none.
            let text =
                signals
                    .as_ref()
                    .map_or_else(crate::identify::CandidateText::default, |signals| {
                        crate::identify::CandidateText::of(
                            &signals.text_pool,
                            &lookup_choices.discounted_catalogs,
                        )
                    });
            resumed_identify_state = identify.verdict.clone().resume_state(&status_of, text);
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
            &crate::import::CandidateAsRead {
                content_hash,
                file_edit_revision: candidate.file_edit_revision(),
                metadata_revision,
            },
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
            metadata_author,
            metadata_revision,
            imported_release,
            release: pane.release,
            records,
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

/// Read the documents of every release the pick claims in the pane's SQL
/// snapshot, the primary first and then its partners.
///
/// The primary's documents are what the draft was read from and what the pane
/// leads with; together with the partners' they are what the release's
/// records are read off. A stored pick always has readable documents — the
/// pick write archives them first, for the primary and every partner alike —
/// so a missing one is stated rather than served as half a pane.
fn claimed_payloads_on(
    sql: &SqlReadContext<'_>,
    candidate: &ReleaseCandidate,
    picked: Option<&MetadataProvenance>,
) -> Result<Vec<crate::import::payloads::ReleasePayloads>, DbError> {
    let Some(MetadataProvenance::ExternalRelease { record, partners }) = picked else {
        return Ok(Vec::new());
    };
    std::iter::once(record.clone())
        .chain(partners.iter().cloned())
        .map(|release| {
            load_release_payloads_on(sql, &release)
                .map_err(|error| DbError::Message(error.to_string()))?
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "{} is claimed for {} but nothing stored its lookups",
                        release.key,
                        candidate.key()
                    ))
                })
        })
        .collect()
}

/// The cover the candidate commits with: its stored selection, read as it
/// stands. A selection naming an image the folder no longer holds describes
/// the candidate no longer, and no other image stands in for it.
fn chosen_cover(
    files: &CategorizedFiles,
    chosen: Option<&CoverSelection>,
    release: Option<&ImportSearchReleaseDetail>,
    embedded_cover: Option<&crate::import::file_tag_snapshot::EmbeddedCoverFact>,
) -> Option<CoverChoice> {
    match chosen {
        None => None,
        Some(CoverSelection::Local(file_id)) => {
            let image = files
                .artwork()
                .find(|image| &image.relative_path == file_id);
            if image.is_none() {
                tracing::warn!(
                    file_id,
                    "the selected cover is no longer among the candidate's images"
                );
            }
            image.map(|image| CoverChoice::local(file_id.clone(), image.path.clone()))
        }
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
    sql: &impl super::super::query::QueryOne,
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
