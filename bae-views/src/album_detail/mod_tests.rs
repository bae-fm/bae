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
            available_storage_actions(state, &Playable, false, false),
            Vec::<ReleaseStorageAction>::new(),
            "no storage blocks all actions for {state:?}"
        );
    }
}

#[test]
fn on_one_device_with_cloud_offers_upload() {
    assert_eq!(available_storage_actions(OnOneDevice, &Playable, false, true), vec![Upload]);
}

/// Uploading reads this device's originals, so a release another device
/// imported offers nothing to upload.
#[test]
fn on_one_device_without_its_originals_here_offers_nothing() {
    let elsewhere =
        OnOtherDevice(bae_records::OtherDevice::Active { name: "Device A".to_string() });
    assert_eq!(
        available_storage_actions(OnOneDevice, &elsewhere, false, true),
        Vec::<ReleaseStorageAction>::new(),
    );
}

/// A release in storage offers Pin or Unpin by whether this device keeps
/// it, and Detach either way; none needs the sync loop running.
#[test]
fn in_storage_offers_pin_or_unpin_and_detach_audio() {
    assert_eq!(available_storage_actions(PINNED, &Playable, false, true), vec![Unpin, DetachAudio]);
    assert_eq!(
        available_storage_actions(NOT_PINNED, &Playable, false, true),
        vec![Pin, DetachAudio]
    );
}

/// A release coven is still copying files of to this device offers Unpin,
/// which stops the copying, in place of Pin, and reads as Copying here.
#[test]
fn a_release_kept_here_offers_unpin_and_reads_copying_here() {
    assert_eq!(
        available_storage_actions(NOT_PINNED, &Playable, true, true),
        vec![Unpin, DetachAudio]
    );
    assert_eq!(
        release_download_status(NOT_PINNED, &[Unpin, DetachAudio], true, None),
        Some(ReleaseDownloadStatus::CopyingHere),
    );
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
                release_download_status(state, &[Unpin, DetachAudio], false, Some(&phase)),
                Some(expected.clone()),
                "a {phase:?} entry decides the control ({state:?})"
            );
        }
    }
}

/// A make-local's entry is not the download control's to show; its own
/// status says where the release stands in being made local.
#[test]
fn a_make_local_entry_is_its_own_status() {
    use bae_transfers::{DownloadEntry, DownloadKind, DownloadStep};
    let make_local = |step| DownloadEntry { kind: DownloadKind::MakeLocal, step };
    let waiting = make_local(DownloadStep::Queued);
    let running = make_local(DownloadStep::Running);
    let failed = make_local(DownloadStep::Failed { error: "disk full".to_string() });
    for entry in [&waiting, &running, &failed] {
        assert_eq!(
            release_download_status(NOT_PINNED, &[Pin, DetachAudio], false, Some(entry)),
            None
        );
    }
    assert_eq!(release_make_local_status(Some(&waiting)), Some(MakeLocalStatus::Queued));
    assert_eq!(release_make_local_status(Some(&running)), Some(MakeLocalStatus::Making));
    assert_eq!(
        release_make_local_status(Some(&failed)),
        Some(MakeLocalStatus::Failed { error: "disk full".to_string() })
    );
    let pin = DownloadEntry { kind: DownloadKind::Pin, step: DownloadStep::Running };
    assert_eq!(release_make_local_status(Some(&pin)), None);
    assert_eq!(release_make_local_status(None), None);
}

/// Pinned with nothing in flight is the settled offline state.
#[test]
fn pinned_with_an_empty_queue_is_downloaded() {
    assert_eq!(
        release_download_status(PINNED, &[Unpin, DetachAudio], false, None),
        Some(ReleaseDownloadStatus::Downloaded),
    );
}

/// Available is exactly "core offers Pin" — never re-derived from the state.
#[test]
fn unpinned_with_pin_offered_is_available() {
    assert_eq!(
        release_download_status(NOT_PINNED, &[Pin, DetachAudio], false, None),
        Some(ReleaseDownloadStatus::Available),
    );
}

/// No Pin on offer means there is nothing to show: no storage, or the
/// release's audio is on one device.
#[test]
fn no_pin_on_offer_means_no_control() {
    assert_eq!(release_download_status(NOT_PINNED, &[], false, None), None);
    assert_eq!(release_download_status(OnOneDevice, &[Upload], false, None), None);
}
