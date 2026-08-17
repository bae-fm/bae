use super::{
    classify_join_error, inspect_device_pairing_offer, prepare_device_pairing_join_at,
    JoinDevicePairingError,
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
