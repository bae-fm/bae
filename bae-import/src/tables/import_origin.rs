//! Which release this device imported from each folder hash
//! (`local_release_origin`): what places a scanned folder in Done wherever it
//! sits, and what makes one hash one release on this device. A release
//! another device imported from the same files is not one.
//!
//! Its table is `import_origin.sql`.

use super::*;

pub(crate) mod fragments;

/// What recording an import's origin found.
pub(super) enum OriginRecord {
    Recorded,
    /// This device already holds `release_id` for the hash.
    Taken {
        release_id: String,
    },
}

/// Record that this device imported `release_id` from the files hashed
/// `content_hash`, inside the import's commit. The unique hash refuses a
/// second release for the same files, naming the one that holds them.
pub(super) fn record_on(
    sql: &CovenWrite<'_>,
    release_id: &str,
    content_hash: &str,
) -> Result<OriginRecord, DbError> {
    match sql.execute(
        "INSERT INTO local_release_origin (release_id, content_hash) VALUES (?1, ?2)",
        params![release_id, content_hash],
    ) {
        Ok(_) => {
            // Imported, its candidates leave the import list, and their
            // pictures' copies with them.
            super::picture_copies::forget_for_hash_on(sql, content_hash)?;
            Ok(OriginRecord::Recorded)
        }
        // The unique hash is the one unique constraint the insert can break:
        // the release id is the one the commit just made.
        Err(rusqlite::Error::SqliteFailure(failure, message))
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            match imported_release_on(sql, content_hash)? {
                Some(release_id) => Ok(OriginRecord::Taken { release_id }),
                None => Err(rusqlite::Error::SqliteFailure(failure, message).into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

/// The release this device imported from the files hashed `content_hash`,
/// if it imported one.
pub(super) fn imported_release_on(
    sql: &impl bae_store::WriterRead,
    content_hash: &str,
) -> Result<Option<String>, DbError> {
    let request = OriginOfParams { content_hash: content_hash.to_string() };
    Ok(statement_optional(sql, &request)?.map(|row| row.origin.origin_release_id))
}

sqlive_compose::read!(
    writer OriginOf(content_hash: String),
    r#"
    SELECT origin.*
    FROM @crate::tables::import_origin::fragments::ReleaseOrigin AS origin
    WHERE origin.origin_hash = :content_hash
"#
);

#[cfg(test)]
#[path = "import_origin_tests.rs"]
mod tests;
