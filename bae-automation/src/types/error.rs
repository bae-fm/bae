use super::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "message")]
pub enum AutomationError {
    Database(String),
    Import(String),
    NotFound(String),
    Validation(String),
    Unavailable(String),
    Timeout(String),
    Internal(String),
}

impl AutomationError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Database(_) => "database",
            Self::Import(_) => "import",
            Self::NotFound(_) => "not_found",
            Self::Validation(_) => "validation",
            Self::Unavailable(_) => "unavailable",
            Self::Timeout(_) => "timeout",
            Self::Internal(_) => "internal",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Database(message)
            | Self::Import(message)
            | Self::NotFound(message)
            | Self::Validation(message)
            | Self::Unavailable(message)
            | Self::Timeout(message)
            | Self::Internal(message) => message,
        }
    }

    pub(crate) fn import(message: impl Into<String>) -> Self {
        Self::Import(message.into())
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

impl std::fmt::Display for AutomationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for AutomationError {}

impl From<LibraryError> for AutomationError {
    fn from(value: LibraryError) -> Self {
        match value {
            LibraryError::Database(e) => Self::Database(e.to_string()),
            LibraryError::Io(e) => Self::Unavailable(e.to_string()),
            LibraryError::Import(e) => Self::Import(e),
            LibraryError::ArtistIdentityConflict(e) => Self::Import(e.to_string()),
            LibraryError::Export(e) | LibraryError::Save(e) => Self::Unavailable(e),
            // A rejected metadata edit is the caller's bad input, not an import
            // failure — MCP sees it as `validation`, the kind it can act on.
            LibraryError::Edit(e) => Self::Validation(e.to_string()),
            LibraryError::TrackMapping(e) => Self::Import(e),
            LibraryError::Encryption(e) => Self::Unavailable(e.to_string()),
            LibraryError::Storage(e) => Self::Unavailable(e),
            LibraryError::Playback(e) => Self::Unavailable(e),
            LibraryError::Internal(e) => Self::Internal(e),
            LibraryError::Config(e) => Self::Internal(e.to_string()),
            LibraryError::Keyring(e) => Self::Unavailable(e.to_string()),
            LibraryError::CloudHome(e) => Self::Unavailable(e.to_string()),
            LibraryError::CloudSetup(e) => Self::Unavailable(e.to_string()),
            LibraryError::CloudUnlock(e) => Self::Unavailable(e.to_string()),
            LibraryError::Sync(e) => Self::Unavailable(e.to_string()),
            LibraryError::RetryBlockedOperation(e) => Self::Unavailable(e.to_string()),
            LibraryError::DevicePairingStart(e) => Self::Unavailable(e.to_string()),
            LibraryError::DevicePairingApproval(e) => Self::Unavailable(e.to_string()),
            LibraryError::DevicePairingTransport(e) => Self::Unavailable(e.to_string()),
            error @ LibraryError::DeviceJoinAbandoned => Self::Unavailable(error.to_string()),
            LibraryError::Validation(e) => Self::Validation(e),
            LibraryError::MasterKey(e) => Self::Unavailable(e.to_string()),
            LibraryError::Identity(e) => Self::Unavailable(e.to_string()),
        }
    }
}

impl From<bae_core::ui::UiError> for AutomationError {
    fn from(value: bae_core::ui::UiError) -> Self {
        use bae_core::ui::{UiError, UiErrorCategory};
        let (category, detail) = match value {
            UiError::NotFound { entity, id } => {
                return Self::NotFound(format!("{entity:?} {id}"));
            }
            UiError::Diagnostic { category, detail } => (category, detail),
        };
        match category {
            UiErrorCategory::Database => Self::Database(detail),
            UiErrorCategory::Import => Self::Import(detail),
            UiErrorCategory::Config => Self::Validation(detail),
            UiErrorCategory::Internal => Self::Internal(detail),
            UiErrorCategory::Export
            | UiErrorCategory::Save
            | UiErrorCategory::CloudSetup(_)
            | UiErrorCategory::DeviceIdentityMissing
            | UiErrorCategory::Credentials
            | UiErrorCategory::Network
            | UiErrorCategory::Keyring
            | UiErrorCategory::KeyringLocked
            | UiErrorCategory::Membership => Self::Unavailable(detail),
        }
    }
}

impl From<ImportError> for AutomationError {
    /// Import failures cross as an opaque `import` error carrying the typed
    /// error's Display as the message.
    fn from(value: ImportError) -> Self {
        Self::Import(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_ui_diagnostics_remain_internal() {
        let error = AutomationError::from(bae_core::ui::UiError::internal("admission invariant"));
        assert!(matches!(
            error,
            AutomationError::Internal(detail) if detail == "admission invariant"
        ));
    }

    #[test]
    fn keyed_ui_absence_remains_not_found() {
        let error = AutomationError::from(bae_core::ui::UiError::NotFound {
            entity: bae_core::ui::UiEntityKind::Release,
            id: "release-id".to_string(),
        });
        assert!(matches!(
            error,
            AutomationError::NotFound(detail) if detail.contains("release-id")
        ));
    }
}
