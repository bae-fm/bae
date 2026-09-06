use super::*;

mod edit_rows;
mod failure_rows;
mod import_commit;
mod pane_rows;
mod preparation_rows;
mod prepared_asset_rows;
mod rows;
mod session_rows;
mod signal_rows;
mod verdict_rows;
mod watched_folder_removal;

use super::folder_scans::{delete_entry, load_scan_item_on, stored_entries, StoredEntry};
use edit_rows::{delete_file_edits, insert_file_edits};
use failure_rows::load_failure_on;
pub(super) use import_commit::require_import_commit_guard;
pub(super) use pane_rows::{insert_draft, load_covers_on, load_drafts_on, load_pane_rows_on};
pub(crate) use preparation_rows::{
    CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved, ScannedCandidateKey,
};
pub(super) use rows::{
    load_matches_on, load_provenance_partners_on, load_states_on, metadata_provenance_of,
};
use session_rows::load_session_on;
use signal_rows::{delete_signals, insert_signals};

use crate::import::folder_scanner::{
    CandidateFileEdits, FolderReleaseDecision, FolderReleaseDecisionAuthor,
    FolderReleaseDecisionKey, FolderReleaseDecisions, StoredCandidateEdits,
};
use rows::{
    load_candidate_file_edits_on, replace_provenance_partners, seed_columns,
    MetadataProvenanceAuthor,
};
use std::collections::HashSet;
use verdict_rows::{
    delete_identify_failure, delete_matches, insert_identify_failure, insert_matches,
    verdict_columns,
};

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
    let expected_file_edit_revision = i64::try_from(expected_file_edit_revision)
        .map_err(|_| DbError::Message("candidate file revision exceeds SQLite range".into()))?;
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
    u64::try_from(generation)
        .map_err(|_| DbError::Message("candidate scan generation is negative".into()))
}

/// The next scan generation. One upsert rather than a read of a seeded row
/// and a write back: the counter's row is created by the first allocation,
/// so a store whose device-local tables were rebuilt without the migration's
/// seed still scans.
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
    /// The spelling the watched-folder tables key `path`'s folder by. Every
    /// entry point that names a root goes through this, so a caller never has
    /// to know what spelling this host stores — and two spellings of one
    /// folder can never become two rows.
    fn canonical_watched_root(path: &str) -> Result<String, DbError> {
        crate::import::watched_folder::canonical_absolute_root(path)
            .map_err(|error| DbError::Message(error.to_string()))
    }

    /// Every watched folder, in the order they were added.
    ///
    /// A stored root is canonical by construction and no two overlap, so a
    /// store that says otherwise is corrupt and is read loudly rather than
    /// quietly rewritten.
    pub async fn load_watched_import_folders(
        &self,
    ) -> Result<Vec<crate::import::WatchedFolder>, DbError> {
        let roots = self.watched_import_roots().await?;
        for (index, root) in roots.iter().enumerate() {
            crate::import::watched_folder::validate_absolute_root(root)
                .map_err(|error| DbError::Message(error.to_string()))?;
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

    /// Every skipped candidate under one root, by relative path — what a scan
    /// pass reads once so each candidate it writes is stamped without a
    /// query of its own. A stored path is normalized by construction, so one
    /// that is not is corrupt durable state and is read loudly.
    pub async fn load_skipped_import_candidates(
        &self,
        watched_folder_path: &str,
    ) -> Result<HashSet<String>, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let paths = self
            .read(move |sql| {
                Ok(sql.query(
                    "SELECT relative_candidate_path FROM skipped_import_candidates \
                     WHERE watched_folder_path = ?",
                    [watched_folder_path],
                    |row| row.get::<_, String>(0),
                )?)
            })
            .await?;
        for path in &paths {
            crate::import::watched_folder::validate_relative_path(path)
                .map_err(|error| DbError::Message(error.to_string()))?;
        }
        Ok(paths.into_iter().collect())
    }

    /// Every watched root, in the order they were added. Read by both entry
    /// points below to answer "is this folder already watched?" before either
    /// opens a write.
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

    /// Watch the folder `path` names, keyed by its canonical spelling. `false`
    /// when that folder is already watched, however it was spelled this time.
    pub async fn add_watched_import_folder(&self, path: &str) -> Result<bool, DbError> {
        let path = Self::canonical_watched_root(path)?;
        let roots = self.watched_import_roots().await?;
        if roots.iter().any(|root| root == &path) {
            return Ok(false);
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
        self.call(move |sql| {
            let position: i64 = sql.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM watched_import_folders",
                [],
                |row| row.get(0),
            )?;
            Ok(sql.execute(
                "INSERT INTO watched_import_folders (path, position) VALUES (?, ?)",
                params![path, position],
            )? == 1)
        })
        .await
    }

    pub async fn set_import_candidate_skipped(
        &self,
        watched_folder_path: &str,
        relative_candidate_path: &str,
        skipped: bool,
    ) -> Result<bool, DbError> {
        crate::import::watched_folder::validate_relative_path(relative_candidate_path)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let watched_folder_path = watched_folder_path.to_string();
        let relative_candidate_path = relative_candidate_path.to_string();
        // Restating what the row already says writes nothing, so it must not
        // open a write to discover that: read the standing answer first.
        let stored = {
            let watched_folder_path = watched_folder_path.clone();
            let relative_candidate_path = relative_candidate_path.clone();
            self.read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT 1 FROM skipped_import_candidates \
                         WHERE watched_folder_path = ? AND relative_candidate_path = ?",
                        params![watched_folder_path, relative_candidate_path],
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
                sql.execute(
                    "INSERT INTO skipped_import_candidates \
                         (watched_folder_path, relative_candidate_path) VALUES (?, ?) \
                     ON CONFLICT DO NOTHING",
                    params![watched_folder_path, relative_candidate_path],
                )?
            } else {
                sql.execute(
                    "DELETE FROM skipped_import_candidates \
                     WHERE watched_folder_path = ? AND relative_candidate_path = ?",
                    params![watched_folder_path, relative_candidate_path],
                )?
            };
            Ok(changed == 1)
        })
        .await
    }
}

impl Database {
    /// Set one watched folder's interpretation atomically and idempotently.
    pub async fn set_folder_release_decision(
        &self,
        key: &FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
        author: FolderReleaseDecisionAuthor,
    ) -> Result<u64, DbError> {
        let (generation, _) = self
            .set_folder_release_decisions(&[(key.clone(), decision)], author)
            .await?;
        Ok(generation)
    }

    /// Store readings for one watched folder. `author` says whose they are:
    /// the user's answer replaces whatever a scan settled on, and a scan's own
    /// reading never replaces the user's.
    pub async fn set_folder_release_decisions(
        &self,
        decisions: &[(FolderReleaseDecisionKey, FolderReleaseDecision)],
        author: FolderReleaseDecisionAuthor,
    ) -> Result<(u64, Vec<String>), DbError> {
        let Some(first) = decisions.first() else {
            return Err(DbError::Message(
                "folder release decision set cannot be empty".to_string(),
            ));
        };
        let watched_folder_path = first.0.watched_folder_path.clone();
        if decisions
            .iter()
            .any(|(key, _)| key.watched_folder_path != watched_folder_path)
        {
            return Err(DbError::Message(
                "folder release decisions must belong to one watched folder".to_string(),
            ));
        }
        for (key, _) in decisions {
            crate::import::watched_folder::validate_relative_path(&key.relative_folder_path)
                .map_err(|error| DbError::Message(error.to_string()))?;
        }
        let decisions = decisions.to_vec();
        let author_column = match author {
            FolderReleaseDecisionAuthor::User => "user",
            FolderReleaseDecisionAuthor::Heuristic => "heuristic",
        };
        self.call(move |sql| {
            let stored = stored_entries(sql, &watched_folder_path)?;
            let persisted: Vec<crate::import::candidates::StoredEntryKey> = stored
                .iter()
                .map(|(key, entry)| crate::import::candidates::StoredEntryKey {
                    key: key.clone(),
                    covers_whole_folder: matches!(
                        entry,
                        StoredEntry::Candidate {
                            whole_folder: true,
                            ..
                        }
                    ),
                })
                .collect();
            let removed_scan_entry_keys =
                crate::import::folder_scanner::release_decision_removed_keys(
                    &persisted, &decisions,
                );
            let stored: HashMap<&str, &StoredEntry> = stored
                .iter()
                .map(|(key, entry)| (key.as_str(), entry))
                .collect();

            for (key, decision) in &decisions {
                let decision = match decision {
                    FolderReleaseDecision::CombineAsOneRelease => "combine_as_one_release",
                    FolderReleaseDecision::KeepAsSeparateReleases => "keep_as_separate_releases",
                };
                sql.execute(
                    "INSERT INTO folder_release_decisions \
                         (watched_folder_path, relative_folder_path, decision, author) \
                     VALUES (?, ?, ?, ?) \
                     ON CONFLICT(watched_folder_path, relative_folder_path) DO UPDATE SET \
                         decision = excluded.decision, author = excluded.author \
                     WHERE excluded.author = 'user' \
                         OR folder_release_decisions.author != 'user'",
                    params![
                        key.watched_folder_path,
                        key.relative_folder_path,
                        decision,
                        author_column
                    ],
                )?;
            }
            for entry_key in &removed_scan_entry_keys {
                if let Some(entry) = stored.get(entry_key.as_str()) {
                    delete_entry(sql, &watched_folder_path, entry)?;
                }
            }
            let generation = next_folder_scan_generation(sql)?;
            sql.execute(
                "INSERT INTO folder_scan_roots \
                     (watched_folder_path, generation, status, error) \
                 VALUES (?, ?, 'scanning', NULL) \
                 ON CONFLICT(watched_folder_path) DO UPDATE SET \
                     generation = excluded.generation, status = 'scanning', error = NULL",
                params![watched_folder_path, generation],
            )?;
            let generation = u64::try_from(generation)
                .map_err(|_| DbError::Message("folder scan generation is negative".to_string()))?;
            Ok((generation, removed_scan_entry_keys))
        })
        .await
    }

    /// Store the reading a scan settled on for one folder, without disturbing
    /// the scan that produced it: no generation bump, no re-scan. A folder the
    /// user has already answered for keeps their answer.
    pub async fn record_scanned_folder_release_decision(
        &self,
        key: &FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
    ) -> Result<(), DbError> {
        crate::import::watched_folder::validate_relative_path(&key.relative_folder_path)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let key = key.clone();
        let decision = match decision {
            FolderReleaseDecision::CombineAsOneRelease => "combine_as_one_release",
            FolderReleaseDecision::KeepAsSeparateReleases => "keep_as_separate_releases",
        };
        self.call(move |sql| {
            sql.execute(
                "INSERT INTO folder_release_decisions \
                     (watched_folder_path, relative_folder_path, decision, author) \
                 VALUES (?, ?, ?, 'heuristic') \
                 ON CONFLICT(watched_folder_path, relative_folder_path) DO UPDATE SET \
                     decision = excluded.decision, author = 'heuristic' \
                 WHERE folder_release_decisions.author != 'user'",
                params![key.watched_folder_path, key.relative_folder_path, decision],
            )?;
            Ok(())
        })
        .await
    }

    /// Every explicit interpretation below one watched root.
    pub async fn load_folder_release_decisions(
        &self,
        watched_folder_path: &str,
    ) -> Result<FolderReleaseDecisions, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| {
            let decisions = sql.query(
                "SELECT relative_folder_path, decision, author \
                 FROM folder_release_decisions WHERE watched_folder_path = ?",
                [watched_folder_path],
                |row| {
                    let path: String = row.get(0)?;
                    crate::import::watched_folder::validate_relative_path(&path).map_err(
                        |error| {
                            coven::rusqlite::Error::FromSqlConversionFailure(
                                0,
                                coven::rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?;
                    let stored: String = row.get(1)?;
                    let decision = match stored.as_str() {
                        "combine_as_one_release" => FolderReleaseDecision::CombineAsOneRelease,
                        "keep_as_separate_releases" => {
                            FolderReleaseDecision::KeepAsSeparateReleases
                        }
                        other => {
                            return Err(coven::rusqlite::Error::FromSqlConversionFailure(
                                1,
                                coven::rusqlite::types::Type::Text,
                                format!("unknown folder release decision {other:?}").into(),
                            ))
                        }
                    };
                    let stored: String = row.get(2)?;
                    let author = match stored.as_str() {
                        "user" => FolderReleaseDecisionAuthor::User,
                        "heuristic" => FolderReleaseDecisionAuthor::Heuristic,
                        other => {
                            return Err(coven::rusqlite::Error::FromSqlConversionFailure(
                                2,
                                coven::rusqlite::types::Type::Text,
                                format!("unknown folder release decision author {other:?}").into(),
                            ))
                        }
                    };
                    Ok((path, (decision, author)))
                },
            )?;
            Ok(FolderReleaseDecisions::new(decisions.into_iter().collect()))
        })
        .await
    }

    /// Every candidate's user-set file decisions, keyed by `content_hash` — the
    /// shape a folder scan takes so the roles it reports are the ones the user
    /// settled, not only the ones its filenames propose.
    ///
    /// One candidate's file decisions. Progressive scans call this after they
    /// compute a candidate content hash so each emitted row performs one indexed
    /// lookup instead of rereading the whole candidate-state table.
    ///
    /// Projected from the one stored-row read rather than a query of its own:
    /// the sweep and the scan want different halves of the same few hundred
    /// rows, and two queries over one table is two things to keep in step.
    pub async fn load_stored_candidate_edits(&self) -> Result<StoredCandidateEdits, DbError> {
        Ok(StoredCandidateEdits::new(
            self.load_import_candidate_states()
                .await?
                .into_iter()
                .map(|(hash, state)| (hash, state.file_edits))
                .collect(),
        ))
    }

    /// One candidate's file decisions. Progressive scans call this after they
    /// compute a candidate content hash so each emitted row performs one indexed
    /// lookup instead of rereading the whole candidate-state table.
    pub async fn load_candidate_file_edits(
        &self,
        content_hash: &str,
    ) -> Result<CandidateFileEdits, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| load_candidate_file_edits_on(&sql, &content_hash))
            .await
    }

    /// Every stored `import_candidate_state` row, keyed by `content_hash`. The
    /// queue is small enough to read whole; callers classify in memory rather
    /// than filtering in SQL.
    ///
    /// A row whose columns hold a spelling no writer here produces cannot come
    /// from either write path above. Loading fails rather than substituting a
    /// plausible candidate shape.
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
        self.read(move |sql| load_states_on(&sql, None)).await
    }

    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<DbImportCandidateState>, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| Ok(load_states_on(&sql, Some(&content_hash))?.remove(&content_hash)))
            .await
    }
}

/// Rewrite the file rows of every scanned candidate at `content_hash` and
/// `expected_revision` to the shape the caller settled, and hand the settled
/// candidates back.
///
/// The caller's set must cover exactly those candidates: it computed the
/// settled files from the same read, so a candidate it did not settle is a
/// scan that moved under it, and writing half the set would leave two rows of
/// one release disagreeing about what its files are.
pub(super) fn settle_scanned_candidates(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    expected_revision: i64,
    next_revision: i64,
    settled_by_key: &HashMap<String, crate::import::folder_scanner::CategorizedFiles>,
) -> Result<Vec<crate::import::folder_scanner::FolderCandidate>, DbError> {
    let scanned = sql.query(
        "SELECT watched_folder_path, path FROM scan_candidate \
         WHERE content_hash = ? AND file_edit_revision = ? ORDER BY watched_folder_path, path",
        params![content_hash, expected_revision],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut updated_keys = HashSet::new();
    let mut updated_candidates = Vec::with_capacity(scanned.len());
    for (watched_folder_path, path) in scanned {
        let settled = settled_by_key.get(&path).ok_or_else(|| {
            DbError::Message(format!(
                "persisted candidate {path} was missing from the settled file edit"
            ))
        })?;
        sql.execute(
            "DELETE FROM scan_candidate_file WHERE watched_folder_path = ? AND candidate_path = ?",
            params![watched_folder_path, path],
        )?;
        folder_scans::insert_candidate_files(sql, &watched_folder_path, &path, settled)?;
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
        let (Some(crate::import::folder_scanner::ScanItem::Discovered(candidate))
        | Some(crate::import::folder_scanner::ScanItem::Valid(candidate))) =
            load_scan_item_on(sql, &path)?
        else {
            return Err(DbError::Message(format!(
                "the candidate at {path} is not a folder candidate after its file decision"
            )));
        };
        updated_candidates.push(candidate);
        updated_keys.insert(path);
    }
    if updated_keys.len() != settled_by_key.len() {
        let missing: Vec<_> = settled_by_key
            .keys()
            .filter(|key| !updated_keys.contains(*key))
            .cloned()
            .collect();
        return Err(DbError::Message(format!(
            "candidate file decision could not update persisted scan entries: {}",
            missing.join(", ")
        )));
    }
    Ok(updated_candidates)
}
