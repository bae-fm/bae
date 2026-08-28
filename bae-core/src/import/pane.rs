//! The whole of what one candidate's pane shows, built from values.
//!
//! Two bodies, one per kind of pick: a release the user chose, described by
//! its archived documents, and the folder read as its own files describe it.
//! Both produce the same three things — the edit form the header binds to, the
//! mapping table, and the source tracks behind it — and both are pure over the
//! measurements and the stored edits they are handed.
//!
//! Two callers run them: the per-candidate query, so a selection draws
//! complete, and the commit, so what it writes is what the pane showed.

use crate::import::edits::{apply_track_edits, CandidateEditOverlay, CandidateTrackEdit};
use crate::import::folder_scanner::CategorizedFiles;
use crate::import::mapping::{mapping_table, MappingTable, PickedTracklist, TracklistSource};
use crate::import::payloads::ReleasePayloads;
use crate::import::probe::ProbedDurations;
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

/// What a pick produces for the pane: the release as its documents describe
/// it, the edit form seeded from it with the stored overlay applied, and the
/// mapping table with the stored row edits applied.
pub struct PanePick {
    /// `None` for a folder read as its own tags — there is no release.
    pub release: Option<ImportSearchReleaseDetail>,
    pub edit: RawReleaseEdit,
    pub mapping: MappingTable,
}

/// The pane for a release pick, from the documents already archived for it.
///
/// The seed is the commit worker's own projection — `prepare_release` mapped
/// into the editor's shape — so the form shows every album artist the release
/// credits, and an untouched artist list compares equal at commit instead of
/// reading as a deletion.
pub fn release_pane(
    payloads: &ReleasePayloads,
    files: &CategorizedFiles,
    durations: &ProbedDurations,
    overlay: &CandidateEditOverlay,
    track_edits: &[CandidateTrackEdit],
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<PanePick, ImportError> {
    let detail = payloads.detail()?;
    let parsed = payloads.parsed(clock, ids)?;
    let seed = parsed_album_to_user_edit(&parsed);
    let source_tracks = source_tracks_of(&seed, &detail);
    let mapping = table_for(
        files,
        durations,
        &source_tracks,
        IMPORT_TRACK_ID_PREFIX,
        TracklistSource::ExternalRelease,
        track_edits,
    );
    Ok(PanePick {
        release: Some(detail),
        edit: edit_form(seed, IMPORT_TRACK_ID_PREFIX, overlay),
        mapping,
    })
}

/// The pane for a folder committed as its stored file-tag snapshot describes it.
pub(crate) fn file_tags_pane(
    files: &CategorizedFiles,
    snapshot: &crate::import::file_tag_snapshot::FileTagSnapshot,
    folder_name: Option<&str>,
    durations: &ProbedDurations,
    overlay: &CandidateEditOverlay,
    track_edits: &[CandidateTrackEdit],
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<PanePick, ImportError> {
    let parsed = crate::import::file_tag_mapper::map_file_tag_snapshot_to_db(
        files,
        snapshot,
        folder_name,
        clock,
        ids,
    )?;
    let seed = parsed_album_to_user_edit(&parsed);
    // The folder's own tracklist knows no position string beyond the track
    // number it printed, and states no length: a length here would be the
    // folder's own audio compared against itself.
    let source_tracks: Vec<SourceTrack> = seed
        .tracks
        .iter()
        .map(|edit| SourceTrack {
            edit: edit.clone(),
            position: edit.track_number.map(|n| n.to_string()),
            duration_ms: None,
        })
        .collect();
    let mapping = table_for(
        files,
        durations,
        &source_tracks,
        FILE_TAG_TRACK_ID_PREFIX,
        TracklistSource::CandidateFiles,
        track_edits,
    );
    Ok(PanePick {
        release: None,
        edit: edit_form(seed, FILE_TAG_TRACK_ID_PREFIX, overlay),
        mapping,
    })
}

/// The pane for metadata entered without consulting tags or online sources.
/// The form begins blank while the mapping retains only physical track slots.
pub fn manual_pane(
    files: &CategorizedFiles,
    durations: &ProbedDurations,
    overlay: &CandidateEditOverlay,
    track_edits: &[CandidateTrackEdit],
) -> PanePick {
    let tracks = crate::import::track_slots::manual_track_rows(files);
    let source_tracks: Vec<SourceTrack> = tracks
        .iter()
        .map(|edit| SourceTrack {
            edit: edit.clone(),
            position: None,
            duration_ms: None,
        })
        .collect();
    let mapping = table_for(
        files,
        durations,
        &source_tracks,
        MANUAL_TRACK_ID_PREFIX,
        TracklistSource::CandidateFiles,
        track_edits,
    );
    let seed = ReleaseUserEdit {
        album_title: String::new(),
        album_artist_assignments: Vec::new(),
        pressing: crate::import::PressingEdit::blank(),
        tracks,
    };
    PanePick {
        release: None,
        edit: edit_form(seed, MANUAL_TRACK_ID_PREFIX, overlay),
        mapping,
    }
}

/// The table for a folder nobody has picked a release for: every source unit
/// the folder offers, with what it becomes left open.
pub fn unpicked_mapping(files: &CategorizedFiles, durations: &ProbedDurations) -> MappingTable {
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

/// The source's tracks, paired with the position and length it printed.
///
/// The seed and the detail describe the same release and come out the same
/// length, so the two ride each seeded row by position. A row the detail runs
/// out for falls back to its own track number, which is the only position
/// anyone could read.
fn source_tracks_of(
    seed: &ReleaseUserEdit,
    detail: &ImportSearchReleaseDetail,
) -> Vec<SourceTrack> {
    seed.tracks
        .iter()
        .enumerate()
        .map(|(index, edit)| {
            let source = detail.tracks.get(index);
            SourceTrack {
                edit: edit.clone(),
                position: match source {
                    Some(track) => Some(track.position.clone()),
                    None => edit.track_number.map(|n| n.to_string()),
                },
                duration_ms: source.and_then(|track| track.duration_ms),
            }
        })
        .collect()
}

fn table_for(
    files: &CategorizedFiles,
    durations: &ProbedDurations,
    source_tracks: &[SourceTrack],
    track_id_prefix: &str,
    source: TracklistSource,
    track_edits: &[CandidateTrackEdit],
) -> MappingTable {
    let slots = slot_table(source_tracks, files, durations);
    let table = mapping_table(
        files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix,
            source,
        }),
        durations,
    );
    apply_track_edits(table, track_edits)
}
