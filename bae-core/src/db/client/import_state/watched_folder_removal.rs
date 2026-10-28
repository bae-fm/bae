use super::*;

impl Database {
    /// Stop watching `roots` in one write, watching `parent` in their place
    /// when given. Returns the keys of the releases that left the queue, or
    /// `None` when none of `roots` is watched and `parent`, if given, is.
    ///
    /// Each root's scan rows go with it. What is known about the folders under
    /// the roots — what was decided, and when each was found — stays when
    /// `parent` takes them over, since the same folders are still watched, and
    /// goes otherwise.
    ///
    /// `parent` must hold exactly `roots` among the watched folders, checked
    /// inside the write.
    pub async fn remove_watched_import_folders(
        &self,
        roots: Vec<String>,
        parent: Option<String>,
    ) -> Result<Option<Vec<String>>, DbError> {
        // Spelled the way the add keyed them.
        let roots = roots
            .iter()
            .map(|root| crate::import::watched_folder::canonical_absolute_root(root))
            .collect::<Result<Vec<_>, _>>()?;
        let parent = parent
            .map(|parent| crate::import::watched_folder::canonical_absolute_root(&parent))
            .transpose()?;
        if roots.is_empty() {
            return Err(DbError::Message(
                "removing watched folders requires at least one".into(),
            ));
        }
        let watched = self.watched_import_roots().await?;
        if roots.iter().all(|root| !watched.contains(root))
            && parent
                .as_ref()
                .is_none_or(|parent| watched.contains(parent))
        {
            return Ok(None);
        }
        self.call(move |sql| {
            let watched: Vec<String> = sql.query(
                "SELECT path FROM watched_import_folders ORDER BY position",
                [],
                |row| row.get(0),
            )?;
            if let Some(root) = roots.iter().find(|root| !watched.contains(root)) {
                return Err(DbError::Message(format!("{root} is not a watched folder")));
            }
            if let Some(parent) = &parent {
                watch_in_place_of(sql, &watched, parent, &roots)?;
            }
            let mut removed: Vec<String> = Vec::new();
            for root in &roots {
                removed.extend(sql.query(
                    "SELECT path FROM scan_candidate WHERE watched_folder_path = ?",
                    [root],
                    |row| row.get::<_, String>(0),
                )?);
                let deleted =
                    sql.execute("DELETE FROM watched_import_folders WHERE path = ?", [root])?;
                if deleted != 1 {
                    return Err(DbError::Message(format!(
                        "removing watched folder {root} changed {deleted} rows; expected one"
                    )));
                }
            }
            if parent.is_none() {
                removed.extend(forget_folders_under(sql, &roots)?);
            }
            removed.sort();
            removed.dedup();
            // Every candidate under the folders leaves the scan, and its
            // identification with it.
            for key in &removed {
                super::end_identification_unless_of_on(sql, key, None)?;
            }
            Ok(Some(removed))
        })
        .await
    }
}

/// Watch `parent` in place of `roots`, which must be exactly the watched
/// folders inside it, with nothing watching it or a folder holding it.
fn watch_in_place_of(
    sql: &SqlContext<'_, '_>,
    watched: &[String],
    parent: &str,
    roots: &[String],
) -> Result<(), DbError> {
    let parent_path = std::path::Path::new(parent);
    if let Some(holder) = watched
        .iter()
        .find(|root| parent_path.starts_with(std::path::Path::new(root.as_str())))
    {
        return Err(DbError::Message(format!(
            "watching {parent} in place of the folders inside it: {holder} already watches it"
        )));
    }
    let mut inside: Vec<&String> = watched
        .iter()
        .filter(|root| std::path::Path::new(root.as_str()).starts_with(parent_path))
        .collect();
    inside.sort();
    let mut expected: Vec<&String> = roots.iter().collect();
    expected.sort();
    if inside != expected {
        return Err(DbError::Message(format!(
            "watching {parent} in place of the folders inside it: they are {inside:?}, \
             not {expected:?}"
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
    Ok(())
}

/// Delete what is known about the folders under `roots` and the state of
/// every candidate found only there. Returns the keys of the grouping
/// releases that went with their groupings. Each table is read and written
/// by the range of folders at or below each root, on its folder index.
fn forget_folders_under(
    sql: &SqlContext<'_, '_>,
    roots: &[String],
) -> Result<Vec<String>, DbError> {
    let mut removed = Vec::new();
    for root in roots {
        let (below_first, below_past) = crate::import::folder_path::paths_below(root);
        let under = |column: &str| {
            format!("({column} = :root OR ({column} >= :below_first AND {column} < :below_past))")
        };
        let params = rusqlite::named_params! {
            ":root": root,
            ":below_first": below_first,
            ":below_past": below_past,
        };
        for (table, column) in [
            ("skipped_import_candidates", "candidate_path"),
            ("folder_discovery", "folder"),
        ] {
            sql.execute(
                &format!("DELETE FROM {table} WHERE {}", under(column)),
                params,
            )?;
        }
        // A grouping goes with its anchor folder or any folder it takes a
        // release from.
        let groupings: Vec<(String, bool)> = sql.query(
            &format!(
                "SELECT grouping.key, \
                        EXISTS (SELECT 1 FROM scan_candidate WHERE path = grouping.key) \
                 FROM (SELECT key FROM release_grouping WHERE {anchored} \
                       UNION \
                       SELECT grouping_key AS key FROM release_grouping_member \
                       WHERE {member}) AS grouping \
                 ORDER BY grouping.key",
                anchored = under("anchor_folder"),
                member = under("member_folder"),
            ),
            params,
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        for (key, listed) in groupings {
            sql.execute("DELETE FROM release_grouping WHERE key = ?", [&key])?;
            if listed {
                removed.push(key);
            }
        }
        let found: Vec<String> = sql.query(
            &format!(
                "SELECT DISTINCT content_hash FROM import_candidate_folder WHERE {}",
                under("folder")
            ),
            params,
            |row| row.get(0),
        )?;
        sql.execute(
            &format!(
                "DELETE FROM import_candidate_folder WHERE {}",
                under("folder")
            ),
            params,
        )?;
        sql.execute(
            "DELETE FROM import_candidate_state \
             WHERE content_hash IN (SELECT value FROM json_each(?)) \
               AND NOT EXISTS (SELECT 1 FROM import_candidate_folder AS found \
                               WHERE found.content_hash = import_candidate_state.content_hash)",
            [serde_json::Value::from(found).to_string()],
        )?;
    }
    Ok(removed)
}
