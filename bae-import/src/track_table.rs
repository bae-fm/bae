//! A release's track table, as the release editor draws it in the import
//! pane and in the Edit release sheet: one heading per side or disc, and on
//! it the tracks that play a whole file, each naming its file, and the
//! tracks a CUE sheet carves, under the sheet's caption. The import pane's
//! draft and a library release's editor's draft each hand the app this one
//! shape, so a release reads the same before and after it is imported.

use crate::folder_scanner::SheetReferenceOptions;
use crate::types::RawTrackEdit;

/// One side of the track table (a disc of a multi-disc set, a side of a
/// record, or the one flat side) and the runs of rows on it, in table
/// order, under one heading however many runs it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackTableSide {
    pub side: bae_types::mediums::TrackSide,
    pub runs: Vec<TrackTableRun>,
}

impl TrackTableSide {
    /// The rows on this side, in table order.
    pub fn rows(&self) -> impl Iterator<Item = &TrackTableRow> {
        self.runs.iter().flat_map(TrackTableRun::rows)
    }
}

/// Consecutive rows of one side that the table draws together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackTableRun {
    /// A track that plays a whole file, which its row names.
    Track(TrackTableRow),
    /// The tracks one CUE sheet carves on this side, under the sheet's
    /// caption, each row giving where it starts; a sheet the release splits
    /// across sides heads its tracks again on the next side.
    Sheet { sheet: SheetGroup, rows: Vec<TrackTableRow> },
}

impl TrackTableRun {
    /// The rows this run draws, in table order.
    pub fn rows(&self) -> &[TrackTableRow] {
        match self {
            Self::Track(row) => std::slice::from_ref(row),
            Self::Sheet { rows, .. } => rows,
        }
    }
}

/// One track of the table: the form row that edits its title and artists,
/// what its source cell names, what playing the row plays, its place, its
/// lengths and its own artists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackTableRow {
    /// The track as the draft holds it. The row edits it in place.
    pub track: RawTrackEdit,
    pub source: TrackSource,
    pub audition: TrackAudition,
    /// How the track's place reads (`8`, `A1`, or `3` under a `Disc 2`
    /// heading), which the table shows and never edits.
    pub position: String,
    /// The length the track's audio measures: the length the library
    /// stores for it, or will store once the import writes it.
    pub length_ms: u64,
    /// The length the track's source lists, kept only when it is more than
    /// [`bae_catalogs::medium_coverage::LENGTH_DISAGREEMENT_MS`] from
    /// `length_ms`.
    pub listed_length_ms: Option<u64>,
    /// The track's own artists, each with how it stands to the library, as
    /// the read that returned the row found them; `None` when the track
    /// takes its album's artists or names none.
    pub artists: Option<crate::ArtistField>,
}

/// What a row's source cell names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackSource {
    /// The file the track plays whole.
    File { name: String },
    /// Where the track's body starts, in milliseconds, in the file its
    /// sheet's caption names: its INDEX 01.
    SheetSlice { start_ms: u64 },
}

/// What playing a row plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackAudition {
    /// The preview player plays this window of one of the folder's files,
    /// pausing the library's playback.
    Preview(bae_track_audio::PreviewTarget),
    /// The library plays the release from the row's track, the library
    /// track the row's `track.id` names.
    LibraryTrack,
    /// The row is a track a save of the editor adds, which the library does
    /// not hold yet, so nothing plays it.
    Unsaved,
}

/// A track sheet, as the caption over the rows it carves, or as a line of
/// the Files list when it carves none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetGroup {
    /// The sheet's file id: in a folder, its relative path; in the library,
    /// its release file's id.
    pub sheet_id: String,
    pub name: String,
    pub bound: SheetBound,
    /// The sheet's FILE lines that play audio, as the caption's Audio file
    /// pop-up offers them; independent of track inclusion.
    pub bound_audio: SheetBoundAudio,
    /// The sheet's FILE lines that play no audio, in the sheet's order, each
    /// offered from a menu of its own; `None` where every line plays audio.
    pub unbound_references: Option<Vec<SheetReferenceOptions>>,
    /// The side or disc the sheet is on, with its heading, which the
    /// caption's place pop-up names; `None` where the sheet is ignored.
    pub assigned: Option<bae_records::DbSheetSideChoice>,
    /// The sides and discs this sheet can be put on, in order.
    pub side_choices: Vec<bae_records::DbSheetSideChoice>,
}

/// The FILE lines of a track sheet that play audio, by how the caption's
/// Audio file pop-up offers them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetBoundAudio {
    /// No line plays audio, so the caption names what the sheet describes
    /// as text.
    None,
    /// One line plays audio: its choices sit directly in the pop-up.
    One(SheetReferenceOptions),
    /// Several lines play audio: the pop-up has a submenu per line, in the
    /// sheet's order.
    Several(Vec<SheetReferenceOptions>),
}

/// What a track sheet describes, with the facts its caption shows about it.
///
/// [`SheetBinding`](crate::folder_scanner::SheetBinding) enriched by the
/// container's name: a caption states both which audio a sheet is on and
/// why it is on none, and carrying the binding separately would be a second
/// way to say the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetBound {
    /// The sheet describes this audio.
    Describes(MappingContainer),
    /// The sheet describes several audio files, one per distinct `FILE`
    /// reference. The caption has no single container to name.
    DescribesFiles { audio_file_count: u32 },
    /// It describes nothing: the directive named audio that is not in the
    /// folder, named several and only some are here, or the user cleared the
    /// binding. `requested` is what the directive asked for, so the caption
    /// can say what the sheet was looking for while it offers the folder's
    /// own audio instead.
    Unresolved { requested: Vec<String> },
    /// The directive resolved, but bae cannot carve tracks out of that codec.
    /// The physical audio files import independently.
    RefusedCodec { codec: String },
    /// All files are associated, but their lengths cannot contain the CUE
    /// boundaries.
    RefusedTiming,
}

/// One of a release's audio files, by its id and the name the screens show:
/// the audio a track sheet describes, the file a `FILE` line plays from, or
/// the file a library track plays.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MappingContainer {
    pub file_id: String,
    pub name: String,
}
