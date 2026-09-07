use super::*;

/// Every key the two device-pairing progress fns can emit. An explicit array
/// of all variants feeds the production fn; the keys are never restated here.
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
        BridgeJoiningDeviceJoinProgress::WaitingForActivation,
        BridgeJoiningDeviceJoinProgress::CatchingUp,
        BridgeJoiningDeviceJoinProgress::SavingLibrary,
    ] {
        keys.push(bridge_joining_device_join_progress_key(&progress));
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
        keys.push(bridge_admitting_device_join_progress_key(progress));
    }

    keys
}

#[test]
fn every_device_pairing_progress_has_one_localization_key() {
    assert_eq!(progress_keys().len(), 18);
}

#[test]
fn joining_device_snapshot_progress_crosses_the_bridge_with_byte_counts() {
    assert_eq!(
        BridgeJoiningDeviceJoinProgress::from_core(
            coven::JoiningDeviceJoinProgress::DownloadingSnapshot {
                bytes_done: 1_024,
                bytes_total: 4_096,
            }
        ),
        BridgeJoiningDeviceJoinProgress::DownloadingSnapshot {
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
