mirror_enum! {
    #[cfg(feature = "desktop")]
    crate::types::BridgeServerStatus = bae_desktop::ServerStatus,
    from_core: pub(super) fn,
    variants: {
        Disabled,
        Running { url },
        Error { error: (crate::types::BridgeServerError) },
    },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    crate::types::BridgeServerError = bae_desktop::ServerError,
    from_core: pub(super) fn,
    variants: {
        InvalidConfig { detail },
        CredentialUnavailable { detail },
        BindFailed { detail },
        ServerFailed { detail },
    },
}
