#[cfg(feature = "desktop")]
impl crate::types::BridgeMcpServerStatus {
    pub(super) fn from_core(status: bae_desktop::McpServerStatus) -> Self {
        use crate::types::BridgeMcpServerStatus;
        match status {
            bae_desktop::McpServerStatus::Disabled => BridgeMcpServerStatus::Disabled,
            bae_desktop::McpServerStatus::Running { url } => BridgeMcpServerStatus::Running { url },
            bae_desktop::McpServerStatus::Error { error } => BridgeMcpServerStatus::Error {
                error: crate::types::BridgeMcpServerError::from_core(error),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeMcpServerError {
    pub(super) fn from_core(error: bae_desktop::McpServerError) -> Self {
        use crate::types::BridgeMcpServerError;
        match error {
            bae_desktop::McpServerError::InvalidConfig { detail } => {
                BridgeMcpServerError::InvalidConfig { detail }
            }
            bae_desktop::McpServerError::TokenUnavailable { detail } => {
                BridgeMcpServerError::TokenUnavailable { detail }
            }
            bae_desktop::McpServerError::BindFailed { detail } => {
                BridgeMcpServerError::BindFailed { detail }
            }
            bae_desktop::McpServerError::ServerFailed { detail } => {
                BridgeMcpServerError::ServerFailed { detail }
            }
        }
    }
}

#[cfg(feature = "cast")]
impl crate::types::BridgeCastStatus {
    pub(super) fn from_core(status: bae_cast::CastStatus) -> Self {
        use crate::types::BridgeCastStatus;
        match status {
            bae_cast::CastStatus::NotCasting => BridgeCastStatus::NotCasting,
            bae_cast::CastStatus::Casting { device_name } => {
                BridgeCastStatus::Casting { device_name }
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeSubsonicServerStatus {
    pub(super) fn from_core(status: bae_desktop::SubsonicServerStatus) -> Self {
        use crate::types::BridgeSubsonicServerStatus;
        match status {
            bae_desktop::SubsonicServerStatus::Disabled => BridgeSubsonicServerStatus::Disabled,
            bae_desktop::SubsonicServerStatus::Running { url } => {
                BridgeSubsonicServerStatus::Running { url }
            }
            bae_desktop::SubsonicServerStatus::Error { error } => {
                BridgeSubsonicServerStatus::Error {
                    error: crate::types::BridgeSubsonicServerError::from_core(error),
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeSubsonicServerError {
    pub(super) fn from_core(error: bae_desktop::SubsonicServerError) -> Self {
        use crate::types::BridgeSubsonicServerError;
        match error {
            bae_desktop::SubsonicServerError::InvalidConfig { detail } => {
                BridgeSubsonicServerError::InvalidConfig { detail }
            }
            bae_desktop::SubsonicServerError::CredentialUnavailable { detail } => {
                BridgeSubsonicServerError::CredentialUnavailable { detail }
            }
            bae_desktop::SubsonicServerError::BindFailed { detail } => {
                BridgeSubsonicServerError::BindFailed { detail }
            }
            bae_desktop::SubsonicServerError::ServerFailed { detail } => {
                BridgeSubsonicServerError::ServerFailed { detail }
            }
        }
    }
}
