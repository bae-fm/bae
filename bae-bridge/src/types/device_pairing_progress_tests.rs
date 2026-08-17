use super::*;

pub(super) fn progress_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for progress in [
        BridgeJoiningDeviceJoinProgress::WaitingForApproval,
        BridgeJoiningDeviceJoinProgress::RequestingProviderAccess,
        BridgeJoiningDeviceJoinProgress::WaitingForProviderAccess,
        BridgeJoiningDeviceJoinProgress::RegisteringDevice,
        BridgeJoiningDeviceJoinProgress::WaitingForLibrary,
        BridgeJoiningDeviceJoinProgress::DownloadingSnapshot {
            bytes_done: 1,
            bytes_total: 2,
        },
        BridgeJoiningDeviceJoinProgress::InstallingSnapshot,
        BridgeJoiningDeviceJoinProgress::DownloadingLibraryFiles {
            files_done: 1,
            files_total: 2,
            bytes_done: 3,
            bytes_total: 4,
        },
        BridgeJoiningDeviceJoinProgress::WaitingForActivation,
        BridgeJoiningDeviceJoinProgress::CatchingUp,
        BridgeJoiningDeviceJoinProgress::SavingLibrary,
    ] {
        let expected = match progress {
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
            BridgeJoiningDeviceJoinProgress::DownloadingLibraryFiles { .. } => {
                "core.pairing.join.downloading_library_files"
            }
            BridgeJoiningDeviceJoinProgress::WaitingForActivation => {
                "core.pairing.join.waiting_for_activation"
            }
            BridgeJoiningDeviceJoinProgress::CatchingUp => "core.pairing.join.catching_up",
            BridgeJoiningDeviceJoinProgress::SavingLibrary => "core.pairing.join.saving_library",
        };
        assert_eq!(bridge_joining_device_join_progress_key(&progress), expected);
        keys.push(expected.to_string());
    }

    for progress in [
        BridgeAdmittingDeviceJoinProgress::PreparingInvitation,
        BridgeAdmittingDeviceJoinProgress::WaitingForProviderAccessRequest,
        BridgeAdmittingDeviceJoinProgress::GrantingProviderAccess,
        BridgeAdmittingDeviceJoinProgress::WaitingForRegistrationRequest,
        BridgeAdmittingDeviceJoinProgress::RegisteringDevice,
        BridgeAdmittingDeviceJoinProgress::PreparingLibrary,
        BridgeAdmittingDeviceJoinProgress::WaitingForJoiningDevice,
        BridgeAdmittingDeviceJoinProgress::ActivatingDevice,
    ] {
        let expected = match progress {
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
            BridgeAdmittingDeviceJoinProgress::RegisteringDevice => {
                "core.pairing.registering_device"
            }
            BridgeAdmittingDeviceJoinProgress::PreparingLibrary => {
                "core.pairing.admit.preparing_library"
            }
            BridgeAdmittingDeviceJoinProgress::WaitingForJoiningDevice => {
                "core.pairing.admit.waiting_for_joining_device"
            }
            BridgeAdmittingDeviceJoinProgress::ActivatingDevice => {
                "core.pairing.admit.activating_device"
            }
        };
        assert_eq!(
            bridge_admitting_device_join_progress_key(progress),
            expected
        );
        keys.push(expected.to_string());
    }
    keys
}

#[test]
fn every_device_pairing_progress_has_one_localization_key() {
    assert_eq!(progress_keys().len(), 19);
}

#[test]
fn joining_device_progress_crosses_the_bridge_with_transfer_counts() {
    assert_eq!(
        BridgeJoiningDeviceJoinProgress::from_core(
            coven::JoiningDeviceJoinProgress::DownloadingLibraryFiles {
                files_done: 3,
                files_total: 7,
                bytes_done: 1_024,
                bytes_total: 4_096,
            }
        ),
        BridgeJoiningDeviceJoinProgress::DownloadingLibraryFiles {
            files_done: 3,
            files_total: 7,
            bytes_done: 1_024,
            bytes_total: 4_096,
        }
    );
}

#[test]
fn admitting_device_progress_crosses_the_bridge_without_collapsing_waits() {
    assert_eq!(
        BridgeAdmittingDeviceJoinProgress::from_core(
            coven::AdmittingDeviceJoinProgress::WaitingForJoiningDevice
        ),
        BridgeAdmittingDeviceJoinProgress::WaitingForJoiningDevice
    );
}
