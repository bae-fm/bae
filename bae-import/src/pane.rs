//! Candidate drafts, their source initialization, and their table projection.
//!
//! A draft holds one track per audio unit of the folder, in the folder's
//! order. Applying a source lays its reading's tracks over those units, and a
//! change to the folder's files redraws them. The pane renders that draft; it
//! does not replay edits over a provider tracklist.

use crate::folder_scanner::CategorizedFiles;
use crate::types::{CandidateDraft, CandidateTrack, RawReleaseEdit, RawTrackEdit};
use crate::ImportError;
use bae_catalogs::release_values::ReleaseValues;

/// Stable identities for the one candidate draft, independent of whichever
/// source last populated it.
pub const CANDIDATE_TRACK_ID_PREFIX: &str = "candidate-track";

/// The draft's row identity for the track at `position`.
pub(crate) fn candidate_track_id(position: usize) -> String {
    format!("{CANDIDATE_TRACK_ID_PREFIX}-{position}")
}

/// The draft a test starts from: one blank track per audio unit of the
/// folder, on the medium of its disc, as the scan stores it with the folder
/// (`blank_draft_*` views), built from files held in memory.
#[cfg(test)]
pub(crate) fn blank_candidate_draft(files: &CategorizedFiles) -> CandidateDraft {
    let layout = crate::audio_layout::folder_tracks(files);
    let numbering: Vec<_> = layout.iter().map(|track| (track.disc, Some(track.number))).collect();
    let (mediums, places) = bae_types::mediums::by_disc(&numbering);
    let tracks = layout
        .into_iter()
        .zip(places)
        .enumerate()
        .map(|(position, (track, place))| {
            CandidateTrack::unsourced(
                RawTrackEdit {
                    id: candidate_track_id(position),
                    title: String::new(),
                    artist_assignments: crate::TrackArtistAssignments::AlbumArtists,
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
        pressing: crate::RawPressingEdit::from_pressing(&bae_types::pressing::Pressing::blank()),
        mediums,
        tracks,
        release_roles: Vec::new(),
        works: Default::default(),
    }
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
    let units = current.tracks.iter().map(|track| track.edit.file.clone()).collect();
    let mut draft = crate::reading_draft::candidate_draft(values, units);
    keep_row_identities(&mut draft.tracks, &current.tracks)?;
    Ok(draft)
}

/// A reading's album fields over the draft's own tracks, for a release that
/// lists no tracks: it says nothing about them, so each keeps what it has —
/// its title, credits, roles and works — and the works with it.
pub(crate) fn album_over_draft(values: ReleaseValues, current: &CandidateDraft) -> CandidateDraft {
    let album = crate::reading_draft::candidate_draft(values, Vec::new());
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
    let mut placer = bae_types::mediums::Placer::default();
    CandidateDraft {
        album_title: edit.album_title,
        album_artist_assignments: edit.album_artist_assignments,
        album_year: edit.album_year,
        pressing: edit.pressing,
        mediums: vec![bae_types::mediums::ReleaseMedium::one_sided(None)],
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
                    placer.place(std::num::NonZeroU16::MIN, std::num::NonZeroU8::MIN, None),
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
            detail: format!("{} tracks were read for a draft of {}", fresh.len(), current.len()),
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

/// The draft over the folder's new audio units, before the library's layout
/// statement moves the CUE sheets the files now put on another side, and
/// those moves.
pub(crate) struct RedrawnDraft {
    pub(crate) draft: CandidateDraft,
    /// Each CUE sheet the files now put on another side the draft holds,
    /// with that side.
    pub(crate) sheet_moves: Vec<(String, bae_types::mediums::MediumSide)>,
}

/// The draft over the folder's new audio units: a unit the draft already
/// had keeps its track, with every value it holds, and a new one takes the
/// track `initialized` read for it, which no source states anything about.
/// Metadata belongs to its audio identity, never to the position another
/// file happens to occupy. The draft keeps only the works its tracks still
/// perform.
///
/// A new track takes the place the folder's layout gives it: the draft's
/// medium at its disc's position, the draft gaining mediums of unstated
/// carrier up to it when it holds fewer. A sheet the files now put on
/// another side the draft's mediums hold is one of the redrawn draft's
/// moves, which the library's layout statement makes; where the draft's medium
/// has no such side, the sheet's retained slices take the place the
/// folder's layout gives them.
pub(crate) fn redraw_draft_for_files(
    previous_files: &CategorizedFiles,
    files: &CategorizedFiles,
    initialized: CandidateDraft,
    draft: &CandidateDraft,
    ids: &dyn coven::IdSource,
) -> RedrawnDraft {
    use bae_types::mediums::{MediumSide, TrackPlace};
    let mut mediums = draft.mediums.clone();
    let mut laid_out = |place: TrackPlace| {
        while mediums.len() <= place.medium_index() {
            mediums.push(bae_types::mediums::ReleaseMedium::one_sided(None));
        }
        place
    };
    let side_of = |files: &CategorizedFiles, sheet_id: &str| {
        files
            .carving_sheets()
            .into_iter()
            .find(|sheet| sheet.file.relative_path == sheet_id)
            .and_then(|sheet| sheet.place.side())
    };
    // Whether the draft's mediums hold the side.
    let held = |side: MediumSide| {
        usize::try_from(side.medium)
            .ok()
            .and_then(|medium| draft.mediums.get(medium.checked_sub(1)?))
            .is_some_and(|held| side.side >= 1 && side.side <= u32::from(held.sides.get()))
    };
    let mut sheet_moves: Vec<(String, MediumSide)> = Vec::new();
    let tracks: Vec<CandidateTrack> = initialized
        .tracks
        .into_iter()
        .map(|fresh| {
            let audio = &fresh.edit.file;
            if let Some(existing) = draft.tracks.iter().find(|track| &track.edit.file == audio) {
                let mut retained = existing.clone();
                if let crate::AudioFile::SheetSlice { sheet_id, .. } = audio {
                    let side = side_of(files, sheet_id);
                    if side_of(previous_files, sheet_id) != side {
                        match side.filter(|side| held(*side)) {
                            Some(side) => {
                                if sheet_moves.iter().all(|(moved, _)| moved != sheet_id) {
                                    sheet_moves.push((sheet_id.clone(), side));
                                }
                            }
                            None => retained.place = laid_out(fresh.place),
                        }
                    }
                }
                return retained;
            }
            let mut edit = fresh.edit;
            edit.id = ids.new_id().to_string();
            CandidateTrack::unsourced(edit, laid_out(fresh.place))
        })
        .collect();
    RedrawnDraft { draft: CandidateDraft { mediums, ..draft.with_tracks(tracks) }, sheet_moves }
}

impl RedrawnDraft {
    /// The draft with its tracks at the places `layout`, the library's
    /// layout statement over the draft and its sheet moves, gives them, and
    /// only the mediums a track sits on. A layout that misses a track of the
    /// draft is refused.
    pub(crate) fn laid_out(
        self,
        layout: bae_records::records::layout::ReleaseLayout,
    ) -> Result<CandidateDraft, ImportError> {
        let mediums = layout.mediums;
        let mut tracks = self.draft.tracks.clone();
        for track in &mut tracks {
            let laid = layout.tracks.iter().find(|laid| laid.track_id == track.edit.id);
            let Some(laid) = laid else {
                return Err(ImportError::Internal {
                    detail: format!("the layout of an import draft misses track {}", track.edit.id),
                });
            };
            track.place = laid.place;
        }
        let (mediums, tracks) = without_empty_mediums(mediums, tracks);
        Ok(CandidateDraft { mediums, ..self.draft.with_tracks(tracks) })
    }
}

/// `mediums` with only the ones a track of `tracks` sits on, in order, and
/// the tracks placed on them: a medium the folder's audio no longer reaches
/// is not one of the release's.
fn without_empty_mediums(
    mediums: Vec<bae_types::mediums::ReleaseMedium>,
    mut tracks: Vec<CandidateTrack>,
) -> (Vec<bae_types::mediums::ReleaseMedium>, Vec<CandidateTrack>) {
    let used: std::collections::BTreeSet<usize> =
        tracks.iter().map(|track| track.place.medium_index()).collect();
    let kept: Vec<usize> = used.into_iter().collect();
    for track in &mut tracks {
        let index = kept
            .iter()
            .position(|kept| *kept == track.place.medium_index())
            .expect("every track's medium is kept");
        track.place.medium = bae_types::mediums::medium_position(index);
    }
    (kept.into_iter().map(|index| mediums[index]).collect(), tracks)
}

/// The draft a folder's stored file-tag reading makes, one track per audio
/// unit in the folder's order.
pub(crate) fn file_metadata_draft(
    candidate: &super::folder_scanner::FolderCandidate,
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
) -> Result<CandidateDraft, ImportError> {
    let values = candidate.file_tag_values(snapshot)?;
    let units = crate::audio_layout::audio_units(&candidate.files);
    if values.tracks.len() != units.len() {
        return Err(ImportError::Internal {
            detail: format!(
                "file tags describe {} tracks for {} audio units",
                values.tracks.len(),
                units.len()
            ),
        });
    }
    Ok(crate::reading_draft::candidate_draft(values, units))
}

/// The candidate draft a release's reading writes, keeping each current
/// track on its file. A reading of tracks lays every value it states over
/// the folder's audio; a release that lists none leaves the draft's tracks
/// as they are. Where the release states no year, the pressing's is
/// `pressing_year`, the folder's — see
/// `identify::row_facts::folder_pressing_year`.
pub(crate) fn external_candidate_draft(
    values: bae_catalogs::release_values::ReleaseValues,
    current: &crate::CandidateDraft,
    pressing_year: Option<i32>,
) -> Result<crate::CandidateDraft, crate::ImportError> {
    let mut draft = if values.tracks.is_empty() {
        album_over_draft(values, current)
    } else {
        values_over_audio(values, current)?
    };
    if draft.pressing.year.trim().is_empty() {
        if let Some(year) = pressing_year {
            draft.pressing.year = year.to_string();
        }
    }
    Ok(draft)
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
