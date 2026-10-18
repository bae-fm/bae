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
                    .map(BridgePendingRecord::from_core)
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
