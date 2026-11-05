//! Which release this device imported from each folder hash
//! (`local_release_origin`): what places a scanned folder in Done wherever it
//! sits, and what makes one hash one release on this device. A release
//! another device imported from the same files is not one.
//!
//! Its table is `import_origin.sql`.

/// The import origin's part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("import_origin.sql")]);

use super::*;

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
        Ok(_) => Ok(OriginRecord::Recorded),
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
    sql: &impl bae_store::query::QueryOne,
    content_hash: &str,
) -> Result<Option<String>, DbError> {
    Ok(sql
        .query_row(
            "SELECT release_id FROM local_release_origin WHERE content_hash = ?",
            [content_hash],
            |row| row.get(0),
        )
        .optional()?)
}

impl ImportTables {
    /// The release this device imported from the files hashed `hash`, if it
    /// imported one.
    pub async fn release_imported_from(&self, hash: &str) -> Result<Option<String>, DbError> {
        let hash = hash.to_string();
        self.store.read(move |sql| imported_release_on(&sql, &hash)).await
    }
}

#[cfg(test)]
#[path = "import_origin_tests.rs"]
mod tests;
