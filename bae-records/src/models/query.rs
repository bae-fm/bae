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
    pub files: DbReleaseFiles,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
    /// What the library records this device holding for each audio file whose
    /// row names no stored file.
    pub unstored_audio: Vec<DbUnstoredAudio>,
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
}

/// A track as the release editor reads its audio: the track, how many audio
/// formats it stores, and its formats' segments in segment order, each with
/// the file it names.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseEditTrack {
    pub track: DbTrack,
    pub audio_format_count: i64,
    pub segments: Vec<DbReleaseEditSegment>,
}

/// One segment of a track's audio as the release editor reads it: whether it
/// is the track's body or its pregap, the file it names, and that file's name
/// and stored audio layout; `file` is `None` when no release file has that id.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseEditSegment {
    pub role: DbAudioSegmentRole,
    pub file_id: String,
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

/// What the library records this device holding for one of a release's audio
/// files on one device only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbUnstoredAudio {
    /// No original here: the file is on another device, as coven's
    /// registration of that device reads.
    Elsewhere(OtherDevice),
    /// This device's original, as coven records it.
    Original,
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

/// A release's files as its screens list them: all of them, and the lists
/// among them that the release's pages show beside, with their count and
/// sizes added up.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseFiles {
    /// Every file, in the release's file order.
    pub files: Vec<DbFile>,
    /// The files that are images.
    pub images: Vec<DbFile>,
    /// The images a cover can be made from.
    pub covers: Vec<DbFile>,
    /// The files no track plays from.
    pub others: Vec<DbFile>,
    pub count: i64,
    pub total_size: u64,
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
/// it of the uploads and the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadQueueWait {
    /// The person paused uploads.
    Paused,
    /// The device has no usable network.
    Offline,
    /// Files move only on unmetered networks, and this one is metered.
    Metered,
    /// A failed attempt is held until `at_ms`, in milliseconds since the
    /// epoch.
    Retrying { at_ms: i64 },
}

impl UploadQueueWait {
    /// What holds the queue, when several things do: the pause first, then
    /// the network, which makes the rest moot, then the metered network and
    /// last the retry time. Nothing holds a queue that is sending a file
    /// (`sending > 0`), except the pause.
    pub fn pick(
        paused: bool,
        sending: u32,
        network_usable: Option<bool>,
        metered: bool,
        retry_at_ms: Option<i64>,
    ) -> Option<Self> {
        if paused {
            Some(Self::Paused)
        } else if sending > 0 {
            None
        } else if network_usable == Some(false) {
            Some(Self::Offline)
        } else if metered {
            Some(Self::Metered)
        } else {
            retry_at_ms.map(|at_ms| Self::Retrying { at_ms })
        }
    }
}
