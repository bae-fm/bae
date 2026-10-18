use super::*;

impl Database {
    /// Every independent release carrying this folder hash.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn release_ids_for_content_hash(&self, hash: &str) -> Result<Vec<String>, DbError> {
        let hash = hash.to_string();
        self.read(move |sql| {
            sql.query(
                "SELECT id FROM releases WHERE content_hash = ?",
                params![hash],
                |row| row.get::<_, String>(0),
            )
            .map_err(DbError::from)
        })
        .await
    }

    /// Whether some release in the library was imported from this exact file
    /// structure (its `content_hash` matches `hash`). The import view uses this
    /// to mark a scanned folder as already added.
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, DbError> {
        let hash = hash.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT 1 FROM releases WHERE content_hash = ? LIMIT 1",
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
