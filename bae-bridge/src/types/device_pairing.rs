#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeJoiningDeviceJoinProgress {
    WaitingForApproval,
    RequestingProviderAccess,
    WaitingForProviderAccess,
    RegisteringDevice,
    WaitingForLibrary,
    DownloadingSnapshot { bytes_done: u64, bytes_total: u64 },
    InstallingSnapshot,
    WaitingForActivation,
    CatchingUp,
    SavingLibrary,
}

mirror_enum! {
    BridgeJoiningDeviceJoinProgress = coven::JoiningDeviceJoinProgress,
    from_core: pub(crate) fn,
    variants: {
        WaitingForApproval,
        RequestingProviderAccess,
        WaitingForProviderAccess,
        RegisteringDevice,
        WaitingForLibrary,
        DownloadingSnapshot { bytes_done, bytes_total },
        InstallingSnapshot,
        WaitingForActivation,
        CatchingUp,
        SavingLibrary,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAdmittingDeviceJoinProgress {
    PreparingInvitation,
    WaitingForProviderAccessRequest,
    GrantingProviderAccess,
    WaitingForRegistrationRequest,
    RegisteringDevice,
    PreparingLibrary,
    WaitingForJoiningDevice,
    ActivatingDevice,
}

mirror_enum! {
    BridgeAdmittingDeviceJoinProgress = coven::AdmittingDeviceJoinProgress,
    from_core: pub(crate) fn,
    variants: {
        PreparingInvitation,
        WaitingForProviderAccessRequest,
        GrantingProviderAccess,
        WaitingForRegistrationRequest,
        RegisteringDevice,
        PreparingLibrary,
        WaitingForJoiningDevice,
        ActivatingDevice,
    },
}

#[uniffi::export(callback_interface)]
pub trait JoiningDeviceJoinProgressCallback: Send + Sync {
    fn on_progress(&self, progress: BridgeJoiningDeviceJoinProgress);
}

#[uniffi::export(callback_interface)]
pub trait AdmittingDeviceJoinProgressCallback: Send + Sync {
    fn on_progress(&self, progress: BridgeAdmittingDeviceJoinProgress);
}

#[uniffi::export]
pub fn bridge_joining_device_join_progress_key(
    progress: &BridgeJoiningDeviceJoinProgress,
) -> String {
    match progress {
        BridgeJoiningDeviceJoinProgress::WaitingForApproval => {
            "core.pairing.join.waiting_for_approval"
        }
        BridgeJoiningDeviceJoinProgress::RequestingProviderAccess => {
            "core.pairing.join.requesting_provider_access"
        }
        BridgeJoiningDeviceJoinProgress::WaitingForProviderAccess => {
            "core.pairing.join.waiting_for_provider_access"
        }
        BridgeJoiningDeviceJoinProgress::RegisteringDevice => "core.pairing.registering_device",
        BridgeJoiningDeviceJoinProgress::WaitingForLibrary => {
            "core.pairing.join.waiting_for_library"
        }
        BridgeJoiningDeviceJoinProgress::DownloadingSnapshot { .. } => {
            "core.pairing.join.downloading_snapshot"
        }
        BridgeJoiningDeviceJoinProgress::InstallingSnapshot => {
            "core.pairing.join.installing_snapshot"
        }
        BridgeJoiningDeviceJoinProgress::WaitingForActivation => {
            "core.pairing.join.waiting_for_activation"
        }
        BridgeJoiningDeviceJoinProgress::CatchingUp => "core.pairing.join.catching_up",
        BridgeJoiningDeviceJoinProgress::SavingLibrary => "core.pairing.join.saving_library",
    }
    .to_string()
}

#[uniffi::export]
pub fn bridge_admitting_device_join_progress_key(
    progress: BridgeAdmittingDeviceJoinProgress,
) -> String {
    match progress {
        BridgeAdmittingDeviceJoinProgress::PreparingInvitation => {
            "core.pairing.admit.preparing_invitation"
        }
        BridgeAdmittingDeviceJoinProgress::WaitingForProviderAccessRequest => {
            "core.pairing.admit.waiting_for_provider_access_request"
        }
        BridgeAdmittingDeviceJoinProgress::GrantingProviderAccess => {
            "core.pairing.admit.granting_provider_access"
        }
        BridgeAdmittingDeviceJoinProgress::WaitingForRegistrationRequest => {
            "core.pairing.admit.waiting_for_registration_request"
        }
        BridgeAdmittingDeviceJoinProgress::RegisteringDevice => "core.pairing.registering_device",
        BridgeAdmittingDeviceJoinProgress::PreparingLibrary => {
            "core.pairing.admit.preparing_library"
        }
        BridgeAdmittingDeviceJoinProgress::WaitingForJoiningDevice => {
            "core.pairing.admit.waiting_for_joining_device"
        }
        BridgeAdmittingDeviceJoinProgress::ActivatingDevice => {
            "core.pairing.admit.activating_device"
        }
    }
    .to_string()
}
