use super::*;

/// Who decided a candidate's stored identity pick. The two outlive different
/// things — identification's goes with the verdict that concluded it, a
/// person's outlives every verdict — so the row records which it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickAuthor {
    User,
    Identification,
}

impl PickAuthor {
    /// The stored `identity_pick_author` value.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Identification => "identification",
        }
    }
}
use crate::import::folder_scanner::{
    CandidateFileEdits, FolderReleaseDecision, FolderReleaseDecisionKey, FolderReleaseDecisions,
    StoredCandidateEdits,
};
use std::collections::HashSet;

fn next_folder_scan_generation(sql: &SqlContext<'_, '_>) -> Result<i64, DbError> {
    let current: i64 = sql.query_row(
        "SELECT last_generation FROM folder_scan_generation_sequence WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    let generation = current
        .checked_add(1)
        .ok_or_else(|| DbError::Message("folder scan generation exhausted".to_string()))?;
    sql.execute(
        "UPDATE folder_scan_generation_sequence SET last_generation = ? WHERE singleton = 1",
        [generation],
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
    pub async fn remove_watched_import_folder(&self, path: &str) -> Result<bool, DbError> {
        let path = Self::canonical_watched_root(path)?;
        if !self.watched_import_roots().await?.contains(&path) {
            return Ok(false);
        }
        self.call(move |sql| {
            Ok(sql.execute("DELETE FROM watched_import_folders WHERE path = ?", [&path])? == 1)
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

    /// Start a durable scan generation for one watched root.
    pub async fn begin_folder_scan(&self, watched_folder_path: &str) -> Result<u64, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.call(move |sql| {
            let generation = next_folder_scan_generation(sql)?;
            sql.execute(
                "INSERT INTO folder_scan_roots \
                     (watched_folder_path, generation, status, error) \
                 VALUES (?, ?, 'scanning', NULL) \
                 ON CONFLICT(watched_folder_path) DO UPDATE SET \
                     generation = excluded.generation, status = 'scanning', error = NULL",
                params![watched_folder_path, generation],
            )?;
            u64::try_from(generation)
                .map_err(|_| DbError::Message("folder scan generation is negative".to_string()))
        })
        .await
    }

    /// Persist one progressive scan result before it is exposed to consumers.
    ///
    /// `removed_keys` are entries this item supersedes in the same in-memory
    /// transition. The generation check and all changes share one transaction,
    /// so a cancelled scan cannot write over its successor.
    pub async fn save_folder_scan_item(
        &self,
        watched_folder_path: &str,
        generation: u64,
        item: &crate::import::folder_scanner::ScanItem,
        removed_keys: &[String],
    ) -> Result<bool, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let generation = i64::try_from(generation).map_err(|_| {
            DbError::Message("folder scan generation exceeds SQLite's integer range".to_string())
        })?;
        let entry_key = item.persisted_key();
        validate_scan_item_ownership(&watched_folder_path, &entry_key, item)?;
        let item = serde_json::to_string(item)
            .map_err(|error| DbError::Message(format!("encoding folder scan item: {error}")))?;
        let removed_keys = removed_keys.to_vec();
        self.call(move |sql| {
            let current: Option<i64> = sql
                .query_row(
                    "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
                    [&watched_folder_path],
                    |row| row.get(0),
                )
                .optional()?;
            if current != Some(generation) {
                return Ok(false);
            }
            for removed_key in removed_keys {
                sql.execute(
                    "DELETE FROM folder_scan_entries \
                     WHERE watched_folder_path = ? AND entry_key = ?",
                    params![watched_folder_path, removed_key],
                )?;
            }
            sql.execute(
                "INSERT INTO folder_scan_entries \
                     (watched_folder_path, entry_key, generation, item) VALUES (?, ?, ?, ?) \
                 ON CONFLICT(watched_folder_path, entry_key) DO UPDATE SET \
                     generation = excluded.generation, item = excluded.item",
                params![watched_folder_path, entry_key, generation, item],
            )?;
            Ok(true)
        })
        .await
    }

    /// Finish one scan generation. Successful completion removes entries not
    /// observed in this generation; failure preserves them.
    pub async fn finish_folder_scan(
        &self,
        watched_folder_path: &str,
        generation: u64,
        error: Option<&str>,
    ) -> Result<bool, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        let generation = i64::try_from(generation).map_err(|_| {
            DbError::Message("folder scan generation exceeds SQLite's integer range".to_string())
        })?;
        let error = error.map(str::to_string);
        // A generation that is no longer the root's finishes nothing, and
        // finding that out is a read — the write below opens only once this
        // generation is the one in force. Its statements still carry the
        // generation, so a later one taking over between the two writes
        // nothing rather than finishing the wrong scan.
        let current: Option<i64> = {
            let watched_folder_path = watched_folder_path.clone();
            self.read(move |sql| {
                Ok(sql
                    .query_row(
                        "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
                        [&watched_folder_path],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await?
        };
        if current != Some(generation) {
            return Ok(false);
        }
        self.call(move |sql| {
            if let Some(error) = error {
                sql.execute(
                    "UPDATE folder_scan_roots SET status = 'failed', error = ? \
                     WHERE watched_folder_path = ? AND generation = ?",
                    params![error, watched_folder_path, generation],
                )?;
            } else {
                sql.execute(
                    "DELETE FROM folder_scan_entries \
                     WHERE watched_folder_path = ? AND generation != ?",
                    params![watched_folder_path, generation],
                )?;
                sql.execute(
                    "UPDATE folder_scan_roots SET status = 'complete', error = NULL \
                     WHERE watched_folder_path = ? AND generation = ?",
                    params![watched_folder_path, generation],
                )?;
            }
            Ok(true)
        })
        .await
    }

    pub async fn load_folder_scan_snapshots(&self) -> Result<Vec<DbFolderScanSnapshot>, DbError> {
        self.read(move |sql| {
            let roots = sql.query(
                "SELECT watched_folder_path, generation, status, error \
                     FROM folder_scan_roots ORDER BY watched_folder_path",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )?;

            let mut snapshots = Vec::with_capacity(roots.len());
            for (watched_folder_path, generation, status, error) in roots {
                let generation = u64::try_from(generation).map_err(|_| {
                    DbError::Message(format!(
                        "folder scan root {watched_folder_path} has a negative generation"
                    ))
                })?;
                let status = match (status.as_str(), error) {
                    ("scanning", None) => crate::import::FolderScanStatus::Scanning,
                    ("complete", None) => crate::import::FolderScanStatus::Complete,
                    ("failed", Some(error)) => {
                        crate::import::FolderScanStatus::Failed { error }
                    }
                    (status, error) => {
                        return Err(DbError::Message(format!(
                            "folder scan root {watched_folder_path} has invalid status {status:?} and error {error:?}"
                        )))
                    }
                };
                let entries = sql.query(
                    "SELECT entry_key, generation, item FROM folder_scan_entries \
                     WHERE watched_folder_path = ? ORDER BY entry_key",
                    [&watched_folder_path],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )?;
                let mut items = Vec::with_capacity(entries.len());
                for (entry_key, entry_generation, stored) in entries {
                    let entry_generation = u64::try_from(entry_generation).map_err(|_| {
                        DbError::Message(format!(
                            "folder scan entry {entry_key} has a negative generation"
                        ))
                    })?;
                    if entry_generation > generation {
                        return Err(DbError::Message(format!(
                            "folder scan entry {entry_key} has generation {entry_generation} newer than root generation {generation}"
                        )));
                    }
                    let item: crate::import::folder_scanner::ScanItem =
                        serde_json::from_str(&stored).map_err(|error| {
                        DbError::Message(format!(
                            "folder scan entry {entry_key} under {watched_folder_path} is unreadable: {error}"
                        ))
                    })?;
                    validate_scan_item_ownership(&watched_folder_path, &entry_key, &item)?;
                    items.push(item);
                }
                snapshots.push(DbFolderScanSnapshot {
                    watched_folder_path,
                    generation,
                    status,
                    items,
                });
            }
            Ok(snapshots)
        })
        .await
    }

    /// Set one watched folder's interpretation atomically and idempotently.
    pub async fn set_folder_release_decision(
        &self,
        key: &FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
    ) -> Result<u64, DbError> {
        let (generation, _) = self
            .set_folder_release_decisions(&[(key.clone(), decision)])
            .await?;
        Ok(generation)
    }

    pub async fn set_folder_release_decisions(
        &self,
        decisions: &[(FolderReleaseDecisionKey, FolderReleaseDecision)],
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
        self.call(move |sql| {
            let stored_items = sql.query(
                "SELECT entry_key, item FROM folder_scan_entries \
                 WHERE watched_folder_path = ? ORDER BY entry_key",
                [&watched_folder_path],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            let persisted_items = stored_items
                .into_iter()
                .map(|(entry_key, stored)| {
                    let item: crate::import::folder_scanner::ScanItem =
                        serde_json::from_str(&stored).map_err(|error| {
                            DbError::Message(format!(
                                "folder scan entry {entry_key} under \
                                 {watched_folder_path} is unreadable: {error}"
                            ))
                        })?;
                    validate_scan_item_ownership(&watched_folder_path, &entry_key, &item)?;
                    Ok(item)
                })
                .collect::<Result<Vec<_>, DbError>>()?;
            let persisted_keys = persisted_items
                .iter()
                .map(crate::import::folder_scanner::ScanItem::persisted_key)
                .collect();
            let removed_scan_entry_keys =
                crate::import::folder_scanner::release_decision_removed_keys(
                    &persisted_keys,
                    &decisions,
                );

            for (key, decision) in &decisions {
                let decision = match decision {
                    FolderReleaseDecision::CombineAsOneRelease => "combine_as_one_release",
                    FolderReleaseDecision::KeepAsSeparateReleases => "keep_as_separate_releases",
                };
                sql.execute(
                    "INSERT INTO folder_release_decisions \
                         (watched_folder_path, relative_folder_path, decision) VALUES (?, ?, ?) \
                     ON CONFLICT(watched_folder_path, relative_folder_path) DO UPDATE SET \
                         decision = excluded.decision",
                    params![key.watched_folder_path, key.relative_folder_path, decision],
                )?;
            }
            for entry_key in &removed_scan_entry_keys {
                sql.execute(
                    "DELETE FROM folder_scan_entries \
                     WHERE watched_folder_path = ? AND entry_key = ?",
                    params![watched_folder_path, entry_key],
                )?;
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

    /// Every explicit interpretation below one watched root.
    pub async fn load_folder_release_decisions(
        &self,
        watched_folder_path: &str,
    ) -> Result<FolderReleaseDecisions, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| {
            let decisions = sql.query(
                "SELECT relative_folder_path, decision \
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
                    Ok((path, decision))
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
            // A pick identification made belongs to the verdict that made it,
            // so this write replaces it — with the new verdict's own
            // conclusion, or with nothing when it concluded none. A pick a
            // person made is theirs and is left exactly as it is: a run whose
            // signals turn up nothing says nothing about a release they chose.
            let wrote = if update_existing {
                sql.execute(
                    "UPDATE import_candidate_state SET \
                         folder_path = :folder_path, verdict = :verdict, \
                         probed_total_duration_ms = :probed, identified_at = :now, \
                         identity_pick = CASE \
                             WHEN identity_pick_author = 'user' THEN identity_pick \
                             ELSE :pick END, \
                         identity_pick_author = CASE \
                             WHEN identity_pick_author = 'user' THEN 'user' \
                             WHEN :pick IS NULL THEN NULL \
                             ELSE 'identification' END \
                     WHERE content_hash = :content_hash AND edit_revision = :expected",
                    named_params! {
                        ":folder_path": verdict.folder_path,
                        ":verdict": verdict.verdict,
                        ":probed": verdict.probed_total_duration_ms,
                        ":now": now,
                        ":pick": verdict.identity_pick,
                        ":content_hash": verdict.content_hash,
                        ":expected": expected,
                    },
                )? == 1
            } else {
                sql.execute(
                    "INSERT INTO import_candidate_state \
                         (content_hash, folder_path, verdict, probed_total_duration_ms, identified_at, identity_pick, identity_pick_author, sheet_bindings, file_roles, sheet_discs, edit_revision) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, '{}', '{}', '{}', 0)",
                    params![
                        verdict.content_hash,
                        verdict.folder_path,
                        verdict.verdict,
                        verdict.probed_total_duration_ms,
                        now,
                        verdict.identity_pick,
                        verdict
                            .identity_pick
                            .as_ref()
                            .map(|_| PickAuthor::Identification.as_str()),
                    ],
                )? == 1
            };
            Ok(wrote)
        })
        .await
    }

    /// Record one candidate's user-set file decisions, **and clear whatever
    /// identification had concluded about it**, in one statement.
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
    /// the same row the verdict lived in rather than orphaning it.
    pub async fn save_import_candidate_file_edits(
        &self,
        content_hash: &str,
        folder_path: &str,
        expected_revision: u64,
        edits: &CandidateFileEdits,
        settled_candidates: &[(String, crate::import::folder_scanner::CategorizedFiles)],
    ) -> Result<u64, DbError> {
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let bindings = serde_json::to_string(&edits.sheet_bindings)
            .map_err(|e| DbError::Message(format!("encoding candidate sheet bindings: {e}")))?;
        let roles = serde_json::to_string(&edits.file_roles)
            .map_err(|e| DbError::Message(format!("encoding candidate file roles: {e}")))?;
        let discs = serde_json::to_string(&edits.sheet_discs)
            .map_err(|e| DbError::Message(format!("encoding candidate sheet discs: {e}")))?;
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
                         folder_path = ?, sheet_bindings = ?, file_roles = ?, sheet_discs = ?, \
                         verdict = NULL, probed_total_duration_ms = NULL, identified_at = NULL, \
                         identity_pick = CASE \
                             WHEN identity_pick_author = 'user' THEN identity_pick \
                             ELSE NULL END, \
                         identity_pick_author = CASE \
                             WHEN identity_pick_author = 'user' THEN 'user' \
                             ELSE NULL END, \
                         edit_revision = ? \
                     WHERE content_hash = ? AND edit_revision = ?",
                    params![
                        folder_path,
                        bindings,
                        roles,
                        discs,
                        next_revision_i64,
                        content_hash,
                        expected_revision_i64,
                    ],
                )?
            } else {
                sql.execute(
                    "INSERT INTO import_candidate_state \
                         (content_hash, folder_path, verdict, probed_total_duration_ms, identified_at, sheet_bindings, file_roles, sheet_discs, edit_revision) \
                     VALUES (?, ?, NULL, NULL, NULL, ?, ?, ?, ?)",
                    params![
                        content_hash,
                        folder_path,
                        bindings,
                        roles,
                        discs,
                        next_revision_i64
                    ],
                )?
            };
            if changed != 1 {
                return Err(DbError::Message(format!(
                    "candidate file decision write changed {changed} rows; expected exactly one"
                )));
            }
            let stored_items = sql.query(
                "SELECT watched_folder_path, entry_key, generation, item \
                 FROM folder_scan_entries ORDER BY watched_folder_path, entry_key",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?;
            let mut updated_keys = HashSet::new();
            for (watched_folder_path, entry_key, generation, stored) in stored_items {
                let mut item: crate::import::folder_scanner::ScanItem =
                    serde_json::from_str(&stored).map_err(|error| {
                        DbError::Message(format!(
                            "folder scan entry {entry_key} under {watched_folder_path} \
                             is unreadable: {error}"
                        ))
                    })?;
                validate_scan_item_ownership(&watched_folder_path, &entry_key, &item)?;
                let candidate = match &mut item {
                    crate::import::folder_scanner::ScanItem::Discovered(candidate)
                    | crate::import::folder_scanner::ScanItem::Valid(candidate) => candidate,
                    crate::import::folder_scanner::ScanItem::Invalid(_)
                    | crate::import::folder_scanner::ScanItem::Boundary(_) => continue,
                };
                if candidate.files.content_hash() != content_hash
                    || candidate.file_edit_revision != expected_revision
                {
                    continue;
                }
                let settled = settled_by_key.get(&entry_key).ok_or_else(|| {
                    DbError::Message(format!(
                        "persisted candidate {entry_key} was missing from the settled file edit"
                    ))
                })?;
                candidate.files = settled.clone();
                candidate.file_edit_revision = next_revision;
                let encoded = serde_json::to_string(&item).map_err(|error| {
                    DbError::Message(format!("encoding folder scan entry {entry_key}: {error}"))
                })?;
                let updated = sql.execute(
                    "UPDATE folder_scan_entries SET item = ? \
                     WHERE watched_folder_path = ? AND entry_key = ? AND generation = ?",
                    params![encoded, watched_folder_path, entry_key, generation],
                )?;
                if updated != 1 {
                    return Err(DbError::Message(format!(
                        "candidate file decision changed {updated} persisted scan entries for \
                         {entry_key}; expected exactly one"
                    )));
                }
                updated_keys.insert(entry_key);
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
            Ok(next_revision)
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
    pub async fn save_candidate_identity_pick(
        &self,
        content_hash: &str,
        folder_path: &str,
        pick_json: &str,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let folder_path = folder_path.to_string();
        let pick_json = pick_json.to_string();
        self.call(move |sql| {
            let changed = sql.execute(
                "INSERT INTO import_candidate_state \
                     (content_hash, folder_path, identity_pick, identity_pick_author) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     folder_path = excluded.folder_path, \
                     identity_pick = excluded.identity_pick, \
                     identity_pick_author = excluded.identity_pick_author",
                params![
                    content_hash,
                    folder_path,
                    pick_json,
                    PickAuthor::User.as_str()
                ],
            )?;
            if changed != 1 {
                return Err(DbError::Message(format!(
                    "identity pick write changed {changed} rows; expected exactly one"
                )));
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
        self.read(move |sql| {
            sql.query_row(
                "SELECT sheet_bindings, file_roles, sheet_discs, edit_revision \
                 FROM import_candidate_state WHERE content_hash = ?",
                [&content_hash],
                |row| Ok(decode_candidate_file_edits_row(row, &content_hash)),
            )
            .optional()?
            .unwrap_or_else(|| Ok(CandidateFileEdits::default()))
        })
        .await
    }

    /// Every stored `import_candidate_state` row, keyed by `content_hash`. The
    /// queue is small enough to read whole; callers classify in memory rather
    /// than filtering in SQL.
    ///
    /// A row whose identify columns are half-set, whose decisions do not decode,
    /// or whose revision is negative cannot come from either write path above.
    /// Loading fails rather than substituting a plausible candidate shape.
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
        self.read(move |sql| {
            let states = sql.query(
                "SELECT content_hash, folder_path, verdict, probed_total_duration_ms, identified_at, sheet_bindings, file_roles, sheet_discs, identity_pick, edit_revision \
                     FROM import_candidate_state",
                [],
                |row| Ok(decode_import_candidate_state_row(row)),
            )?;
            let mut out = HashMap::with_capacity(states.len());
            for state in states {
                let state = state?;
                out.insert(state.content_hash.clone(), state);
            }
            Ok(out)
        })
        .await
    }

    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<DbImportCandidateState>, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| {
            sql.query_row(
                "SELECT content_hash, folder_path, verdict, probed_total_duration_ms, identified_at, sheet_bindings, file_roles, sheet_discs, identity_pick, edit_revision \
                 FROM import_candidate_state WHERE content_hash = ?",
                [content_hash],
                |row| Ok(decode_import_candidate_state_row(row)),
            )
            .optional()?
            .transpose()
        })
        .await
    }
}

fn decode_import_candidate_state_row(
    row: &coven::rusqlite::Row<'_>,
) -> Result<DbImportCandidateState, DbError> {
    let content_hash: String = row.get("content_hash")?;
    let verdict: Option<String> = row.get("verdict")?;
    let probed_total_duration_ms: Option<i64> = row.get("probed_total_duration_ms")?;
    let identified_at: Option<String> = row.get("identified_at")?;
    let identify = match (verdict, probed_total_duration_ms, identified_at) {
        (Some(verdict), Some(probed_total_duration_ms), Some(_)) => {
            Some(DbCandidateIdentifyResult {
                verdict,
                probed_total_duration_ms,
                identified_at: rfc3339_column(row, "identified_at")?,
            })
        }
        (None, None, None) => None,
        _ => {
            return Err(DbError::Message(format!(
                "import_candidate_state row {content_hash} holds a half-written identify result"
            )));
        }
    };
    let file_edits = decode_candidate_file_edits_row(row, &content_hash)?;
    Ok(DbImportCandidateState {
        content_hash,
        folder_path: row.get("folder_path")?,
        identify,
        file_edits,
        identity_pick: row.get("identity_pick")?,
    })
}

fn decode_candidate_file_edits_row(
    row: &coven::rusqlite::Row<'_>,
    content_hash: &str,
) -> Result<CandidateFileEdits, DbError> {
    let stored: String = row.get("sheet_bindings")?;
    let sheet_bindings = serde_json::from_str(&stored).map_err(|error| {
        DbError::Message(format!(
            "import_candidate_state row {content_hash} has unreadable sheet bindings: {error}"
        ))
    })?;
    let stored: String = row.get("file_roles")?;
    let file_roles = serde_json::from_str(&stored).map_err(|error| {
        DbError::Message(format!(
            "import_candidate_state row {content_hash} has unreadable file roles: {error}"
        ))
    })?;
    let stored: String = row.get("sheet_discs")?;
    let sheet_discs = serde_json::from_str(&stored).map_err(|error| {
        DbError::Message(format!(
            "import_candidate_state row {content_hash} has unreadable sheet discs: {error}"
        ))
    })?;
    let edit_revision: i64 = row.get("edit_revision")?;
    let edit_revision = u64::try_from(edit_revision).map_err(|_| {
        DbError::Message(format!(
            "import_candidate_state row {content_hash} has negative edit_revision"
        ))
    })?;
    Ok(CandidateFileEdits {
        sheet_bindings,
        file_roles,
        sheet_discs,
        revision: edit_revision,
    })
}

fn validate_scan_item_ownership(
    watched_folder_path: &str,
    entry_key: &str,
    item: &crate::import::folder_scanner::ScanItem,
) -> Result<(), DbError> {
    if item.persisted_key() != entry_key {
        return Err(DbError::Message(format!(
            "folder scan entry key {entry_key} does not match its item key {}",
            item.persisted_key()
        )));
    }
    let root = std::path::Path::new(watched_folder_path);
    let (item_root, item_path) = match item {
        crate::import::folder_scanner::ScanItem::Discovered(candidate)
        | crate::import::folder_scanner::ScanItem::Valid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        crate::import::folder_scanner::ScanItem::Invalid(candidate) => (
            candidate.watched_folder_path.as_str(),
            candidate.path.as_path(),
        ),
        crate::import::folder_scanner::ScanItem::Boundary(boundary) => (
            boundary.key.watched_folder_path.as_str(),
            std::path::Path::new(entry_key),
        ),
    };
    if item_root != watched_folder_path || !item_path.starts_with(root) {
        return Err(DbError::Message(format!(
            "folder scan entry {entry_key} does not belong to watched folder {watched_folder_path}"
        )));
    }
    match item {
        crate::import::folder_scanner::ScanItem::Discovered(candidate)
        | crate::import::folder_scanner::ScanItem::Valid(candidate) => {
            if !candidate.file_root.starts_with(root) {
                return Err(DbError::Message(format!(
                    "folder scan entry {entry_key} reads files outside its watched folder"
                )));
            }
            for resolved in &candidate.resolved_boundaries {
                validate_decision_key_ownership(watched_folder_path, &resolved.key)?;
            }
            if let Some(key) = &candidate.combine_ancestor_key {
                validate_decision_key_ownership(watched_folder_path, key)?;
            }
        }
        crate::import::folder_scanner::ScanItem::Invalid(candidate) => {
            for resolved in &candidate.resolved_boundaries {
                validate_decision_key_ownership(watched_folder_path, &resolved.key)?;
            }
        }
        crate::import::folder_scanner::ScanItem::Boundary(_) => {}
    }
    if let crate::import::folder_scanner::ScanItem::Boundary(boundary) = item {
        validate_decision_key_ownership(watched_folder_path, &boundary.key)?;
        for row in &boundary.tree_rows {
            validate_decision_key_ownership(watched_folder_path, &row.decision_key)?;
            for ancestor in &row.ancestor_decision_keys {
                validate_decision_key_ownership(watched_folder_path, ancestor)?;
            }
        }
        if boundary
            .candidate_keys
            .iter()
            .any(|key| !std::path::Path::new(key).starts_with(root))
        {
            return Err(DbError::Message(format!(
                "folder scan boundary {entry_key} contains a candidate outside its watched folder"
            )));
        }
    }
    Ok(())
}

fn validate_decision_key_ownership(
    watched_folder_path: &str,
    key: &FolderReleaseDecisionKey,
) -> Result<(), DbError> {
    if key.watched_folder_path != watched_folder_path {
        return Err(DbError::Message(format!(
            "folder release decision belongs to {} instead of {watched_folder_path}",
            key.watched_folder_path
        )));
    }
    crate::import::folder_registry::validate_relative_path(&key.relative_folder_path)
        .map_err(|error| DbError::Message(error.to_string()))
}
