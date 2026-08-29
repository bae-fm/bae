use super::*;

impl Database {
    /// Stop watching the folder `path` names. Keyed the same way as the add,
    /// so whichever spelling reaches here names the row the add created.
    /// The same transaction removes candidate state that belonged only to this
    /// root, so adding the folder again starts from its files rather than an
    /// earlier edit or identification result. State shared by an identical
    /// candidate under another watched root remains. Returns the keys of the
    /// scan entries the removal cascaded away, or `None` when the folder was
    /// not watched.
    pub async fn remove_watched_import_folder(
        &self,
        path: &str,
    ) -> Result<Option<Vec<String>>, DbError> {
        let path = Self::canonical_watched_root(path)?;
        if !self.watched_import_roots().await?.contains(&path) {
            return Ok(None);
        }
        self.call(move |sql| {
            let root = std::path::Path::new(&path);
            let mut candidate_hashes: HashSet<String> = sql
                .query(
                    "SELECT content_hash FROM scan_candidate \
                     WHERE watched_folder_path = ? AND content_hash IS NOT NULL",
                    [&path],
                    |row| row.get(0),
                )?
                .into_iter()
                .collect();
            candidate_hashes.extend(
                sql.query(
                    "SELECT content_hash, folder_path FROM import_candidate_state",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )?
                .into_iter()
                .filter_map(|(content_hash, folder_path)| {
                    std::path::Path::new(&folder_path)
                        .starts_with(root)
                        .then_some(content_hash)
                }),
            );
            let entry_keys = stored_entries(sql, &path)?
                .into_iter()
                .map(|(key, _)| key)
                .collect();
            let removed =
                sql.execute("DELETE FROM watched_import_folders WHERE path = ?", [&path])?;
            if removed != 1 {
                return Err(DbError::Message(format!(
                    "removing watched folder {path} changed {removed} rows; expected one"
                )));
            }
            for content_hash in candidate_hashes {
                let remaining_references: i64 = sql.query_row(
                    "SELECT EXISTS(SELECT 1 FROM scan_candidate WHERE content_hash = ?)",
                    [&content_hash],
                    |row| row.get(0),
                )?;
                if remaining_references == 0 {
                    sql.execute(
                        "DELETE FROM import_candidate_state WHERE content_hash = ?",
                        [&content_hash],
                    )?;
                }
            }
            Ok(Some(entry_keys))
        })
        .await
    }
}
