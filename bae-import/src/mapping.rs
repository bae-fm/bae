//! The mapping table: every source unit the folder offers, alongside the track
//! committing makes of it.
//!
//! The draft holds one track per audio unit, in the folder's order, so row `i`
//! pairs unit `i` with draft track `i`. Surfaces render this projection without
//! joining separate lists.

use crate::folder_scanner::{
    AudioDamage, CandidateFile, CategorizedFiles, FileRole, InvalidCandidate, ProposedRole,
    SheetDisc, SheetReferenceOptions,
};
use crate::types::RawTrackEdit;

/// The mapping table: every source unit the folder offers, alongside the track
/// committing makes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingTable {
    /// Every image the folder holds, in the scan's authoritative order.
    pub images: Vec<MappingImage>,
    /// The rows that can become release tracks, laid out by side, with a
    /// bound sheet retaining ownership of the slices it carves.
    pub track_sides: Vec<MappingTrackSide>,
    /// Files carried with the release but not represented by track rows.
    pub files: Vec<MappingFileRow>,
    /// How many track rows nobody has named yet. Committing anyway titles
    /// each after its file.
    pub untitled_track_count: u32,
}

/// One side of the track table — a disc of a multi-disc set, a side of a
/// record, or the one flat side — and the runs of rows on it, in table order.
/// The same shape the release editor's table draws
/// ([`crate::ReleaseEditTrackSide`]): one heading per side,
/// however many runs it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingTrackSide {
    pub side: bae_types::mediums::TrackSide,
    pub runs: Vec<MappingTrackRun>,
}

/// Consecutive rows of one side that the table draws together. The run makes
/// the physical source shape explicit: independent track rows, or the entries
/// carved by one track sheet.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingTrackRun {
    /// Rows supplied independently rather than carved by a track sheet.
    Tracks(Vec<TrackMapping>),
    /// A track sheet and the entries it carves, which are its child rows.
    Sheet { sheet: SheetGroup, entries: Vec<TrackMapping> },
}

impl MappingTrackRun {
    /// The source-to-track mappings this run carries.
    pub fn mappings(&self) -> &[TrackMapping] {
        match self {
            MappingTrackRun::Tracks(mappings) => mappings,
            MappingTrackRun::Sheet { entries, .. } => entries,
        }
    }
}

impl MappingTrackSide {
    /// The source-to-track mappings on this side, in table order.
    pub fn mappings(&self) -> impl Iterator<Item = &TrackMapping> {
        self.runs.iter().flat_map(MappingTrackRun::mappings)
    }
}

/// One row in the files section of the mapping table.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingFileRow {
    File(MappingFile),
    /// A sheet that currently carves no track rows and can be assigned audio.
    Sheet(SheetGroup),
}

/// One of the folder's images, as the gallery shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingImage {
    /// The file's identity within the release (its relative path).
    pub file_id: String,
    /// The file's own name, without its directory prefix.
    pub name: String,
    /// The version the scan read — what a thumbnail and the lightbox draw.
    pub file: crate::folder_scanner::FileVersion,
}

/// One source-to-track mapping row: an audio unit of the folder, and the
/// draft track committing makes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackMapping {
    pub source: MappingSource,
    /// The track of the release being committed. The row edits it in place.
    pub track: RawTrackEdit,
    /// The position this row commits, rendered from the track's own side and
    /// number and the release's format — `8`, `A1`, or `3` beneath a `Disc 2`
    /// heading.
    pub position: String,
    /// The duration this row displays: the applied release's length for the
    /// track where it lists one, otherwise the candidate's stored probe.
    pub duration_ms: Option<u64>,
    /// Whether the folder's length for this row and the applied release's
    /// are far enough apart for the row to say so.
    pub lengths_disagree: bool,
}

/// The left half of a row: what the folder offers for it.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingSource {
    /// A file the folder holds, whole.
    File(MappingFile),
    /// One entry of a track sheet, carved out of the container it is bound to.
    SheetEntry(MappingEntry),
}

/// What one of a release's files is, as a row of the mapping table or of a
/// library release's files.
///
/// Narrower than the role the scan proposes: a folder's track sheet is not a
/// row here — it heads a group of rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingRole {
    /// Playable audio.
    Audio,
    /// Readable evidence: a rip log, a tracklist, a playlist, a CUE that
    /// could not be parsed as a sheet, or a library release's track sheet,
    /// which reads as the text it is.
    Document,
    /// An image the release carries, listed with its other files; a folder's
    /// images are also the gallery above its tracks.
    Image,
    /// Named as an image, but its bytes do not decode as one: carried with
    /// the release, and kept out of the gallery the cover is picked from.
    DamagedImage,
    /// Named as audio, and bae can't play it. Only a folder that failed
    /// validation holds one — it is why the folder failed.
    DamagedAudio { damage: AudioDamage },
    /// In the folder and carried with the release, unrecognized.
    Other,
}

/// A file of the folder, as the mapping table's left half shows it, or a
/// file of a library release, as its editor lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingFile {
    /// The id the file is read by: in a folder, its relative path, which the
    /// sheet bindings take; in the library, its release file's id.
    pub file_id: String,
    /// The file's identity within the release — its relative path, the same
    /// name the storage manager lists it under. Every file lists flat; a
    /// directory shows up only as the prefix its files carry.
    pub name: String,
    pub size: u64,
    /// The whole-file audition target when this file currently supplies audio.
    pub preview_target: Option<bae_track_audio::PreviewTarget>,
    /// Playing time from the scan's stored facts. `None` for non-audio files.
    pub duration_ms: Option<u64>,
    pub audio_format: Option<bae_types::audio_format::AudioFormat>,
    pub role: MappingRole,
}

/// One entry of a track sheet, as the mapping table's left half shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingEntry {
    pub sheet_id: String,
    /// Counts this sheet's playable entries from zero — the index the audio
    /// binding carries.
    pub index: u32,
    /// The number the sheet prints for this entry.
    pub number: u32,
    pub title: Option<String>,
    /// This slice's stored source duration: the next sheet boundary, or the
    /// scanned container duration closing the final entry.
    pub duration_ms: Option<u64>,
    /// The container this entry's samples come from — what auditioning plays.
    pub container_id: String,
    pub container_name: String,
    /// The exact window of the container that auditioning this entry plays.
    pub preview_target: bae_track_audio::PreviewTarget,
    pub audio_format: bae_types::audio_format::AudioFormat,
}

/// A track sheet, as the header of the group of rows it carves.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetGroup {
    pub sheet_id: String,
    pub name: String,
    pub size: u64,
    pub bound: SheetBound,
    /// Current FILE associations and choices, independent of track inclusion.
    pub reference_options: Vec<SheetReferenceOptions>,
    pub assignment: SheetDisc,
    /// The discs this sheet may be assigned to, counting from one.
    pub disc_options: Vec<u32>,
}

/// What a track sheet describes, with the facts its header shows about it.
///
/// [`SheetBinding`] enriched by the
/// container's name and size: a header states both which audio a sheet is on and
/// why it is on none, and carrying the binding separately would be a second way
/// to say the first.
#[derive(Debug, Clone, PartialEq)]
pub enum SheetBound {
    /// The sheet describes this audio.
    Describes(MappingContainer),
    /// The sheet describes several audio files, one per distinct `FILE`
    /// reference. The rows name their own physical files; the group header has
    /// no single container to name.
    DescribesFiles { audio_file_count: u32 },
    /// It describes nothing: the directive named audio that is not in the
    /// folder, named several and only some are here, or the user cleared the
    /// binding. `requested` is what the directive asked for, so the header can
    /// say what the sheet was looking for while it offers the folder's own
    /// audio instead.
    Unresolved { requested: Vec<String> },
    /// The directive resolved, but bae cannot carve tracks out of that codec.
    /// The physical audio files import independently.
    RefusedCodec { codec: String },
    /// All files are associated, but their lengths cannot contain the CUE boundaries.
    RefusedTiming,
}

/// The audio a track sheet describes.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingContainer {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub audio_format: bae_types::audio_format::AudioFormat,
}

/// The folder's images, in the scan's order: its gallery.
pub(crate) fn folder_images(files: &CategorizedFiles) -> Vec<MappingImage> {
    files
        .files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Artwork))
        .map(mapping_image)
        .collect()
}

/// The table's track rows in commit order — what the editor shapes into the
/// release it writes.
pub fn mapping_tracks(table: &MappingTable) -> Vec<RawTrackEdit> {
    table
        .track_sides
        .iter()
        .flat_map(MappingTrackSide::mappings)
        .map(|mapping| mapping.track.clone())
        .collect()
}

/// One of the folder's images, as the gallery carries it.
///
/// Which image leads the release is not a property of the image: it is the
/// cover choice, which the stored row answers first, then the picked release's
/// own art, then the folder's images by name. The gallery lists what the folder
/// has; the card shows what was chosen.
fn mapping_image(entry: &CandidateFile) -> MappingImage {
    MappingImage {
        file_id: entry.file.relative_path.clone(),
        name: entry.file.file_name.clone(),
        file: entry.file.version.clone(),
    }
}

/// A folder that failed validation, as the pane looking into it reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidCandidateDetail {
    pub candidate: InvalidCandidate,
    /// The images that decode, in name order, as the gallery shows them.
    pub images: Vec<MappingImage>,
    /// Every other file, in name order, each with what it is — the damaged
    /// audio among them is why the folder failed.
    pub files: Vec<MappingFile>,
}

impl InvalidCandidateDetail {
    pub fn of(candidate: InvalidCandidate) -> Self {
        let mut images = Vec::new();
        let mut files = Vec::new();
        for skipped in &candidate.files {
            let entry = |role| {
                let scanned = skipped.scanned();
                MappingFile {
                    file_id: scanned.relative_path.clone(),
                    name: scanned.relative_path.clone(),
                    size: scanned.version.size,
                    preview_target: (role == MappingRole::Audio)
                        .then(|| bae_track_audio::PreviewTarget::whole_file(scanned.version.path)),
                    duration_ms: None,
                    audio_format: None,
                    role,
                }
            };
            match skipped.role {
                ProposedRole::Image => images.push(MappingImage {
                    file_id: skipped.relative_path.clone(),
                    name: skipped.scanned().file_name,
                    file: skipped.file.clone(),
                }),
                ProposedRole::Audio => files.push(entry(MappingRole::Audio)),
                ProposedRole::DamagedAudio { damage } => {
                    files.push(entry(MappingRole::DamagedAudio { damage }))
                }
                // A sheet in a folder that is never imported is never bound:
                // it is the text it is.
                ProposedRole::Cue | ProposedRole::Document { .. } => {
                    files.push(entry(MappingRole::Document))
                }
                ProposedRole::DamagedImage => files.push(entry(MappingRole::DamagedImage)),
                ProposedRole::Other => files.push(entry(MappingRole::Other)),
            }
        }
        Self { candidate, images, files }
    }
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
pub(crate) mod tests;
