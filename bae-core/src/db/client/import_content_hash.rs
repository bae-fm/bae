use super::*;

impl Database {
    /// The release this device imported from files with this folder hash
    /// (`local_release_origin`), if it imported one: what places a scanned
    /// folder in Done. A release another device imported from the same files
    /// is not one.
    pub async fn release_imported_from(&self, hash: &str) -> Result<Option<String>, DbError> {
        let hash = hash.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT release_id FROM local_release_origin WHERE content_hash = ?",
                params![hash],
                |row| row.get(0),
            )
            .optional()
            .map_err(DbError::from)
        })
        .await
    }
}
