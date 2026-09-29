//! Turning item references into items, and reading one candidate whole.
//!
//! Everything expensive about the import tab lives here: a candidate's
//! resolved boundaries, a boundary's tree, and the fetched releases behind a
//! pick. All three are read for the entries inside a requested window and for
//! the one key a selection names — never for the queue.

use super::super::release_groupings::{load_candidate_on, skipped_on};
use super::super::import_state::{load_pane_rows_on, load_states_on};
use super::super::source_releases::load_source_release_on;
use super::super::records::check_releases_in_library_on;
use super::*;
use crate::identify::{TerminalVerdict, VerdictSummary};
use crate::import::cover_art::CoverChoice;
use crate::import::folder_scanner::{CategorizedFiles, FileVersion, InvalidCandidate};
use crate::import::list::{window_refs, Flattened, ImportListItem, ItemRef};
use crate::import::folder_scanner::FolderCandidate;
use crate::import::search::MetadataResult;
use crate::import::triage::MatchedRelease;
use crate::import::{CoverSelection, PressingLink, ReleaseLink};
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
                // A Done row is the library release it became, read from the
                // library: nothing the candidate says — its draft, its pick,
                // its cover — is read for it.
                if placed.row.placement == crate::import::TriagePlacement::Done {
                    return Ok(WindowItemRows::Ready(ImportListItem::Imported {
                        row: imported_row(sql, &placed.row)?,
                        live: (),
                    }));
                }
                let row = placed.row.clone();
                let content_hash = scanned.content_hash.as_deref().ok_or_else(|| {
                    DbError::Message(format!("candidate {} has no content hash", scanned.path))
                })?;
                // Only a row inside a window shows its cover, so only such a
                // row's cover is read.
                let selected = match rows
                    .states
                    .get(content_hash)
                    .filter(|state| state.edit_revision == scanned.file_edit_revision)
                {
                    Some(_) => super::super::import_state::load_covers_on(sql, Some(content_hash))?
                        .remove(content_hash),
                    None => None,
                };
                let pressing = row.release_link.as_ref().and_then(ReleaseLink::pressing);
                let files = if pressing.is_some() {
                    Some(
                        load_candidate_on(sql, &scanned.path)?
                            .ok_or_else(|| {
                                DbError::Message(format!(
                                    "candidate {} vanished while reading its presentation",
                                    scanned.path
                                ))
                            })?
                            .candidate
                            .files
                            .clone(),
                    )
                } else {
                    None
                };
                // A linked release outranks the verdict's lead. A pressing's
                // releases are read in this snapshot and interpreted after it
                // ends; an album names its records itself.
                let picked = match row.release_link.as_ref() {
                    Some(ReleaseLink::Pressing(pressing)) => Some(LinkedRows::Pressing(
                        picked_release(
                            sql,
                            pressing,
                            files
                                .as_ref()
                                .expect("a candidate linked to a pressing has fetched files"),
                        )?,
                    )),
                    Some(ReleaseLink::Album(album)) => Some(LinkedRows::Album(album.records())),
                    None => None,
                };
                let cover = selected
                    .as_ref()
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
                        path: PathBuf::from(&scanned.folder),
                        name: scanned.name.clone(),
                        watched_folder_path: scanned.watched_folder_path.clone(),
                        display_path: scanned.display_path.clone(),
                        grouping: scanned
                            .grouping
                            .as_ref()
                            .map(|_| scanned.path.clone()),
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

/// A Done row, read from the library release its import wrote: the album's
/// title, credited artists and year, the release's cover and its catalog
/// records. Read here, inside the list's snapshot, so a re-identify, an edit, a
/// new cover or a deleted release reruns the list and reaches the row. Its
/// words are the ones the filter tests: both read them with
/// [`load_imported_release_text_on`].
fn imported_row(
    sql: &SqlReadContext<'_>,
    placed: &crate::import::triage::TriageRow,
) -> Result<crate::import::ImportedRow, DbError> {
    let Some(crate::import::TriageImportStatus::Complete { release }) = &placed.import_status
    else {
        return Err(DbError::Message(format!(
            "candidate {} is placed Done with no imported release",
            placed.candidate_key
        )));
    };
    let text = load_imported_release_text_on(sql, Some(&release.release_id))?
        .remove(&release.release_id)
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {} is placed Done on release {}, which has no album",
                placed.candidate_key, release.release_id
            ))
        })?;
    let cover_version = sql
        .query_row(
            "SELECT blob_id FROM covers WHERE id = ?",
            params![release.release_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let records = super::super::read::get_release_records_on(sql, &release.release_id)?;
    let crate::import::ImportedReleaseText {
        title,
        artist,
        year,
    } = text;
    Ok(crate::import::ImportedRow {
        candidate_key: placed.candidate_key.clone(),
        display_path: placed.display_path.clone(),
        action_basis: placed.action_basis.clone(),
        selected: placed.selected,
        release: crate::import::ImportedReleaseSummary {
            cover: cover_version.map(|version| crate::album_detail::ImageRef {
                id: release.release_id.clone(),
                version,
                image_type: crate::db::LibraryImageType::Cover,
            }),
            release_id: release.release_id.clone(),
            album_id: release.album_id.clone(),
            title,
            artist,
            year,
            records,
        },
    })
}

pub(super) enum WindowItemRows {
    Ready(ImportListItem),
    Candidate {
        row: crate::import::triage::TriageRow,
        picked: Option<LinkedRows>,
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
                    Some(LinkedRows::Pressing(picked)) => {
                        let PickedRelease { matched, records } = picked.process()?;
                        row.matched = Some(matched);
                        // The reading the queue placed the row with named no
                        // records; this is where the documents are read.
                        row.reading = crate::import::triage::TriageReading::of(
                            row.metadata_summary.as_ref(),
                            row.release_link.as_ref(),
                            records,
                        );
                    }
                    // An album is no pressing, so nothing leads the row: the
                    // verdict's lead is one of the pressings the person could
                    // not tell their copy from.
                    Some(LinkedRows::Album(records)) => {
                        row.matched = None;
                        row.reading = crate::import::triage::TriageReading::of(
                            row.metadata_summary.as_ref(),
                            row.release_link.as_ref(),
                            records,
                        );
                    }
                    // A draft read from somewhere with no link names no
                    // release, so nothing leads the row: the verdict's lead
                    // does not stand in for a pick.
                    None if row.metadata_provenance.is_some() => row.matched = None,
                    None => {}
                }
                row.cover = cover;
                Ok(ImportListItem::Candidate {
                    row,
                    live: (),
                    is_group_member,
                })
            }
        }
    }
}

/// What a linked row's link reads: a pressing's stored releases, or an
/// album's records, which the link names itself.
pub(super) enum LinkedRows {
    Pressing(PickedReleaseRows),
    Album(Vec<crate::import::ReleaseRecord>),
}

pub(super) struct PickedReleaseRows {
    /// Every release the link claims, the primary first and then its
    /// partners.
    claimed: Vec<crate::import::source_release::SourceRelease>,
    files: CategorizedFiles,
}

/// What the picked releases say: the release the row leads with, and every
/// catalog that describes it.
pub(super) struct PickedRelease {
    matched: MatchedRelease,
    records: Vec<crate::import::ReleaseRecord>,
}

impl PickedReleaseRows {
    fn process(self) -> Result<PickedRelease, DbError> {
        let durations = crate::import::probe::source_durations(&self.files)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let audio_durations = crate::import::audio_layout::audio_durations(&self.files, &durations)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let records = crate::import::source_release::claimed_records(
            &self.claimed.iter().collect::<Vec<_>>(),
        );
        // Only the link's primary release states the row's facts; a
        // partner's own release is not a second set of them. Its artwork is
        // another matter: a row's cover is the pick's, so the partners go to
        // the detail that carries the cover options.
        let (primary, partners) = self
            .claimed
            .split_first()
            .expect("a pick claims at least its primary");
        let detail = primary
            .detail_for_audio(&audio_durations, partners)
            .map_err(|error| DbError::Message(error.to_string()))?;
        Ok(PickedRelease {
            matched: MatchedRelease::of_pick(
                primary.release().catalog,
                &detail,
                primary.source_tracks_for_audio(&audio_durations),
            ),
            records,
        })
    }
}

fn row_cover_source(
    sql: &SqlReadContext<'_>,
    candidate: &ScanCandidateListRow,
    cover: &CoverSelection,
) -> Result<crate::import::CoverImageSource, DbError> {
    match cover {
        CoverSelection::Remote(image, _) => Ok(crate::import::CoverImageSource::Remote {
            image: image.clone(),
        }),
        CoverSelection::Local(file_id) => Ok(crate::import::CoverImageSource::Local {
            file: scanned_file_version(
                sql,
                &candidate.watched_folder_path,
                &candidate.path,
                file_id,
            )?,
        }),
        CoverSelection::Embedded(source_file_id) => {
            let (source, data) = stored_embedded_cover(
                sql,
                &candidate.watched_folder_path,
                &candidate.path,
                source_file_id,
            )?;
            Ok(crate::import::CoverImageSource::Embedded { source, data })
        }
    }
}

/// The version of one of the candidate's files that its scan read.
fn scanned_file_version(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &str,
    candidate_path: &str,
    file_id: &str,
) -> Result<FileVersion, DbError> {
    let (path, size, modified_at_ns) = sql
        .query_row(
            "SELECT absolute_path, size, modified_at_ns FROM scan_candidate_file \
             WHERE watched_folder_path = ? AND candidate_path = ? \
               AND relative_path = ?",
            params![watched_folder_path, candidate_path, file_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {candidate_path} names missing file {file_id}"
            ))
        })?;
    Ok(FileVersion {
        path: PathBuf::from(path),
        size: to_u64(size, "a scanned file's size")?,
        modified_at_ns,
    })
}

/// The embedded cover the candidate selects from `source_file_id`: the bytes
/// its file-tag snapshot stored, and the version of the source file the
/// snapshot read them from.
fn stored_embedded_cover(
    sql: &SqlReadContext<'_>,
    watched_folder_path: &str,
    candidate_path: &str,
    source_file_id: &str,
) -> Result<(FileVersion, Vec<u8>), DbError> {
    let snapshot = super::super::folder_scans::load_candidate_file_tag_snapshot(
        sql,
        watched_folder_path,
        candidate_path,
    )?
    .and_then(|stored| stored.snapshot)
    .ok_or_else(|| {
        DbError::Message(format!(
            "candidate {candidate_path} selects embedded cover without a file-tag snapshot"
        ))
    })?;
    let cover = snapshot.embedded_cover.ok_or_else(|| {
        DbError::Message(format!(
            "candidate {candidate_path} selects embedded cover without stored artwork"
        ))
    })?;
    if cover.source_relative_path != source_file_id {
        return Err(DbError::Message(format!(
            "candidate {candidate_path} selects embedded cover from {source_file_id}, but its snapshot stores {}",
            cover.source_relative_path
        )));
    }
    let observation = snapshot
        .files
        .iter()
        .map(|fact| &fact.observation)
        .find(|observation| observation.relative_path == source_file_id)
        .ok_or_else(|| {
            DbError::Message(format!(
                "candidate {candidate_path}'s snapshot stores a cover from {source_file_id} but no reading of that file"
            ))
        })?;
    let scanned = scanned_file_version(sql, watched_folder_path, candidate_path, source_file_id)?;
    Ok((
        FileVersion {
            path: scanned.path,
            size: observation.size,
            modified_at_ns: observation.modified_at_ns,
        },
        cover.data,
    ))
}

/// Every release a linked pressing claims, with the stored release for each.
fn picked_release(
    sql: &SqlReadContext<'_>,
    link: &PressingLink,
    files: &CategorizedFiles,
) -> Result<PickedReleaseRows, DbError> {
    let claimed = link
        .claimed()
        .map(|release| {
            load_source_release_on(sql, release)?.ok_or_else(|| {
                DbError::Message(format!(
                    "{} release {} is linked but nothing stored it",
                    release.catalog.as_str(),
                    release.key
                ))
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(PickedReleaseRows {
        claimed,
        files: files.clone(),
    })
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
    let content_hash = candidate.files.content_hash();

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
    let current = state.filter(|state| state.file_edits.revision == candidate.file_edit_revision);
    let identify = current.as_ref().and_then(|state| state.identify.as_ref());
    let metadata_provenance = current
        .as_ref()
        .and_then(|state| state.metadata_provenance.clone());
    let release_link = current
        .as_ref()
        .and_then(|state| state.release_link.clone());
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
    // Read here, inside the pane's live query, so an artist another import
    // commits — or any artist row that changes — reads the credits again.
    let artist_resolutions = super::super::artist_resolution::resolve_credits_on(
        sql,
        pane_rows.draft.credits(),
    )?;
    let metadata_revision = sql.query_row(
        "SELECT s.metadata_revision \
             FROM scan_candidate c JOIN import_candidate_state s \
               ON s.content_hash = c.content_hash \
             WHERE c.watched_folder_path = ? AND c.path = ?",
        params![candidate.watched_folder_path, candidate.key()],
        |row| row.get::<_, i64>(0),
    )?;
    let metadata_revision = u64::try_from(metadata_revision)
        .map_err(|_| DbError::Message("candidate metadata revision is negative".to_string()))?;

    let statuses = identify
        .map(|identify| library_statuses(sql, &identify.verdict))
        .transpose()?;
    // Only identity keys are needed for the next SQL query. Track and artwork
    // processing runs after the snapshot ends.
    let claimed = claimed_releases_on(
        sql,
        &candidate,
        release_link.as_ref().and_then(ReleaseLink::pressing),
    )?;
    // The release the draft's tracks were read from, whose track lengths the
    // mapping sets beside the files': the linked one when the draft was read
    // from it, which is the one already read above.
    let draft_source = match &metadata_provenance {
        Some(MetadataProvenance::ExternalRelease { record })
            if claimed.first().map(|release| release.release()) != Some(record) =>
        {
            Some(load_source_release_on(sql, record)?.ok_or_else(|| {
                DbError::Message(format!(
                    "{} is read from {} release {} that nothing stored",
                    candidate.key(),
                    record.catalog.as_str(),
                    record.key
                ))
            })?)
        }
        _ => None,
    };
    // Every catalog record of what the candidate is linked to: the linked
    // pressing's releases describe it, an album names its own.
    let records = match &release_link {
        Some(ReleaseLink::Album(album)) => album.records(),
        Some(ReleaseLink::Pressing(_)) | None => {
            crate::import::source_release::claimed_records(&claimed.iter().collect::<Vec<_>>())
        }
    };
    let picked_library_status = match claimed.first() {
        Some(release) => check_releases_in_library_on(sql, &[release.library_check()])?
            .into_iter()
            .next(),
        None => None,
    };
    let embedded_cover = match pane_rows.cover.as_ref() {
        Some(CoverSelection::Embedded(source_file_id)) => Some(stored_embedded_cover(
            sql,
            &candidate.watched_folder_path,
            &candidate.key(),
            source_file_id,
        )?),
        _ => None,
    };
    Ok(Some(move || {
        let audio = crate::signals::AudioFacts::of_files(&candidate.files)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let durations = &audio.durations;
        let audio_durations =
            crate::import::audio_layout::audio_durations(&candidate.files, durations)
                .map_err(|error| DbError::Message(error.to_string()))?;
        let release = claimed
            .split_first()
            .map(|(primary, partners)| {
                primary
                    .detail_for_audio(&audio_durations, partners)
                    .map_err(|error| DbError::Message(error.to_string()))
            })
            .transpose()?;
        let mut verdict = None;
        let mut resumed_identify_state = crate::identify::IdentifyState::Idle;
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
            verdict = Some(VerdictSummary::of(&identify.verdict, identify.kept_own_draft));
            // The candidate's own text is what the rows are judged and ordered
            // against, live or resumed: striking a number out re-orders the
            // rows the next time they are read, with nothing asked again.
            let text =
                crate::identify::CandidateText::of_stored(signals.as_ref(), &lookup_choices);
            resumed_identify_state =
                identify
                    .verdict
                    .clone()
                    .resume_state(&status_of, text, audio.clone());
        }
        let source_lengths = match (&metadata_provenance, &draft_source) {
            (Some(MetadataProvenance::ExternalRelease { .. }), Some(source)) => {
                crate::import::pane::track_lengths(Some(
                    &source
                        .detail_for_audio(&audio_durations, &[])
                        .map_err(|error| DbError::Message(error.to_string()))?,
                ))
            }
            (Some(MetadataProvenance::ExternalRelease { .. }), None) => {
                crate::import::pane::track_lengths(release.as_ref())
            }
            (Some(MetadataProvenance::FileMetadata) | None, _) => Vec::new(),
        };
        let pane = crate::import::pane::draft_pane(
            release,
            &source_lengths,
            &candidate.files,
            durations,
            &pane_rows.draft,
        )
        .map_err(|error| {
            DbError::Message(format!(
                "candidate {} cannot be drawn: {error}",
                candidate.key()
            ))
        })?;
        let remote_covers = pane
            .release
            .as_ref()
            .map(|release| release.cover_art.clone())
            .unwrap_or_default();
        let cover = chosen_cover(
            &candidate.files,
            pane_rows.cover.as_ref(),
            embedded_cover,
        );
        Ok(ImportCandidateDetailProjection {
            is_added: imported_release.is_some(),
            candidate,
            source_error,
            actionable,
            skipped,
            resumed_identify_state,
            verdict,
            metadata_provenance,
            release_link,
            metadata_author,
            metadata_revision,
            imported_release,
            release: pane.release,
            records,
            picked_library_status,
            metadata_draft: pane.edit,
            artist_resolutions,
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

/// Read every release the linked pressing claims in the pane's SQL snapshot,
/// the primary first and then its partners. None for a candidate linked to no
/// pressing.
///
/// The primary is what the draft was read from and what the pane leads with;
/// together with the partners it is what the release's records are read off.
/// A stored pick always has its releases — the pick write fetches them first,
/// for the primary and every partner alike — so a missing one is stated
/// rather than served as half a pane.
fn claimed_releases_on(
    sql: &SqlReadContext<'_>,
    candidate: &FolderCandidate,
    link: Option<&PressingLink>,
) -> Result<Vec<crate::import::source_release::SourceRelease>, DbError> {
    let Some(link) = link else {
        return Ok(Vec::new());
    };
    link.claimed()
        .map(|release| {
            load_source_release_on(sql, release)?
                .ok_or_else(|| {
                    DbError::Message(format!(
                        "{} is claimed for {} but nothing fetched it",
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
    embedded_cover: Option<(FileVersion, Vec<u8>)>,
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
            image.map(|image| CoverChoice::local(file_id.clone(), image.version()))
        }
        Some(CoverSelection::Embedded(source_file_id)) => embedded_cover
            .map(|(source, data)| CoverChoice::embedded(source_file_id.clone(), source, data)),
        Some(CoverSelection::Remote(image, source)) => {
            Some(CoverChoice::remote(image.clone(), *source))
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
