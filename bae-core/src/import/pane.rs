//! The whole of what one candidate's pane shows, built from values.
//!
//! Two bodies, one per kind of pick: a release the user chose, described by
//! its archived documents, and the folder read as its own files describe it.
//! Both produce the same three things — the edit form the header binds to, the
//! mapping table, and the source tracks behind it — and both are pure over the
//! measurements and the stored edits they are handed.
//!
//! Candidate preparation runs them once and stores their output. The pane reads
//! that candidate revision, and import commits it without projecting again.

use crate::import::edits::{apply_track_edits, CandidateEditOverlay, CandidateTrackEdit};
use crate::import::folder_scanner::CategorizedFiles;
use crate::import::mapping::{
    mapping_table, MappingBecomes, MappingTable, MappingTrackSection, PickedTracklist,
    TracklistSource,
};
use crate::import::payloads::ReleasePayloads;
use crate::import::probe::SourceDurations;
use crate::import::search::ImportSearchReleaseDetail;
use crate::import::track_slots::{slot_table, SourceTrack};
use crate::import::types::{RawReleaseEdit, ReleaseUserEdit};
use crate::import::{parsed_album_to_user_edit, ImportError};

/// The row identity the mapping table's tracks carry when a picked release
/// names them.
pub const IMPORT_TRACK_ID_PREFIX: &str = "import-track";

/// The row identity they carry when the folder's file tags name them.
pub const FILE_TAG_TRACK_ID_PREFIX: &str = "file-tag-track";

/// The row identity the mapping table's tracks carry for manual entry.
pub const MANUAL_TRACK_ID_PREFIX: &str = "manual-track";

/// Stable identities for the one candidate draft, independent of whichever
/// source last populated it.
pub const CANDIDATE_TRACK_ID_PREFIX: &str = "candidate-track";

/// The source-less editable draft created with a discovered candidate.
/// Candidate files determine only how many physical slots exist; their names
/// and tags do not become metadata until a source is explicitly applied.
#[cfg(test)]
pub(crate) fn blank_candidate_draft(files: &CategorizedFiles) -> RawReleaseEdit {
    blank_candidate_source(files).edit
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
    candidate_draft_from_edit(draft)
}

/// What a pick produces for the pane: the release as its documents describe
/// it, the edit form seeded from it with the stored overlay applied, and the
/// mapping table with the stored row edits applied.
pub struct PanePick {
    /// `None` for a folder read as its own tags — there is no release.
    pub release: Option<ImportSearchReleaseDetail>,
    pub edit: RawReleaseEdit,
    pub mapping: MappingTable,
    pub(crate) source_discogs_artist_ids: std::collections::BTreeSet<String>,
}

pub(crate) struct CandidateSourceDraft {
    pub edit: RawReleaseEdit,
    pub track_mappings: Vec<crate::import::CandidateTrackMappingEdit>,
    pub source_discogs_artist_ids: std::collections::BTreeSet<String>,
    pub mapped_new_discogs_artist_ids: std::collections::BTreeSet<String>,
}

/// Normalize a source projection into the one candidate draft. Physical file
/// bindings remain in the mapping table and are stored independently.
pub(crate) fn candidate_draft_from_source(pane: PanePick) -> CandidateSourceDraft {
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
            MappingBecomes::Track {
                track,
                named_by_source,
                ..
            } => Some((track.clone(), *named_by_source)),
            MappingBecomes::AwaitingPick => None,
        })
        .collect::<Vec<_>>();
    let (tracks, named_by_source) = track_rows.into_iter().unzip();
    draft.tracks = tracks;
    detach_candidate_mappings(draft, pane.source_discogs_artist_ids, named_by_source)
}

pub(crate) fn candidate_draft_from_edit(draft: RawReleaseEdit) -> CandidateSourceDraft {
    let named_by_source = vec![true; draft.tracks.len()];
    detach_candidate_mappings(draft, std::collections::BTreeSet::new(), named_by_source)
}

fn detach_candidate_mappings(
    mut draft: RawReleaseEdit,
    source_discogs_artist_ids: std::collections::BTreeSet<String>,
    named_by_source: Vec<bool>,
) -> CandidateSourceDraft {
    assert_eq!(
        draft.tracks.len(),
        named_by_source.len(),
        "every candidate draft track has one namedness answer"
    );
    let mapped_new_discogs_artist_ids = draft.new_discogs_artist_ids_for_bound_tracks();
    let mut track_mappings = Vec::with_capacity(draft.tracks.len());
    for (position, (track, named_by_source)) in
        draft.tracks.iter_mut().zip(named_by_source).enumerate()
    {
        track.id = format!("{CANDIDATE_TRACK_ID_PREFIX}-{position}");
        track_mappings.push(crate::import::CandidateTrackMappingEdit {
            track_id: track.id.clone(),
            named_by_source,
            dropped: false,
            file: crate::import::CandidateTrackFileBinding::Automatic(track.file.take()),
        });
    }
    CandidateSourceDraft {
        edit: draft,
        track_mappings,
        source_discogs_artist_ids,
        mapped_new_discogs_artist_ids,
    }
}

/// Project the stored draft onto the candidate's physical units. Metadata is
/// already authoritative; provenance supplies only the exact external release
/// card and whether source/file durations are independent evidence.
pub(crate) fn draft_pane(
    release: Option<ImportSearchReleaseDetail>,
    files: &CategorizedFiles,
    durations: &SourceDurations,
    draft: RawReleaseEdit,
    mapping_edits: &[crate::import::edits::CandidateTrackMappingEdit],
    provenance: Option<&crate::import::MetadataProvenance>,
) -> Result<PanePick, ImportError> {
    let table = draft_table(files, durations, &draft, mapping_edits, provenance)?;
    Ok(PanePick {
        release,
        edit: draft,
        mapping: crate::import::edits::apply_track_mapping_edits(table, mapping_edits),
        source_discogs_artist_ids: std::collections::BTreeSet::new(),
    })
}

/// Recalculate automatic file bindings for the current draft against a changed
/// candidate file shape. Stored mappings supply source membership only; their
/// user decisions are carried onto this result by the caller.
pub(crate) fn automatic_mappings_for_draft(
    files: &CategorizedFiles,
    durations: &SourceDurations,
    draft: RawReleaseEdit,
    stored_mappings: &[crate::import::edits::CandidateTrackMappingEdit],
    provenance: Option<&crate::import::MetadataProvenance>,
) -> Result<Vec<crate::import::CandidateTrackMappingEdit>, ImportError> {
    let table = draft_table(files, durations, &draft, stored_mappings, provenance)?;
    Ok(candidate_draft_from_source(PanePick {
        release: None,
        edit: draft,
        mapping: table,
        source_discogs_artist_ids: std::collections::BTreeSet::new(),
    })
    .track_mappings)
}

fn draft_table(
    files: &CategorizedFiles,
    durations: &SourceDurations,
    draft: &RawReleaseEdit,
    mapping_edits: &[crate::import::edits::CandidateTrackMappingEdit],
    provenance: Option<&crate::import::MetadataProvenance>,
) -> Result<MappingTable, ImportError> {
    let source_tracks: Vec<SourceTrack> = draft
        .tracks
        .iter()
        .map(|track| {
            let named_by_source = mapping_edits
                .iter()
                .find(|mapping| mapping.track_id == track.id)
                .ok_or_else(|| ImportError::Internal {
                    detail: format!("candidate track {} has no stored source row", track.id),
                })?
                .named_by_source;
            Ok(SourceTrack {
                edit: crate::import::TrackUserEdit {
                    title: track.title.clone(),
                    artist_assignments: track.artist_assignments.clone(),
                    side: track.side,
                    track_number: track.track_number,
                    file: None,
                },
                named_by_source,
                duration_ms: None,
            })
        })
        .collect::<Result<_, ImportError>>()?;
    let source = match provenance {
        Some(crate::import::MetadataProvenance::ExternalRelease { .. }) => {
            TracklistSource::ExternalRelease
        }
        Some(crate::import::MetadataProvenance::FileTags) | None => TracklistSource::CandidateFiles,
    };
    Ok(table_for(
        files,
        durations,
        &source_tracks,
        CANDIDATE_TRACK_ID_PREFIX,
        source,
        Some(draft.pressing.format.as_str()),
        &[],
    ))
}

/// The pane for a release pick, from the documents already archived for it.
///
/// The seed is the candidate preparation projection, so the form shows every
/// album artist the release credits and an untouched artist list remains intact.
pub fn release_pane(
    payloads: &ReleasePayloads,
    files: &CategorizedFiles,
    durations: &SourceDurations,
    overlay: &CandidateEditOverlay,
    track_edits: &[CandidateTrackEdit],
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<PanePick, ImportError> {
    let audio_durations = crate::import::track_slots::audio_durations(files, durations)?;
    let detail = payloads.detail_for_audio(&audio_durations)?;
    let mut parsed = payloads.parsed_for_audio(&audio_durations, clock, ids)?;
    let seed = parsed_album_to_user_edit(&parsed);
    let source_tracks = source_tracks_of(&seed, &detail);
    let mapping = table_for(
        files,
        durations,
        &source_tracks,
        IMPORT_TRACK_ID_PREFIX,
        TracklistSource::ExternalRelease,
        seed.pressing.format.as_deref(),
        track_edits,
    );
    let mapped_tracks = crate::import::mapping_tracks(&mapping);
    retain_mapped_source_track_metadata(&mut parsed, &mapped_tracks, IMPORT_TRACK_ID_PREFIX);
    let source_discogs_artist_ids = source_discogs_artist_ids(&parsed);
    Ok(PanePick {
        release: Some(detail),
        edit: edit_form(seed, IMPORT_TRACK_ID_PREFIX, overlay),
        mapping,
        source_discogs_artist_ids,
    })
}

pub(crate) fn retain_mapped_source_track_metadata(
    parsed: &mut crate::import::ParsedAlbum,
    mapped_tracks: &[crate::import::RawTrackEdit],
    track_id_prefix: &str,
) {
    let retained_track_ids = parsed
        .tracks
        .iter()
        .enumerate()
        .filter(|(position, _)| {
            let expected_id = format!("{track_id_prefix}-{position}");
            mapped_tracks
                .iter()
                .any(|track| track.id == expected_id && track.file.is_some())
        })
        .map(|(_, track)| track.id.clone())
        .collect();
    crate::import::service::retain_track_metadata(parsed, &retained_track_ids);
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
pub(crate) fn file_tags_pane(
    candidate: &super::release_candidate::ReleaseCandidate,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
    durations: &SourceDurations,
    overlay: &CandidateEditOverlay,
    track_edits: &[CandidateTrackEdit],
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<PanePick, ImportError> {
    let seed = candidate.file_tag_edit(snapshot, clock, ids)?;
    let files = candidate.files();
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
        track_edits,
    );
    Ok(PanePick {
        release: None,
        edit: edit_form(seed, FILE_TAG_TRACK_ID_PREFIX, overlay),
        mapping,
        source_discogs_artist_ids: std::collections::BTreeSet::new(),
    })
}

/// The pane for metadata entered without consulting tags or online sources.
/// The form begins blank while the mapping retains only physical track slots.
pub fn manual_pane(
    files: &CategorizedFiles,
    durations: &SourceDurations,
    overlay: &CandidateEditOverlay,
    track_edits: &[CandidateTrackEdit],
) -> PanePick {
    let tracks = crate::import::track_slots::direct_entry_track_rows(files);
    let source_tracks: Vec<SourceTrack> = tracks
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
        MANUAL_TRACK_ID_PREFIX,
        TracklistSource::CandidateFiles,
        None,
        track_edits,
    );
    let seed = ReleaseUserEdit {
        album_title: String::new(),
        album_artist_assignments: Vec::new(),
        album_year: None,
        pressing: crate::import::PressingEdit::blank(),
        tracks,
    };
    PanePick {
        release: None,
        edit: edit_form(seed, MANUAL_TRACK_ID_PREFIX, overlay),
        mapping,
        source_discogs_artist_ids: std::collections::BTreeSet::new(),
    }
}

/// The table for a folder nobody has picked a release for: every source unit
/// the folder offers, with what it becomes left open.
pub fn unpicked_mapping(files: &CategorizedFiles, durations: &SourceDurations) -> MappingTable {
    mapping_table(files, None, durations)
}

/// The edit form: the seed with the stored overlay laid over it.
///
/// The track rows are cleared — the mapping table is where a track row is
/// edited, and carrying a second copy of them here would be a second answer to
/// which tracks this release has.
fn edit_form(
    seed: ReleaseUserEdit,
    track_id_prefix: &str,
    overlay: &CandidateEditOverlay,
) -> RawReleaseEdit {
    let mut form = RawReleaseEdit::from_user_edit(seed, track_id_prefix);
    form.tracks.clear();
    overlay.apply(form)
}

/// The source's tracks, paired with the length it printed. Every one is in
/// the source's tracklist — that is what the seed is.
fn source_tracks_of(
    seed: &ReleaseUserEdit,
    detail: &ImportSearchReleaseDetail,
) -> Vec<SourceTrack> {
    seed.tracks
        .iter()
        .enumerate()
        .map(|(index, edit)| SourceTrack {
            edit: edit.clone(),
            named_by_source: true,
            duration_ms: detail.tracks.get(index).and_then(|track| track.duration_ms),
        })
        .collect()
}

fn table_for(
    files: &CategorizedFiles,
    durations: &SourceDurations,
    source_tracks: &[SourceTrack],
    track_id_prefix: &str,
    source: TracklistSource,
    format: Option<&str>,
    track_edits: &[CandidateTrackEdit],
) -> MappingTable {
    let slots = slot_table(source_tracks, files, durations);
    let table = mapping_table(
        files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix,
            source,
            format,
        }),
        durations,
    );
    apply_track_edits(table, track_edits)
}
