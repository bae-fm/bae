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
    Lookup(AutomationLookupFailure),
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
            Self::Lookup(_) => "lookup",
            Self::Internal(_) => "internal",
        }
    }

    pub fn message(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Database(message)
            | Self::Import(message)
            | Self::NotFound(message)
            | Self::Validation(message)
            | Self::Unavailable(message)
            | Self::Timeout(message)
            | Self::Internal(message) => std::borrow::Cow::Borrowed(message),
            Self::Lookup(failure) => std::borrow::Cow::Owned(match failure {
                AutomationLookupFailure::Network => {
                    "couldn't reach the metadata provider".to_string()
                }
                AutomationLookupFailure::Provider {
                    status: Some(status),
                } => format!("metadata provider returned HTTP {status}"),
                AutomationLookupFailure::Provider { status: None } => {
                    "metadata provider returned an error".to_string()
                }
                AutomationLookupFailure::RateLimited => {
                    "metadata provider rate limit reached".to_string()
                }
                AutomationLookupFailure::Credentials => {
                    "metadata provider needs valid credentials".to_string()
                }
                AutomationLookupFailure::Timeout => {
                    "metadata provider request timed out".to_string()
                }
                AutomationLookupFailure::ArtworkAnalysis => "artwork analysis failed".to_string(),
                AutomationLookupFailure::Diagnostic { detail } => detail.clone(),
            }),
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

impl From<ImportError> for AutomationError {
    /// Import failures cross as an opaque `import` error carrying the typed
    /// error's Display as the message.
    fn from(value: ImportError) -> Self {
        Self::Import(value.to_string())
    }
}

impl From<SearchError> for AutomationError {
    fn from(value: SearchError) -> Self {
        match value {
            SearchError::Lookup { failure } => {
                Self::Lookup(crate::convert::automation_lookup_failure(failure))
            }
            SearchError::Diagnostic { error } => match error {
                bae_core::ui::UiError::NotFound { entity, id } => {
                    Self::NotFound(format!("{entity:?} '{id}' not found"))
                }
                bae_core::ui::UiError::Diagnostic { category, detail } => {
                    use bae_core::ui::UiErrorCategory;
                    match category {
                        UiErrorCategory::Database => Self::Database(detail),
                        UiErrorCategory::Import => Self::Import(detail),
                        UiErrorCategory::Internal | UiErrorCategory::Config => {
                            Self::Internal(detail)
                        }
                        UiErrorCategory::CloudSetup(_)
                        | UiErrorCategory::DeviceIdentityMissing
                        | UiErrorCategory::Credentials
                        | UiErrorCategory::Network
                        | UiErrorCategory::Keyring
                        | UiErrorCategory::KeyringLocked
                        | UiErrorCategory::Membership => Self::Unavailable(detail),
                        UiErrorCategory::Export | UiErrorCategory::Save => {
                            Self::Unavailable(detail)
                        }
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_lookup_error_preserves_provider_status() {
        let error = AutomationError::from(SearchError::Lookup {
            failure: bae_core::signals::LookupFailure::Provider { status: Some(503) },
        });

        assert_eq!(error.kind(), "lookup");
        assert_eq!(error.message(), "metadata provider returned HTTP 503");
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "kind": "lookup",
                "message": { "kind": "provider", "status": 503 }
            })
        );
    }
}
