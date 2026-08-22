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
        self.read(move |sql| imported_content_hashes_on(&sql)).await
    }

    pub async fn imported_releases_for_content_hashes(
        &self,
        content_hashes: &[String],
    ) -> Result<HashMap<String, crate::import::ImportedRelease>, DbError> {
        let content_hashes = content_hashes.to_vec();
        self.read(move |sql| imported_releases_for_content_hashes_on(&sql, &content_hashes))
            .await
    }
}

pub(super) fn imported_releases_for_content_hashes_on(
    sql: &SqlReadContext<'_>,
    content_hashes: &[String],
) -> Result<HashMap<String, crate::import::ImportedRelease>, DbError> {
    if content_hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let mut imported = HashMap::new();
    for chunk in content_hashes.chunks(SQL_MAX_IN_VARS) {
        let placeholders = in_clause_placeholders(chunk.len());
        let query = format!(
            "SELECT content_hash, id, album_id \
             FROM releases \
             WHERE content_hash IN ({placeholders})"
        );
        for (content_hash, release) in sql.query(
            &query,
            coven::rusqlite::params_from_iter(chunk.iter()),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    crate::import::ImportedRelease {
                        release_id: row.get(1)?,
                        album_id: row.get(2)?,
                    },
                ))
            },
        )? {
            if imported.insert(content_hash.clone(), release).is_some() {
                return Err(DbError::Message(format!(
                    "content hash {content_hash} names more than one imported release"
                )));
            }
        }
    }
    Ok(imported)
}

pub(super) fn imported_content_hashes_on(
    sql: &SqlReadContext<'_>,
) -> Result<std::collections::HashSet<String>, DbError> {
    Ok(sql
        .query(
            "SELECT DISTINCT content_hash FROM releases WHERE content_hash IS NOT NULL",
            [],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .collect())
}
