use super::*;

mod duration_rows;
mod edit_rows;
mod pane_rows;
mod rows;
mod signal_rows;
mod verdict_rows;

use super::folder_scans::{delete_entry, load_scan_item_on, stored_entries, StoredEntry};
use duration_rows::{delete_durations, delete_slice_durations, insert_durations};
use edit_rows::{delete_file_edits, insert_file_edits};
pub(super) use pane_rows::load_pane_rows_on;
pub(super) use rows::{load_states_on, metadata_seed_of};
use signal_rows::{delete_signals, insert_signals};

use rows::{load_candidate_file_edits_on, seed_columns};
use verdict_rows::{delete_matches, insert_matches, verdict_columns};

/// Who decided a candidate's stored metadata seed. The two outlive different
/// things — identification's goes with the verdict that concluded it, a
/// person's outlives every verdict — so the row records which it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataSeedAuthor {
    User,
    Identification,
}

impl MetadataSeedAuthor {
    /// The stored `metadata_seed_author` value.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Identification => "identification",
        }
    }
}
use crate::import::folder_scanner::{
    CandidateFileEdits, FolderReleaseDecision, FolderReleaseDecisionAuthor,
    FolderReleaseDecisionKey, FolderReleaseDecisions, StoredCandidateEdits,
};
use std::collections::HashSet;

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
        crate::import::folder_registry::canonical_absolute_root(path)
            .map_err(|error| DbError::Message(error.to_string()))
    }

    pub async fn load_import_folder_registry(
        &self,
    ) -> Result<crate::import::ImportFolderRegistry, DbError> {
        self.read(move |sql| {
            let folders = sql.query(
                "SELECT path FROM watched_import_folders ORDER BY position",
                [],
                |row| row.get::<_, String>(0),
            )?;
            let skipped = sql.query(
                "SELECT watched_folder_path, relative_candidate_path \
                     FROM skipped_import_candidates \
                     ORDER BY watched_folder_path, relative_candidate_path",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            crate::import::ImportFolderRegistry::from_stored(folders, skipped)
                .map_err(|error| DbError::Message(error.to_string()))
        })
        .await
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
            crate::import::folder_registry::paths_overlap(
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

    /// Stop watching the folder `path` names. Keyed the same way as the add,
    /// so whichever spelling reaches here names the row the add created.
    /// Returns the keys of the scan entries the removal cascaded away, or
    /// `None` when the folder was not watched.
    pub async fn remove_watched_import_folder(
        &self,
        path: &str,
    ) -> Result<Option<Vec<String>>, DbError> {
        let path = Self::canonical_watched_root(path)?;
        if !self.watched_import_roots().await?.contains(&path) {
            return Ok(None);
        }
        self.call(move |sql| {
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
            Ok(Some(entry_keys))
        })
        .await
    }

    pub async fn set_import_candidate_skipped(
        &self,
        watched_folder_path: &str,
        relative_candidate_path: &str,
        skipped: bool,
    ) -> Result<bool, DbError> {
        crate::import::folder_registry::validate_relative_path(relative_candidate_path)
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
            crate::import::folder_registry::validate_relative_path(&key.relative_folder_path)
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
        crate::import::folder_registry::validate_relative_path(&key.relative_folder_path)
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
                    crate::import::folder_registry::validate_relative_path(&path).map_err(
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

    /// Record one candidate's terminal identify verdict, keyed by
    /// `content_hash`. Never synced. `identified_at` is stamped here from the
    /// injected clock, not taken from `verdict` — see
    /// [`NewImportCandidateVerdict`]'s doc.
    ///
    /// The row's other half — the user's file decisions — is left untouched: an
    /// upsert rather than a replace, because a candidate can hold a decision
    /// before anything has identified it, and a verdict must not erase the
    /// decision that produced the shape it was derived from.
    pub async fn save_import_candidate_verdict(
        &self,
        verdict: &NewImportCandidateVerdict,
    ) -> Result<bool, DbError> {
        let verdict = verdict.clone();
        let expected = i64::try_from(verdict.expected_edit_revision).map_err(|_| {
            DbError::Message(format!(
                "candidate edit revision {} exceeds SQLite's integer range",
                verdict.expected_edit_revision
            ))
        })?;
        // The column is the sum of the duration rows this same write lays
        // down, derived here so the two can never disagree.
        let probed = i64::try_from(verdict.signals.probed_total_duration_ms()).map_err(|_| {
            DbError::Message("a probed total exceeds SQLite's integer range".to_string())
        })?;
        let now = self.inner.clock.now().to_rfc3339();
        // Which of the two writes applies — and whether either does — is a
        // question about the stored revision, so it is asked on the read
        // connection. A verdict derived from file decisions the row has since
        // moved past writes nothing, and must not open a write to say so.
        let current: Option<i64> = {
            let content_hash = verdict.content_hash.clone();
            self.read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT edit_revision FROM import_candidate_state WHERE content_hash = ?",
                        [&content_hash],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await?
        };
        // Both statements below carry the revision they were chosen for, so a
        // decision landing between the read and the write leaves them writing
        // nothing rather than writing over it.
        let update_existing = match current {
            Some(current) if current == expected => true,
            None if expected == 0 => false,
            // The row has moved past the file decisions this verdict was
            // derived from, or it names a revision no row ever had. Either way
            // there is nothing to write.
            Some(_) | None => return Ok(false),
        };
        self.call(move |sql| {
            let columns = verdict_columns(&verdict.verdict);
            let pick = verdict.metadata_seed.as_ref().map(seed_columns);
            // A verdict never replaces a person's pick, so only an
            // identification pick can be superseded here — and only then does
            // the metadata typed over the release it named go with it.
            let superseded = stored_pick(sql, &verdict.content_hash)?
                .filter(|stored| stored.author == MetadataSeedAuthor::Identification);
            // A pick identification made belongs to the verdict that made it,
            // so this write replaces it — with the new verdict's own
            // conclusion, or with nothing when it concluded none. A pick a
            // person made is theirs and is left exactly as it is: a run whose
            // signals turn up nothing says nothing about a release they chose.
            let wrote = if update_existing {
                sql.execute(
                    "UPDATE import_candidate_state SET \
                         folder_path = :folder_path, \
                         verdict_kind = :kind, verdict_track_count = :track_count, \
                         verdict_matched_barcode = :matched_barcode, \
                         probed_total_duration_ms = :probed, identified_at = :now, \
                         seed_kind = CASE WHEN metadata_seed_author = 'user' \
                             THEN seed_kind ELSE :seed_kind END, \
                         seed_source = CASE WHEN metadata_seed_author = 'user' \
                             THEN seed_source ELSE :seed_source END, \
                         seed_release_id = CASE WHEN metadata_seed_author = 'user' \
                             THEN seed_release_id ELSE :seed_release_id END, \
                         metadata_seed_author = CASE \
                             WHEN metadata_seed_author = 'user' THEN 'user' \
                             WHEN :seed_kind IS NULL THEN NULL \
                             ELSE 'identification' END \
                     WHERE content_hash = :content_hash AND edit_revision = :expected",
                    named_params! {
                        ":folder_path": verdict.folder_path,
                        ":kind": columns.kind,
                        ":track_count": columns.track_count,
                        ":matched_barcode": columns.matched_barcode,
                        ":probed": probed,
                        ":now": now,
                        ":seed_kind": pick.as_ref().map(|pick| pick.kind),
                        ":seed_source": pick.as_ref().and_then(|pick| pick.source),
                        ":seed_release_id": pick.as_ref().and_then(|pick| pick.release_id),
                        ":content_hash": verdict.content_hash,
                        ":expected": expected,
                    },
                )? == 1
            } else {
                sql.execute(
                    "INSERT INTO import_candidate_state \
                         (content_hash, folder_path, verdict_kind, verdict_track_count, \
                          verdict_matched_barcode, probed_total_duration_ms, identified_at, \
                          seed_kind, seed_source, seed_release_id, metadata_seed_author, \
                          edit_revision) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
                    params![
                        verdict.content_hash,
                        verdict.folder_path,
                        columns.kind,
                        columns.track_count,
                        columns.matched_barcode,
                        probed,
                        now,
                        pick.as_ref().map(|pick| pick.kind),
                        pick.as_ref().and_then(|pick| pick.source),
                        pick.as_ref().and_then(|pick| pick.release_id),
                        pick.as_ref()
                            .map(|_| MetadataSeedAuthor::Identification.as_str()),
                    ],
                )? == 1
            };
            if wrote {
                // The result lists belong to the verdict that named them: the
                // superseded ones go in the same transaction that replaces it.
                delete_matches(sql, &verdict.content_hash)?;
                insert_matches(sql, &verdict.content_hash, &verdict.verdict)?;
                if let Some(superseded) = superseded {
                    if superseded.identity != pick_identity(verdict.metadata_seed.as_ref()) {
                        clear_pick_dependent_rows(sql, &verdict.content_hash)?;
                    }
                }
                delete_durations(sql, &verdict.content_hash)?;
                insert_durations(sql, &verdict.content_hash, &verdict.signals.durations)?;
                delete_signals(sql, &verdict.content_hash)?;
                insert_signals(sql, &verdict.content_hash, &verdict.signals)?;
            }
            Ok(wrote)
        })
        .await
    }

    /// Record one candidate's user-set file decisions, **and clear whatever
    /// identification had concluded about it**, in one transaction.
    ///
    /// The two are one operation, not two: binding a sheet or taking a file out
    /// of the tracklist changes what the folder is — a one-track image becomes
    /// a twelve-track disc, and its disc ID becomes computable — so the stored
    /// verdict was derived from a shape that no longer exists. Writing the
    /// decision without clearing it would leave the queue believing an answer
    /// to a question that changed.
    ///
    /// A pick identification concluded from that verdict goes with it, for the
    /// same reason. A pick a person made stays: their choice names a release,
    /// not a shape, and the mapping re-derives against the reshaped folder.
    ///
    /// The content hash covers files, never role decisions, so this addresses
    /// the same row the verdict lived in rather than orphaning it — and the
    /// scanned candidates that share the hash have their file rows rewritten
    /// to the settled shape in the same transaction.
    pub async fn save_import_candidate_file_edits(
        &self,
        content_hash: &str,
        folder_path: &str,
        expected_revision: u64,
        edits: &CandidateFileEdits,
        settled_candidates: &[(String, crate::import::folder_scanner::CategorizedFiles)],
    ) -> Result<(u64, Vec<crate::import::folder_scanner::FolderCandidate>), DbError> {
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let edits = edits.clone();
        let settled_candidates = settled_candidates.to_vec();
        let next_revision = expected_revision.checked_add(1).ok_or_else(|| {
            DbError::Message("candidate edit revision exhausted the u64 range".to_string())
        })?;
        let expected_revision_i64 = i64::try_from(expected_revision).map_err(|_| {
            DbError::Message(format!(
                "candidate edit revision {expected_revision} exceeds SQLite's integer range"
            ))
        })?;
        let next_revision_i64 = i64::try_from(next_revision).map_err(|_| {
            DbError::Message(format!(
                "candidate edit revision {next_revision} exceeds SQLite's integer range"
            ))
        })?;
        self.call(move |sql| {
            let settled_by_key: HashMap<_, _> = settled_candidates.iter().cloned().collect();
            if settled_by_key.len() != settled_candidates.len() {
                return Err(DbError::Message(
                    "candidate file decision received duplicate scan entry keys".to_string(),
                ));
            }
            let current: Option<i64> = sql
                .query_row(
                    "SELECT edit_revision FROM import_candidate_state WHERE content_hash = ?",
                    [&content_hash],
                    |row| row.get(0),
                )
                .optional()?;
            let current_revision = current
                .map(|value| {
                    u64::try_from(value).map_err(|_| {
                        DbError::Message(format!(
                            "candidate row {content_hash} has negative edit_revision"
                        ))
                    })
                })
                .transpose()?;
            if current_revision != Some(expected_revision)
                && !(current_revision.is_none() && expected_revision == 0)
            {
                return Err(DbError::Message(format!(
                    "candidate file decisions changed at revision {expected_revision}"
                )));
            }
            let changed = if current_revision.is_some() {
                sql.execute(
                    "UPDATE import_candidate_state SET \
                         folder_path = ?, \
                         verdict_kind = NULL, verdict_track_count = NULL, \
                         verdict_matched_barcode = NULL, \
                         probed_total_duration_ms = NULL, identified_at = NULL, \
                         seed_kind = CASE WHEN metadata_seed_author = 'user' \
                             THEN seed_kind ELSE NULL END, \
                         seed_source = CASE WHEN metadata_seed_author = 'user' \
                             THEN seed_source ELSE NULL END, \
                         seed_release_id = CASE WHEN metadata_seed_author = 'user' \
                             THEN seed_release_id ELSE NULL END, \
                         metadata_seed_author = CASE \
                             WHEN metadata_seed_author = 'user' THEN 'user' \
                             ELSE NULL END, \
                         edit_revision = ? \
                     WHERE content_hash = ? AND edit_revision = ?",
                    params![
                        folder_path,
                        next_revision_i64,
                        content_hash,
                        expected_revision_i64,
                    ],
                )?
            } else {
                sql.execute(
                    "INSERT INTO import_candidate_state \
                         (content_hash, folder_path, edit_revision) VALUES (?, ?, ?)",
                    params![content_hash, folder_path, next_revision_i64],
                )?
            };
            if changed != 1 {
                return Err(DbError::Message(format!(
                    "candidate file decision write changed {changed} rows; expected exactly one"
                )));
            }
            delete_matches(sql, &content_hash)?;
            // A binding change carves a different container, so every slice
            // measurement describes a shape that is gone; a whole file's own
            // length is a fact about its bytes and the hash covers those, so
            // it stays. The disc ID is recomputed for the same reason the
            // verdict is cleared, which takes the signals with it. And the
            // table's rows are a different set now, so the row edits addressed
            // them by identities that no longer mean the same thing.
            delete_slice_durations(sql, &content_hash)?;
            delete_signals(sql, &content_hash)?;
            pane_rows::delete_track_edits(sql, &content_hash)?;
            delete_file_edits(sql, &content_hash)?;
            insert_file_edits(sql, &content_hash, &edits)?;
            let updated_candidates = settle_scanned_candidates(
                sql,
                &content_hash,
                expected_revision_i64,
                next_revision_i64,
                &settled_by_key,
            )?;
            Ok((next_revision, updated_candidates))
        })
        .await
    }

    /// Every candidate's user-set file decisions, keyed by `content_hash` — the
    /// shape a folder scan takes so the roles it reports are the ones the user
    /// settled, not only the ones its filenames propose.
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

    /// Record the identity the user chose for one candidate, keyed by
    /// `content_hash` — the pressing they picked, or the decision to read the
    /// folder's own tags. An upsert that leaves the row's other halves alone:
    /// a choice can precede identification (a manual pick on a folder nothing
    /// matched) and must not erase a verdict or a file decision.
    ///
    /// It is recorded as the person's, which is what keeps a later run's
    /// verdict from revising it.
    pub async fn save_candidate_metadata_seed(
        &self,
        content_hash: &str,
        folder_path: &str,
        pick: &crate::import::MetadataSeed,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let pick = pick.clone();
        self.call(move |sql| {
            let columns = seed_columns(&pick);
            // An edit to release A's year is not an edit to release B's, and
            // the table's row identities mean different tracks under a
            // different release. Re-picking the same release keeps them.
            let replaced = stored_pick(sql, &content_hash)?
                .is_some_and(|stored| stored.identity != pick_identity(Some(&pick)));
            let changed = sql.execute(
                "INSERT INTO import_candidate_state \
                     (content_hash, folder_path, seed_kind, seed_source, seed_release_id, \
                      metadata_seed_author) \
                 VALUES (?, ?, ?, ?, ?, ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     folder_path = excluded.folder_path, \
                     seed_kind = excluded.seed_kind, \
                     seed_source = excluded.seed_source, \
                     seed_release_id = excluded.seed_release_id, \
                     metadata_seed_author = excluded.metadata_seed_author",
                params![
                    content_hash,
                    folder_path,
                    columns.kind,
                    columns.source,
                    columns.release_id,
                    MetadataSeedAuthor::User.as_str()
                ],
            )?;
            if changed != 1 {
                return Err(DbError::Message(format!(
                    "metadata seed write changed {changed} rows; expected exactly one"
                )));
            }
            if replaced {
                clear_pick_dependent_rows(sql, &content_hash)?;
            }
            Ok(())
        })
        .await
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
        self.read(move |sql| load_import_candidate_states_on(&sql))
            .await
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

/// What a candidate's pick names, as the three columns that say which release
/// it is. The claim is deliberately left out: lowering a claim on the same
/// release is not picking a different release.
type PickIdentity = (String, Option<String>, Option<String>);

/// The pick a candidate row holds, with who made it.
struct StoredPick {
    author: MetadataSeedAuthor,
    identity: Option<PickIdentity>,
}

fn pick_identity(pick: Option<&crate::import::MetadataSeed>) -> Option<PickIdentity> {
    pick.map(|pick| {
        let columns = seed_columns(pick);
        (
            columns.kind.to_string(),
            columns.source.map(str::to_string),
            columns.release_id.map(str::to_string),
        )
    })
}

fn stored_pick(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
) -> Result<Option<StoredPick>, DbError> {
    struct PickRow {
        kind: Option<String>,
        source: Option<String>,
        release_id: Option<String>,
        author: Option<String>,
    }
    let row = sql
        .query_row(
            "SELECT seed_kind, seed_source, seed_release_id, metadata_seed_author \
             FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |row| {
                Ok(PickRow {
                    kind: row.get(0)?,
                    source: row.get(1)?,
                    release_id: row.get(2)?,
                    author: row.get(3)?,
                })
            },
        )
        .optional()?;
    let Some(PickRow {
        kind,
        source,
        release_id,
        author: Some(author),
    }) = row
    else {
        return Ok(None);
    };
    let author = match author.as_str() {
        "user" => MetadataSeedAuthor::User,
        "identification" => MetadataSeedAuthor::Identification,
        other => {
            return Err(DbError::Message(format!(
                "import candidate column metadata_seed_author holds {other:?}"
            )))
        }
    };
    Ok(Some(StoredPick {
        author,
        identity: kind.map(|kind| (kind, source, release_id)),
    }))
}

/// What belongs to a pick and not to the folder: the metadata typed over the
/// picked release, the row edits addressed by its track identities, and remote
/// art offered by that release. A local cover remains valid across seeds.
fn clear_pick_dependent_rows(sql: &SqlContext<'_, '_>, content_hash: &str) -> Result<(), DbError> {
    pane_rows::delete_edit(sql, content_hash)?;
    pane_rows::delete_track_edits(sql, content_hash)?;
    pane_rows::delete_remote_cover(sql, content_hash)?;
    Ok(())
}

/// Rewrite the file rows of every scanned candidate at `content_hash` and
/// `expected_revision` to the shape the caller settled, and hand the settled
/// candidates back.
///
/// The caller's set must cover exactly those candidates: it computed the
/// settled files from the same read, so a candidate it did not settle is a
/// scan that moved under it, and writing half the set would leave two rows of
/// one release disagreeing about what its files are.
fn settle_scanned_candidates(
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
            "UPDATE scan_candidate SET file_edit_revision = ?, format_label = ? \
             WHERE watched_folder_path = ? AND path = ? AND file_edit_revision = ?",
            params![
                next_revision,
                settled.format_label,
                watched_folder_path,
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

pub(super) fn load_import_candidate_states_on(
    sql: &SqlReadContext<'_>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
    load_states_on(sql, None)
}
