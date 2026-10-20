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
    fn from_core(value: coven::S3Credentials) -> Self {
        Self {
            access_key_id: value.access_key_id,
            secret_access_key: value.secret_access_key.as_str().to_string(),
        }
    }
}

/// This person's restore code as their own device shows it: the code to type
/// or copy, and the QR text, which carries this device's S3 key beside it.
#[derive(Clone, uniffi::Record)]
pub struct BridgeRestoreCodeShare {
    pub code: String,
    pub qr_text: String,
    /// Whether scanning the QR code fills in the S3 key, so the new device
    /// asks for none.
    pub qr_carries_s3_key: bool,
}
impl BridgeRestoreCodeShare {
    pub(crate) fn from_core(value: bae_core::sync::restore_code::RestoreCodeShare) -> Self {
        Self {
            code: value.code,
            qr_text: value.qr_text,
            qr_carries_s3_key: value.qr_carries_s3_key,
        }
    }
}

/// A scanned QR code's text read back: the code, and the S3 key that came
/// beside it from the person's own device.
#[derive(Clone, uniffi::Record)]
pub struct BridgeScannedCode {
    pub code: String,
    pub s3_credentials: Option<BridgeS3Credentials>,
}

/// Read a scanned QR code's text. Text with nothing beside the code, such as
/// an invite, reads as the code alone; `decode_code_info` checks the code.
#[uniffi::export]
pub fn read_scanned_code(text: String) -> Result<BridgeScannedCode, BridgeError> {
    let scanned =
        bae_core::sync::restore_code::read_scanned_code(&text).map_err(BridgeError::config)?;
    Ok(BridgeScannedCode {
        code: scanned.code,
        s3_credentials: scanned.s3_credentials.map(BridgeS3Credentials::from_core),
    })
}
#[derive(Clone, uniffi::Enum)]
pub enum BridgeBootstrapStatus {
    Connecting,
    WaitingForApproval,
    Loading,
    Pending { records: Vec<BridgePendingRecord> },
}
impl BridgeBootstrapStatus {
    pub(crate) fn from_core(value: coven::BootstrapStatus) -> Self {
        match value {
            coven::BootstrapStatus::Connecting => Self::Connecting,
            coven::BootstrapStatus::WaitingForApproval => Self::WaitingForApproval,
            coven::BootstrapStatus::Loading => Self::Loading,
            coven::BootstrapStatus::Pending(records) => Self::Pending {
                records: records
                    .into_iter()
                    .map(|record| {
                        BridgePendingRecord::from_core(bae_core::library::PendingItem::before_open(
                            record,
                        ))
                    })
                    .collect(),
            },
        }
    }
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
    Declined,
}
impl BridgeBootstrapResult {
    pub(crate) fn opened(handle: coven::CovenHandle, host: Arc<crate::BridgeHost>) -> Self {
        Self::Opened {
            library: Arc::new(crate::OpenedLibrary::new(handle, host)),
        }
    }
    pub(crate) fn failure(
        error: bae_core::library::RestoreFromCodeError,
        host: Arc<crate::BridgeHost>,
    ) -> Result<Self, BridgeError> {
        let category = BridgeErrorCategory::from_core(error.category());
        match error {
            bae_core::library::RestoreFromCodeError::Cancelled => Err(BridgeError::Cancelled),
            bae_core::library::RestoreFromCodeError::Restore(error) => match *error {
                coven::BootstrapError::Published { handle, source } => Ok(Self::Published {
                    library: Arc::new(crate::OpenedLibrary::new(handle, host)),
                    error: BridgeError::diagnostic(category, source),
                }),
                error => Err(BridgeError::diagnostic(category, error)),
            },
            error => Err(BridgeError::diagnostic(category, error)),
        }
    }
}

/// One library's entry in iCloud Keychain, as listing found it. Only a
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
            coven::SavedRestoreEntry::Readable(saved) => Self::Readable {
                code: Arc::new(BridgeSavedRestoreCode { saved }),
            },
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

/// A restore code saved in iCloud Keychain that bae can restore from.
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
    pub has_s3_credentials: bool,
}
#[uniffi::export]
impl BridgeSavedRestoreCode {
    pub fn info(&self) -> BridgeSavedRestoreCodeInfo {
        BridgeSavedRestoreCodeInfo {
            name: self.saved.name.clone(),
            library_id: self.saved.store_id.to_string(),
            location: BridgeStorageLocation::from_core(self.saved.location.clone()),
            has_s3_credentials: self.saved.has_s3_credentials,
        }
    }
}
