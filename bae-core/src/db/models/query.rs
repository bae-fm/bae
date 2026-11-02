use super::*;

/// What the Storage Manager's inspector shows of one release, read at once
/// so its queue entries, its files' pins and its uploads are of one moment.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbStorageInspector {
    /// The release's files in their order, each with its pin and upload.
    pub files: Vec<DbStorageInspectorFile>,
    /// What of the release waits in this device's upload queue.
    pub upload: Option<DbReleaseUpload>,
    /// The release's entries in the download and export queues.
    pub entries: crate::library::ReleaseQueueEntries,
}

/// Raw release-detail aggregate, which the release detail view resolves into
/// the display-ready `crate::album_detail::ReleaseDetail`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseDetail {
    pub release: DbRelease,
    /// The release's mediums, which its tracks' places are read against.
    pub mediums: DbReleaseMediums,
    pub tracks: Vec<DbTrackWithArtists>,
    pub files: Vec<DbFile>,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<crate::catalog_model::records::ReleaseRecord>,
    /// What the library records this device holding for each audio file whose
    /// row names no stored file.
    pub unstored_audio: Vec<DbUnstoredAudio>,
    /// Whether this device keeps every file of the release pinned, as the
    /// release's listing counts its pinned files.
    pub pinned: bool,
    /// Whether every file of the release names its stored file, as the
    /// release's listing says.
    pub remote: bool,
    /// What of the release waits in this device's upload queue.
    pub upload: Option<DbReleaseUpload>,
    /// The release's entry in this device's download queue, `None` when it
    /// has none.
    pub download: Option<crate::library::DownloadPhase>,
    /// The storage transition this process runs for the release; `None`
    /// when it runs none.
    pub transfer_action: Option<bae_types::release_storage::ReleaseStorageAction>,
    /// The version of the release's cover; `None` when it has none.
    pub cover_version: Option<String>,
}

/// Where one release file stands for its pin: whether this device keeps it
/// whole, and while this process downloads it to be kept, the file coven
/// downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbFilePin {
    pub kept: bool,
    pub downloading: Option<coven::FileId>,
}

/// One release file waiting in this device's upload queue, read with its
/// row: the file being uploaded, whether this process is sending it now, and
/// the record of what it waits for.
#[derive(Debug, Clone, PartialEq)]
pub struct DbFileUpload {
    pub file: coven::FileId,
    pub sending: bool,
    pub pending: Option<coven::PendingRecord>,
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

/// One segment of a track's audio as the release editor reads it: the file it
/// names, and that file's name and stored audio layout; `file` is `None` when
/// no release file has that id.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseEditSegment {
    pub file_id: String,
    pub file: Option<DbReleaseEditSourceFile>,
}

/// The file a release editor's segment names: its name, and its stored audio
/// layout (`None` when it has none).
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseEditSourceFile {
    pub name: String,
    pub layout: Option<crate::db::source_audio::SourceAudioLayout>,
}

/// One file of a release as the Storage Manager's inspector reads it: the
/// file, where this device's pin of it stands (`None` while it isn't pinned
/// or being pinned), and its upload while it waits in this device's upload
/// queue.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct DbStorageInspectorFile {
    pub file: DbFile,
    pub pin: Option<crate::album_detail::FilePin>,
    pub upload: Option<DbFileUpload>,
}

/// One file this process is sending now: when it is one of a release's
/// files, its release and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSendingFile {
    pub file: coven::FileId,
    pub release_id: Option<String>,
    pub file_size: Option<i64>,
}

/// A release with files this process is sending now: of its whole size, the
/// bytes of its files stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSendingRelease {
    pub release_id: String,
    pub bytes_stored: i64,
    pub bytes_total: i64,
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
    Elsewhere(crate::album_detail::OtherDevice),
    /// This device's original, as coven records it.
    Original,
}

/// A track row with its resolved artist rows (many-to-many join from the DB).
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackWithArtists {
    pub track: DbTrack,
    /// The track's own credits, in list order; none when it takes the
    /// album's.
    pub artists: Vec<crate::db::credits::LibraryCredit>,
}

/// A release's files in this device's upload queue, as one read finds them:
/// how many wait there, how many of those this process is sending now, and
/// how many are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbReleaseUpload {
    pub queued: i64,
    pub sending: i64,
    /// How many of the release's files are stored: those not queued.
    pub stored: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}
