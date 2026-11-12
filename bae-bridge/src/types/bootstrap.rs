use super::*;
use std::sync::Arc;

#[derive(Clone, uniffi::Record)]
pub struct BridgeS3Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
}
impl BridgeS3Credentials {
    pub(crate) fn into_core(self) -> coven::S3Credentials {
        coven::S3Credentials {
            access_key_id: self.access_key_id,
            secret_access_key: coven::SecretText::new(self.secret_access_key),
        }
    }
}

/// This person's restore code as their own device shows it: the code to type
/// or copy, and the code for the QR, which holds this device's S3 key.
#[derive(Clone, uniffi::Record)]
pub struct BridgeRestoreCodeShare {
    pub code: String,
    pub qr_text: String,
    /// Whether the QR's code holds the S3 key, so the new device asks for
    /// none.
    pub qr_carries_s3_key: bool,
}
impl BridgeRestoreCodeShare {
    pub(crate) fn from_core(value: bae_library::sync::restore_code::RestoreCodeShare) -> Self {
        Self {
            code: value.code,
            qr_text: value.qr_text,
            qr_carries_s3_key: value.qr_carries_s3_key,
        }
    }
}

/// Where a restore, a join or a device reset stands: what the screen shows,
/// and what it observes. The counts of a step that counts, opening or
/// loading, are the setup's stream's [`BridgeBootstrapStep`], drawn each
/// display frame.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeBootstrapStatus {
    /// A new installation's database opening, which runs every migration,
    /// before it connects.
    Opening,
    Connecting,
    /// The request is sent. `confirmation` is the number the admin compares
    /// before approving, as coven writes it (`041 778`).
    WaitingForApproval {
        confirmation: String,
    },
    /// Loading the library from storage. A join reports it for the check
    /// that finds its approval, then, after `Committing`, for loading the
    /// library it published.
    Loading,
    /// Registering this device with the library once it is loaded, in a
    /// restore or a reset; a join's approval registered its device. No count:
    /// how much the registration reads isn't known until it has read it.
    Registering,
    /// Keeping this device's keys and publishing the library, the last work
    /// before it opens; a join loads the library after it. No count.
    Committing,
    Pending {
        records: Vec<BridgePendingRecord>,
    },
}
/// Where a restore or a join stands, and whether Cancel can still stop it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeBootstrapState {
    pub status: BridgeBootstrapStatus,
    /// Whether the call can still be cancelled: a restore until it ends, a
    /// join while it opens or connects, a device reset never. A join that
    /// has found its approval opens the library whatever a cancel says.
    pub cancellable: bool,
}

/// The counted step a restore, a join or a device reset runs while it opens
/// the new installation's database or loads the library: its setup's
/// stream's one key, `""`, holds it while the step runs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeBootstrapStep {
    Opening { progress: BridgeOpenProgress },
    Loading { progress: BridgeLoadProgress },
}

impl BridgeBootstrapStatus {
    /// Where the call stands, and the counted step it runs, if one.
    pub(crate) fn from_core(value: coven::BootstrapStatus) -> (Self, Option<BridgeBootstrapStep>) {
        let status = match value {
            coven::BootstrapStatus::Loading(progress) => {
                let progress = BridgeLoadProgress::from_core(progress);
                return (Self::Loading, Some(BridgeBootstrapStep::Loading { progress }));
            }
            coven::BootstrapStatus::Opening(progress) => {
                let progress = BridgeOpenProgress::from_core(progress);
                return (Self::Opening, Some(BridgeBootstrapStep::Opening { progress }));
            }
            coven::BootstrapStatus::Connecting => Self::Connecting,
            coven::BootstrapStatus::WaitingForApproval { confirmation } => {
                Self::WaitingForApproval { confirmation: confirmation.to_string() }
            }
            coven::BootstrapStatus::Registering => Self::Registering,
            coven::BootstrapStatus::Committing => Self::Committing,
            coven::BootstrapStatus::Pending(records) => Self::Pending {
                records: records
                    .into_iter()
                    .map(|record| {
                        BridgePendingRecord::from_core(bae_sync_status::PendingItem::before_open(
                            record,
                        ))
                    })
                    .collect(),
            },
        };
        (status, None)
    }
}
/// A step of opening a library, in the order they run. Mirrors
/// `coven::OpenStep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOpenStep {
    /// Rebuilding the library's indexes after a new version of SQLite or of
    /// one of bae's orders or functions.
    Rebuilding,
    /// Running the migrations the library hasn't run yet.
    Migrating,
}

/// How far an open has come: its step, and how many of the step's units are
/// done out of its total. Its stream's one key, `""`, holds it while a step
/// runs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeOpenProgress {
    pub step: BridgeOpenStep,
    /// The `Core` table key of the step's line, over the progress bar.
    pub key: String,
    pub done: u64,
    pub total: u64,
}

impl BridgeOpenStep {
    fn key(self) -> &'static str {
        match self {
            Self::Rebuilding => "core.library.open_step.rebuilding",
            Self::Migrating => "core.library.open_step.migrating",
        }
    }
}

impl BridgeOpenProgress {
    pub(crate) fn new(step: BridgeOpenStep, done: u64, total: u64) -> Self {
        Self { step, key: step.key().to_string(), done, total }
    }

    pub(crate) fn from_core(value: coven::Progress<coven::OpenStep>) -> Self {
        Self::new(
            match value.step {
                coven::OpenStep::Rebuilding => BridgeOpenStep::Rebuilding,
                coven::OpenStep::Migrating => BridgeOpenStep::Migrating,
            },
            value.done,
            value.total,
        )
    }
}

impl BridgeSetupStep {
    fn key(self) -> &'static str {
        match self {
            Self::FinishingRemovals => "core.library.setup_step.finishing_removals",
            Self::RemovingLibraries => "core.library.setup_step.removing_libraries",
            Self::ForgettingRestoreCodes => "core.library.setup_step.forgetting_restore_codes",
            Self::ClearingAppData => "core.library.setup_step.clearing_app_data",
            Self::CreatingLibrary => "core.library.setup_step.creating_library",
            Self::SettingUpLibrary => "core.library.setup_step.setting_up_library",
            Self::ConnectingStorage => "core.library.setup_step.connecting_storage",
        }
    }
}

/// A step a setup call runs before a library is open, or while one is first
/// set up, in the order a dev reset runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSetupStep {
    /// Finishing removals of libraries an earlier run left unfinished.
    FinishingRemovals,
    /// Removing libraries from this device, counted by library.
    RemovingLibraries,
    /// Deleting saved restore codes, counted by code.
    ForgettingRestoreCodes,
    /// Deleting bae's own files on this device.
    ClearingAppData,
    /// Creating a library.
    CreatingLibrary,
    /// Starting a new library and setting its preferences.
    SettingUpLibrary,
    /// Connecting a library to its cloud storage.
    ConnectingStorage,
}

/// How far a setup step has come: the step, and how many of its units are
/// done out of its total; a total of 0 is a step with nothing to count. Its
/// stream's one key, `""`, holds it while the step runs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSetupProgress {
    pub step: BridgeSetupStep,
    /// The `Core` table key of the step's line, over the progress bar.
    pub key: String,
    pub done: u64,
    pub total: u64,
}

impl BridgeSetupProgress {
    pub(crate) fn new(step: BridgeSetupStep, done: u64, total: u64) -> Self {
        Self { step, key: step.key().to_string(), done, total }
    }
}

/// A step of loading a library from storage onto this device, in the order
/// they run; a step runs only when it has work. Mirrors `coven::LoadStep`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLoadStep {
    /// Reading the library's history for copies of the person's keys.
    FindingKeys,
    /// Downloading the saved copy of the data every member reads.
    StreamingLibrary,
    /// Downloading the saved copy of one circle's data.
    StreamingCircle,
    /// Writing the downloaded copies into the library here.
    Installing,
    /// Applying the changes saved with those copies.
    InstallingSlots,
    /// Downloading the changes made since.
    Reading,
    /// Applying those changes.
    Applying,
}

/// How far loading a library has come: its step, and how many of the step's
/// units are done out of its total, which can grow while the step runs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLoadProgress {
    pub step: BridgeLoadStep,
    /// The `Core` table key of the step's line, over the progress bar.
    pub key: String,
    pub done: u64,
    pub total: u64,
}

/// A step closing or removing the open library runs, in the order they run.
/// Mirrors `bae_library::ClosingStep`, after the desktop's own first
/// step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeClosingStep {
    /// Stopping the servers the desktop hosts for the library.
    StoppingServers,
    /// Stopping playback, saving where it was.
    StoppingPlayback,
    /// Stopping identification and import, waiting for the file each is on.
    StoppingImports,
    /// Stopping sync, once the pass that runs has ended.
    StoppingSync,
    /// Ending the library's background work.
    FinishingWork,
    /// Closing the library's database.
    ClosingStore,
    /// Leaving the library and deleting its store on this device.
    LeavingLibrary,
    /// Deleting this device's preferences and caches for the library.
    DeletingFiles,
}

/// The step a close or a removal runs: what it is, and the `Core` table key
/// of the line the app shows while it runs.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeClosingProgress {
    pub step: BridgeClosingStep,
    pub key: String,
}

impl BridgeClosingProgress {
    pub(crate) fn new(step: BridgeClosingStep) -> Self {
        Self { step, key: step.key().to_string() }
    }
}

impl BridgeClosingStep {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::StoppingServers => "core.library.closing_step.stopping_servers",
            Self::StoppingPlayback => "core.library.closing_step.stopping_playback",
            Self::StoppingImports => "core.library.closing_step.stopping_imports",
            Self::StoppingSync => "core.library.closing_step.stopping_sync",
            Self::FinishingWork => "core.library.closing_step.finishing_work",
            Self::ClosingStore => "core.library.closing_step.closing_store",
            Self::LeavingLibrary => "core.library.closing_step.leaving_library",
            Self::DeletingFiles => "core.library.closing_step.deleting_files",
        }
    }

    pub(crate) fn from_core(step: bae_library::ClosingStep) -> Self {
        match step {
            bae_library::ClosingStep::StoppingPlayback => Self::StoppingPlayback,
            #[cfg(desktop)]
            bae_library::ClosingStep::StoppingImports => Self::StoppingImports,
            bae_library::ClosingStep::StoppingSync => Self::StoppingSync,
            bae_library::ClosingStep::FinishingWork => Self::FinishingWork,
            bae_library::ClosingStep::ClosingStore => Self::ClosingStore,
            bae_library::ClosingStep::LeavingLibrary => Self::LeavingLibrary,
            bae_library::ClosingStep::DeletingFiles => Self::DeletingFiles,
        }
    }
}

impl BridgeLoadStep {
    pub(crate) fn from_core(step: coven::LoadStep) -> Self {
        match step {
            coven::LoadStep::FindingKeys => Self::FindingKeys,
            coven::LoadStep::Streaming(coven::Audience::Store) => Self::StreamingLibrary,
            coven::LoadStep::Streaming(coven::Audience::Circle(_)) => Self::StreamingCircle,
            coven::LoadStep::Installing => Self::Installing,
            coven::LoadStep::InstallingSlots => Self::InstallingSlots,
            coven::LoadStep::Reading => Self::Reading,
            coven::LoadStep::Applying => Self::Applying,
        }
    }
}

impl BridgeLoadStep {
    pub(crate) fn key(&self) -> &'static str {
        match self {
            Self::FindingKeys => "core.library.load_step.finding_keys",
            Self::StreamingLibrary => "core.library.load_step.streaming_library",
            Self::StreamingCircle => "core.library.load_step.streaming_circle",
            Self::Installing => "core.library.load_step.installing",
            Self::InstallingSlots => "core.library.load_step.installing_slots",
            Self::Reading => "core.library.load_step.reading",
            Self::Applying => "core.library.load_step.applying",
        }
    }
}

impl BridgeLoadProgress {
    pub(crate) fn new(step: BridgeLoadStep, done: u64, total: u64) -> Self {
        Self { key: step.key().to_string(), step, done, total }
    }

    pub(crate) fn from_core(value: coven::Progress<coven::LoadStep>) -> Self {
        Self::new(BridgeLoadStep::from_core(value.step), value.done, value.total)
    }
}

#[derive(Clone, uniffi::Enum)]
pub enum BridgeBootstrapResult {
    Opened {
        library: Arc<crate::OpenedLibrary>,
    },
    /// The library is published and opens, and `error` failed after: keeping
    /// it on this device, or a joined library's first load, such as one that
    /// lost its storage access, which its sync state then shows and its sync
    /// retries.
    Published {
        library: Arc<crate::OpenedLibrary>,
        error: BridgeError,
    },
    SavedCodeRemoved,
    /// The invite ended without letting this device in: declined or
    /// cancelled, expired, or the device that made it gone for good, which
    /// the joining device can't tell apart.
    Declined,
    /// A cancel came after the join's request may have been sent. The join
    /// stays under way; `unfinished_joins` lists it to resume.
    JoinRequestSent,
    /// The library's first installation on this device belongs to another
    /// call under way, which nothing changed: a join waiting for approval,
    /// which a restore can't start on, or a restore past its record, which a
    /// join can't. `unfinished_joins` or `unfinished_restores` lists it to
    /// resume, and the app goes on with it.
    CallUnderWay,
}
impl BridgeBootstrapResult {
    pub(crate) fn opened(handle: coven::CovenHandle, host: Arc<crate::BridgeHost>) -> Self {
        Self::Opened { library: Arc::new(crate::OpenedLibrary::new(handle, host)) }
    }
    /// A join's outcome: the library, with what failed once it was
    /// published, or the invite ended without letting this device in.
    pub(crate) fn joined(
        result: Result<Option<bae_library::JoinedLibrary>, bae_library::RestoreFromCodeError>,
        host: Arc<crate::BridgeHost>,
    ) -> Result<Self, BridgeError> {
        match result {
            Ok(Some(bae_library::JoinedLibrary::Loaded(handle))) => Ok(Self::opened(handle, host)),
            Ok(Some(bae_library::JoinedLibrary::Published { handle, failure })) => {
                Ok(Self::Published {
                    library: Arc::new(crate::OpenedLibrary::new(handle, host)),
                    error: BridgeError::categorized(
                        bae_diagnostics::bootstrap_category(&failure),
                        failure,
                    ),
                })
            }
            Ok(None) => Ok(Self::Declined),
            Err(error) => Self::failure(error, host),
        }
    }
    pub(crate) fn failure(
        error: bae_library::RestoreFromCodeError,
        host: Arc<crate::BridgeHost>,
    ) -> Result<Self, BridgeError> {
        let category = error.category();
        match error {
            bae_library::RestoreFromCodeError::Cancelled => Err(BridgeError::Cancelled),
            bae_library::RestoreFromCodeError::Restore(error) => match *error {
                coven::BootstrapError::JoinRequestSent => Ok(Self::JoinRequestSent),
                coven::BootstrapError::Directory(coven::BootstrapDirectoryError::Lock(
                    coven::StoreLockError::BootstrapPending(_),
                )) => Ok(Self::CallUnderWay),
                coven::BootstrapError::Published { handle, source } => Ok(Self::Published {
                    library: Arc::new(crate::OpenedLibrary::new(handle, host)),
                    error: BridgeError::categorized(category, source),
                }),
                error => Err(BridgeError::categorized(category, error)),
            },
            error => Err(BridgeError::categorized(category, error)),
        }
    }
}

/// One library's saved entry, in iCloud Keychain on Apple or Block Store on
/// Android, as listing found it. Only a
/// readable entry carries a code to restore from; either can be deleted by
/// its library id.
#[derive(Clone, uniffi::Enum)]
pub enum BridgeSavedRestoreEntry {
    Readable {
        code: Arc<BridgeSavedRestoreCode>,
    },
    /// An entry bae can't restore from, kept listed so the person can delete
    /// it.
    Unreadable {
        library_id: String,
        reason: BridgeUnreadableSavedCode,
    },
}
impl BridgeSavedRestoreEntry {
    pub(crate) fn from_core(value: coven::SavedRestoreEntry) -> Self {
        match value {
            coven::SavedRestoreEntry::Readable(saved) => {
                Self::Readable { code: Arc::new(BridgeSavedRestoreCode { saved }) }
            }
            coven::SavedRestoreEntry::Unreadable { store_id, reason } => Self::Unreadable {
                library_id: store_id.to_string(),
                reason: BridgeUnreadableSavedCode::from_core(reason),
            },
        }
    }
}

/// Why bae can't restore from a saved entry.
#[derive(Clone, Copy, uniffi::Enum)]
pub enum BridgeUnreadableSavedCode {
    /// Saved in a format bae doesn't read, such as an older one, or damaged.
    Format,
    /// Holds the code of a different library than the one it's saved under.
    OtherLibrary,
    /// Saved more than once, so reading it could give either copy.
    Ambiguous,
}
impl BridgeUnreadableSavedCode {
    fn from_core(value: coven::UnreadableSavedCode) -> Self {
        match value {
            coven::UnreadableSavedCode::Format => Self::Format,
            coven::UnreadableSavedCode::OtherStore => Self::OtherLibrary,
            coven::UnreadableSavedCode::Ambiguous => Self::Ambiguous,
        }
    }
}

/// A saved restore code bae can restore from.
#[derive(uniffi::Object)]
pub struct BridgeSavedRestoreCode {
    pub(crate) saved: coven::SavedRestoreCode,
}
#[derive(Clone, uniffi::Record)]
pub struct BridgeSavedRestoreCodeInfo {
    /// The library's name as one of the person's devices last saw it.
    pub name: String,
    pub library_id: String,
    pub location: BridgeStorageLocation,
    /// The provider sign-in restoring needs first; none on S3 and iCloud.
    pub oauth_access: Option<crate::BridgeOAuthAccess>,
    pub has_s3_credentials: bool,
}
#[uniffi::export]
impl BridgeSavedRestoreCode {
    pub fn info(&self) -> BridgeSavedRestoreCodeInfo {
        BridgeSavedRestoreCodeInfo {
            name: self.saved.name.clone(),
            library_id: self.saved.store_id.to_string(),
            location: BridgeStorageLocation::from_core(self.saved.location.clone()),
            oauth_access: self
                .saved
                .location
                .oauth_access()
                .map(crate::BridgeOAuthAccess::from_core),
            has_s3_credentials: self.saved.has_s3_credentials,
        }
    }
}

/// A join this device has under way, from its saved invite: shown on its
/// waiting screen, and continued with `resume_join`.
#[derive(uniffi::Object)]
pub struct BridgeUnfinishedJoin {
    pub(crate) join: coven::InviteInfo,
}
#[uniffi::export]
impl BridgeUnfinishedJoin {
    pub fn info(&self) -> crate::BridgeInviteInfo {
        crate::BridgeInviteInfo::from_core(self.join.clone())
    }
}

/// A restore of a library onto this device that stopped after it recorded
/// itself, from its recorded restore code: continued with `resume_restore`.
#[derive(uniffi::Object)]
pub struct BridgeUnfinishedRestore {
    pub(crate) restore: coven::CodeMetadata,
}
#[uniffi::export]
impl BridgeUnfinishedRestore {
    pub fn info(&self) -> crate::BridgeCodeMetadata {
        crate::BridgeCodeMetadata::from_core(self.restore.clone())
    }
}

/// The `Core` table keys of the words the recovery screens show, which no
/// value those screens hold carries: a failed open's offer to reset the
/// device, a reset or restore that stopped partway, a removal that did, and
/// the warning before a reset.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeRecoveryLines {
    pub device_reset_required: String,
    pub reset_unfinished: String,
    pub restore_unfinished: String,
    pub deletion_unfinished: String,
    pub reset_warning: String,
}

/// The words of the recovery screens, as `Core` table keys.
#[uniffi::export]
pub fn bridge_recovery_lines() -> BridgeRecoveryLines {
    use super::{bridge_error_category_key as key, BridgeErrorCategory};
    BridgeRecoveryLines {
        device_reset_required: key(BridgeErrorCategory::DeviceResetRequired),
        reset_unfinished: key(BridgeErrorCategory::ReplacementUnfinished {
            by: BridgeReplacedBy::Reset,
        }),
        restore_unfinished: key(BridgeErrorCategory::ReplacementUnfinished {
            by: BridgeReplacedBy::Restore,
        }),
        deletion_unfinished: key(BridgeErrorCategory::DeletionUnfinished),
        reset_warning: "core.sync.reset_device.warning".to_string(),
    }
}
