//! Candidate drafts, their source initialization, and their table projection.
//!
//! A draft holds one track per audio unit of the folder, in the folder's
//! order. Metadata application lays a release's tracks over those units, and a
//! change to the folder's files redraws them. The pane renders that draft; it
//! does not replay edits over a provider tracklist.

use crate::import::folder_scanner::CategorizedFiles;
use crate::import::mapping::{mapping_table, MappingTable};
use crate::import::probe::SourceDurations;
use crate::import::search::ImportSearchReleaseDetail;
use crate::import::types::{CandidateDraft, CandidateTrack, RawReleaseEdit, ReleaseUserEdit};
use crate::import::ImportError;

/// Stable identities for the one candidate draft, independent of whichever
/// source last populated it.
pub const CANDIDATE_TRACK_ID_PREFIX: &str = "candidate-track";

/// The source-less editable draft created with a discovered candidate.
/// Candidate files determine only which audio units exist; their names and
/// tags do not become metadata until a source is explicitly applied.
#[cfg(test)]
pub(crate) fn blank_candidate_draft(files: &CategorizedFiles) -> CandidateDraft {
    blank_candidate_source(files).draft
}

pub(crate) fn blank_candidate_source(files: &CategorizedFiles) -> CandidateSourceDraft {
    blank_source_for_tracks(crate::import::audio_layout::direct_entry_track_rows(files))
}

pub(crate) fn blank_source_for_tracks(
    tracks: Vec<crate::import::TrackUserEdit>,
) -> CandidateSourceDraft {
    let draft = RawReleaseEdit::from_user_edit(
        ReleaseUserEdit {
            album_title: String::new(),
            album_artist_assignments: Vec::new(),
            album_year: None,
            pressing: crate::pressing::Pressing::blank(),
            tracks,
        },
        CANDIDATE_TRACK_ID_PREFIX,
    );
    candidate_draft_from_edit(draft).expect("direct-entry rows have audio")
}

/// The release preview, editable metadata, and audio rows presented by a pane.
pub struct PanePick {
    /// `None` when no external release was applied.
    pub release: Option<ImportSearchReleaseDetail>,
    pub edit: RawReleaseEdit,
    pub mapping: MappingTable,
}

pub(crate) struct CandidateSourceDraft {
    pub draft: CandidateDraft,
    pub source_discogs_artist_ids: std::collections::BTreeSet<String>,
    pub mapped_credit_discogs_artist_ids: std::collections::BTreeSet<String>,
}

/// Lay `metadata`'s tracks over the draft's audio: track `i` becomes what the
/// audio of the draft's row `i` commits as, under that row's identity. The
/// discs and sides are the metadata's, whatever the draft grouped the audio
/// into: an LP ripped as one tagged disc takes the record's two sides, and a
/// release that states one disc where the files were tagged two takes one.
///
/// Refuses metadata listing a different number of tracks than the folder has
/// audio units.
pub(crate) fn metadata_over_audio(
    mut metadata: RawReleaseEdit,
    current: &CandidateDraft,
) -> Result<CandidateSourceDraft, ImportError> {
    if metadata.tracks.len() != current.tracks.len() {
        return Err(ImportError::MetadataTrackCount {
            metadata_tracks: metadata.tracks.len(),
            audio_tracks: current.tracks.len(),
        });
    }
    for (track, existing) in metadata.tracks.iter_mut().zip(&current.tracks) {
        track.file = Some(existing.edit.file.clone());
    }
    let mut source = candidate_draft_from_edit(metadata)?;
    for (track, existing) in source.draft.tracks.iter_mut().zip(&current.tracks) {
        track.edit.id.clone_from(&existing.edit.id);
    }
    Ok(source)
}

/// Carry the draft's row identities onto tracks read afresh from the same
/// audio units.
pub(crate) fn keep_row_identities(
    fresh: &mut [CandidateTrack],
    current: &[CandidateTrack],
) -> Result<(), ImportError> {
    if fresh.len() != current.len() {
        return Err(ImportError::Internal {
            detail: format!(
                "{} tracks were read for a draft of {}",
                fresh.len(),
                current.len()
            ),
        });
    }
    for (track, existing) in fresh.iter_mut().zip(current) {
        if track.edit.file != existing.edit.file {
            return Err(ImportError::Internal {
                detail: format!(
                    "draft track {} plays {:?}, not the {:?} read for it",
                    existing.edit.id, existing.edit.file, track.edit.file
                ),
            });
        }
        track.edit.id.clone_from(&existing.edit.id);
    }
    Ok(())
}

/// The draft an editor form makes, its rows renamed by position. Every row
/// must name its audio.
pub(crate) fn candidate_draft_from_edit(
    draft: RawReleaseEdit,
) -> Result<CandidateSourceDraft, ImportError> {
    let mapped_credit_discogs_artist_ids = draft.credit_discogs_artist_ids_for_bound_tracks();
    let tracks = draft
        .tracks
        .into_iter()
        .enumerate()
        .map(|(position, mut edit)| {
            edit.id = format!("{CANDIDATE_TRACK_ID_PREFIX}-{position}");
            CandidateTrack::from_edit(edit, position)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CandidateSourceDraft {
        draft: CandidateDraft {
            album_title: draft.album_title,
            album_artist_assignments: draft.album_artist_assignments,
            album_year: draft.album_year,
            pressing: draft.pressing,
            tracks,
        },
        source_discogs_artist_ids: std::collections::BTreeSet::new(),
        mapped_credit_discogs_artist_ids,
    })
}

/// Project the stored draft onto the candidate's physical units, beside the
/// linked `release`. Metadata is already authoritative; the mapping sets each
/// track's `source_lengths` entry — from the release the draft was read from,
/// by the source index a track keeps — beside its file's own length.
pub(crate) fn draft_pane(
    release: Option<ImportSearchReleaseDetail>,
    source_lengths: &[Option<u64>],
    files: &CategorizedFiles,
    durations: &SourceDurations,
    draft: &CandidateDraft,
) -> Result<PanePick, ImportError> {
    let mapping = mapping_table(files, durations, draft, source_lengths)?;
    Ok(PanePick {
        release,
        edit: draft.release_edit(),
        mapping,
    })
}

/// Each track's length as `release` lists it, in its order; empty with no
/// release.
pub(crate) fn track_lengths(release: Option<&ImportSearchReleaseDetail>) -> Vec<Option<u64>> {
    release
        .iter()
        .flat_map(|release| &release.tracks)
        .map(|track| track.duration_ms)
        .collect()
}

/// The draft over the folder's new audio units: a unit the draft already
/// had keeps its track, and a new one takes the track `initialized` read for
/// it. Metadata belongs to its audio identity, never to the position another
/// file happens to occupy.
pub(crate) fn redraw_draft_for_files(
    previous_files: &CategorizedFiles,
    initialized: CandidateDraft,
    draft: &CandidateDraft,
    ids: &dyn coven::IdProvider,
) -> CandidateDraft {
    let mut result = draft.clone();
    result.tracks = initialized
        .tracks
        .into_iter()
        .map(|mut fresh| {
            let audio = &fresh.edit.file;
            if let Some(existing) = draft.tracks.iter().find(|track| &track.edit.file == audio) {
                let mut retained = existing.clone();
                if let crate::import::AudioFile::SheetSlice { sheet_id, .. } = audio {
                    let previous_disc = previous_files
                        .carving_sheets()
                        .into_iter()
                        .find(|sheet| sheet.file.relative_path == *sheet_id)
                        .and_then(|sheet| sheet.disc_number());
                    if previous_disc.map(|disc| disc as i32) != fresh.edit.side {
                        retained.edit.side = fresh.edit.side;
                    }
                }
                return retained;
            }
            fresh.edit.id = ids.new_id();
            fresh.source_index = None;
            fresh
        })
        .collect();
    result
}

pub(crate) fn source_discogs_artist_ids(
    parsed: &crate::import::ParsedAlbum,
) -> std::collections::BTreeSet<String> {
    let credited_artist_ids: std::collections::HashSet<&str> = parsed
        .release_artist_roles
        .iter()
        .map(|role| role.artist_id.as_str())
        .chain(
            parsed
                .track_artist_roles
                .iter()
                .map(|role| role.artist_id.as_str()),
        )
        .chain(
            parsed
                .work_graph
                .work_artists
                .iter()
                .map(|credit| credit.artist_id.as_str()),
        )
        .collect();
    parsed
        .artists
        .iter()
        .filter(|artist| credited_artist_ids.contains(artist.id.as_str()))
        .filter_map(|artist| artist.discogs_artist_id.clone())
        .collect()
}

/// The metadata a folder's stored file-tag snapshot describes, one track per
/// audio unit in the folder's order.
pub(crate) fn file_metadata_edit(
    candidate: &super::folder_scanner::FolderCandidate,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<RawReleaseEdit, ImportError> {
    let mut seed = candidate.file_tag_edit(snapshot, clock, ids)?;
    let units = crate::import::audio_layout::audio_units(&candidate.files);
    if seed.tracks.len() != units.len() {
        return Err(ImportError::Internal {
            detail: format!(
                "file tags describe {} tracks for {} audio units",
                seed.tracks.len(),
                units.len()
            ),
        });
    }
    for (track, unit) in seed.tracks.iter_mut().zip(units) {
        track.file = Some(unit);
    }
    // Tags that name no album artist leave a blank credit, which is no
    // assignment at all.
    seed.album_artist_assignments
        .retain(|assignment| !assignment.is_blank());
    Ok(RawReleaseEdit::from_user_edit(seed, CANDIDATE_TRACK_ID_PREFIX))
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
