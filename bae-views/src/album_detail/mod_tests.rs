use super::*;
use ReleasePlayability::*;
use ReleaseStorageAction::*;
use ReleaseStorageState::*;

#[test]
fn storage_state_is_just_the_remote_fact() {
    assert_eq!(storage_state(false), Local);
    assert_eq!(storage_state(true), Remote);
}

#[test]
fn no_cloud_home_has_no_actions() {
    for state in [Local, Remote] {
        for pinned in [false, true] {
            assert_eq!(
                available_storage_actions(state, &Playable, pinned, false),
                Vec::<ReleaseStorageAction>::new(),
                "no cloud home blocks all actions for {state:?} (pinned={pinned})"
            );
        }
    }
}

#[test]
fn local_with_cloud_offers_upload() {
    // `pinned` is irrelevant for a local release.
    assert_eq!(available_storage_actions(Local, &Playable, false, true), vec![Upload]);
    assert_eq!(available_storage_actions(Local, &Playable, true, true), vec![Upload]);
}

/// Uploading reads this device's originals, so a release another device
/// imported offers nothing to upload.
#[test]
fn local_without_its_originals_here_offers_nothing() {
    let elsewhere = OnOtherDevice(bae_db::OtherDevice::Active { name: "Device A".to_string() });
    assert_eq!(
        available_storage_actions(Local, &elsewhere, false, true),
        Vec::<ReleaseStorageAction>::new(),
    );
}

/// A stopped loop delays the upload; it does not prevent admitting the work.
/// The durable outbox remains the truthful queued state while the loop is
/// stopped and until the loop drains it.
#[test]
fn local_without_a_running_loop_still_offers_upload() {
    for pinned in [false, true] {
        assert_eq!(
            available_storage_actions(Local, &Playable, pinned, true),
            vec![Upload],
            "Upload queues durably even with the sync loop stopped (pinned={pinned})"
        );
    }
}

/// A Remote release's actions move bytes through the connected manager's
/// storage and need no drain loop.
#[test]
fn remote_actions_do_not_need_a_running_loop() {
    assert_eq!(available_storage_actions(Remote, &Playable, true, true), vec![Unpin, DetachAudio]);
    assert_eq!(available_storage_actions(Remote, &Playable, false, true), vec![Pin, DetachAudio]);
}

#[test]
fn remote_pinned_offers_unpin_and_detach_audio() {
    assert_eq!(available_storage_actions(Remote, &Playable, true, true), vec![Unpin, DetachAudio]);
}

#[test]
fn remote_unpinned_offers_pin_and_detach_audio() {
    assert_eq!(available_storage_actions(Remote, &Playable, false, true), vec![Pin, DetachAudio]);
}

/// The whole point of the join: on pin success the release value and
/// the queue-drop snapshot arrive in either order, so while an entry is still
/// in the queue it decides the control — even once `pinned` has flipped.
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
    for (state, expected) in cases {
        for pinned in [false, true] {
            assert_eq!(
                release_download_status(pinned, &[Unpin, DetachAudio], Some(&state),),
                Some(expected.clone()),
                "a {state:?} entry decides the control (pinned={pinned})"
            );
        }
    }
}

/// Pinned with nothing in flight is the settled offline state.
#[test]
fn pinned_with_an_empty_queue_is_downloaded() {
    assert_eq!(
        release_download_status(true, &[Unpin, DetachAudio], None,),
        Some(ReleaseDownloadStatus::Downloaded),
    );
}

/// Available is exactly "core offers Pin" — never re-derived from `pinned`.
#[test]
fn unpinned_with_pin_offered_is_available() {
    assert_eq!(
        release_download_status(false, &[Pin, DetachAudio], None,),
        Some(ReleaseDownloadStatus::Available),
    );
}

/// No Pin on offer means there is nothing to show: no cloud home, or the
/// release's audio is already local.
#[test]
fn no_pin_on_offer_means_no_control() {
    assert_eq!(release_download_status(false, &[], None), None);
    assert_eq!(release_download_status(false, &[Upload], None,), None,);
}
