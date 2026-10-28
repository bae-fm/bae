use super::query::CandidateRows;
use super::*;

mod album_pick_rows;
mod edit_rows;
pub(super) mod failure_rows;
mod import_commit;
mod lookup_choice_rows;
mod lookup_failure_columns;
mod pane_rows;
mod preparation_rows;
mod prepared_asset_rows;
mod rows;
mod session_rows;
mod signal_rows;
mod standing_rows;
mod verdict_rows;
mod watched_folder_removal;

pub(super) use album_pick_rows::load_album_pick_on;
use edit_rows::{delete_file_edits, insert_file_edits};
use failure_rows::{delete_failure_on, load_failure_on};
pub(super) use import_commit::{
    end_import_attempt_of_commit, record_import_origin_on, require_import_commit_guard,
};
pub(super) use pane_rows::{insert_draft, load_covers_on, load_pane_rows_on};
pub(crate) use preparation_rows::{
    CandidatePaneWrite, CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved,
    RunVerdictSaved, ScannedCandidateKey,
};
pub(super) use rows::{
    load_candidate_file_edits_on, load_provenance_on, load_release_links_on, load_states_on,
};
use session_rows::load_session_on;
use signal_rows::{delete_signals, insert_signals};
pub(super) use standing_rows::{
    change_identification_on, end_identification_unless_of_on, load_standing_on, move_import_on,
    RuntimeFactColumns, RUNTIME_FACT_COLUMNS,
};
pub(crate) use standing_rows::{ImportClaim, StandingsRead};

use crate::import::folder_scanner::{
    CandidateFileEdits, FolderReleaseDecision, FolderReleaseDecisionAuthor,
    FolderReleaseDecisionKey, FolderReleaseDecisions, StoredCandidateEdits,
};
use rows::{insert_provenance, load_states_rows_on};
use std::collections::HashSet;
use verdict_rows::{delete_verdict, insert_verdict};

/// Move the pane inside a candidate write, without splitting the transaction.
pub(super) fn move_pane_on(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    pane_move: crate::import::PaneMove,
) -> Result<(), DbError> {
    session_rows::move_pane_on(sql, content_hash, pane_move)
}

/// Replace every file decision held for `content_hash` with `edits`.
pub(super) fn store_file_edits(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    edits: &CandidateFileEdits,
) -> Result<(), DbError> {
    delete_file_edits(sql, content_hash)?;
    insert_file_edits(sql, content_hash, edits)
}

/// Seed a candidate's draft from the folder's file tags, authored by the
/// pre-fill, with provenance naming the tags as its source.
pub(crate) fn insert_file_tags_draft(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    draft: &crate::import::CandidateDraft,
) -> Result<(), DbError> {
    pane_rows::insert_draft(
        sql,
        content_hash,
        draft,
        crate::import::MetadataAuthor::Prefill,
    )?;
    insert_provenance(
        sql,
        content_hash,
        &crate::import::MetadataProvenance::FileMetadata,
    )?;
    Ok(())
}

pub(super) fn require_current_candidate(
    sql: &SqlContext<'_, '_>,
    watched_folder_path: &str,
    candidate_path: &str,
    content_hash: &str,
    expected_file_edit_revision: u64,
) -> Result<u64, DbError> {
    let current: Option<(String, i64, i64)> = sql
        .query_row(
            "SELECT content_hash, generation, file_edit_revision \
             FROM scan_candidate WHERE watched_folder_path = ? AND path = ? AND kind = 'valid'",
            params![watched_folder_path, candidate_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let expected_file_edit_revision =
        to_i64(expected_file_edit_revision, "candidate file revision")?;
    let Some((current_hash, generation, current_file_edit_revision)) = current else {
        return Err(DbError::Message(format!(
            "candidate changed before metadata was stored: {candidate_path} is no longer valid"
        )));
    };
    if current_hash != content_hash || current_file_edit_revision != expected_file_edit_revision {
        return Err(DbError::Message(format!(
            "candidate changed before metadata was stored: {candidate_path} no longer names the prepared files"
        )));
    }
    u64::try_from(generation).map_err(|error| {
        DbError::Message(format!("candidate scan generation is negative: {error}"))
    })
}

/// `folder`, a stored folder under the watched folder `root`, by its path
/// below it.
pub(super) fn relative_below(root: &str, folder: &str) -> Result<String, DbError> {
    crate::import::watched_folder::candidate_relative_path(root, std::path::Path::new(folder))
        .map_err(|error| DbError::Message(error.to_string()))
}

/// The watched folder covering `folder`. A decision about a folder is deleted
/// with the watched folder covering it, so one stored outside every watched
/// folder would never be deleted.
pub(super) fn require_watched(sql: &SqlContext<'_, '_>, folder: &str) -> Result<String, DbError> {
    watched_folder_among_on(sql, std::path::Path::new(folder).ancestors())?
        .ok_or_else(|| DbError::Message(format!("{folder} is under no watched folder")))
}

/// The watched folder among `folders`, each looked up by the watched
/// folders' key. Watched folders never overlap, so the ancestors of one
/// folder name at most one.
pub(super) fn watched_folder_among_on<'a>(
    sql: &(impl QueryOne + QueryRows),
    folders: impl Iterator<Item = &'a std::path::Path>,
) -> Result<Option<String>, DbError> {
    let folders = serde_json::Value::from(
        folders
            .map(|folder| folder.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    )
    .to_string();
    Ok(sql
        .query_row(
            "SELECT path FROM watched_import_folders \
             WHERE path IN (SELECT value FROM json_each(?))",
            [folders],
            |row| row.get(0),
        )
        .optional()?)
}

/// The next scan generation. An upsert, so a missing seed row is created
/// rather than failing the scan.
pub(super) fn next_folder_scan_generation(sql: &SqlContext<'_, '_>) -> Result<i64, DbError> {
    let generation: i64 = sql.query_row(
        "INSERT INTO folder_scan_generation_sequence (singleton, last_generation) \
         VALUES (1, 1) \
         ON CONFLICT(singleton) DO UPDATE SET last_generation = last_generation + 1 \
         RETURNING last_generation",
        [],
        |row| row.get(0),
    )?;
    Ok(generation)
}

impl Database {
    /// Every watched folder, in the order they were added. A stored root that
    /// is not canonical, or overlaps another, fails the read.
    pub async fn load_watched_import_folders(
        &self,
    ) -> Result<Vec<crate::import::WatchedFolder>, DbError> {
        let roots = self.watched_import_roots().await?;
        for (index, root) in roots.iter().enumerate() {
            crate::import::watched_folder::validate_absolute_root(root)?;
            if let Some(conflict) = roots[index + 1..].iter().find(|other| {
                crate::import::watched_folder::paths_overlap(
                    std::path::Path::new(root),
                    std::path::Path::new(other),
                )
            }) {
                return Err(DbError::Message(format!(
                    "watched folders cannot overlap: {root} conflicts with {conflict}"
                )));
            }
        }
        Ok(roots
            .into_iter()
            .map(crate::import::WatchedFolder::from_path)
            .collect())
    }

    /// Every skipped candidate under one root, by its path below the root, read
    /// once per scan pass. A stored path that is not canonical fails the read.
    pub async fn load_skipped_import_candidates(
        &self,
        watched_folder_path: &str,
    ) -> Result<HashSet<String>, DbError> {
        let paths = self
            .read(move |sql| {
                Ok(sql.query(
                    "SELECT candidate_path FROM skipped_import_candidates",
                    [],
                    |row| row.get::<_, String>(0),
                )?)
            })
            .await?;
        let mut below = HashSet::new();
        for path in &paths {
            crate::import::watched_folder::validate_stored_folder(path)?;
            if std::path::Path::new(path).starts_with(watched_folder_path) {
                below.insert(relative_below(watched_folder_path, path)?);
            }
        }
        Ok(below)
    }

    /// Every watched root, in the order they were added.
    /// Whether `path`, spelled as the store keys it, is a watched folder,
    /// read by the watched folders' key.
    pub async fn is_watched_import_folder(&self, path: &str) -> Result<bool, DbError> {
        let path = path.to_string();
        self.read(move |sql| {
            Ok(sql.query_row(
                "SELECT EXISTS (SELECT 1 FROM watched_import_folders WHERE path = ?)",
                [path],
                |row| row.get(0),
            )?)
        })
        .await
    }

    /// The watched folder that holds `path` inside it, not `path` itself.
    pub async fn watched_import_folder_above(&self, path: &str) -> Result<Option<String>, DbError> {
        let path = path.to_string();
        self.read(move |sql| {
            watched_folder_among_on(&sql, std::path::Path::new(&path).ancestors().skip(1))
        })
        .await
    }

    /// The watched folders inside `path`, not `path` itself, in their order:
    /// those whose key lies in the range of paths below it.
    pub async fn watched_import_folders_below(&self, path: &str) -> Result<Vec<String>, DbError> {
        let (first, past) = crate::import::folder_path::paths_below(path);
        let path = path.to_string();
        self.read(move |sql| {
            Ok(sql.query(
                "SELECT path FROM watched_import_folders \
                 WHERE path >= ? AND path < ? AND path <> ? ORDER BY position",
                params![first, past, path],
                |row| row.get(0),
            )?)
        })
        .await
    }

    async fn watched_import_roots(&self) -> Result<Vec<String>, DbError> {
        self.read(move |sql| {
            Ok(sql.query(
                "SELECT path FROM watched_import_folders ORDER BY position",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
    }

    /// Watch the folder `path` names, keyed by its canonical spelling; `false`
    /// when it is already watched.
    ///
    /// The overlap check runs inside the inserting write, so two overlapping
    /// folders added at once cannot both pass. Already-watched is answered by
    /// a read first, since the store refuses a write that changes nothing; a
    /// folder watched in between fails the write.
    pub async fn add_watched_import_folder(&self, path: &str) -> Result<bool, DbError> {
        let path = crate::import::watched_folder::canonical_absolute_root(path)?;
        if self.is_watched_import_folder(&path).await? {
            return Ok(false);
        }
        self.call(move |sql| {
            let roots: Vec<String> = sql.query(
                "SELECT path FROM watched_import_folders ORDER BY position",
                [],
                |row| row.get(0),
            )?;
            if roots.iter().any(|root| root == &path) {
                return Err(DbError::Message(format!(
                    "watched folder {path} was added while this add was deciding"
                )));
            }
            if let Some(conflict) = roots.iter().find(|root| {
                crate::import::watched_folder::paths_overlap(
                    std::path::Path::new(&path),
                    std::path::Path::new(root),
                )
            }) {
                return Err(DbError::Message(format!(
                    "watched folders cannot overlap: {path} conflicts with {conflict}"
                )));
            }
            let position: i64 = sql.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM watched_import_folders",
                [],
                |row| row.get(0),
            )?;
            sql.execute(
                "INSERT INTO watched_import_folders (path, position) VALUES (?, ?)",
                params![path, position],
            )?;
            Ok(true)
        })
        .await
    }

    /// Set the candidate whose folder on disk is `candidate_path` aside, or
    /// back; `false` when it already was. Setting it aside ends its
    /// identification in the same write.
    pub async fn set_import_candidate_skipped(
        &self,
        candidate_path: &str,
        skipped: bool,
    ) -> Result<bool, DbError> {
        crate::import::watched_folder::validate_stored_folder(candidate_path)?;
        let candidate_path = candidate_path.to_string();
        // The store refuses a write that changes nothing, so read first.
        let stored = {
            let candidate_path = candidate_path.clone();
            self.read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT 1 FROM skipped_import_candidates WHERE candidate_path = ?",
                        [candidate_path],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some())
            })
            .await?
        };
        if stored == skipped {
            return Ok(false);
        }
        self.call(move |sql| {
            let changed = if skipped {
                require_watched(sql, &candidate_path)?;
                // Set aside, the candidate is not identification's to answer:
                // its identification ends with the skip.
                change_identification_on(
                    sql,
                    &candidate_path,
                    &crate::import::candidate_runtime::IdentificationChange::Ended,
                )?;
                sql.execute(
                    "INSERT INTO skipped_import_candidates (candidate_path) VALUES (?) \
                     ON CONFLICT DO NOTHING",
                    [candidate_path],
                )?
            } else {
                sql.execute(
                    "DELETE FROM skipped_import_candidates WHERE candidate_path = ?",
                    [candidate_path],
                )?
            };
            Ok(changed == 1)
        })
        .await
    }
}

impl Database {
    /// Store the reading a scan settled on for one folder under the grouping
    /// key the scan gave it, without a new scan generation. A user's reading
    /// is kept; the write fails when the folder is stored under another key.
    pub async fn record_scanned_folder_release_decision(
        &self,
        key: &FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
        grouping: &str,
    ) -> Result<(), DbError> {
        let key = key.clone();
        let grouping = grouping.to_string();
        self.call(move |sql| {
            super::folder_scans::store_folder_reading(
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
        reading: &crate::import::folder_scanner::FolderReading,
    ) -> Result<(), DbError> {
        let key = key.clone();
        let reading = reading.clone();
        self.call(move |sql| {
            super::folder_scans::store_folder_reading(
                sql,
                &key,
                reading.decision,
                reading.author,
                &reading.grouping,
            )
        })
        .await
    }

    /// The grouping the folder `folder` is read as, by the groupings' anchor
    /// index; `None` when no reading of it is stored.
    pub async fn folder_reading_grouping(
        &self,
        folder: &FolderReleaseDecisionKey,
    ) -> Result<Option<String>, DbError> {
        let anchor = crate::import::folder_path::folder_below(
            &folder.watched_folder_path,
            &folder.relative_folder_path,
        )
        .map_err(|error| DbError::Message(error.to_string()))?;
        self.read(move |sql| {
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

    /// The keys of the scanned releases read from `folder` or a folder below
    /// it under the watched folder `watched_folder_path`, by the candidates'
    /// folder index.
    pub async fn scanned_release_keys_at_or_below(
        &self,
        watched_folder_path: &str,
        folder: &str,
    ) -> Result<std::collections::BTreeSet<String>, DbError> {
        let (below_first, below_past) = crate::import::folder_path::paths_below(folder);
        let (root, folder) = (watched_folder_path.to_string(), folder.to_string());
        self.read(move |sql| {
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
        watched_folder_path: &str,
    ) -> Result<FolderReleaseDecisions, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let (below_first, below_past) =
            crate::import::folder_path::paths_below(&watched_folder_path);
        self.read(move |sql| {
            let rows = sql.query(
                "SELECT anchor_folder, combined, author, key FROM release_grouping \
                 WHERE anchor_folder = :root \
                    OR (anchor_folder >= :below_first AND anchor_folder < :below_past)",
                rusqlite::named_params! {
                    ":root": watched_folder_path,
                    ":below_first": below_first,
                    ":below_past": below_past,
                },
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?;
            let mut readings = HashMap::new();
            for (folder, combined, author, grouping) in rows {
                crate::import::watched_folder::validate_stored_folder(&folder)?;
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
                    crate::import::folder_scanner::FolderReading {
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

    /// Every candidate's user-set file decisions, keyed by `content_hash`, so a
    /// folder scan reports the roles the user settled.
    pub async fn load_stored_candidate_edits(&self) -> Result<StoredCandidateEdits, DbError> {
        self.read(move |sql| rows::load_stored_candidate_edits_on(&sql))
            .process(|process| process())
            .await
    }

    /// Every release a scan stored reading the files `content_hash` names at
    /// file decision revision `file_edit_revision`, under whichever root, with
    /// those files in key order: what a file decision settles together.
    pub async fn load_releases_reading(
        &self,
        content_hash: &str,
        file_edit_revision: u64,
    ) -> Result<Vec<(String, crate::import::folder_scanner::CategorizedFiles)>, DbError> {
        let content_hash = content_hash.to_string();
        let file_edit_revision = to_i64(file_edit_revision, "a file decision revision")?;
        self.read(move |sql| {
            let keys = sql.query(
                "SELECT path FROM scan_candidate \
                 WHERE content_hash = ? AND file_edit_revision = ? \
                   AND kind IN ('tentative', 'valid') \
                 ORDER BY watched_folder_path, path",
                params![content_hash, file_edit_revision],
                |row| row.get::<_, String>(0),
            )?;
            keys.into_iter()
                .map(|key| {
                    folder_scans::read::load_item_by_key_rows(
                        &sql,
                        &key,
                        folder_scans::RowSources::Any,
                    )?
                    .ok_or_else(|| {
                        DbError::Message(format!("{key} vanished between its two reads"))
                    })
                })
                .collect::<Result<Vec<_>, DbError>>()
        })
        .process(|releases| {
            releases
                .into_iter()
                .map(|release| match release()?.1 {
                    folder_scans::read::StoredScanItem {
                        key,
                        item:
                            crate::import::folder_scanner::ScanItem::Discovered(candidate)
                            | crate::import::folder_scanner::ScanItem::Valid(candidate),
                        ..
                    } => Ok((key, candidate.files)),
                    folder_scans::read::StoredScanItem { key, .. } => Err(DbError::Message(
                        format!("{key} is stored as a release with files but reads as none"),
                    )),
                })
                .collect()
        })
        .await
    }

    /// One candidate's file decisions, by one indexed lookup, for a scan that
    /// has just computed the content hash.
    pub async fn load_candidate_file_edits(
        &self,
        content_hash: &str,
    ) -> Result<CandidateFileEdits, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| load_candidate_file_edits_on(&sql, &content_hash))
            .process(|process| process())
            .await
    }

    /// Every stored `import_candidate_state` row, keyed by `content_hash`, for
    /// the tests that read back everything a write stored. A column holding a
    /// value no writer produces fails the read.
    #[cfg(test)]
    pub(crate) async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
        self.read(move |sql| load_states_rows_on(&sql, CandidateRows::All))
            .process(|process| process())
            .await
    }

    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<DbImportCandidateState>, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| {
            Ok((
                load_states_rows_on(&sql, CandidateRows::One(&content_hash))?,
                content_hash,
            ))
        })
        .process(|(process, content_hash)| Ok(process()?.remove(&content_hash)))
        .await
    }
}

/// Give every candidate sharing the preparation its settled files and rebuild
/// every grouping built from one of them. Every row must still be at the
/// revision the caller prepared against.
pub(super) fn settle_scanned_candidates(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    expected_revision: i64,
    next_revision: i64,
    settled_by_key: &HashMap<String, crate::import::folder_scanner::CategorizedFiles>,
) -> Result<Vec<crate::import::folder_scanner::FolderCandidate>, DbError> {
    let scanned = sql.query(
        "SELECT watched_folder_path, path, source_kind, file_edit_revision FROM scan_candidate \
         WHERE content_hash = ? ORDER BY watched_folder_path, path",
        [content_hash],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )?;
    let mut updated_folders = HashSet::new();
    let mut updated_candidates = Vec::with_capacity(scanned.len());
    for (watched_folder_path, path, source_kind, revision) in scanned {
        if revision != expected_revision {
            return Err(DbError::Message(format!(
                "candidate {path} changed before its source decisions were stored"
            )));
        }
        match source_kind.as_str() {
            "folder" | "grouping" => {
                let settled = settled_by_key.get(&path).ok_or_else(|| {
                    DbError::Message(format!(
                        "persisted candidate {path} was missing from the settled file edit"
                    ))
                })?;
                // Deleting the file rows deletes the file-tag reading with
                // them; the files themselves did not change, so it is put
                // back as it was, at the revision it was taken at.
                let reading =
                    folder_scans::read::load_file_tag_snapshot(sql, &watched_folder_path, &path)?;
                sql.execute(
                    "DELETE FROM scan_candidate_file WHERE watched_folder_path = ? AND candidate_path = ?",
                    params![watched_folder_path, path],
                )?;
                folder_scans::insert_candidate_files(sql, &watched_folder_path, &path, settled)?;
                if let Some(reading) = reading {
                    folder_scans::write::replace_candidate_file_tag_snapshot(
                        sql,
                        &watched_folder_path,
                        &path,
                        &reading,
                    )?;
                }
                updated_folders.insert(path.clone());
            }
            other => {
                return Err(DbError::Message(format!(
                    "unknown candidate source {other}"
                )))
            }
        }
        let changed = sql.execute(
            "UPDATE scan_candidate SET file_edit_revision = ? \
             WHERE watched_folder_path = ? AND path = ? AND file_edit_revision = ?",
            params![next_revision, watched_folder_path, path, expected_revision],
        )?;
        if changed != 1 {
            return Err(DbError::Message(format!(
                "candidate file decision changed {changed} persisted scan entries for {path}; \
                 expected exactly one"
            )));
        }
        // The decision makes the candidate other files than its
        // identification was of, which ends with it.
        end_identification_unless_of_on(
            sql,
            &path,
            Some(&crate::import::CandidateFiles {
                content_hash: content_hash.to_string(),
                file_edit_revision: u64::try_from(next_revision).map_err(|error| {
                    DbError::Message(format!("file edit revision {next_revision}: {error}"))
                })?,
            }),
        )?;
        let stored = super::release_groupings::load_candidate_on(sql, &path)?.ok_or_else(|| {
            DbError::Message(format!(
                "candidate {path} disappeared while its source decisions were stored"
            ))
        })?;
        updated_candidates.push(stored.candidate);
    }
    if updated_folders.len() != settled_by_key.len() {
        let missing: Vec<_> = settled_by_key
            .keys()
            .filter(|key| !updated_folders.contains(*key))
            .cloned()
            .collect();
        return Err(DbError::Message(format!(
            "candidate file decision could not update persisted scan entries: {}",
            missing.join(", ")
        )));
    }
    let settled: Vec<String> = updated_folders.into_iter().collect();
    let regrouped = super::release_groupings::rebuild_groupings(sql, &settled, &[])?;
    updated_candidates.extend(regrouped.written.into_iter().filter_map(|item| match item {
        crate::import::folder_scanner::ScanItem::Valid(candidate) => Some(candidate),
        _ => None,
    }));
    Ok(updated_candidates)
}

/// The stored medium conflict of a verdict row.
pub(super) fn medium_conflict_of(
    kind: Option<String>,
) -> Result<Option<crate::identify::MediumConflict>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    Ok(Some(match kind.as_str() {
        "cd_rip" => crate::identify::MediumConflict::CdRip,
        "not_cd_audio" => crate::identify::MediumConflict::NotCdAudio,
        other => return Err(verdict_rows::unreadable("medium_conflict", other)),
    }))
}
