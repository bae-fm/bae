use super::{
    abandon_pending_device_pairing_join_at, classify_join_error, inspect_device_pairing_offer,
    pending_device_pairing_join_at, prepare_device_pairing_join_at, JoinDevicePairingError,
};
use crate::sync::membership::pubkey_fingerprint;

#[test]
fn approval_cancellation_remains_a_cancellation_inside_the_library() {
    let error = super::LibraryError::from(coven::ApproveDevicePairingError::Cancelled);

    assert!(matches!(
        error,
        super::LibraryError::DevicePairingApproval(inner)
            if matches!(*inner, coven::ApproveDevicePairingError::Cancelled)
    ));
}

#[test]
fn pairing_offer_preview_comes_from_the_scanned_offer() {
    let pairing_key = coven::UserKeypair::generate();
    let offer = coven::DevicePairingOffer::new(
        &pairing_key,
        vec!["192.0.2.10:24821".parse().expect("pairing endpoint")],
        "Shared Library".to_string(),
        coven::CloudProvider::GoogleDrive,
        1_900_000_000,
    )
    .expect("pairing offer");

    let preview = inspect_device_pairing_offer(&offer.encode()).expect("inspect pairing offer");

    assert_eq!(preview.library_name, "Shared Library");
    assert_eq!(preview.cloud_provider, coven::CloudProvider::GoogleDrive);
    assert!(preview.needs_oauth);
    assert_eq!(preview.expires_at_unix_seconds, 1_900_000_000);
}

#[test]
fn owner_cancellation_is_not_reported_as_an_internal_join_failure() {
    let error = coven::BootstrapError::Pairing(coven::DevicePairingTransportError::Cancelled);

    assert!(matches!(
        classify_join_error(error),
        JoinDevicePairingError::Abandoned
    ));
}

#[tokio::test]
async fn joining_device_can_display_the_exact_identity_it_submits() {
    crate::config::install_test_keyring();
    let pairing_key = coven::UserKeypair::generate();
    let offer = coven::DevicePairingOffer::new(
        &pairing_key,
        vec!["192.0.2.10:24821".parse().expect("pairing endpoint")],
        "Shared Library".to_string(),
        coven::CloudProvider::S3,
        1_900_000_000,
    )
    .expect("pairing offer");
    let app = tempfile::tempdir().expect("pairing app directory");
    let layout = coven::StoreLayout::new(app.path());

    let prepared = prepare_device_pairing_join_at(&offer.encode(), None, None, layout)
        .await
        .expect("prepare pairing join");

    assert_eq!(
        prepared.fingerprint(),
        pubkey_fingerprint(prepared.pairing.request().public_key())
    );
}

#[tokio::test]
async fn a_pending_pairing_is_discoverable_after_the_operation_object_is_gone() {
    crate::config::install_test_keyring();
    let pairing_key = coven::UserKeypair::generate();
    let offer = coven::DevicePairingOffer::new(
        &pairing_key,
        vec!["192.0.2.10:24821".parse().expect("pairing endpoint")],
        "Shared Library".to_string(),
        coven::CloudProvider::S3,
        1_900_000_000,
    )
    .expect("pairing offer");
    let app = tempfile::tempdir().expect("pairing app directory");
    let layout = coven::StoreLayout::new(app.path());
    let prepared = prepare_device_pairing_join_at(&offer.encode(), None, None, layout.clone())
        .await
        .expect("prepare pairing join");
    let expected_fingerprint = prepared.fingerprint();
    drop(prepared);

    let pending = pending_device_pairing_join_at(layout)
        .expect("enumerate pending pairing")
        .expect("one pending pairing");

    assert_eq!(pending.pairing_code, offer.encode());
    assert_eq!(pending.offer.library_name, "Shared Library");
    assert_eq!(pending.fingerprint, expected_fingerprint);
    assert_eq!(pending.phase, coven::DevicePairingPhase::AwaitingInvitation);
}

#[tokio::test]
async fn abandoning_a_pending_pairing_removes_its_durable_attempt() {
    crate::config::install_test_keyring();
    let pairing_key = coven::UserKeypair::generate();
    let offer = coven::DevicePairingOffer::new(
        &pairing_key,
        vec!["192.0.2.10:24821".parse().expect("pairing endpoint")],
        "Shared Library".to_string(),
        coven::CloudProvider::S3,
        1_900_000_000,
    )
    .expect("pairing offer");
    let app = tempfile::tempdir().expect("pairing app directory");
    let layout = coven::StoreLayout::new(app.path());
    let prepared = prepare_device_pairing_join_at(&offer.encode(), None, None, layout.clone())
        .await
        .expect("prepare pairing join");
    drop(prepared);

    abandon_pending_device_pairing_join_at(layout.clone()).expect("abandon pending pairing");

    assert!(pending_device_pairing_join_at(layout)
        .expect("enumerate pending pairing")
        .is_none());
}
