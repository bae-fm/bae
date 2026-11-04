//! The folder-scan rows a file decision settles: every release a scan stored
//! reading the decided files, and their file rows rewritten to the decided
//! shape, stamped with the decision's revision.

use super::*;
use std::collections::HashMap;

/// Give every scanned release reading `content_hash` its files from
/// `settled_by_key`, stamped `next_revision`, and return their keys in key
/// order. Every row must still be at `expected_revision`, and every settled
/// key must be one of them.
pub(crate) fn settle_candidate_files(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    expected_revision: i64,
    next_revision: i64,
    settled_by_key: &HashMap<CandidateKey, crate::folder_scanner::CategorizedFiles>,
) -> Result<Vec<CandidateKey>, DbError> {
    let scanned = sql.query(
        "SELECT watched_folder_path, path, source_kind, file_edit_revision FROM scan_candidate \
         WHERE content_hash = ? ORDER BY watched_folder_path, path",
        [content_hash],
        |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, CandidateKey>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )?;
    let mut settled = Vec::with_capacity(scanned.len());
    for (watched_folder_path, path, source_kind, revision) in scanned {
        if revision != expected_revision {
            return Err(DbError::Message(format!(
                "candidate {path} changed before its source decisions were stored"
            )));
        }
        match source_kind.as_str() {
            "folder" | "grouping" => {
                let files = settled_by_key.get(&path).ok_or_else(|| {
                    DbError::Message(format!(
                        "persisted candidate {path} was missing from the settled file edit"
                    ))
                })?;
                // Deleting the file rows deletes the file-tag reading with
                // them; the files themselves did not change, so it is put
                // back as it was, at the revision it was taken at.
                let reading =
                    tag_snapshot::load_file_tag_snapshot(sql, &watched_folder_path, &path)?;
                sql.execute(
                    "DELETE FROM scan_candidate_file WHERE watched_folder_path = ? AND candidate_path = ?",
                    params![coven::path_bytes(&watched_folder_path), path],
                )?;
                insert_candidate_files(sql, &watched_folder_path, &path, files)?;
                if let Some(reading) = reading {
                    tag_snapshot::replace_candidate_file_tag_snapshot(
                        sql,
                        &watched_folder_path,
                        &path,
                        &reading,
                    )?;
                }
            }
            other => return Err(DbError::unreadable("scan_candidate.source_kind", other)),
        }
        let changed = sql.execute(
            "UPDATE scan_candidate SET file_edit_revision = ? \
             WHERE watched_folder_path = ? AND path = ? AND file_edit_revision = ?",
            params![
                next_revision,
                coven::path_bytes(&watched_folder_path),
                path,
                expected_revision
            ],
        )?;
        if changed != 1 {
            return Err(DbError::Message(format!(
                "candidate file decision changed {changed} persisted scan entries for {path}; \
                 expected exactly one"
            )));
        }
        settled.push(path);
    }
    let missing: Vec<_> = settled_by_key
        .keys()
        .filter(|key| !settled.contains(key))
        .map(CandidateKey::to_string)
        .collect();
    if !missing.is_empty() {
        return Err(DbError::Message(format!(
            "candidate file decision could not update persisted scan entries: {}",
            missing.join(", ")
        )));
    }
    Ok(settled)
}

impl ImportTables {
    /// Every release a scan stored reading the files `content_hash` names at
    /// file decision revision `file_edit_revision`, under whichever root, with
    /// those files in key order: what a file decision settles together.
    pub async fn load_releases_reading(
        &self,
        content_hash: &str,
        file_edit_revision: u64,
    ) -> Result<Vec<(CandidateKey, crate::folder_scanner::CategorizedFiles)>, DbError> {
        let content_hash = content_hash.to_string();
        let file_edit_revision = to_i64(file_edit_revision, "a file decision revision")?;
        self.database
            .read(move |sql| {
                let keys = sql.query(
                    "SELECT path FROM scan_candidate \
                 WHERE content_hash = ? AND file_edit_revision = ? \
                   AND kind IN ('tentative', 'valid') \
                 ORDER BY watched_folder_path, path",
                    params![content_hash, file_edit_revision],
                    |row| row.get::<_, CandidateKey>(0),
                )?;
                keys.into_iter()
                    .map(|key| {
                        read::load_item_by_key_rows(&sql, &key, RowSources::Any)?.ok_or_else(|| {
                            DbError::Message(format!("{key} vanished between its two reads"))
                        })
                    })
                    .collect::<Result<Vec<_>, DbError>>()
            })
            .process(|releases| {
                releases
                    .into_iter()
                    .map(|release| match release()?.1 {
                        read::StoredScanItem {
                            key,
                            item: ScanItem::Discovered(candidate) | ScanItem::Valid(candidate),
                            ..
                        } => Ok((key, candidate.files)),
                        read::StoredScanItem { key, .. } => Err(DbError::Message(format!(
                            "{key} is stored as a release with files but reads as none"
                        ))),
                    })
                    .collect()
            })
            .await
    }
}
