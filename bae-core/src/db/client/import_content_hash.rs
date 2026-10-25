use super::*;

impl Database {
    /// Whether this device imported a release from files with this folder
    /// hash (`local_release_origin`): what places a scanned folder in Done.
    /// A release another device imported from the same files is not one.
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, DbError> {
        let hash = hash.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT 1 FROM local_release_origin WHERE content_hash = ?",
                params![hash],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
            .map_err(DbError::from)
        })
        .await
    }
}
