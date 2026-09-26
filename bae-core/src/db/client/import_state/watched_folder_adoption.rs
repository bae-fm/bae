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
    /// They are the same files under a new root, so everything stored about
    /// them moves under `parent` rather than being dropped and read back:
    /// what their scans found, keyed again with their paths below `parent`,
    /// so the list never loses a row and a candidate being identified keeps
    /// its key; skipped candidates and folder readings; the releases
    /// groupings take in; and every candidate's state — edits, verdicts,
    /// picks, keyed by content hash. Reading `parent` afterwards adds only
    /// what lies outside them.
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
            adopt_scan_root(sql, &parent, &inner)?;
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
                let scanned: Vec<(String, String)> = sql.query(
                    "SELECT path, display_path FROM scan_candidate WHERE watched_folder_path = ?",
                    [root],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                // Every table below a scanned candidate follows its key.
                for (path, display_path) in scanned {
                    sql.execute(
                        "UPDATE scan_candidate SET watched_folder_path = ?, display_path = ? \
                         WHERE watched_folder_path = ? AND path = ?",
                        params![
                            parent,
                            relative_under_parent(&prefix, &display_path),
                            root,
                            path
                        ],
                    )?;
                }
                sql.execute(
                    "UPDATE scan_sidecar SET watched_folder_path = ? WHERE watched_folder_path = ?",
                    params![parent, root],
                )?;
                sql.execute(
                    "INSERT INTO import_candidate_watched_root (content_hash, watched_folder_path) \
                     SELECT content_hash, ? FROM import_candidate_watched_root \
                     WHERE watched_folder_path = ? ON CONFLICT DO NOTHING",
                    params![parent, root],
                )?;
                // Nothing names the retired root any more but its scan status
                // and the directories its last walk saw, which the delete
                // takes with it: `parent`'s own walk records its directories.
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

/// The scan status `parent` starts from: what its folders' scans stored, one
/// row for all of them. A failure any of them stored stands; failing that, one
/// still reading keeps `parent` reading; else it is complete. Its generation
/// is the newest of theirs, so every entry they found is from a generation at
/// or before it — the next read of `parent` starts a newer one. Nothing is
/// written when none of them has been read.
fn adopt_scan_root(
    sql: &SqlContext<'_, '_>,
    parent: &str,
    inner: &[String],
) -> Result<(), DbError> {
    let mut merged: Option<(i64, String, Option<String>, String)> = None;
    for root in inner {
        let row: Option<(i64, String, Option<String>, String)> = sql
            .query_row(
                "SELECT generation, status, error, volume FROM folder_scan_roots \
                 WHERE watched_folder_path = ?",
                [root],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some(row) = row else { continue };
        merged = Some(match merged {
            None => row,
            Some(held) => {
                let rank = |status: &str| match status {
                    "failed" => 2,
                    "scanning" => 1,
                    _ => 0,
                };
                let (status, error) = if rank(&row.1) > rank(&held.1) {
                    (row.1, row.2)
                } else {
                    (held.1, held.2)
                };
                let volume = if held.3 == "network" || row.3 == "network" {
                    "network".to_string()
                } else {
                    held.3
                };
                (held.0.max(row.0), status, error, volume)
            }
        });
    }
    let Some((generation, status, error, volume)) = merged else {
        return Ok(());
    };
    sql.execute(
        "INSERT INTO folder_scan_roots (watched_folder_path, generation, status, error, volume) \
         VALUES (?, ?, ?, ?, ?)",
        params![parent, generation, status, error, volume],
    )?;
    Ok(())
}
