mirror_enum! {
    #[cfg(feature = "desktop")]
    crate::types::BridgeMcpServerStatus = bae_desktop::McpServerStatus,
    from_core: pub(super) fn,
    variants: {
        Disabled,
        Running { url },
        Error { error: (crate::types::BridgeMcpServerError) },
    },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    crate::types::BridgeMcpServerError = bae_desktop::McpServerError,
    from_core: pub(super) fn,
    variants: {
        InvalidConfig { detail },
        TokenUnavailable { detail },
        BindFailed { detail },
        ServerFailed { detail },
    },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    crate::types::BridgeSubsonicServerStatus = bae_desktop::SubsonicServerStatus,
    from_core: pub(super) fn,
    variants: {
        Disabled,
        Running { url },
        Error { error: (crate::types::BridgeSubsonicServerError) },
    },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    crate::types::BridgeSubsonicServerError = bae_desktop::SubsonicServerError,
    from_core: pub(super) fn,
    variants: {
        InvalidConfig { detail },
        CredentialUnavailable { detail },
        BindFailed { detail },
        ServerFailed { detail },
    },
}
