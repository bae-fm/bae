use super::*;

/// `relative`, a path under the retired root `inner`, as the adopting root
/// `parent` names it.
fn relative_under_parent(inner_prefix: &str, relative: &str) -> String {
    if relative.is_empty() {
        inner_prefix.to_string()
    } else {
        format!("{inner_prefix}/{relative}")
    }
}

impl Database {
    /// Watch `parent` in place of the watched folders inside it, in one write.
    ///
    /// They are the same files under a new root, so what a person decided
    /// about them carries over: skipped candidates and folder readings are
    /// keyed again under `parent`, the releases groupings take in are listed
    /// under it, and every candidate's state — edits, verdicts, picks, keyed
    /// by content hash — stays, now found under `parent`. What the scans of
    /// the retired folders cached goes with them; reading `parent` stores it
    /// again.
    ///
    /// `inner` must be exactly the watched folders inside `parent`, and
    /// nothing may watch `parent` or a folder holding it: the check reads the
    /// roots inside the write that changes them, so a watched folder added or
    /// removed since the caller looked refuses the write rather than being
    /// adopted, or left overlapping, by accident.
    pub async fn adopt_watched_import_folders(
        &self,
        parent: &str,
        inner: Vec<String>,
    ) -> Result<(), DbError> {
        let parent = crate::import::watched_folder::canonical_absolute_root(parent)?;
        self.call(move |sql| {
            let roots: Vec<String> = sql.query(
                "SELECT path FROM watched_import_folders ORDER BY position",
                [],
                |row| row.get(0),
            )?;
            let parent_path = std::path::Path::new(&parent);
            let mut inside: Vec<&String> = roots
                .iter()
                .filter(|root| {
                    let root = std::path::Path::new(root.as_str());
                    root != parent_path && root.starts_with(parent_path)
                })
                .collect();
            inside.sort();
            let mut expected: Vec<&String> = inner.iter().collect();
            expected.sort();
            if inside.is_empty() || inside != expected {
                return Err(DbError::Message(format!(
                    "adopting {parent}: the watched folders inside it are {inside:?}, not {expected:?}"
                )));
            }
            if let Some(holder) = roots
                .iter()
                .find(|root| parent_path.starts_with(std::path::Path::new(root.as_str())))
            {
                return Err(DbError::Message(format!(
                    "adopting {parent}: {holder} already watches it"
                )));
            }
            let position: i64 = sql.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM watched_import_folders",
                [],
                |row| row.get(0),
            )?;
            sql.execute(
                "INSERT INTO watched_import_folders (path, position) VALUES (?, ?)",
                params![parent, position],
            )?;
            for root in &inner {
                let prefix = crate::import::watched_folder::candidate_relative_path(
                    &parent,
                    std::path::Path::new(root),
                )
                .map_err(|error| DbError::Message(error.to_string()))?;
                let skipped: Vec<String> = sql.query(
                    "SELECT relative_candidate_path FROM skipped_import_candidates \
                     WHERE watched_folder_path = ?",
                    [root],
                    |row| row.get(0),
                )?;
                for relative in skipped {
                    sql.execute(
                        "INSERT INTO skipped_import_candidates \
                         (watched_folder_path, relative_candidate_path) VALUES (?, ?)",
                        params![parent, relative_under_parent(&prefix, &relative)],
                    )?;
                }
                let anchors: Vec<(String, Option<String>)> = sql.query(
                    "SELECT key, anchor_relative_path FROM release_grouping \
                     WHERE watched_folder_path = ?",
                    [root],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                for (key, anchor) in anchors {
                    sql.execute(
                        "UPDATE release_grouping SET watched_folder_path = ?, \
                         anchor_relative_path = ? WHERE key = ?",
                        params![
                            parent,
                            anchor.map(|anchor| relative_under_parent(&prefix, &anchor)),
                            key
                        ],
                    )?;
                }
                sql.execute(
                    "UPDATE release_grouping_member SET watched_folder_path = ? \
                     WHERE watched_folder_path = ?",
                    params![parent, root],
                )?;
                sql.execute(
                    "INSERT INTO import_candidate_watched_root (content_hash, watched_folder_path) \
                     SELECT content_hash, ? FROM import_candidate_watched_root \
                     WHERE watched_folder_path = ? ON CONFLICT DO NOTHING",
                    params![parent, root],
                )?;
                // Nothing names the retired root any more but its scan cache,
                // which the delete takes with it.
                let removed =
                    sql.execute("DELETE FROM watched_import_folders WHERE path = ?", [root])?;
                if removed != 1 {
                    return Err(DbError::Message(format!(
                        "retiring watched folder {root} changed {removed} rows; expected one"
                    )));
                }
            }
            Ok(())
        })
        .await
    }
}
