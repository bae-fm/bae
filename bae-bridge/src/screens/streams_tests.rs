use super::*;

/// The files being sent together are in their stream only while any is
/// being sent: a total with no bar holds nothing.
#[test]
fn the_upload_total_is_held_only_while_files_are_sent() {
    let bar = bae_core::library::UploadBar { bytes_done: 10, bytes_total: 40 };
    let sending = bae_core::library::UploadTransfer { bar: Some(bar), throughput_bps: 5 };
    assert_eq!(
        upload_total(sending),
        Some(BridgeFastValue::UploadTotal {
            transfer: crate::types::BridgeUploadTransfer {
                bar: Some(crate::types::BridgeUploadBar { bytes_done: 10, bytes_total: 40 }),
                throughput_bps: 5,
            },
        })
    );
    assert_eq!(
        upload_total(bae_core::library::UploadTransfer { bar: None, throughput_bps: 0 }),
        None
    );
}

/// A load's download is held under its track's key, and nothing is held
/// while no load waits on one.
#[test]
fn a_track_download_is_held_under_its_track() {
    let held = track_download(Some(bae_core::playback::TrackDownload {
        track_id: "track-1".into(),
        here: 3,
        total: 8,
    }));
    assert_eq!(
        held,
        HashMap::from([(
            b"track-1".to_vec(),
            BridgeFastValue::TrackDownload {
                progress: crate::types::BridgeTrackDownload { here: 3, total: 8 },
            },
        )])
    );
    assert!(track_download(None).is_empty());
}
