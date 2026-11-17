//! The wait for a copy behind another installation of its device, which
//! core resets at once.

use super::*;

/// A copy restored from a backup after its device sent more finds another
/// installation using its device's id once it syncs, and the wait answers
/// with the edits and files the copy had not sent.
#[test]
fn a_copy_behind_another_installation_answers_with_its_unsent_work() {
    crate::test_libraries::run_two_device_test(async |devices| {
        devices
            .a_mut()
            .restore_backup_behind(async |device| {
                device.manager().records.insert_album(&create_test_album()).await.unwrap();
            })
            .await;
        let copy = devices.a();
        copy.manager().records.insert_album(&create_test_album()).await.unwrap();
        let unsent = copy.store().unsent_before_removal().await.unwrap();
        assert!(unsent.writes > 0, "the copy has not sent its own write yet");

        let (behind, started) =
            tokio::join!(copy.manager().copy_behind(), copy.store().start_sync());
        started.unwrap();
        assert_eq!(behind.unwrap(), unsent);
    });
}

/// A library that closes while the wait runs ends it as cancelled, with no
/// reset to run.
#[tokio::test]
async fn closing_the_library_ends_the_wait() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();

    let (behind, closed) = tokio::join!(manager.copy_behind(), manager.close());
    closed.unwrap();
    assert!(matches!(behind, Err(bae_store::live_query::LiveQueryError::Cancelled)), "{behind:?}");
}
