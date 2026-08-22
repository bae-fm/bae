use super::*;

mod rows;
use super::folder_scans::validate_scan_item_ownership;
use rows::*;

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
            let entry_keys = sql.query(
                "SELECT entry_key FROM folder_scan_entries \
                 WHERE watched_folder_path = ? ORDER BY entry_key",
                [&path],
                |row| row.get::<_, String>(0),
            )?;
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
    ) -> Result<(u64, Vec<crate::import::folder_scanner::FolderCandidate>), DbError> {
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
            let mut updated_candidates = Vec::with_capacity(settled_by_key.len());
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
                updated_candidates.push(candidate.clone());
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
        self.read(move |sql| load_import_candidate_states_on(&sql))
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

pub(super) fn load_import_candidate_states_on(
    sql: &SqlReadContext<'_>,
) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
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
}
