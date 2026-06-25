#![cfg(feature = "test-utils")]
//! `create_audio_reader` source dispatch: which reader (or error) a resolved
//! file source maps to. A `Managed` source fetches the cloud home and the
//! master key from the `LibraryManager`; an `Unreachable` source never consults
//! the manager at all — the variant already carries the "no readable location"
//! verdict.

mod support;

use std::sync::Arc;

use bae_core::encryption::EncryptionService;
use bae_core::library::manager::ReadableFileSource;
use bae_core::playback::data_source::create_audio_reader;
use bae_core::playback::PlaybackError;
use support::{setup_fresh_library, MockCloudHome};

/// A `Managed` source with no cloud connection: sync is disconnected. The
/// reader-builder returns `SyncDisconnected` so the UI can prompt for reconnect.
#[test]
fn managed_no_cloud_returns_sync_disconnected() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (manager, _tmp) = setup_fresh_library(&runtime);

    let err = create_audio_reader(
        ReadableFileSource::Managed,
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

/// A track whose upload is still queued and whose source file is gone: the cloud
/// object may not exist yet, so the reader-builder reports the pending upload
/// before consulting the manager — it never issues a read that would 404.
#[test]
fn upload_pending_reports_pending() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (manager, _tmp) = setup_fresh_library(&runtime);

    let err = create_audio_reader(
        ReadableFileSource::UploadPendingSourceMissing,
        "file-1",
        "Artist Name/Album Title/01 Track Title.flac",
        &manager,
        |_| unreachable!("a pending upload should not build a read config"),
    )
    .err()
    .expect("expected error for a pending upload with no readable source");
    assert!(
        matches!(err, PlaybackError::UploadPending),
        "expected UploadPending, got: {err:?}",
    );
}

/// An `Unreachable` source — an unmanaged track whose local file is gone — has
/// no readable location anywhere. The reader-builder returns `NotFound`.
#[test]
fn unreachable_returns_not_found() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (manager, _tmp) = setup_fresh_library(&runtime);

    let err = create_audio_reader(
        ReadableFileSource::Unreachable,
        "file-1",
        "Artist Name/Album Title/01 Track Title.flac",
        &manager,
        |_| unreachable!("no source should not build read config"),
    )
    .err()
    .expect("expected error for an unreachable track");
    assert!(
        matches!(err, PlaybackError::NotFound(_, _)),
        "expected NotFound, got: {err:?}",
    );
}

/// An `Unreachable` source stays `NotFound` even with a cloud home connected:
/// an unmanaged track's audio never went to the cloud, so a connected home must
/// not turn its missing local file into a doomed cloud read. The variant carries
/// the verdict, so the builder never consults the cloud at all.
#[test]
fn unreachable_returns_not_found_even_with_cloud_connected() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (mut manager, _tmp) = setup_fresh_library(&runtime);
    manager.set_cloud_override(
        Arc::new(MockCloudHome::new()),
        EncryptionService::new_with_key(&[7u8; 32]),
    );

    let err = create_audio_reader(
        ReadableFileSource::Unreachable,
        "file-1",
        "Artist Name/Album Title/01 Track Title.flac",
        &manager,
        |_| unreachable!("an unreachable source must not build a read config"),
    )
    .err()
    .expect("expected error for an unreachable track");
    assert!(
        matches!(err, PlaybackError::NotFound(_, _)),
        "expected NotFound even with a cloud home connected, got: {err:?}",
    );
}
