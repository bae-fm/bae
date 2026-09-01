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
            let candidate_hashes: HashSet<String> = sql
                .query(
                    "SELECT content_hash FROM import_candidate_watched_root \
                     WHERE watched_folder_path = ?",
                    [&path],
                    |row| row.get(0),
                )?
                .into_iter()
                .collect();
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
                let remaining_roots: i64 = sql.query_row(
                    "SELECT EXISTS(SELECT 1 FROM import_candidate_watched_root \
                     WHERE content_hash = ?)",
                    [&content_hash],
                    |row| row.get(0),
                )?;
                if remaining_roots == 0 {
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
