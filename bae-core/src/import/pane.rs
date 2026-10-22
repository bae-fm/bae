//! Candidate drafts, their source initialization, and their table projection.
//!
//! A draft holds one track per audio unit of the folder, in the folder's
//! order. Applying a source lays its reading's tracks over those units, and a
//! change to the folder's files redraws them. The pane renders that draft; it
//! does not replay edits over a provider tracklist.

use crate::import::folder_scanner::CategorizedFiles;
use crate::import::mapping::{mapping_table, MappingTable};
use crate::import::probe::SourceDurations;
use crate::import::release_values::ReleaseValues;
use crate::import::types::{CandidateDraft, CandidateTrack, RawReleaseEdit, RawTrackEdit};
use crate::import::ImportError;

/// Stable identities for the one candidate draft, independent of whichever
/// source last populated it.
pub const CANDIDATE_TRACK_ID_PREFIX: &str = "candidate-track";

/// The draft's row identity for the track at `position`.
pub(crate) fn candidate_track_id(position: usize) -> String {
    format!("{CANDIDATE_TRACK_ID_PREFIX}-{position}")
}

/// The source-less editable draft created with a discovered candidate.
/// Candidate files determine only which audio units exist, and their discs
/// and numbers; their names and tags do not become metadata until a source is
/// explicitly applied.
pub(crate) fn blank_candidate_draft(files: &CategorizedFiles) -> CandidateDraft {
    let layout = crate::import::audio_layout::folder_tracks(files);
    let numbering: Vec<_> = layout
        .iter()
        .map(|track| (track.disc, Some(track.number)))
        .collect();
    let (mediums, places) = crate::mediums::by_disc(&numbering);
    let tracks = layout
        .into_iter()
        .zip(places)
        .enumerate()
        .map(|(position, (track, place))| {
            CandidateTrack::unsourced(
                RawTrackEdit {
                    id: candidate_track_id(position),
                    title: String::new(),
                    artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                    file: track.file,
                },
                place,
            )
        })
        .collect();
    CandidateDraft {
        album_title: String::new(),
        album_artist_assignments: Vec::new(),
        album_year: String::new(),
        pressing: crate::import::RawPressingEdit::from_pressing(
            &crate::pressing::Pressing::blank(),
        ),
        mediums,
        tracks,
        release_roles: Vec::new(),
        works: Default::default(),
    }
}

/// The editable metadata and audio rows a pane presents.
pub struct PanePick {
    pub edit: RawReleaseEdit,
    pub mapping: MappingTable,
}

/// Lay a reading's tracks over the draft's audio: track `i` becomes what the
/// audio of the draft's row `i` commits as, under that row's identity, with
/// every value the reading holds. The discs and sides are the reading's,
/// whatever the draft grouped the audio into: an LP ripped as one tagged disc
/// takes the record's two sides, and a release that states one disc where the
/// files were tagged two takes one.
///
/// Refuses a reading of a different number of tracks than the folder has
/// audio units.
pub(crate) fn values_over_audio(
    values: ReleaseValues,
    current: &CandidateDraft,
) -> Result<CandidateDraft, ImportError> {
    if values.tracks.len() != current.tracks.len() {
        return Err(ImportError::MetadataTrackCount {
            folder_tracks: track_count(current.tracks.len()),
            release_tracks: track_count(values.tracks.len()),
        });
    }
    let units = current
        .tracks
        .iter()
        .map(|track| track.edit.file.clone())
        .collect();
    let mut draft = values.candidate_draft(units);
    keep_row_identities(&mut draft.tracks, &current.tracks)?;
    Ok(draft)
}

/// A reading's album fields over the draft's own tracks, for a release that
/// lists no tracks: it says nothing about them, so each keeps what it has —
/// its title, credits, roles and works — and the works with it.
pub(crate) fn album_over_draft(values: ReleaseValues, current: &CandidateDraft) -> CandidateDraft {
    let album = values.candidate_draft(Vec::new());
    CandidateDraft {
        mediums: current.mediums.clone(),
        tracks: current.tracks.clone(),
        works: current.works.clone(),
        ..album
    }
}

/// The form's values over the draft's audio, read from no source: what a
/// test or the debug menu types in. Refuses a form of a different number of
/// tracks than the folder has audio units.
#[cfg(any(test, feature = "test-utils", debug_assertions))]
pub(crate) fn edit_over_audio(
    edit: RawReleaseEdit,
    current: &CandidateDraft,
) -> Result<CandidateDraft, ImportError> {
    if edit.tracks.len() != current.tracks.len() {
        return Err(ImportError::MetadataTrackCount {
            folder_tracks: track_count(current.tracks.len()),
            release_tracks: track_count(edit.tracks.len()),
        });
    }
    let tracks = edit
        .tracks
        .into_iter()
        .zip(&current.tracks)
        .map(|(track, existing)| {
            CandidateTrack::unsourced(
                RawTrackEdit {
                    id: existing.edit.id.clone(),
                    title: track.title,
                    artist_assignments: track.artist_assignments,
                    file: existing.edit.file.clone(),
                },
                existing.place,
            )
        })
        .collect();
    Ok(CandidateDraft {
        album_title: edit.album_title,
        album_artist_assignments: edit.album_artist_assignments,
        album_year: edit.album_year,
        pressing: edit.pressing,
        mediums: current.mediums.clone(),
        tracks,
        release_roles: Vec::new(),
        works: Default::default(),
    })
}

/// The draft a form with its audio bound makes, read from no source: what a
/// test seeds a candidate with. Its tracks are one disc of unstated carrier,
/// numbered in order.
#[cfg(test)]
pub(crate) fn draft_of_edit(edit: RawReleaseEdit) -> CandidateDraft {
    let mut placer = crate::mediums::Placer::default();
    CandidateDraft {
        album_title: edit.album_title,
        album_artist_assignments: edit.album_artist_assignments,
        album_year: edit.album_year,
        pressing: edit.pressing,
        mediums: vec![crate::mediums::ReleaseMedium::one_sided(None)],
        tracks: edit
            .tracks
            .into_iter()
            .enumerate()
            .map(|(position, track)| {
                CandidateTrack::unsourced(
                    RawTrackEdit {
                        id: candidate_track_id(position),
                        title: track.title,
                        artist_assignments: track.artist_assignments,
                        file: track.file.expect("a seeded track names its audio"),
                    },
                    placer.place(
                        std::num::NonZeroU16::MIN,
                        std::num::NonZeroU8::MIN,
                        None,
                    ),
                )
            })
            .collect(),
        release_roles: Vec::new(),
        works: Default::default(),
    }
}

fn track_count(tracks: usize) -> u32 {
    u32::try_from(tracks).expect("a tracklist's length fits u32")
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

/// Project the stored draft onto the candidate's physical units. The draft
/// holds every value; the mapping shows each track's stated length beside its
/// audio's own.
pub(crate) fn draft_pane(
    files: &CategorizedFiles,
    durations: &SourceDurations,
    draft: &CandidateDraft,
) -> Result<PanePick, ImportError> {
    let mapping = mapping_table(files, durations, draft)?;
    Ok(PanePick {
        edit: draft.release_edit(),
        mapping,
    })
}

/// The draft over the folder's new audio units: a unit the draft already
/// had keeps its track, with every value it holds, and a new one takes the
/// track `initialized` read for it, which no source states anything about.
/// Metadata belongs to its audio identity, never to the position another
/// file happens to occupy. The draft keeps only the works its tracks still
/// perform.
///
/// A new track, and a retained slice whose sheet the files now put on another
/// disc, take the place the folder's layout gives them: the draft's medium at
/// that disc's position, the draft gaining mediums of unstated carrier up to
/// it when it holds fewer. A medium no track sits on any more is dropped.
pub(crate) fn redraw_draft_for_files(
    previous_files: &CategorizedFiles,
    files: &CategorizedFiles,
    initialized: CandidateDraft,
    draft: &CandidateDraft,
    ids: &dyn coven::IdSource,
) -> CandidateDraft {
    let mut mediums = draft.mediums.clone();
    let mut laid_out = |place: crate::mediums::TrackPlace| {
        while mediums.len() <= place.medium_index() {
            mediums.push(crate::mediums::ReleaseMedium::one_sided(None));
        }
        place
    };
    let tracks = initialized
        .tracks
        .into_iter()
        .map(|fresh| {
            let audio = &fresh.edit.file;
            if let Some(existing) = draft.tracks.iter().find(|track| &track.edit.file == audio) {
                let mut retained = existing.clone();
                if let crate::import::AudioFile::SheetSlice { sheet_id, .. } = audio {
                    let disc_of = |files: &CategorizedFiles| {
                        files
                            .carving_sheets()
                            .into_iter()
                            .find(|sheet| sheet.file.relative_path == *sheet_id)
                            .and_then(|sheet| sheet.disc_number())
                    };
                    if disc_of(previous_files) != disc_of(files) {
                        retained.place = laid_out(fresh.place);
                    }
                }
                return retained;
            }
            let mut edit = fresh.edit;
            edit.id = ids.new_id().to_string();
            CandidateTrack::unsourced(edit, laid_out(fresh.place))
        })
        .collect();
    let (mediums, tracks) = without_empty_mediums(mediums, tracks);
    CandidateDraft {
        mediums,
        ..draft.with_tracks(tracks)
    }
}

/// `mediums` with only the ones a track of `tracks` sits on, in order, and
/// the tracks placed on them: a medium the folder's audio no longer reaches
/// is not one of the release's.
fn without_empty_mediums(
    mediums: Vec<crate::mediums::ReleaseMedium>,
    mut tracks: Vec<CandidateTrack>,
) -> (Vec<crate::mediums::ReleaseMedium>, Vec<CandidateTrack>) {
    let used: std::collections::BTreeSet<usize> = tracks
        .iter()
        .map(|track| track.place.medium_index())
        .collect();
    let kept: Vec<usize> = used.into_iter().collect();
    for track in &mut tracks {
        let index = kept
            .iter()
            .position(|kept| *kept == track.place.medium_index())
            .expect("every track's medium is kept");
        track.place.medium = crate::mediums::medium_position(index);
    }
    (kept.into_iter().map(|index| mediums[index]).collect(), tracks)
}

/// The draft a folder's stored file-tag reading makes, one track per audio
/// unit in the folder's order.
pub(crate) fn file_metadata_draft(
    candidate: &super::folder_scanner::FolderCandidate,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
) -> Result<CandidateDraft, ImportError> {
    let values = candidate.file_tag_values(snapshot)?;
    let units = crate::import::audio_layout::audio_units(&candidate.files);
    if values.tracks.len() != units.len() {
        return Err(ImportError::Internal {
            detail: format!(
                "file tags describe {} tracks for {} audio units",
                values.tracks.len(),
                units.len()
            ),
        });
    }
    Ok(values.candidate_draft(units))
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
