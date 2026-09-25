//! Candidate drafts, their source initialization, and their table projection.
//!
//! Metadata application and audio replacement update the stored draft. The
//! pane renders that draft; it does not replay edits over a provider tracklist.

use crate::import::folder_scanner::CategorizedFiles;
use crate::import::mapping::{
    mapping_table, MappingBecomes, MappingTable, MappingTrackSection, PickedTracklist,
    TracklistSource,
};
use crate::import::probe::SourceDurations;
use crate::import::search::ImportSearchReleaseDetail;
use crate::import::track_slots::{slot_table, SourceTrack};
use crate::import::types::{CandidateDraft, CandidateTrack, RawReleaseEdit, ReleaseUserEdit};
use crate::import::ImportError;

/// The row identity they carry when the folder's file tags name them.
pub const FILE_TAG_TRACK_ID_PREFIX: &str = "file-tag-track";

/// Stable identities for the one candidate draft, independent of whichever
/// source last populated it.
pub const CANDIDATE_TRACK_ID_PREFIX: &str = "candidate-track";

/// The source-less editable draft created with a discovered candidate.
/// Candidate files determine only how many physical slots exist; their names
/// and tags do not become metadata until a source is explicitly applied.
#[cfg(test)]
pub(crate) fn blank_candidate_draft(files: &CategorizedFiles) -> CandidateDraft {
    blank_candidate_source(files).draft
}

pub(crate) fn blank_candidate_source(files: &CategorizedFiles) -> CandidateSourceDraft {
    blank_source_for_tracks(crate::import::track_slots::direct_entry_track_rows(files))
}

pub(crate) fn blank_source_for_tracks(
    tracks: Vec<crate::import::TrackUserEdit>,
) -> CandidateSourceDraft {
    let draft = RawReleaseEdit::from_user_edit(
        ReleaseUserEdit {
            album_title: String::new(),
            album_artist_assignments: Vec::new(),
            album_year: None,
            pressing: crate::import::PressingEdit::blank(),
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

/// Normalize a source projection into the one candidate draft: the table's
/// track rows, in order, each bound as the projection paired it.
pub(crate) fn candidate_draft_from_source(
    pane: PanePick,
) -> Result<CandidateSourceDraft, ImportError> {
    if let Some(reconciliation) = pane.mapping.reconciliation {
        use crate::import::track_slots::SlotReconciliation;
        match reconciliation {
            SlotReconciliation::Agrees { .. } => {}
            SlotReconciliation::MoreFiles { files, tracks }
            | SlotReconciliation::MoreTracks { files, tracks } => {
                return Err(ImportError::MetadataTrackCount {
                    metadata_tracks: tracks as usize,
                    audio_tracks: files as usize,
                });
            }
        }
    }
    let mut draft = pane.edit;
    draft
        .album_artist_assignments
        .retain(|assignment| !assignment.is_blank());
    let track_rows = pane
        .mapping
        .track_sections
        .iter()
        .flat_map(MappingTrackSection::mappings)
        .filter_map(|mapping| match &mapping.becomes {
            MappingBecomes::Track { track, .. } => Some(track.clone()),
            MappingBecomes::AwaitingPick | MappingBecomes::NotIncluded { .. } => None,
        })
        .collect::<Vec<_>>();
    draft.tracks = track_rows;
    detach_candidate_mappings(draft, std::collections::BTreeSet::new())
}

/// Applying metadata preserves the included audio and the identity of every row.
///
/// The discs and sides are the metadata's, whatever the draft grouped the
/// audio into: an LP ripped as one tagged disc takes the record's two sides,
/// and a release that states one disc where the files were tagged two takes
/// one.
pub(crate) fn apply_metadata_tracks(
    proposed: &mut CandidateDraft,
    current: &CandidateDraft,
) -> Result<(), ImportError> {
    if proposed.tracks.len() != current.tracks.len() {
        return Err(ImportError::MetadataTrackCount {
            metadata_tracks: proposed.tracks.len(),
            audio_tracks: current.tracks.len(),
        });
    }
    for (metadata, existing) in proposed.tracks.iter_mut().zip(&current.tracks) {
        metadata.edit.id.clone_from(&existing.edit.id);
        metadata.edit.file.clone_from(&existing.edit.file);
    }
    Ok(())
}

pub(crate) fn file_metadata_tracks(
    metadata: &[CandidateTrack],
    included: &[CandidateTrack],
) -> Vec<CandidateTrack> {
    included
        .iter()
        .map(|existing| {
            let mut updated = metadata
                .iter()
                .find(|track| track.edit.file == existing.edit.file)
                .expect("included audio has file metadata")
                .clone();
            updated.edit.id.clone_from(&existing.edit.id);
            updated
        })
        .collect()
}

pub(crate) fn candidate_draft_from_edit(
    draft: RawReleaseEdit,
) -> Result<CandidateSourceDraft, ImportError> {
    detach_candidate_mappings(draft, std::collections::BTreeSet::new())
}

fn detach_candidate_mappings(
    draft: RawReleaseEdit,
    source_discogs_artist_ids: std::collections::BTreeSet<String>,
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
        source_discogs_artist_ids,
        mapped_credit_discogs_artist_ids,
    })
}

/// Project the stored draft onto the candidate's physical units. Metadata is
/// already authoritative; provenance supplies only the exact external release
/// card and whether source/file durations are independent evidence.
pub(crate) fn draft_pane(
    release: Option<ImportSearchReleaseDetail>,
    files: &CategorizedFiles,
    durations: &SourceDurations,
    draft: &CandidateDraft,
    read: &crate::import::CandidateAsRead,
) -> PanePick {
    let table = crate::import::mapping::draft_mapping_table(files, durations, draft, read);
    PanePick {
        release,
        edit: draft.release_edit(),
        mapping: table,
    }
}

/// Replace changed audio with newly initialized tracks. Metadata belongs to
/// its audio identity, never to the position another file happens to occupy.
pub(crate) fn redraw_draft_for_files(
    previous_files: &CategorizedFiles,
    initialized: CandidateDraft,
    draft: &CandidateDraft,
    ids: &dyn coven::IdProvider,
) -> CandidateDraft {
    let previous_audio: std::collections::HashSet<_> =
        crate::import::track_slots::audio_units(previous_files)
            .into_iter()
            .collect();
    let mut result = draft.clone();
    result.tracks = initialized
        .tracks
        .into_iter()
        .filter_map(|mut fresh| {
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
                return Some(retained);
            }
            if previous_audio.contains(audio) {
                // Available audio absent from the draft was deliberately removed.
                return None;
            }
            fresh.edit.id = ids.new_id();
            fresh.source_index = None;
            Some(fresh)
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

/// The pane for a folder committed as its stored file-tag snapshot describes it.
pub(crate) fn file_metadata_pane(
    candidate: &super::folder_scanner::FolderCandidate,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
    durations: &SourceDurations,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<PanePick, ImportError> {
    let seed = candidate.file_tag_edit(snapshot, clock, ids)?;
    let files = &candidate.files;
    // The folder's own tracklist states no length: a length here would be
    // the folder's own audio compared against itself.
    let source_tracks: Vec<SourceTrack> = seed
        .tracks
        .iter()
        .map(|edit| SourceTrack {
            edit: edit.clone(),
            named_by_source: true,
            duration_ms: None,
        })
        .collect();
    let mapping = table_for(
        files,
        durations,
        &source_tracks,
        FILE_TAG_TRACK_ID_PREFIX,
        TracklistSource::CandidateFiles,
        seed.pressing.format.as_deref(),
    );
    Ok(PanePick {
        release: None,
        edit: edit_form(seed, FILE_TAG_TRACK_ID_PREFIX),
        mapping,
    })
}

/// The album fields of the source reading.
///
/// The track rows are cleared — the mapping table is where a track row is
/// edited, and carrying a second copy of them here would be a second answer to
/// which tracks this release has.
fn edit_form(seed: ReleaseUserEdit, track_id_prefix: &str) -> RawReleaseEdit {
    let mut form = RawReleaseEdit::from_user_edit(seed, track_id_prefix);
    form.tracks.clear();
    form
}

fn table_for(
    files: &CategorizedFiles,
    durations: &SourceDurations,
    source_tracks: &[SourceTrack],
    track_id_prefix: &str,
    source: TracklistSource,
    format: Option<&str>,
) -> MappingTable {
    let slots = slot_table(source_tracks, files, durations);
    mapping_table(
        files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix,
            source,
            format,
        }),
        durations,
    )
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
