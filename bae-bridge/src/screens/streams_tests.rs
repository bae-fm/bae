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

/// A track's wait on storage is at rest from the moment it is held: its
/// value cannot change, so a view is told once when it comes and when it goes.
#[test]
fn a_storage_wait_is_at_rest() {
    let values = StreamValues::new();
    let writer = StreamWriter::new(values.clone());
    write_storage_wait(&writer, Some("track-1".into()));
    assert!(!values.moving(b"track-1"));
    assert_eq!(values.latest(b"track-1"), Some(BridgeFastValue::TrackWaitsOnStorage));
    write_storage_wait(&writer, None);
    assert_eq!(values.latest(b"track-1"), None);
}

/// A stream of one kind of download names only the releases doing that
/// kind: a release that turns to the other kind leaves it, and a reset
/// names only its own.
#[test]
fn a_download_stream_holds_only_its_own_kind_of_download() {
    use bae_transfers::{
        DownloadTransfer, MoveToThisDeviceTransferProgress, ReleaseTransferChange,
    };
    let pin = DownloadTransfer::Pin(
        bae_transfers::DownloadTransferProgress::new("pinning", 10, 40).unwrap(),
    );
    let local = DownloadTransfer::MoveToThisDevice(MoveToThisDeviceTransferProgress::begin(40));
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
        move_to_this_device_progress,
    );
    let DownloadTransfer::MoveToThisDevice(local_progress) = local else { unreachable!() };
    assert_eq!(
        locals,
        vec![
            ReleaseTransferChange::Reset([("local".to_string(), local_progress)].into()),
            ReleaseTransferChange::Removed { release_id: "pinning".to_string() },
            ReleaseTransferChange::Removed { release_id: "local".to_string() },
        ]
    );
}

/// One batch of the outbox's transfers writes both streams: the release's
/// transfer into the releases' stream and the total into the total's, which
/// holds nothing once no file is sent.
#[test]
fn one_batch_of_outbox_transfers_writes_both_streams() {
    use bae_library::{OutboxTransferChange, ReleaseUploadTransfer, UploadBar, UploadTransfer};
    use bae_transfers::ReleaseTransferChange;
    let (release_values, total_values) = (StreamValues::new(), StreamValues::new());
    let (releases, total) =
        (StreamWriter::new(release_values.clone()), StreamWriter::new(total_values.clone()));
    let release = ReleaseUploadTransfer {
        bar: UploadBar { bytes_done: 5, bytes_total: 20 },
        throughput_bps: 7,
    };
    let sending = UploadTransfer {
        bar: Some(UploadBar { bytes_done: 5, bytes_total: 20 }),
        throughput_bps: 7,
    };
    write_outbox_changes(
        &releases,
        &total,
        vec![
            OutboxTransferChange::Release(ReleaseTransferChange::Reset(
                [("release-1".to_string(), release)].into(),
            )),
            OutboxTransferChange::Total(sending),
        ],
    );
    assert!(matches!(
        release_values.latest(b"release-1"),
        Some(BridgeFastValue::ReleaseUpload { .. })
    ));
    assert!(matches!(total_values.latest(b""), Some(BridgeFastValue::UploadTotal { .. })));

    write_outbox_changes(
        &releases,
        &total,
        vec![
            OutboxTransferChange::Release(ReleaseTransferChange::Removed {
                release_id: "release-1".to_string(),
            }),
            OutboxTransferChange::Total(UploadTransfer { bar: None, throughput_bps: 0 }),
        ],
    );
    assert_eq!(release_values.latest(b"release-1"), None);
    assert_eq!(total_values.latest(b""), None);
}

/// The Activity lines' bars are held under each line's name while its batch
/// holds a release: the Pinning line's share of its batch's bytes while the
/// pin batch lasts, and nothing once it empties.
#[test]
fn the_activity_bars_hold_pins_while_a_pin_batch_holds_a_release() {
    use bae_library::{ActivityBars, BatchBar};
    let values = StreamValues::new();
    let writer = StreamWriter::new(values.clone());
    let pinning = BatchBar { bytes_done: 250, bytes_total: 400 };
    writer.reset(activity_bars_held(ActivityBars {
        uploads: None,
        pins: Some(pinning),
        moving_to_this_device: None,
    }));
    assert_eq!(values.latest(b"pins"), Some(BridgeFastValue::BatchShare { fraction: 0.625 }));
    assert_eq!(values.latest(b"uploads"), None);
    assert_eq!(values.latest(b"moving_to_this_device"), None);

    writer.reset(activity_bars_held(ActivityBars {
        uploads: None,
        pins: None,
        moving_to_this_device: None,
    }));
    assert_eq!(values.latest(b"pins"), None, "the pin batch is over");
}

/// A track is at rest in its stream once it is held in one stretch from its
/// start to its end, and not before.
#[test]
fn a_track_held_from_start_to_end_in_one_stretch_is_at_rest() {
    let held = |ranges: Vec<(f64, f64)>| {
        track_buffered_held(Some(bae_playback::TrackBuffered {
            track_id: "track-1".into(),
            ranges: ranges
                .into_iter()
                .map(|(start, end)| bae_playback::TrackSpan { start, end })
                .collect(),
        }))
        .into_values()
        .next()
        .unwrap()
    };
    assert!(track_is_fully_buffered(&held(vec![(0.0, 1.0)])));
    assert!(!track_is_fully_buffered(&held(vec![(0.0, 0.5)])));
    assert!(!track_is_fully_buffered(&held(vec![(0.0, 0.5), (0.75, 1.0)])));
    assert!(!track_is_fully_buffered(&held(vec![])));
}

/// A scan's stored folders are held under its root's path, and a scan that
/// has ended holds nothing.
#[cfg(desktop)]
#[test]
fn a_scans_found_count_is_held_under_its_root() {
    let root = std::path::PathBuf::from("/music");
    let held = scan_found_held(bae_import::ScanFoundCounts::from([(root.clone(), 7)]));
    assert_eq!(
        held,
        HashMap::from([(coven::path_bytes(&root), BridgeFastValue::ScanFoundCount { count: 7 })])
    );
    assert!(scan_found_held(bae_import::ScanFoundCounts::new()).is_empty());
}

/// Every tab's count is held under the tab's name, a tab with none too.
#[cfg(desktop)]
#[test]
fn every_tabs_count_is_held_under_its_name() {
    let held = tab_counts_held(bae_import::TriageTabCounts { pending: 5, done: 0, skipped: 2 });
    assert_eq!(
        held,
        HashMap::from([
            (b"pending".to_vec(), BridgeFastValue::TabCount { count: 5 }),
            (b"done".to_vec(), BridgeFastValue::TabCount { count: 0 }),
            (b"skipped".to_vec(), BridgeFastValue::TabCount { count: 2 }),
        ])
    );
}

/// The positions' stream holds a new anchor on a seek, a pause and a resume,
/// and the position ticks between them leave the value it holds as it is.
#[test]
fn the_position_stream_holds_a_new_anchor_only_as_playback_leaves_its_line() {
    const MS: u64 = 1_000_000;
    let values = StreamValues::new();
    let writer = StreamWriter::new(values.clone());
    let mut fold = bae_playback::AnchorFold::new();
    let mut positions = bae_playback::PlaybackPositions {
        library: None,
        seek_revision: 0,
        playing: false,
        preview: None,
        preview_seek_revision: 0,
    };
    let mut at = |now_ms: u64, position_ms: i64, playing: bool, seek_revision: u64| {
        positions.library = Some(bae_playback::PlaybackPosition {
            track_id: "track-1".into(),
            position_ms,
            duration_ms: Some(300_000),
            progress: 0.0,
            rate: 1.0,
        });
        positions.playing = playing;
        positions.seek_revision = seek_revision;
        write_track_anchor(&mut fold, &writer, &positions, now_ms * MS);
        match values.latest(b"track-1") {
            Some(BridgeFastValue::TrackPosition { position }) => position,
            other => panic!("expected a track position, held {other:?}"),
        }
    };

    let started = at(0, 10_000, true, 0);
    assert_eq!((started.position_ms, started.rate, started.anchored_at_ns), (10_000, 1.0, 0));
    assert!(values.moving(b"track-1"));

    for step in 1..=20 {
        let held = at(step * 200, 10_000 + step as i64 * 200, true, 0);
        assert_eq!(held, started, "a tick along the line wrote at {step}");
    }

    let seeked = at(4_200, 90_000, true, 1);
    assert_eq!(
        (seeked.position_ms, seeked.seek_revision, seeked.anchored_at_ns),
        (90_000, 1, 4_200 * MS)
    );

    let paused = at(4_400, 90_000, false, 1);
    assert_eq!((paused.position_ms, paused.rate), (90_000, 0.0));
    assert!(!values.moving(b"track-1"), "a paused track's anchor is at rest");

    let resumed = at(9_000, 90_000, true, 1);
    assert_eq!(
        (resumed.position_ms, resumed.rate, resumed.anchored_at_ns),
        (90_000, 1.0, 9_000 * MS)
    );
    assert!(values.moving(b"track-1"));
}
