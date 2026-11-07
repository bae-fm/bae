use super::super::*;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSearchResults {
    pub albums: Vec<BridgeAlbumSearchResult>,
    pub artists: Vec<BridgeArtistSummary>,
    pub tracks: Vec<BridgeTrackSearchResult>,
    pub composers: Vec<BridgeComposerSummary>,
    pub works: Vec<BridgeWorkSummary>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub artist_name: String,
    /// Reference to the album's cover (the primary release's cover), or `None`.
    /// The UI fetches the bytes by id and caches under `(id, version)`.
    pub cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackSearchResult {
    pub id: String,
    pub title: String,
    /// The track length as a clock label's fields ("3:07"). The raw
    /// milliseconds do not cross — the search row only ever shows the clock,
    /// never the number.
    pub duration_clock: BridgeDurationClock,
    /// The release the track is on — what playing it from a search starts.
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
    /// The cover of the track's own release, or `None`. Same fetch/caching
    /// contract as [`BridgeAlbumSearchResult::cover`].
    pub cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerSummary {
    pub artist_id: String,
    pub name: String,
    pub sort_name: Option<String>,
    /// The line under its name: how many works it is credited on.
    pub count_line: BridgeCountLabel,
    pub linked_release_count: i64,
    pub unlinked_credit_count: i64,
    pub image: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkSummary {
    pub work_id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    pub composer_names: Option<String>,
    pub linked_release_count: i64,
    pub representative_release_id: Option<String>,
    pub representative_cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseRoleSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub source: BridgeCatalog,
    pub source_credit: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackRoleSummary {
    pub track_id: String,
    pub track_title: String,
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_id: String,
    pub artist_name: String,
    pub source: BridgeCatalog,
    pub source_credit: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkTrackSummary {
    pub track_id: String,
    pub track_title: String,
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub name: BridgeReleaseName,
    /// What the release is made of, worded: "2×CD", "Vinyl".
    pub media: Vec<crate::types::BridgeFactTerm>,
    pub cover: Option<BridgeImageRef>,
}

/// A composer's page above its rows: the composer, and the work the page
/// opens first. Its rows are read in windows
/// (`AppHandle::subscribe_composer_detail_rows`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerDetail {
    pub composer: BridgeComposerSummary,
    pub default_work_id: Option<String>,
}

/// One row of a composer's page, in the page's order: its works, then its
/// unlinked release credits, then its unlinked track credits.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerDetailRow {
    /// What a screen tells the row apart by.
    pub id: i64,
    /// The heading to draw above the row: set on the first row of each part
    /// of the page.
    pub heading: Option<BridgeComposerHeading>,
    pub item: BridgeComposerDetailItem,
}

/// The heading over a part of a composer's page.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeComposerHeading {
    Works,
    Credits,
    Recordings,
}

/// What a row of a composer's page shows.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeComposerDetailItem {
    /// A work; `part` when it is listed under the work heading its group,
    /// which the page indents it below.
    Work {
        work: BridgeWorkSummary,
        part: bool,
    },
    ReleaseCredit {
        credit: BridgeReleaseRoleSummary,
    },
    TrackCredit {
        credit: BridgeTrackRoleSummary,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkDetail {
    pub work: BridgeWorkSummary,
    pub child_works: Vec<BridgeWorkSummary>,
    pub releases: Vec<BridgeWorkReleaseSummary>,
    pub tracks: Vec<BridgeWorkTrackSummary>,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeComposerSortField {
    Name,
    WorkCount,
    LinkedReleaseCount,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerSortCriterion {
    pub field: BridgeComposerSortField,
    pub direction: BridgeSortDirection,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSummary {
    pub artist_id: String,
    pub name: String,
    /// The line under its name: how many albums credit it.
    pub count_line: BridgeCountLabel,
    pub image: Option<BridgeImageRef>,
}

/// One existing library artist offered by an artist picker. This keeps every
/// stored identity field so equal display names remain distinguishable.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSearchResult {
    pub artist: BridgeExistingArtist,
    pub image: Option<BridgeImageRef>,
    /// The artist's other printed names, which the picker offers under it.
    pub credit_names: Vec<BridgeArtistCreditName>,
}

impl BridgeArtistSearchResult {
    pub(crate) fn from_core(result: bae_views::album_detail::ArtistSearchResult) -> Self {
        let bae_views::album_detail::ArtistSearchResult { artist, image, credit_names } = result;
        Self {
            artist: BridgeExistingArtist::from_core(bae_records::ExistingArtist::from(artist)),
            image: image.map(BridgeImageRef::from_core),
            credit_names: credit_names.into_iter().map(BridgeArtistCreditName::from_core).collect(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistDetail {
    pub artist: BridgeArtistSummary,
    /// The artist's other printed names, in name order, which its page lists
    /// and edits.
    pub credit_names: Vec<BridgeArtistCreditName>,
    /// The artist's albums in discography order: year ascending with unknown
    /// years last, then title.
    pub albums: Vec<BridgeAlbum>,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeArtistSortField {
    Name,
    AlbumCount,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSortCriterion {
    pub field: BridgeArtistSortField,
    pub direction: BridgeSortDirection,
}

/// One row on the Storage Manager: a release paired with its parent
/// album, what its storage state shows, and when the release was added. The
/// UI splits these into separate slices (releases + summaries) so in-band
/// metadata changes (album rename, pin toggle) re-render affected rows
/// without list rebuilds.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageRow {
    pub release: BridgeReleaseSummary,
    pub album: BridgeAlbum,
    pub status: BridgeStorageRowStatus,
    pub date_added: std::time::SystemTime,
}

/// What a Storage Manager row's storage state shows: where its files are, in
/// the order the list sorts by storage, or a transition this process runs
/// while they rest. Mirrors `StorageRowStatus`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStorageRowStatus {
    /// The download queue holds it failed, with the error.
    DownloadFailed {
        error: String,
    },
    Uploading,
    Pinning {
        step: BridgeDownloadEntryStep,
    },
    Transferring {
        action: crate::types::BridgeReleaseStorageAction,
    },
    OnOneDevice,
    Pinned,
    InStorage,
}

/// Where a release stands in the download queue while its files are coming to
/// this device. Mirrors `DownloadEntryStep`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDownloadEntryStep {
    /// Waiting, with `ahead` entries before it that have not failed.
    Queued { ahead: u32, wait: Option<crate::types::BridgeQueueWait> },
    /// Downloading.
    Running { wait: Option<crate::types::BridgeQueueWait> },
}

/// Column the Storage Manager can sort by. Mirrors `StorageSortField`.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageSortField {
    AlbumTitle,
    ArtistNames,
    Media,
    FileCount,
    TotalSize,
    /// Uploading, pinning, local, pinned, then in the cloud only, ascending.
    Storage,
    /// When the release was added, oldest first ascending.
    DateAdded,
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct BridgeStorageSort {
    pub field: BridgeStorageSortField,
    pub direction: BridgeSortDirection,
}

/// Filter applied to the Storage Manager list: every release, the uploaded
/// ones, or the local ones. Mirrors `StorageFilter`.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageFilter {
    All,
    Remote,
    Local,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueEntry {
    /// The row's id, unique across both lanes and never given to another row:
    /// every row is an entry of its own, so the same track queued twice is two
    /// rows. The UI keys each row on it and targets remove/reorder/skip at one
    /// row.
    pub entry_id: String,
    pub track_id: String,
    pub display: BridgeTrackDisplay,
    /// The track length as a clock label's fields ("3:07"). The raw
    /// milliseconds do not cross — a queue row only ever shows the clock, never
    /// the number.
    pub duration_clock: BridgeDurationClock,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueSnapshot {
    pub manual: Vec<BridgeQueueEntry>,
    pub context: Option<BridgePlaybackContext>,
    pub has_next: bool,
    pub has_previous: bool,
    /// How many writes have changed Up Next: a screen holding an Up Next row
    /// it moved shows this value's order once it reaches the version the
    /// move answered with.
    pub manual_version: u64,
}

/// The last time tracks went into Up Next: which addition since playback
/// started with the library it was, counting from one, and how many tracks it
/// put in. Each addition is a value of its own, even two alike in a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeQueueAddition {
    pub number: u64,
    pub tracks: u32,
}

mirror_struct! {
    BridgeQueueAddition = bae_playback::QueueAddition,
    from_core: pub(crate) fn,
    fields: { number, tracks },
}

/// Which kind of source the context plays from, so the UI labels the section
/// (a release's "Playing from", a list of tracks, or the whole library). A
/// single- or multi-release source is both `Release` here — the queue pane's
/// "Playing from" title is the same for one album or several. A single
/// release's id stays in core (the UI labels by kind, not by id here).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackSourceKind {
    Release,
    Tracks,
    Library,
}

impl BridgePlaybackSourceKind {
    pub(crate) fn from_core(source: &bae_playback::ContextSource) -> Self {
        match source {
            bae_playback::ContextSource::Release(_) | bae_playback::ContextSource::Releases => {
                Self::Release
            }
            bae_playback::ContextSource::Tracks => Self::Tracks,
            bae_playback::ContextSource::Library => Self::Library,
        }
    }
}

/// The context lane (what the queue is playing from), delivered alongside the
/// manual lane so each UI renders the two as distinct sections: its kind
/// (release, list of tracks or library, for the section label) and whether it
/// was ordered by shuffle (the UI shows a shuffle indicator when so). Its
/// tracks are read through `subscribe_queue_upcoming`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackContext {
    pub kind: BridgePlaybackSourceKind,
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `None` for a multi-release source or
    /// the whole library. The UI appends it to the section label; the label
    /// prose itself stays UI-side (localized).
    pub source_title: Option<String>,
    pub shuffled: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncStatusSnapshot {
    /// The top block of the Sync page: the state in words, its color, its
    /// cause and its fix, and when sync last finished.
    pub shown: BridgeSyncShown,
    /// What coven waits on, each with the action the person can take.
    pub pending: Vec<crate::types::BridgePendingRecord>,
    /// A storage setup runs: a setup screen's, or one past its point of no
    /// return that sync's connection finishes after a relaunch. It records
    /// the library's location as it finishes, so a surface that shows the
    /// location reads it again (`AppHandle::storage_location`) once this
    /// turns false.
    pub setup_running: bool,
    /// A storage setup that ended undone with no setup screen there to hear
    /// it, shown until the person dismisses it with
    /// `AppHandle::dismiss_undone_storage_setup`.
    pub undone_setup: Option<BridgeUndoneStorageSetup>,
}

/// A storage setup that ended undone, and why: its line says storage isn't
/// set up and to set it up again.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUndoneStorageSetup {
    /// Which setup it was, for dismissing it.
    pub operation: i64,
    pub error: BridgeError,
}

impl BridgeSyncStatusSnapshot {
    pub(crate) fn from_core(snapshot: bae_sync_status::SyncStatusSnapshot) -> Self {
        let shown = BridgeSyncShown::from_core(snapshot.shown());
        let bae_sync_status::SyncStatusSnapshot {
            coven: _,
            pending,
            error: _,
            retry_effect: _,
            storage_setup,
            provider: _,
        } = snapshot;
        let setup_running = storage_setup == Some(bae_sync_status::StorageSetupState::Running);
        let undone_setup = match storage_setup {
            Some(bae_sync_status::StorageSetupState::Undone { operation, why }) => {
                Some(BridgeUndoneStorageSetup {
                    operation: operation.0,
                    error: crate::types::BridgeError::from_core(why),
                })
            }
            Some(bae_sync_status::StorageSetupState::Running) | None => None,
        };
        Self {
            shown,
            setup_running,
            undone_setup,
            pending: pending
                .into_iter()
                .map(crate::types::BridgePendingRecord::from_core)
                .collect(),
        }
    }
}

/// The color a sync state reads in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSyncTone {
    Neutral,
    Success,
    Warning,
    Danger,
}

/// The sync state, named; `bridge_sync_status_key` gives its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSyncStatusWord {
    /// No storage is set up on this device. The Mac makes libraries; a
    /// phone joins one, so it never reads this.
    NotSynced,
    /// A storage setup runs: its call's, or one past its point of no return
    /// that sync's connection finishes.
    SettingUpStorage,
    Syncing,
    WaitingToSync,
    Synced,
    OfflineWithUnsentChanges,
    Offline,
    UpdateNeeded,
    SignedOutOfDropbox,
    SignedOutOfOneDrive,
    SignedOutOfICloud,
    /// An S3 library has no sign-in: its key was not accepted.
    AccessKeyNotAccepted,
    DeviceNeedsReset,
    NoAccess,
    FolderMissing,
    ClockWrong,
    RemovedFromTheLibrary,
    StorageFull,
    SyncFailed,
}

/// Why a sync state is what it is: a catalog message, whose arguments are
/// the message's own. A cause that names when sync resumes also carries that
/// moment: the UI formats it in the person's locale and passes it as the
/// message's last argument.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeSyncCause {
    pub message: BridgeMessage,
    pub resumes_at: Option<std::time::SystemTime>,
}

/// What the person can do about a sync state. The button's words are the
/// platform's own.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSyncFix {
    ResumeUploads,
    RetryNow,
    /// Start sync again after a start that failed.
    Reconnect,
    CheckForUpdates,
    /// Dropbox and OneDrive sign in from bae.
    SignIn {
        provider: BridgeCloudProvider,
    },
    EnterAccessKey,
    /// iCloud signs in from the system's settings.
    OpenSystemSettings,
    ResetDevice,
    RemoveFromThisDevice,
    OpenInBrowser,
}

/// The top block of the Sync page.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeSyncShown {
    pub status: BridgeSyncStatusWord,
    pub tone: BridgeSyncTone,
    pub cause: Option<BridgeSyncCause>,
    pub fix: Option<BridgeSyncFix>,
    /// When sync last finished, under a state that is a problem, never
    /// under Synced.
    pub last_synced: Option<std::time::SystemTime>,
}

impl BridgeSyncShown {
    pub(crate) fn from_core(shown: bae_sync_status::SyncShown) -> Self {
        Self {
            status: BridgeSyncStatusWord::from_core(shown.status),
            tone: match shown.tone {
                bae_sync_status::SyncTone::Neutral => BridgeSyncTone::Neutral,
                bae_sync_status::SyncTone::Success => BridgeSyncTone::Success,
                bae_sync_status::SyncTone::Warning => BridgeSyncTone::Warning,
                bae_sync_status::SyncTone::Danger => BridgeSyncTone::Danger,
            },
            cause: shown.cause.map(BridgeSyncCause::from_core),
            fix: shown.fix.map(BridgeSyncFix::from_core),
            last_synced: shown.last_synced,
        }
    }
}

impl BridgeSyncStatusWord {
    pub(crate) fn from_core(word: bae_sync_status::SyncStatusWord) -> Self {
        use bae_sync_status::SyncStatusWord as W;
        match word {
            W::NotSynced => Self::NotSynced,
            W::SettingUpStorage => Self::SettingUpStorage,
            W::Syncing => Self::Syncing,
            W::WaitingToSync => Self::WaitingToSync,
            W::Synced => Self::Synced,
            W::OfflineWithUnsentChanges => Self::OfflineWithUnsentChanges,
            W::Offline => Self::Offline,
            W::UpdateNeeded => Self::UpdateNeeded,
            W::SignedOut(coven::CloudProvider::Dropbox) => Self::SignedOutOfDropbox,
            W::SignedOut(coven::CloudProvider::OneDrive) => Self::SignedOutOfOneDrive,
            W::SignedOut(coven::CloudProvider::CloudKit) => Self::SignedOutOfICloud,
            W::SignedOut(coven::CloudProvider::S3) => Self::AccessKeyNotAccepted,
            W::DeviceNeedsReset => Self::DeviceNeedsReset,
            W::NoAccess => Self::NoAccess,
            W::FolderMissing => Self::FolderMissing,
            W::ClockWrong => Self::ClockWrong,
            W::RemovedFromTheLibrary => Self::RemovedFromTheLibrary,
            W::StorageFull => Self::StorageFull,
            W::SyncFailed => Self::SyncFailed,
        }
    }
}

/// The name a provider goes by in a sentence, the same in every language.
fn provider_name(provider: coven::CloudProvider) -> &'static str {
    match provider {
        coven::CloudProvider::S3 => "S3",
        coven::CloudProvider::Dropbox => "Dropbox",
        coven::CloudProvider::OneDrive => "OneDrive",
        coven::CloudProvider::CloudKit => "iCloud",
    }
}

impl BridgeSyncCause {
    pub(crate) fn from_core(cause: bae_sync_status::SyncCauseLine) -> Self {
        use bae_sync_status::SyncCauseLine as C;
        let plain = |key: &str| BridgeMessage { key: key.to_string(), args: Vec::new() };
        let mut resumes_at = None;
        let message = match cause {
            C::UploadsPaused => plain("core.sync.cause.uploads_paused"),
            C::UploadFailedWillRetry => plain("core.sync.cause.upload_failed"),
            C::ProviderWait { provider, until } => {
                resumes_at = Some(until);
                BridgeMessage {
                    key: "core.sync.cause.provider_wait".to_string(),
                    args: vec![BridgeMessageArg::new("provider", provider_name(provider).into())],
                }
            }
            C::UpdateBae => plain("core.sync.cause.update_bae"),
            C::AskForAccess => plain("core.sync.cause.ask_for_access"),
            C::CheckTheClock => plain("core.error.storage.clock_skew"),
            C::OldCopy => plain("core.sync.cause.old_copy"),
            C::FreeStorageSpace => plain("core.sync.cause.free_space"),
            C::EnterAccessKey => plain("core.sync.cause.enter_key"),
            C::SignInToICloud => plain("core.sync.cause.sign_in_icloud"),
            C::FileMissingOrChanged { file: Some(file) } => BridgeMessage {
                key: "core.sync.cause.file_changed".to_string(),
                args: vec![BridgeMessageArg::new("file", file)],
            },
            C::FileMissingOrChanged { file: None } => plain("core.sync.cause.file_changed_unnamed"),
            C::DeviceDiskFailed => plain("core.sync.cause.disk_failed"),
            C::DeviceKeysUnreadable => plain("core.sync.cause.keys_unreadable"),
            C::LibraryUnreadable => plain("core.sync.cause.library_unreadable"),
            C::ProviderError { provider, detail } => BridgeMessage {
                key: "core.sync.cause.provider_error".to_string(),
                args: vec![
                    BridgeMessageArg::new("provider", provider_name(provider).into()),
                    BridgeMessageArg::new("detail", detail),
                ],
            },
            C::ChangeDamaged => plain("core.sync.cause.change_damaged"),
            C::SomethingWentWrong => plain("core.sync.cause.went_wrong"),
            C::Pending(reason) => plain(&BridgePendingReason::from_core(reason, None).key),
            C::Failure(error) => match crate::types::BridgeError::from_core(error) {
                BridgeError::Diagnostic { category, .. } => {
                    plain(&bridge_error_category_key(category))
                }
                BridgeError::Cancelled | BridgeError::NotFound { .. } => {
                    plain("core.sync.cause.went_wrong")
                }
            },
        };
        Self { message, resumes_at }
    }
}

impl BridgeSyncFix {
    pub(crate) fn from_core(fix: bae_sync_status::SyncFix) -> Self {
        use bae_sync_status::SyncFix as F;
        match fix {
            F::ResumeUploads => Self::ResumeUploads,
            F::RetryNow => Self::RetryNow,
            F::Reconnect => Self::Reconnect,
            F::CheckForUpdates => Self::CheckForUpdates,
            F::SignIn(provider) => {
                Self::SignIn { provider: crate::types::BridgeCloudProvider::from_core(&provider) }
            }
            F::EnterAccessKey => Self::EnterAccessKey,
            F::OpenSystemSettings => Self::OpenSystemSettings,
            F::ResetDevice => Self::ResetDevice,
            F::RemoveFromThisDevice => Self::RemoveFromThisDevice,
            F::OpenInBrowser => Self::OpenInBrowser,
        }
    }
}

/// The catalog key of a sync state's words.
#[uniffi::export]
pub fn bridge_sync_status_key(status: BridgeSyncStatusWord) -> String {
    match status {
        BridgeSyncStatusWord::NotSynced => "core.sync.status.not_synced",
        BridgeSyncStatusWord::SettingUpStorage => "core.sync.setting_up_storage",
        BridgeSyncStatusWord::Syncing => "core.sync.syncing",
        BridgeSyncStatusWord::WaitingToSync => "core.sync.status.waiting_to_sync",
        BridgeSyncStatusWord::Synced => "core.sync.synced",
        BridgeSyncStatusWord::OfflineWithUnsentChanges => "core.sync.status.offline_unsent",
        BridgeSyncStatusWord::Offline => "core.sync.offline",
        BridgeSyncStatusWord::UpdateNeeded => "core.sync.status.update_needed",
        BridgeSyncStatusWord::SignedOutOfDropbox => "core.sync.status.signed_out_dropbox",
        BridgeSyncStatusWord::SignedOutOfOneDrive => "core.sync.status.signed_out_onedrive",
        BridgeSyncStatusWord::SignedOutOfICloud => "core.sync.status.signed_out_icloud",
        BridgeSyncStatusWord::AccessKeyNotAccepted => "core.sync.status.access_key_not_accepted",
        BridgeSyncStatusWord::DeviceNeedsReset => "core.sync.status.device_needs_reset",
        BridgeSyncStatusWord::NoAccess => "core.sync.status.no_access",
        BridgeSyncStatusWord::FolderMissing => "core.sync.status.folder_missing",
        BridgeSyncStatusWord::ClockWrong => "core.sync.status.clock_wrong",
        BridgeSyncStatusWord::RemovedFromTheLibrary => "core.sync.status.removed",
        BridgeSyncStatusWord::StorageFull => "core.sync.status.storage_full",
        BridgeSyncStatusWord::SyncFailed => "core.sync.failed",
    }
    .to_string()
}
