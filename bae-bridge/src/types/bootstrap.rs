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

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeBootstrapStatus {
    /// A new installation's database opening, which runs every migration,
    /// before it connects.
    Opening {
        progress: BridgeOpenProgress,
    },
    Connecting,
    /// The request is sent. `confirmation` is set for a sent invite, as coven
    /// writes it (`041 778`).
    WaitingForApproval {
        confirmation: Option<String>,
        next_check: BridgeNextCheck,
    },
    /// Loading the library from storage, at this step.
    Loading {
        progress: BridgeLoadProgress,
    },
    /// Registering this device with the library once it is loaded. No count:
    /// how much the registration reads isn't known until it has read it.
    Registering,
    /// Keeping this device's keys and publishing the library, the last work
    /// before it opens. No count.
    Committing,
    Pending {
        records: Vec<BridgePendingRecord>,
    },
}
/// When a waiting join checks for approval next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeNextCheck {
    /// A check is running now.
    Checking,
    /// On the join's backoff; checking now ends the wait.
    At { time: std::time::SystemTime },
    /// The provider asked to slow down until then; checking now waits too.
    CoolingDown { until: std::time::SystemTime },
}
impl BridgeNextCheck {
    fn from_core(value: coven::NextCheck) -> Self {
        match value {
            coven::NextCheck::Checking => Self::Checking,
            coven::NextCheck::At(time) => Self::At { time },
            coven::NextCheck::CoolingDown(until) => Self::CoolingDown { until },
        }
    }
}
impl BridgeBootstrapStatus {
    pub(crate) fn from_core(value: coven::BootstrapStatus) -> Self {
        match value {
            coven::BootstrapStatus::Connecting => Self::Connecting,
            coven::BootstrapStatus::WaitingForApproval { confirmation, next_check } => {
                Self::WaitingForApproval {
                    confirmation: confirmation.map(|number| number.to_string()),
                    next_check: BridgeNextCheck::from_core(next_check),
                }
            }
            coven::BootstrapStatus::Loading(progress) => {
                Self::Loading { progress: BridgeLoadProgress::from_core(progress) }
            }
            coven::BootstrapStatus::Opening(progress) => {
                Self::Opening { progress: BridgeOpenProgress::from_core(progress) }
            }
            coven::BootstrapStatus::Registering => Self::Registering,
            coven::BootstrapStatus::Committing => Self::Committing,
            coven::BootstrapStatus::Pending(records) => Self::Pending {
                records: records
                    .into_iter()
                    .map(|record| {
                        BridgePendingRecord::from_core(bae_db::PendingItem::before_open(record))
                    })
                    .collect(),
            },
        }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeOpenProgress {
    pub step: BridgeOpenStep,
    pub done: u64,
    pub total: u64,
}

impl BridgeOpenProgress {
    pub(crate) fn from_core(value: coven::Progress<coven::OpenStep>) -> Self {
        Self {
            step: match value.step {
                coven::OpenStep::Rebuilding => BridgeOpenStep::Rebuilding,
                coven::OpenStep::Migrating => BridgeOpenStep::Migrating,
            },
            done: value.done,
            total: value.total,
        }
    }
}

/// Localization key for an open's step, the line over its progress bar. The
/// UI resolves it against the `Core` table.
#[uniffi::export]
pub fn bridge_open_step_key(step: BridgeOpenStep) -> String {
    match step {
        BridgeOpenStep::Rebuilding => "core.library.open_step.rebuilding",
        BridgeOpenStep::Migrating => "core.library.open_step.migrating",
    }
    .to_string()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSetupProgress {
    pub step: BridgeSetupStep,
    pub done: u64,
    pub total: u64,
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

impl BridgeClosingStep {
    pub(crate) fn from_core(step: bae_library::ClosingStep) -> Self {
        match step {
            bae_library::ClosingStep::StoppingPlayback => Self::StoppingPlayback,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            bae_library::ClosingStep::StoppingImports => Self::StoppingImports,
            bae_library::ClosingStep::StoppingSync => Self::StoppingSync,
            bae_library::ClosingStep::FinishingWork => Self::FinishingWork,
            bae_library::ClosingStep::ClosingStore => Self::ClosingStore,
            bae_library::ClosingStep::LeavingLibrary => Self::LeavingLibrary,
            bae_library::ClosingStep::DeletingFiles => Self::DeletingFiles,
        }
    }
}

/// Localization key for a step of a close or a removal, the line the app
/// shows while it runs. The UI resolves it against the `Core` table.
#[uniffi::export]
pub fn bridge_closing_step_key(step: BridgeClosingStep) -> String {
    match step {
        BridgeClosingStep::StoppingServers => "core.library.closing_step.stopping_servers",
        BridgeClosingStep::StoppingPlayback => "core.library.closing_step.stopping_playback",
        BridgeClosingStep::StoppingImports => "core.library.closing_step.stopping_imports",
        BridgeClosingStep::StoppingSync => "core.library.closing_step.stopping_sync",
        BridgeClosingStep::FinishingWork => "core.library.closing_step.finishing_work",
        BridgeClosingStep::ClosingStore => "core.library.closing_step.closing_store",
        BridgeClosingStep::LeavingLibrary => "core.library.closing_step.leaving_library",
        BridgeClosingStep::DeletingFiles => "core.library.closing_step.deleting_files",
    }
    .to_string()
}

/// Localization key for a setup step, the line over its progress bar. The
/// UI resolves it against the `Core` table.
#[uniffi::export]
pub fn bridge_setup_step_key(step: BridgeSetupStep) -> String {
    match step {
        BridgeSetupStep::FinishingRemovals => "core.library.setup_step.finishing_removals",
        BridgeSetupStep::RemovingLibraries => "core.library.setup_step.removing_libraries",
        BridgeSetupStep::ForgettingRestoreCodes => {
            "core.library.setup_step.forgetting_restore_codes"
        }
        BridgeSetupStep::ClearingAppData => "core.library.setup_step.clearing_app_data",
        BridgeSetupStep::CreatingLibrary => "core.library.setup_step.creating_library",
        BridgeSetupStep::SettingUpLibrary => "core.library.setup_step.setting_up_library",
        BridgeSetupStep::ConnectingStorage => "core.library.setup_step.connecting_storage",
    }
    .to_string()
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

impl BridgeLoadProgress {
    pub(crate) fn from_core(value: coven::Progress<coven::LoadStep>) -> Self {
        Self { step: BridgeLoadStep::from_core(value.step), done: value.done, total: value.total }
    }
}

/// Localization key for a load's step, the line over its progress bar. The
/// UI resolves it against the `Core` table.
#[uniffi::export]
pub fn bridge_load_step_key(step: BridgeLoadStep) -> String {
    match step {
        BridgeLoadStep::FindingKeys => "core.library.load_step.finding_keys",
        BridgeLoadStep::StreamingLibrary => "core.library.load_step.streaming_library",
        BridgeLoadStep::StreamingCircle => "core.library.load_step.streaming_circle",
        BridgeLoadStep::Installing => "core.library.load_step.installing",
        BridgeLoadStep::InstallingSlots => "core.library.load_step.installing_slots",
        BridgeLoadStep::Reading => "core.library.load_step.reading",
        BridgeLoadStep::Applying => "core.library.load_step.applying",
    }
    .to_string()
}

#[derive(Clone, uniffi::Enum)]
pub enum BridgeBootstrapResult {
    Opened {
        library: Arc<crate::OpenedLibrary>,
    },
    Published {
        library: Arc<crate::OpenedLibrary>,
        error: BridgeError,
    },
    ProfileRequired {
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
}
impl BridgeBootstrapResult {
    pub(crate) fn opened(handle: coven::CovenHandle, host: Arc<crate::BridgeHost>) -> Self {
        Self::Opened { library: Arc::new(crate::OpenedLibrary::new(handle, host)) }
    }
    /// A join's outcome: the library with its profile written, or kept for a
    /// replacement; the library without a profile to retry; or the invite
    /// ended without letting this device in.
    pub(crate) fn joined(
        result: Result<Option<bae_library::JoinedLibrary>, bae_library::RestoreFromCodeError>,
        host: Arc<crate::BridgeHost>,
    ) -> Result<Self, BridgeError> {
        match result {
            Ok(Some((handle, None | Some(Ok(_))))) => Ok(Self::opened(handle, host)),
            Ok(Some((handle, Some(Err(error))))) => Ok(Self::ProfileRequired {
                library: Arc::new(crate::OpenedLibrary::new(handle, host)),
                error: error.into(),
            }),
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
