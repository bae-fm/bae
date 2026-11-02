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

/// The playing track's held stretches are held under its track's key, and
/// nothing is held while no track plays.
#[test]
fn a_tracks_held_stretches_are_held_under_its_track() {
    let held = track_buffered_held(Some(bae_core::playback::TrackBuffered {
        track_id: "track-1".into(),
        ranges: vec![bae_core::playback::TrackSpan { start: 0.0, end: 0.25 }],
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
