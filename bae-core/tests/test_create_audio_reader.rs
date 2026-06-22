#![cfg(feature = "test-utils")]
//! `create_audio_reader` source dispatch: which reader (or error) a resolved
//! file source maps to. The source is `{ Local, CloudOnly }` — a
//! `CloudOnly` source fetches the cloud home and master key from the
//! `LibraryManager`, or reports `SyncDisconnected` when no home is connected.

mod support;

use bae_core::library::manager::ReadableFileSource;
use bae_core::playback::data_source::create_audio_reader;
use bae_core::playback::PlaybackError;
use support::setup_fresh_library;

/// A `CloudOnly` source with no cloud connection: sync is disconnected. The
/// reader-builder returns `SyncDisconnected` so the UI can prompt for reconnect.
#[test]
fn cloud_only_no_cloud_returns_sync_disconnected() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (manager, _tmp) = setup_fresh_library(&runtime);

    let err = create_audio_reader(
        ReadableFileSource::CloudOnly,
        "file-1",
        "Artist Name/Album Title/01 Track Title.flac",
        &manager,
        |_| unreachable!("no source should not build read config"),
    )
    .err()
    .expect("expected error for a cloud-only track with no cloud connection");
    assert!(
        matches!(err, PlaybackError::SyncDisconnected),
        "expected SyncDisconnected, got: {err:?}",
    );
}
