//! Where each release's files are on this device: the `release_storage`
//! table, which triggers keep from the release's files, coven's records of
//! them, pins and the upload queue.

/// Release storage's part of the schema: `release_storage.sql`.
pub const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("release_storage.sql")]);

/// Plant, in the library file under `directory`, the device id `device`, as
/// coven stores device ids, as the one `release`'s storage row names as alone
/// holding one of its audio files, as damage to the row would leave it.
#[cfg(test)]
pub(crate) fn plant_audio_elsewhere_device_for_test(
    directory: &std::path::Path,
    release: &str,
    device: &[u8],
) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "release_storage",
        &format!(
            "UPDATE release_storage SET audio_elsewhere_device = x'{}' WHERE release_id = '{release}';",
            hex::encode(device)
        ),
    );
}

#[cfg(test)]
#[path = "release_storage_tests.rs"]
mod tests;
