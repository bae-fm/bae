use super::*;

/// Raw release-detail aggregate, which the release detail view resolves into
/// the display-ready release detail.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseDetail {
    pub release: DbRelease,
    /// The release's tracks added up: how long they play.
    pub total_duration_ms: u64,
    /// The release's tracks in playing order, a group for each side.
    pub track_groups: Vec<DbTrackGroup>,
    /// The release's credits in every role, in the order its detail lists
    /// them: the release's own, then each track's in playing order, a
    /// track's own before the non-writing credits of the works it performs.
    pub credits: Vec<DbCreditRow>,
    /// Whether the release has any credit.
    pub has_credits: bool,
    /// Whether any track of the release performs a work.
    pub performs_works: bool,
    pub files: DbReleaseFiles,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
    /// Whether any catalog describes this release.
    pub has_records: bool,
    /// The device that alone holds the release's audio file with the lowest
    /// file id among the files another device alone holds. The field is
    /// `None` when no other device alone holds any of the release's audio
    /// files. The release cannot play here while the field is `Some`.
    pub elsewhere: Option<OtherDevice>,
    /// Whether this device keeps every file of the release pinned, as the
    /// release's storage row counts its pinned files.
    pub pinned: bool,
    /// Whether every file of the release names its stored file, as the
    /// release's storage row says.
    pub remote: bool,
    /// What of the release waits in this device's upload queue.
    pub upload: Option<DbReleaseUpload>,
    /// The version of the release's cover; `None` when it has none.
    pub cover_version: Option<String>,
    /// The release's place among its album's releases, counted from 1 by
    /// album order, then id.
    pub release_number: i64,
}

/// A track as the release editor reads its audio: the track, its place on the
/// release, the length listed for it there, how many audio formats it
/// stores, how many main segments they hold, the first of those, the track's
/// body, with the file it names, and the CUE sheet that carves it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseEditTrack {
    pub track: DbTrack,
    pub position: bae_types::mediums::TrackPosition,
    /// The length the source of the track's place lists for it (the catalog
    /// the editor applied, else the one the library's values came from), when
    /// it is further from the track's measured length than the import pane
    /// tolerates; `None` where it lists none or the two agree.
    pub listed_length_ms: Option<u64>,
    pub audio_format_count: i64,
    pub main_segment_count: i64,
    pub main_segment: Option<DbReleaseEditSegment>,
    /// The CUE sheet that carves the track out of an image; `None` for a
    /// track that plays a whole file.
    pub sheet: Option<DbReleaseEditSheet>,
}

/// A track a save of the release editor's form adds, which plays a whole
/// audio file no track plays any more: the file, its length and the place
/// the form's layout gives the track.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseEditAdded {
    pub file: crate::records::sheet_files::AddedFile,
    pub duration_ms: u64,
    pub position: bae_types::mediums::TrackPosition,
}

/// One row of the release editor's tracks statement: a track the library
/// holds, a track the form's save adds, or a side or disc that the draft
/// keeps the heading of, with no track on it.
#[derive(Debug, Clone, PartialEq)]
pub enum DbReleaseEditRow {
    Stored(Box<DbReleaseEditTrack>),
    Added(DbReleaseEditAdded),
    EmptySide(bae_types::mediums::TrackSide),
}

/// A CUE sheet that carves a track out of an image, as the release editor
/// names it: its file's id and name, the side of the release its first
/// track sits on with that side's heading, as the release's choices of side
/// name it, how many `FILE` lines of the sheet carve the release's tracks,
/// the text of the sheet's `FILE` line the track is under, and the audio
/// files of the release that line can play from, in file name order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DbReleaseEditSheet {
    pub file_id: String,
    pub name: String,
    pub place: DbSheetSideChoice,
    pub side_choices: Vec<DbSheetSideChoice>,
    pub line_count: u32,
    /// The sheet's `FILE` lines that play no audio once the form's edits are
    /// written, in the order the sheet's tracks reach them.
    pub unbound_lines: Option<Vec<crate::records::sheet_files::DbSheetLine>>,
    pub file_reference: String,
    pub audio_options: Vec<DbSheetAudioOption>,
}

/// One audio file of a release as a choice for a `FILE` line of one of its
/// CUE sheets, as the release editor's
/// tracks statement lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DbSheetAudioOption {
    pub file_id: String,
    pub name: String,
    pub content_type: ContentType,
    /// Whether the line plays from this file once the picks are written.
    pub bound: bool,
    /// Whether the file is long enough for every track the line carves: no
    /// track's body ends, or the last one starts, past the file's end.
    pub fits: bool,
}

/// One side or disc a CUE sheet's entries can be put on: the side of the
/// medium, and the heading the release's screens name it by. A disc of a
/// release of one medium, which no heading names, is "Disc 1" here, since
/// the choice needs a name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DbSheetSideChoice {
    pub side: bae_types::mediums::MediumSide,
    pub heading: bae_types::mediums::TrackSide,
}

/// The SQL columns of a CUE sheet's side or disc choice.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DbSheetSideColumns {
    pub(crate) medium: u32,
    pub(crate) side: u32,
    pub(crate) kind: String,
    pub(crate) ordinal: u32,
}

/// A track's main segment as the release editor reads it: the file it names,
/// where in that file it starts, and that file's name and stored audio
/// layout; `file` is `None` when no release file has that id.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseEditSegment {
    pub file_id: String,
    /// Where the segment starts in its file, in milliseconds: its first
    /// sample at the track's sample rate.
    pub start_ms: u64,
    pub file: Option<DbReleaseEditSourceFile>,
}

/// The file a release editor's segment names: its name, and its stored audio
/// layout (`None` when it has none).
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseEditSourceFile {
    pub name: String,
    pub layout: Option<crate::source_audio::SourceAudioLayout>,
}

/// One file this process is sending now: when it is one of a release's
/// files, its release and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSendingFile {
    pub file: coven::FileId,
    pub release_id: Option<String>,
    pub file_size: Option<u64>,
}

/// A release with files this process is sending now: of its whole size, the
/// bytes of its files stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSendingRelease {
    pub release_id: String,
    pub bytes_stored: u64,
    pub bytes_total: u64,
}

/// The files this process is sending now, the releases they belong to, and
/// whether uploads are paused, as one read finds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSendingUploads {
    pub files: Vec<DbSendingFile>,
    pub releases: Vec<DbSendingRelease>,
    pub paused: bool,
}

/// The other device a file is on, as coven's registration of it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtherDevice {
    /// A device of the library, by its name.
    Active { name: String },
    /// A device that lost access to the library, by its name: removed, or
    /// replaced when it was reset. Its files stay on it, and no device can
    /// move them to the cloud any more.
    Former { name: String, state: bae_store::FormerDeviceState },
}

impl OtherDevice {
    /// The device named `name` that coven registers in `state`.
    pub fn of(name: String, state: coven::DeviceState) -> Self {
        match bae_store::FormerDeviceState::of(state) {
            Some(state) => Self::Former { name, state },
            None => Self::Active { name },
        }
    }

    /// coven's state of the device.
    pub fn state(&self) -> coven::DeviceState {
        match self {
            Self::Active { .. } => coven::DeviceState::Active,
            Self::Former { state, .. } => state.device_state(),
        }
    }
}

/// The tracks of one side of a release, as its detail groups them: a run of
/// tracks in playing order that sit on one side or disc.
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackGroup {
    pub side: bae_types::mediums::TrackSide,
    pub tracks: Vec<DbTrackWithArtists>,
    /// How long this group's tracks play.
    pub total_duration_ms: u64,
    /// Whether the release has more than one group, so each shows its own
    /// time beside the release's.
    pub shows_time: bool,
    /// The place among the group's tracks of the first one its second column
    /// lists: half of them, rounded up, when the group has more than eight;
    /// `None` when one column lists them all.
    pub second_column_start: Option<u32>,
}

/// A track with its credits, works and the stars this device's person gave
/// it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackWithArtists {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    /// Where the track sits on its release.
    pub position: bae_types::mediums::TrackPosition,
    /// The track's own credits, in list order; none when it takes the
    /// album's.
    pub artists: Vec<crate::credits::LibraryCredit>,
    /// The credits the track shows: its own, else its album's.
    pub shown_artists: Vec<crate::credits::LibraryCredit>,
    /// The works the track performs, in the order it performs them.
    pub works: Vec<DbPerformedWork>,
    /// The stars this device's person gave the track, if any.
    pub my_stars: Option<bae_types::stars::Stars>,
}

/// A work a track performs, as a release's detail names it.
#[derive(Debug, Clone, PartialEq)]
pub struct DbPerformedWork {
    pub work_id: String,
    pub title: String,
    /// The work's artists printed in credit order, the way a work's summary
    /// prints them; `None` when it has none.
    pub composer_names: Option<String>,
}

/// Where a track sits on its release. Playback reads it to decide whether to
/// pause between sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTrackPlace {
    pub track_id: String,
    pub release_id: String,
    /// The medium's carrier, where the source states one.
    pub carrier: Option<bae_types::pressing::Medium>,
    /// The side or disc the release shows the track on.
    pub on: bae_types::mediums::TrackSide,
}

/// A track with how a surface outside bae numbers it: a saved file's track
/// and disc tags and file name, and a Subsonic song.
#[derive(Debug, Clone, PartialEq)]
pub struct DbNumberedTrack {
    pub track: DbTrack,
    /// The track's number among its medium's tracks, from one.
    pub number: u32,
    /// Its medium's position, for a release of several mediums.
    pub disc: Option<u32>,
}

/// One row of a release's credits: a role, the names of the artists who hold
/// it over the same tracks, and those tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCreditRow {
    pub role: bae_types::credit_role::CreditedAs,
    /// The names the artists print, in the order of their first credit.
    pub artists: Vec<String>,
    /// Where each track the row covers sits, in playing order; empty for a
    /// row covering every track of the release.
    pub tracks: Vec<bae_types::mediums::TrackPosition>,
}

/// A release's files as its screens list them: all of them, and the images,
/// covers and other files that the release's pages list separately, with
/// their count and total size.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseFiles {
    /// Every file, in the release's file order.
    pub files: Vec<DbFile>,
    /// The images bae can show, which the gallery shows and the Files list
    /// leaves out.
    pub images: Vec<DbFile>,
    /// The files no track plays from: a file a track's segment reads, and a
    /// CUE sheet that carves a track, are played.
    pub others: Vec<DbFile>,
    /// What the release editor's Files list shows: the files no track plays
    /// from, each CUE sheet none of whose tracks stay in its place among them. Empty
    /// where the read is not an editor's.
    pub carried: Vec<DbCarriedFile>,
    /// Whether any file is one no track plays from.
    pub has_others: bool,
    pub count: i64,
    /// How many files are images bae can show: the files `images` lists.
    pub image_count: u32,
    pub total_size: u64,
    /// What the release's audio is, summarized over the audio files its
    /// tracklist plays (`source_audio_summary_sql`); `None` when it has none.
    pub source_audio: Option<crate::source_audio::SourceAudioSummary>,
}

/// One entry of the release editor's Files list: a file no track plays from,
/// or a CUE sheet none of whose tracks stay.
#[derive(Debug, Clone, PartialEq)]
pub enum DbCarriedFile {
    File(DbFile),
    Sheet(crate::records::sheet_files::DbSheetWithoutTracks),
}

/// A release's files in this device's upload queue, as one read finds them:
/// how many only wait there, how many this process is sending now, and how
/// many are stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbReleaseUpload {
    pub waiting: u32,
    pub sending: u32,
    /// How many of the release's files are stored: those not queued.
    pub stored: u32,
    /// The name of the release file whose original is missing, while none
    /// of its files is being sent: the move waits for the person to restore
    /// it, and nothing else lets it go on.
    pub missing: Option<String>,
}

/// What keeps the upload queue from sending its waiting files, as coven says
/// it of each file's upload and of the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadQueueWait {
    /// Nothing sends the queue's files: the person paused uploads, or sync
    /// isn't running.
    Paused { by: PausedBy },
    /// The device has no usable network.
    Offline,
    /// Files move only on unmetered networks, and this one is metered.
    Metered,
    /// A file waits for its retry after a failed attempt, or for a
    /// provider's cooldown, until `at_ms`, in milliseconds since the epoch.
    Retrying { at_ms: i64 },
}

/// Who holds a paused upload queue: the person, whose Resume lets it go on,
/// or sync that isn't running, which Resume doesn't change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PausedBy {
    Person,
    SyncStopped,
}
