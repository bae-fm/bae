//! Where each release's files are on this device: the `release_storage`
//! table, which triggers keep from the release's files, coven's records of
//! them, pins and the upload queue.

// Each release's storage row, and what a read says from it:
//
// - `pinned`: this device keeps every file of the release pinned and whole.
//   Its pinned count, which local triggers keep from the pins coven's
//   `coven_pins` lists as kept, has reached its file count.
// - `waiting_files`, `sending_files` and `stored_files`: of the release's
//   files, how many are queued and only wait, how many this process is
//   sending now, and how many are not queued. How far the files being sent
//   have got is the release's transfer.
// - `bytes_stored`: the release's whole size less the sizes of its files
//   still queued to upload.
// - `elsewhere`: the JSON text `null` when no other device alone holds an
//   audio file of the release, and otherwise a JSON array of the device the
//   row names as holding one, in hex, and coven's registration of it: its
//   name, its state, whether a member removed its person from the library and
//   whether it is this device, or `null` when coven registers no such device.
sqlive_compose::fragment!(
    ReleaseStorage {
        pinned: bool,
        waiting_files: i64,
        sending_files: i64,
        stored_files: i64,
        bytes_stored: i64,
        elsewhere: String,
    },
    r#"
    SELECT kept.release_id, kept.file_count, kept.total_size, kept.stored,
           kept.queued_count, kept.first_queued_position, kept.failed_move_file,
           kept.failed_move_cause, kept.missing_original_file,
           kept.audio_elsewhere_device,
           (kept.file_count > 0 AND kept.pinned_count = kept.file_count) AS pinned,
           kept.queued_count - COALESCE((
               SELECT sending.files
               FROM @crate::records::queued_uploads::ReleaseSending AS sending
               WHERE sending.release_id = kept.release_id), 0) AS waiting_files,
           COALESCE((
               SELECT sending.files
               FROM @crate::records::queued_uploads::ReleaseSending AS sending
               WHERE sending.release_id = kept.release_id), 0) AS sending_files,
           kept.file_count - kept.queued_count AS stored_files,
           kept.total_size - COALESCE((
               SELECT SUM(coven.size)
               FROM @crate::records::spread_fragments::ReleaseFileRow AS file
               JOIN coven_upload_queue AS queued ON queued.file = file.stored_file
               JOIN coven_files AS coven ON coven.id = file.stored_file
               WHERE file.release_id = kept.release_id), 0) AS bytes_stored,
           CASE WHEN kept.audio_elsewhere_device IS NULL THEN 'null'
                ELSE json_array(hex(kept.audio_elsewhere_device), json((
                    SELECT json_array(device.name, device.state, member.removed, device.is_self)
                    FROM coven_devices AS device
                    JOIN coven_members AS member ON member.member = device.member
                    WHERE device.device = kept.audio_elsewhere_device)))
           END AS elsewhere
    FROM release_storage AS kept
"#
);

impl ReleaseStorage {
    /// Whether this device keeps every file of the release pinned and whole.
    pub(crate) fn pinned(&self) -> bool {
        self.pinned
    }

    /// Whether the release is in storage.
    pub(crate) fn stored(&self) -> Result<bool, bae_store::DbError> {
        match self.stored {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(bae_store::DbError::unreadable("a release's stored flag", other)),
        }
    }

    /// The text that says which other device alone holds the release's audio.
    pub(crate) fn elsewhere(&self) -> &str {
        &self.elsewhere
    }
}

// Every release's storage row, whose SQL a statement built as text holds.
sqlive_compose::read!(
    writer ReleaseStorageRows,
    r#"
    SELECT storage.*
    FROM @crate::release_storage::ReleaseStorage AS storage
"#
);

/// Plant, in the library file under `directory`, the device `device` as the
/// one `release`'s storage row names as alone holding one of its audio files,
/// as damage to the row would leave it.
#[cfg(test)]
pub(crate) fn plant_audio_elsewhere_device_for_test(
    directory: &std::path::Path,
    release: &str,
    device: coven::DeviceId,
) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "release_storage",
        &format!(
            "UPDATE release_storage SET audio_elsewhere_device = x'{}' WHERE release_id = '{release}';",
            hex::encode(device.0.to_be_bytes())
        ),
    );
}

#[cfg(test)]
#[path = "release_storage_tests.rs"]
mod tests;
