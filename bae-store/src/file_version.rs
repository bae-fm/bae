//! The version of a bae-own file (a cover, an artist image or an avatar):
//! the coven file its row names, then `@` and the hex of the device that has
//! the file while `coven_files` records it on one device only. A new image is
//! a new file, so the version moves on every device with the write that sets
//! it. The image's references are bound, so another device receives the row
//! with its file stored, except when a breaking schema raise falls between
//! the write and its upload's completion: the row then reaches other devices
//! in the raise, naming a file on its uploader only, and the completion
//! follows. The version moves again when the file is stored, so an app that
//! couldn't read the image then asks for it again.

// Each stored file's version, for a fragment to read the version of the file
// its row names. SQLite's `hex` of NULL is an empty string, not NULL, so the
// device's part is chosen on `coven_file_device` itself.
sqlive_compose::fragment!(
    StoredFileVersion { version: String },
    r#"
    SELECT coven.id AS file,
           coven.id || CASE WHEN coven_file_device(coven.coordinates) IS NULL THEN ''
                       ELSE '@' || lower(hex(coven_file_device(coven.coordinates))) END
               AS version
    FROM coven_files AS coven
"#
);

// Every stored file's version, whose SQL a statement built as text holds.
sqlive_compose::read!(
    StoredFileVersions,
    r#"
    SELECT stored.*
    FROM @crate::file_version::StoredFileVersion AS stored
"#
);

/// The version of the file the column `file` names (a SQL expression over
/// it), NULL when it names none, for a statement built as text.
pub fn file_version_sql(file: &str) -> String {
    format!(
        "(SELECT stored_version.version FROM ({versions}) AS stored_version \
          WHERE stored_version.file = {file})",
        versions = StoredFileVersions::SQL,
    )
}
