use super::*;
use ReleasePlayability::*;
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

#[test]
fn no_storage_has_no_actions() {
    for state in [OnOneDevice, NOT_PINNED, PINNED] {
        assert_eq!(
            available_storage_actions(state, &Playable, false),
            Vec::<ReleaseStorageAction>::new(),
            "no storage blocks all actions for {state:?}"
        );
    }
}

#[test]
fn on_one_device_with_cloud_offers_upload() {
    assert_eq!(available_storage_actions(OnOneDevice, &Playable, true), vec![Upload]);
}

/// Uploading reads this device's originals, so a release another device
/// imported offers nothing to upload.
#[test]
fn on_one_device_without_its_originals_here_offers_nothing() {
    let elsewhere =
        OnOtherDevice(bae_records::OtherDevice::Active { name: "Device A".to_string() });
    assert_eq!(
        available_storage_actions(OnOneDevice, &elsewhere, true),
        Vec::<ReleaseStorageAction>::new(),
    );
}

/// A release in storage offers Pin or Unpin by whether this device keeps
/// it, and Detach either way; none needs the sync loop running.
#[test]
fn in_storage_offers_pin_or_unpin_and_detach_audio() {
    assert_eq!(available_storage_actions(PINNED, &Playable, true), vec![Unpin, DetachAudio]);
    assert_eq!(available_storage_actions(NOT_PINNED, &Playable, true), vec![Pin, DetachAudio]);
}

/// The whole point of the join: on pin success the release value and
/// the queue-drop snapshot arrive in either order, so while an entry is still
/// in the queue it decides the control — even once the release reads pinned.
#[test]
fn a_queue_entry_outranks_pinned() {
    use bae_transfers::DownloadPhase;
    let cases = [
        (DownloadPhase::Queued, ReleaseDownloadStatus::Queued),
        (DownloadPhase::Active { progress: () }, ReleaseDownloadStatus::Downloading),
        (
            DownloadPhase::Failed { error: "read failed".to_string() },
            ReleaseDownloadStatus::Failed { error: "read failed".to_string() },
        ),
    ];
    for (phase, expected) in cases {
        for state in [NOT_PINNED, PINNED] {
            assert_eq!(
                release_download_status(state, &[Unpin, DetachAudio], Some(&phase)),
                Some(expected.clone()),
                "a {phase:?} entry decides the control ({state:?})"
            );
        }
    }
}

/// Pinned with nothing in flight is the settled offline state.
#[test]
fn pinned_with_an_empty_queue_is_downloaded() {
    assert_eq!(
        release_download_status(PINNED, &[Unpin, DetachAudio], None),
        Some(ReleaseDownloadStatus::Downloaded),
    );
}

/// Available is exactly "core offers Pin" — never re-derived from the state.
#[test]
fn unpinned_with_pin_offered_is_available() {
    assert_eq!(
        release_download_status(NOT_PINNED, &[Pin, DetachAudio], None),
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
