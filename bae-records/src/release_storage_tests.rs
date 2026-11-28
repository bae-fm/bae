use crate::test_fixtures::storage_listing::*;

/// The device id, as coven stores it, that `release`'s storage row on
/// `store` names as alone holding one of its audio files, or `None`.
async fn audio_elsewhere(
    store: &bae_store::Store,
    release: &'static str,
) -> Option<coven::DeviceId> {
    let statement = super::AudioElsewhereDeviceParams { release: release.to_string() };
    store.read_row(statement).await.unwrap().audio_elsewhere_device
}

/// A release's storage row names the device that alone holds one of its
/// audio files in the write that applies that file's record here, and names
/// none on that device itself or for a release whose only file elsewhere is
/// not audio.
#[test]
fn the_storage_row_names_the_device_that_alone_holds_its_audio() {
    use bae_util::content_type::ContentType::{Flac, Jpeg};
    bae_store::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        async |devices| {
            let dir = bae_util::test_files::TestFolder::new();
            let (a, b) = (devices.a(), devices.b());
            write_library(&crate::test_fixtures::test_records(a)).await;
            devices.converge_rows(|| crate::records::test_statements::ReleaseIdsParams {}).await;

            import_file(a, &dir, RELEASE_EARLY, "track.flac", Flac).await;
            import_file(a, &dir, RELEASE_EARLY, "cover.jpg", Jpeg).await;
            import_file(a, &dir, RELEASE_LATE, "front.jpg", Jpeg).await;
            devices
                .converge_rows(|| crate::records::test_statements::ReleaseFileRowsParams {})
                .await;
            devices.converge_rows(|| crate::records::test_statements::CovenFileIdsParams {}).await;

            let other = crate::records::test_statements::DeviceWhereSelfParams { is_self: false };
            let device_a = b.read_row(other).await.unwrap().device;
            assert_eq!(audio_elsewhere(b, RELEASE_EARLY).await, Some(device_a));
            assert_eq!(audio_elsewhere(a, RELEASE_EARLY).await, None, "the device that holds it");
            assert_eq!(audio_elsewhere(b, RELEASE_LATE).await, None, "artwork is not audio");
        },
    );
}
