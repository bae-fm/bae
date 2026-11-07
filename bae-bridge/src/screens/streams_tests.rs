use super::*;

/// The files being sent together are in their stream only while any is
/// being sent: a total with no bar holds nothing.
#[test]
fn the_upload_total_is_held_only_while_files_are_sent() {
    let bar = bae_library::UploadBar { bytes_done: 10, bytes_total: 40 };
    let sending = bae_library::UploadTransfer { bar: Some(bar), throughput_bps: 5 };
    assert_eq!(
        upload_total(sending),
        Some(BridgeFastValue::UploadTotal {
            transfer: crate::types::BridgeUploadTransfer {
                bar: Some(crate::types::BridgeUploadBar { bytes_done: 10, bytes_total: 40 }),
                throughput_bps: 5,
            },
        })
    );
    assert_eq!(upload_total(bae_library::UploadTransfer { bar: None, throughput_bps: 0 }), None);
}

/// The playing track's held stretches are held under its track's key, and
/// nothing is held while no track plays.
#[test]
fn a_tracks_held_stretches_are_held_under_its_track() {
    let held = track_buffered_held(Some(bae_playback::TrackBuffered {
        track_id: "track-1".into(),
        ranges: vec![bae_playback::TrackSpan { start: 0.0, end: 0.25 }],
    }));
    assert_eq!(
        held,
        HashMap::from([(
            b"track-1".to_vec(),
            BridgeFastValue::TrackBuffered {
                buffered: crate::types::BridgeTrackBuffered {
                    ranges: vec![crate::types::BridgeTrackSpan { start: 0.0, end: 0.25 }],
                },
            },
        )])
    );
    assert!(track_buffered_held(None).is_empty());
}

/// A load's wait on storage is held under its track's key, and nothing is
/// held while no load waits on storage.
#[test]
fn a_storage_wait_is_held_under_its_track() {
    assert_eq!(
        track_storage_wait(Some("track-1".into())),
        HashMap::from([(b"track-1".to_vec(), BridgeFastValue::TrackWaitsOnStorage)])
    );
    assert!(track_storage_wait(None).is_empty());
}

/// A stream of one kind of download names only the releases doing that
/// kind: a release that turns to the other kind leaves it, and a reset
/// names only its own.
#[test]
fn a_download_stream_holds_only_its_own_kind_of_download() {
    use bae_transfers::{DownloadTransfer, MakeLocalTransferProgress, ReleaseTransferChange};
    let pin = DownloadTransfer::Pin(
        bae_transfers::DownloadTransferProgress::new("pinning", 10, 40).unwrap(),
    );
    let local = DownloadTransfer::MakeLocal(MakeLocalTransferProgress::begin(40));
    let reset = || {
        ReleaseTransferChange::Reset(
            [("pinning".to_string(), pin.clone()), ("local".to_string(), local.clone())].into(),
        )
    };
    let moved = |release: &str, transfer: &DownloadTransfer| ReleaseTransferChange::Updated {
        release_id: release.to_string(),
        progress: transfer.clone(),
    };

    let pins = download_changes(vec![reset(), moved("local", &local)], pin_progress);
    let DownloadTransfer::Pin(pin_progress) = pin.clone() else { unreachable!() };
    assert_eq!(
        pins,
        vec![
            ReleaseTransferChange::Reset([("pinning".to_string(), pin_progress)].into()),
            ReleaseTransferChange::Removed { release_id: "local".to_string() },
        ]
    );

    let locals = download_changes(
        vec![
            reset(),
            moved("pinning", &pin),
            ReleaseTransferChange::Removed { release_id: "local".to_string() },
        ],
        make_local_progress,
    );
    let DownloadTransfer::MakeLocal(local_progress) = local else { unreachable!() };
    assert_eq!(
        locals,
        vec![
            ReleaseTransferChange::Reset([("local".to_string(), local_progress)].into()),
            ReleaseTransferChange::Removed { release_id: "pinning".to_string() },
            ReleaseTransferChange::Removed { release_id: "local".to_string() },
        ]
    );
}
