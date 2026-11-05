//! How a folder reads: kept as separate releases or combined as one, by the
//! person or by the scan's own reading. A reading is the grouping anchored at
//! the folder, so it lives in `release_grouping` with the groupings it is one
//! kind of; a person's reading is never replaced by the scan's.

use super::*;
use crate::folder_scanner::{FolderReleaseDecisionAuthor, FolderReleaseDecisions};
use crate::tables::watched_folders::relative_below;

/// Store the grouping `grouping` anchored at the folder `key` names; a
/// heuristic reading never replaces the person's. A folder's grouping keeps
/// its key while stored, since its release is addressed by it, so a reading
/// naming a different key is stale and refused.
pub(crate) fn store_folder_reading(
    sql: &CovenWrite<'_>,
    key: &FolderReleaseDecisionKey,
    decision: crate::folder_scanner::FolderReleaseDecision,
    author: crate::folder_scanner::FolderReleaseDecisionAuthor,
    grouping: &CandidateKey,
) -> Result<(), DbError> {
    if key.relative_folder.is_root() {
        return Err(DbError::Message(format!(
            "{} is never a release, so it has no reading to store",
            coven::path_text(&key.watched_folder_path)
        )));
    }
    let author = match author {
        crate::folder_scanner::FolderReleaseDecisionAuthor::User => "user",
        crate::folder_scanner::FolderReleaseDecisionAuthor::Heuristic => "heuristic",
    };
    let combined =
        matches!(decision, crate::folder_scanner::FolderReleaseDecision::CombineAsOneRelease);
    let anchor =
        crate::tables::watched_path::folder_below(&key.watched_folder_path, &key.relative_folder);
    let covering = crate::tables::watched_folders::require_watched(sql, &anchor)?;
    if covering.as_os_str() != key.watched_folder_path.as_os_str() {
        return Err(DbError::Message(format!(
            "{} is watched under {}, not {}",
            coven::path_text(&anchor),
            coven::path_text(&covering),
            coven::path_text(&key.watched_folder_path)
        )));
    }
    let anchor = coven::path_bytes(&anchor);
    sql.execute(
        "INSERT INTO release_grouping (key, anchor_folder, combined, author) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(anchor_folder) DO UPDATE SET \
             combined = excluded.combined, author = excluded.author \
         WHERE excluded.author = 'user' \
             OR release_grouping.author != 'user'",
        params![grouping, anchor, combined, author],
    )?;
    let stored: CandidateKey = sql.query_row(
        "SELECT key FROM release_grouping WHERE anchor_folder = ?",
        [&anchor],
        |row| row.get(0),
    )?;
    if &stored != grouping {
        return Err(DbError::Message(format!(
            "{} is read as grouping {stored}, not {grouping}: the store changed after the \
             reading was taken",
            key.relative_folder.text()
        )));
    }
    Ok(())
}

pub(crate) fn validate_decision_key_ownership(
    watched_folder_path: &Path,
    key: &FolderReleaseDecisionKey,
) -> Result<(), DbError> {
    if key.watched_folder_path.as_os_str() != watched_folder_path.as_os_str() {
        return Err(DbError::Message(format!(
            "folder release decision belongs to {} instead of {}",
            coven::path_text(&key.watched_folder_path),
            coven::path_text(watched_folder_path)
        )));
    }
    Ok(())
}

impl ImportTables {
    /// Store the reading a scan settled on for one folder under the grouping
    /// key the scan gave it, without a new scan generation. A user's reading
    /// is kept; the write fails when the folder is stored under another key.
    pub async fn record_scanned_folder_release_decision(
        &self,
        key: &FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
        grouping: &CandidateKey,
    ) -> Result<(), DbError> {
        let key = key.clone();
        let grouping = grouping.clone();
        self.store
            .call(move |sql| {
                store_folder_reading(
                    sql,
                    &key,
                    decision,
                    FolderReleaseDecisionAuthor::Heuristic,
                    &grouping,
                )
            })
            .await
    }

    /// Store `reading` for the folder `key` names, by its own author: how a
    /// library holds a reading before a scan reads under it.
    #[cfg(test)]
    pub(crate) async fn store_folder_reading(
        &self,
        key: &FolderReleaseDecisionKey,
        reading: &crate::folder_scanner::FolderReading,
    ) -> Result<(), DbError> {
        let key = key.clone();
        let reading = reading.clone();
        self.store
            .call(move |sql| {
                store_folder_reading(sql, &key, reading.decision, reading.author, &reading.grouping)
            })
            .await
    }

    /// The grouping the folder `folder` is read as, by the groupings' anchor
    /// index; `None` when no reading of it is stored.
    pub async fn folder_reading_grouping(
        &self,
        folder: &FolderReleaseDecisionKey,
    ) -> Result<Option<CandidateKey>, DbError> {
        let anchor = coven::path_bytes(&crate::tables::watched_path::folder_below(
            &folder.watched_folder_path,
            &folder.relative_folder,
        ));
        self.store
            .read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT key FROM release_grouping WHERE anchor_folder = ?",
                        [anchor],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await
    }

    /// Whether reading the folder at `key` as `decision` is something the
    /// stored scan offers: combined when two releases or more are read from
    /// it or below it, separate when the release read from it is its
    /// grouping. Read by the candidates' folder index.
    pub async fn folder_reading_offered(
        &self,
        key: &FolderReleaseDecisionKey,
        decision: crate::folder_scanner::FolderReleaseDecision,
    ) -> Result<bool, DbError> {
        let folder = crate::tables::watched_path::folder_below(
            &key.watched_folder_path,
            &key.relative_folder,
        );
        let (below_first, below_past) = crate::tables::watched_path::paths_below(&folder);
        let folder = coven::path_bytes(&folder);
        let root = coven::path_bytes(&key.watched_folder_path);
        self.store
            .read(move |sql| match decision {
                crate::folder_scanner::FolderReleaseDecision::CombineAsOneRelease => Ok(sql
                    .query_row(
                        "SELECT COUNT(*) >= 2 FROM ( \
                         SELECT 1 FROM scan_candidate \
                         WHERE watched_folder_path = :root AND source_kind = 'folder' \
                           AND (folder = :folder \
                                OR (folder >= :below_first AND folder < :below_past)) \
                         LIMIT 2)",
                        rusqlite::named_params! {
                            ":root": root,
                            ":folder": folder,
                            ":below_first": below_first,
                            ":below_past": below_past,
                        },
                        |row| row.get(0),
                    )?),
                crate::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases => Ok(sql
                    .query_row(
                        "SELECT EXISTS (SELECT 1 FROM scan_candidate \
                         WHERE watched_folder_path = :root AND source_kind = 'folder' \
                           AND folder = :folder AND grouping_key IS NOT NULL)",
                        rusqlite::named_params! { ":root": root, ":folder": folder },
                        |row| row.get(0),
                    )?),
            })
            .await
    }

    /// The keys of the scanned releases read from `folder` or a folder below
    /// it under the watched folder `watched_folder_path`, by the candidates'
    /// folder index.
    pub async fn scanned_release_keys_at_or_below(
        &self,
        watched_folder_path: &Path,
        folder: &Path,
    ) -> Result<std::collections::BTreeSet<CandidateKey>, DbError> {
        let (below_first, below_past) = crate::tables::watched_path::paths_below(folder);
        let (root, folder) = (coven::path_bytes(watched_folder_path), coven::path_bytes(folder));
        self.store
            .read(move |sql| {
                Ok(sql
                    .query(
                        "SELECT path FROM scan_candidate \
                     WHERE watched_folder_path = :root AND source_kind = 'folder' \
                       AND (folder = :folder \
                            OR (folder >= :below_first AND folder < :below_past))",
                        rusqlite::named_params! {
                            ":root": root,
                            ":folder": folder,
                            ":below_first": below_first,
                            ":below_past": below_past,
                        },
                        |row| row.get(0),
                    )?
                    .into_iter()
                    .collect())
            })
            .await
    }

    /// Each stored folder reading below one watched root, by path below it.
    pub async fn load_folder_release_decisions(
        &self,
        watched_folder_path: &Path,
    ) -> Result<FolderReleaseDecisions, DbError> {
        let watched_folder_path = watched_folder_path.to_path_buf();
        let (below_first, below_past) =
            crate::tables::watched_path::paths_below(&watched_folder_path);
        self.store
            .read(move |sql| {
                let rows = sql.query(
                    "SELECT anchor_folder, combined, author, key FROM release_grouping \
                 WHERE anchor_folder = :root \
                    OR (anchor_folder >= :below_first AND anchor_folder < :below_past)",
                    rusqlite::named_params! {
                        ":root": coven::path_bytes(&watched_folder_path),
                        ":below_first": below_first,
                        ":below_past": below_past,
                    },
                    |row| {
                        Ok((
                            path_column::get(row, 0)?,
                            row.get::<_, bool>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, CandidateKey>(3)?,
                        ))
                    },
                )?;
                let mut readings = HashMap::new();
                for (folder, combined, author, grouping) in rows {
                    crate::tables::watched_path::validate_stored_folder(&folder)?;
                    let path = relative_below(&watched_folder_path, &folder)?;
                    let author = match author.as_str() {
                        "user" => FolderReleaseDecisionAuthor::User,
                        "heuristic" => FolderReleaseDecisionAuthor::Heuristic,
                        other => {
                            return Err(DbError::Message(format!(
                                "unknown folder release decision author {other:?}"
                            )))
                        }
                    };
                    readings.insert(
                        path,
                        crate::folder_scanner::FolderReading {
                            decision: if combined {
                                FolderReleaseDecision::CombineAsOneRelease
                            } else {
                                FolderReleaseDecision::KeepAsSeparateReleases
                            },
                            author,
                            grouping,
                        },
                    );
                }
                Ok(FolderReleaseDecisions::new(readings))
            })
            .await
    }
}

#[cfg(test)]
#[path = "folder_readings_tests.rs"]
pub(crate) mod tests;
