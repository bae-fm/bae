//! Candidate drafts, their source initialization, and their table projection.
//!
//! A draft holds its source's discs, and one track for each of the folder's
//! audio units that lands on one of them, in the folder's order: a unit that
//! lands on none is left out of the import. Applying a source lays its
//! reading's tracks over the units, and a change to the folder's files redraws
//! them. The pane renders that draft; it does not replay edits over a
//! provider tracklist.

use crate::folder_scanner::CategorizedFiles;
use crate::types::{CandidateDraft, CandidateTrack};
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
                crate::types::RawTrackEdit {
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

/// Lay a reading's tracks over the folder's audio `units`: track `i` becomes
/// what unit `i` commits as, with every value the reading holds, under the
/// identity of the draft's row that plays the unit. The discs and sides are
/// the reading's, whatever the draft grouped the audio into: an LP ripped as
/// one tagged disc takes the record's two sides, and a release that states one
/// disc where the files were tagged two takes one.
///
/// Refuses a reading of a different number of tracks than the folder has
/// audio units.
pub(crate) fn values_over_audio(
    values: ReleaseValues,
    units: Vec<crate::AudioFile>,
    current: &CandidateDraft,
) -> Result<CandidateDraft, ImportError> {
    if values.tracks.len() != units.len() {
        return Err(ImportError::MetadataTrackCount {
            folder_tracks: track_count(units.len()),
            release_tracks: track_count(values.tracks.len()),
        });
    }
    let mut draft = crate::reading_draft::candidate_draft(values, units);
    keep_row_identities(&mut draft.tracks, &current.tracks);
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
    edit: crate::types::RawReleaseEdit,
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
                crate::types::RawTrackEdit {
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
pub(crate) fn draft_of_edit(edit: crate::types::RawReleaseEdit) -> CandidateDraft {
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
                    crate::types::RawTrackEdit {
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

/// Carry the draft's row identities onto tracks read afresh: a track whose
/// audio a row of `current` plays takes that row's identity, and any other
/// takes one no row of `current` has.
pub(crate) fn keep_row_identities(fresh: &mut [CandidateTrack], current: &[CandidateTrack]) {
    let mut taken: std::collections::HashSet<String> =
        current.iter().map(|track| track.edit.id.clone()).collect();
    let mut unused = (0..).map(candidate_track_id);
    for track in fresh {
        match current.iter().find(|existing| existing.edit.file == track.edit.file) {
            Some(existing) => track.edit.id.clone_from(&existing.edit.id),
            None => {
                if taken.contains(&track.edit.id) {
                    track.edit.id = unused
                        .find(|id| !taken.contains(id))
                        .expect("the row identities never run out");
                }
                taken.insert(track.edit.id.clone());
            }
        }
    }
}

/// The draft over the folder's new audio units, before the library's layout
/// statement moves the tracks of the CUE sheet the person put on a side, and
/// that move.
pub(crate) struct RedrawnDraft {
    pub(crate) draft: CandidateDraft,
    /// The CUE sheet the person put on another side than some of its tracks
    /// are on, with that side.
    pub(crate) sheet_moves: Vec<(String, bae_types::mediums::MediumSide)>,
}

/// The draft over the folder's new audio units, on the draft's own mediums,
/// which are its source's. A unit the draft already had keeps its track, with
/// every value it holds, and its place: only `placed_sheet`, the CUE sheet
/// the person just put on a side or took out, moves, by the library's layout
/// statement. Metadata belongs to its audio identity, never to the position
/// another file happens to occupy. The draft keeps only the works its tracks
/// still perform.
///
/// A unit the draft left out stays out, unless the person just put its sheet
/// on a side. Such a unit, and one the decision gained, takes the track
/// `initialized` read for it, which no source states anything about: on the
/// side the person just put its sheet on, else on the side the draft has the
/// sheet's other tracks on, else where the folder's layout, `initialized`'s,
/// puts it. A unit whose side the draft's mediums lack is left out.
pub(crate) fn redraw_draft_for_files(
    previous_files: &CategorizedFiles,
    files: &CategorizedFiles,
    initialized: CandidateDraft,
    draft: &CandidateDraft,
    placed_sheet: Option<&str>,
    ids: &dyn coven::IdSource,
) -> RedrawnDraft {
    use bae_types::mediums::{MediumSide, TrackPlace};
    let side_of = |sheet_id: &str| {
        files
            .carving_sheets()
            .into_iter()
            .find(|sheet| sheet.file.relative_path == sheet_id)
            .and_then(|sheet| sheet.place.side())
    };
    // The place on `side` of a track numbered `number`, where the draft's
    // mediums have that side.
    let on_side = |side: MediumSide, number: std::num::NonZeroU32| {
        let medium = std::num::NonZeroU16::new(u16::try_from(side.medium).ok()?)?;
        let held = draft.mediums.get(usize::from(medium.get()) - 1)?;
        let side = std::num::NonZeroU8::new(u8::try_from(side.side).ok()?)?;
        (side <= held.sides).then_some(TrackPlace { medium, side, number })
    };
    let previous_units = crate::audio_layout::audio_units(previous_files);
    let mut sheet_moves: Vec<(String, MediumSide)> = Vec::new();
    let tracks: Vec<CandidateTrack> = initialized
        .tracks
        .into_iter()
        .filter_map(|fresh| {
            let audio = &fresh.edit.file;
            let sheet_id = match audio {
                crate::AudioFile::SheetSlice { sheet_id, .. } => Some(sheet_id.as_str()),
                crate::AudioFile::Standalone { .. } => None,
            };
            let placed = sheet_id.is_some() && sheet_id == placed_sheet;
            if let Some(existing) = draft.tracks.iter().find(|track| &track.edit.file == audio) {
                if let Some(sheet_id) = sheet_id.filter(|_| placed) {
                    let side = side_of(sheet_id)
                        .filter(|side| on_side(*side, existing.place.number).is_some());
                    if let Some(side) = side {
                        if sheet_moves.iter().all(|(moved, _)| moved != sheet_id) {
                            sheet_moves.push((sheet_id.to_string(), side));
                        }
                    }
                }
                return Some(existing.clone());
            }
            if !placed && previous_units.contains(audio) {
                return None;
            }
            let number = fresh.place.number;
            let folders_side = MediumSide {
                medium: u32::from(fresh.place.medium.get()),
                side: u32::from(fresh.place.side.get()),
            };
            let place = match sheet_id {
                Some(sheet_id) if placed => {
                    side_of(sheet_id).and_then(|side| on_side(side, number))
                }
                Some(sheet_id) => match draft.first_sheet_track(sheet_id) {
                    Some(drawn) => Some(TrackPlace { number, ..drawn.place }),
                    None => on_side(folders_side, number),
                },
                None => on_side(folders_side, number),
            }?;
            let mut edit = fresh.edit;
            edit.id = ids.new_id().to_string();
            Some(CandidateTrack::unsourced(edit, place))
        })
        .collect();
    RedrawnDraft { draft: draft.with_tracks(tracks), sheet_moves }
}

impl RedrawnDraft {
    /// The draft with its tracks at the places `layout`, the library's
    /// layout statement over the draft and its sheet moves, gives them. A
    /// layout that misses a track of the draft is refused.
    pub(crate) fn laid_out(
        self,
        layout: bae_records::records::layout::ReleaseLayout,
    ) -> Result<CandidateDraft, ImportError> {
        let mut draft = self.draft;
        for track in &mut draft.tracks {
            let laid = layout.tracks.iter().find(|laid| laid.track_id == track.edit.id);
            let Some(laid) = laid else {
                return Err(ImportError::Internal {
                    detail: format!("the layout of an import draft misses track {}", track.edit.id),
                });
            };
            track.place = laid.place;
        }
        Ok(draft)
    }
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
/// row's identity on its file. A reading of tracks lays every value it states
/// over the folder's audio `units`; a release that lists none leaves the
/// draft's tracks as they are. Where the release states no year, the pressing's is
/// `pressing_year`, the folder's — see
/// `identify::row_facts::folder_pressing_year`.
pub(crate) fn external_candidate_draft(
    values: bae_catalogs::release_values::ReleaseValues,
    units: Vec<crate::AudioFile>,
    current: &crate::CandidateDraft,
    pressing_year: Option<i32>,
) -> Result<crate::CandidateDraft, crate::ImportError> {
    let mut draft = if values.tracks.is_empty() {
        album_over_draft(values, current)
    } else {
        values_over_audio(values, units, current)?
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
