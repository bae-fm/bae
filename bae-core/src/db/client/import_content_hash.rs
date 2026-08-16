use super::*;

impl Database {
    /// Release ids whose stored `content_hash` equals `hash`. Normally zero or
    /// one (the import overwrite path keeps the hash unique), but returns all
    /// matches so a re-import sweeps any pre-existing duplicates.
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

    pub async fn imported_content_hashes(
        &self,
    ) -> Result<std::collections::HashSet<String>, DbError> {
        self.read(move |sql| {
            Ok(sql
                .query(
                    "SELECT DISTINCT content_hash FROM releases WHERE content_hash IS NOT NULL",
                    [],
                    |row| row.get::<_, String>(0),
                )?
                .into_iter()
                .collect())
        })
        .await
    }
}
