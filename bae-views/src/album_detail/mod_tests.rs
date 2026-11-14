use super::*;
use ReleaseStorageAction::*;
use ReleaseStorageState::*;

const PINNED: ReleaseStorageState = InStorage { pinned: true };
const NOT_PINNED: ReleaseStorageState = InStorage { pinned: false };

#[test]
fn storage_state_is_the_stored_fact_with_pinning_inside_it() {
    assert_eq!(storage_state(false, false), OnOneDevice);
    assert_eq!(storage_state(true, false), NOT_PINNED);
    assert_eq!(storage_state(true, true), PINNED);
}

/// A pin's entry in the queue decides the control, whatever the release's
/// pin state reads.
#[test]
fn a_pin_entry_decides_the_control() {
    use bae_transfers::{DownloadEntry, DownloadKind, DownloadStep};
    let pin = |step| DownloadEntry { kind: DownloadKind::Pin, step };
    let cases = [
        (pin(DownloadStep::Queued), ReleaseDownloadStatus::Queued),
        (pin(DownloadStep::Running), ReleaseDownloadStatus::Downloading),
        (
            pin(DownloadStep::Failed { error: "read failed".to_string() }),
            ReleaseDownloadStatus::Failed { error: "read failed".to_string() },
        ),
    ];
    for (phase, expected) in cases {
        for state in [NOT_PINNED, PINNED] {
            assert_eq!(
                release_download_status(state, &[Unpin, MoveToThisDevice], Some(&phase)),
                Some(expected.clone()),
                "a {phase:?} entry decides the control ({state:?})"
            );
        }
    }
}

/// A move-to-this-device's entry is not the download control's to show; its own
/// status says where the release stands in being moved to this device.
#[test]
fn a_move_to_this_device_entry_is_its_own_status() {
    use bae_transfers::{DownloadEntry, DownloadKind, DownloadStep};
    let move_to_this_device = |step| DownloadEntry { kind: DownloadKind::MoveToThisDevice, step };
    let waiting = move_to_this_device(DownloadStep::Queued);
    let running = move_to_this_device(DownloadStep::Running);
    let failed = move_to_this_device(DownloadStep::Failed { error: "disk full".to_string() });
    for entry in [&waiting, &running, &failed] {
        assert_eq!(
            release_download_status(NOT_PINNED, &[Pin, MoveToThisDevice], Some(entry)),
            None
        );
    }
    assert_eq!(
        release_move_to_this_device_status(Some(&waiting)),
        Some(MoveToThisDeviceStatus::Queued)
    );
    assert_eq!(
        release_move_to_this_device_status(Some(&running)),
        Some(MoveToThisDeviceStatus::Making)
    );
    assert_eq!(
        release_move_to_this_device_status(Some(&failed)),
        Some(MoveToThisDeviceStatus::Failed { error: "disk full".to_string() })
    );
    let pin = DownloadEntry { kind: DownloadKind::Pin, step: DownloadStep::Running };
    assert_eq!(release_move_to_this_device_status(Some(&pin)), None);
    assert_eq!(release_move_to_this_device_status(None), None);
}

/// Pinned with nothing in flight is the settled offline state.
#[test]
fn pinned_with_an_empty_queue_is_downloaded() {
    assert_eq!(
        release_download_status(PINNED, &[Unpin, MoveToThisDevice], None),
        Some(ReleaseDownloadStatus::Downloaded),
    );
}

/// Available is exactly "core offers Pin" — never re-derived from the state.
#[test]
fn unpinned_with_pin_offered_is_available() {
    assert_eq!(
        release_download_status(NOT_PINNED, &[Pin, MoveToThisDevice], None),
        Some(ReleaseDownloadStatus::Available),
    );
}

/// No Pin on offer means there is nothing to show: no storage, or the
/// release's audio is on one device.
#[test]
fn no_pin_on_offer_means_no_control() {
    assert_eq!(release_download_status(NOT_PINNED, &[], None), None);
    assert_eq!(release_download_status(OnOneDevice, &[Upload], None), None);
}
