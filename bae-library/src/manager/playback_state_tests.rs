//! Reading a release's audio: what a device says it can play, and the failure a
//! read reports when sync is stopped or the original is gone.

use super::*;
use crate::manager::tests::*;
use bae_records::DbRelease;
use tempfile::TempDir;
use uuid::Uuid;

async fn insert_partially_uploaded_release(
    manager: &LibraryManager,
    temp_dir: &std::path::Path,
) -> DbRelease {
    let source_dir = temp_dir.join(Uuid::new_v4().to_string());
    let release = insert_local_release_with_files(
        manager,
        &source_dir,
        "Partially Uploaded",
        &[("a.flac", b"uploaded"), ("b.flac", b"missing")],
    )
    .await;

    manager.upload_release(&release.id, true).await.unwrap();
    assert_eq!(queued_upload_count(manager).await, 2);

    std::fs::remove_file(source_dir.join("b.flac")).unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    // The move switches no file to the cloud until every one is stored, and
    // waits for the missing original.
    assert_eq!(queued_upload_count(manager).await, 2);
    assert_eq!(
        release_files(manager, &release.id).await.iter().filter(|file| file.stored).count(),
        0
    );
    release
}

/// A Remote track whose bytes must come from the cloud, read while sync is
/// stopped, reports `SyncDisconnected` — the reconnect-sync state — not a
/// generic diagnostic. Stopping releases coven's storage for transfers, so
/// coven raises `FileReadError::NoStorage` for the cloud miss, and
/// `read_error_for_file_read` maps it.
#[tokio::test]
async fn remote_read_with_sync_stopped_reports_sync_disconnected() {
    use bae_playback::PlaybackErrorReason;
    let (manager, dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release_id = uploaded_release_with_files(
        &manager,
        dir.path(),
        "Album Title",
        &[("track.flac", b"track-bytes")],
        false,
    )
    .await;
    manager.stop_sync().await.unwrap();

    let files = release_files(&manager, &release_id).await;
    let file = files.first().expect("the release has a file");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(matches!(reason, PlaybackErrorReason::SyncDisconnected), "got {reason:?}");
}

/// An upload whose original vanished cannot finish by waiting. Playback reports
/// the source failure, and the pending list offers restoring the file.
#[tokio::test]
async fn pending_upload_with_missing_source_reports_the_missing_original() {
    use bae_playback::PlaybackErrorReason;
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_partially_uploaded_release(&manager, temp_dir.path()).await;

    let files = release_files(&manager, &release.id).await;
    let file = files
        .iter()
        .find(|f| f.original_filename == "b.flac")
        .expect("the un-uploaded file whose source was removed");
    let reason = playback_error_reason_for_file(&manager, file).await;
    assert!(matches!(reason, PlaybackErrorReason::OriginalMissing), "got {reason:?}");
    assert!(manager
        .pending()
        .await
        .unwrap()
        .iter()
        .any(|record| bae_sync_status::pending_action(record, None)
            == bae_sync_status::PendingAction::RestoreFile));
}

/// Each device says whether it can play a release from the library's
/// records: the importing device plays its own originals, and another device
/// names the device the audio is on. An original removed on the importing
/// device changes nothing shown; playing it fails as missing.
#[test]
fn a_release_says_whether_this_device_can_play_it() {
    crate::test_libraries::run_two_device_test(async |devices| {
        let first = TempDir::new().unwrap();
        let elsewhere = release_on_the_first_device(devices, first.path(), "Album Elsewhere").await;
        let originals = TempDir::new().unwrap();
        check_playability(devices.b().manager(), originals.path(), &elsewhere).await;
    });
}

async fn check_playability(
    manager: &LibraryManager,
    originals: &std::path::Path,
    elsewhere: &DbRelease,
) {
    use bae_playback::PlaybackErrorReason;
    use bae_views::album_detail::ReleasePlayability;
    let here = insert_local_release_with_files(
        manager,
        originals,
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    let playability = |release_id: String| async move {
        bae_views::LibraryViews::new(manager.store.clone())
            .find_release_detail(&release_id)
            .await
            .unwrap()
            .expect("the release exists")
            .playability
    };
    assert_eq!(playability(here.id.clone()).await, ReleasePlayability::Playable);

    assert_eq!(
        playability(elsewhere.id.clone()).await,
        ReleasePlayability::OnOtherDevice(bae_records::OtherDevice::Active {
            name: "Device A".to_string()
        })
    );

    std::fs::remove_file(originals.join("track.flac")).unwrap();
    assert_eq!(
        playability(here.id.clone()).await,
        ReleasePlayability::Playable,
        "the disk is not asked"
    );
    let file = release_files(manager, &here.id).await.remove(0);
    let reason = playback_error_reason_for_file(manager, &file).await;
    assert!(matches!(reason, PlaybackErrorReason::OriginalMissing), "got {reason:?}");
}

async fn queued_upload_count(manager: &LibraryManager) -> i64 {
    manager.store.queued_upload_count_for_test().await.unwrap()
}
