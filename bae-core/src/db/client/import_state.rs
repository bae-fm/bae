use super::*;

mod edit_rows;
mod failure_rows;
mod import_commit;
mod lookup_choice_rows;
mod pane_rows;
mod preparation_rows;
mod prepared_asset_rows;
mod rows;
mod session_rows;
mod signal_rows;
mod verdict_rows;
mod watched_folder_removal;

use edit_rows::{delete_file_edits, insert_file_edits};
use failure_rows::load_failure_on;
pub(super) use import_commit::require_import_commit_guard;
pub(super) use pane_rows::{
    author_of, insert_draft, load_album_artist_assignments_on, load_covers_on, load_pane_rows_on,
};
pub(crate) use preparation_rows::{
    CandidateLookupUpdate, CandidatePaneWrite, CandidateSaveExpectation,
    CandidateSaveExtras, CandidateSaved, CandidateScanExpectation, ScannedCandidateKey,
};
pub(super) use rows::{
    load_candidate_file_edits_on, load_matches_on, load_provenance_on, load_states_on,
};
use session_rows::load_session_on;
use signal_rows::{delete_signals, insert_signals};

use crate::import::folder_scanner::{
    CandidateFileEdits, FolderReleaseDecision, FolderReleaseDecisionAuthor,
    FolderReleaseDecisionKey, FolderReleaseDecisions, StoredCandidateEdits,
};
use rows::{insert_provenance, load_states_rows_on};
use std::collections::HashSet;
use verdict_rows::{delete_verdict, insert_verdict};

impl Database {
    /// Whether a draft is already stored for `content_hash`. A candidate that
    /// has one is never re-seeded — a rescan re-reads files, not decisions —
    /// so this is what the pre-fill asks before reading any tags.
    pub(crate) async fn candidate_has_draft(&self, content_hash: &str) -> Result<bool, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| {
            Ok(sql
                .query_row(
                    "SELECT 1 FROM import_candidate_edit WHERE content_hash = ?",
                    [&content_hash],
                    |_| Ok(()),
                )
                .optional()?
                .is_some())
        })
        .await
    }
}

/// Seed a candidate's stored draft from the folder's own file tags: the draft
/// they project and the provenance naming them as its source, written by
/// discovery rather than by anyone who looked at it. The cover the tags embed
/// is stored with the folder's own cover, beside this.
/// Store `edits` as every file decision held for `content_hash`, in place of
/// whatever was held.
pub(super) fn store_file_edits(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    edits: &CandidateFileEdits,
) -> Result<(), DbError> {
    delete_file_edits(sql, content_hash)?;
    insert_file_edits(sql, content_hash, edits)
}

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
            crate::import::watched_folder::validate_relative_path(path)?;
        }
        Ok(paths.into_iter().collect())
    }

    /// Every watched root, in the order they were added.
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
    ///
    /// The overlap check reads the roots inside the write that inserts, so
    /// the check and the insert are one decision: two overlapping folders
    /// added at once cannot both pass a check that saw neither. The answer
    /// that the folder is already watched writes nothing, so it is read
    /// first; a folder watched in between refuses the write instead, and
    /// asking again answers `false`.
    pub async fn add_watched_import_folder(&self, path: &str) -> Result<bool, DbError> {
        // Keyed by the one spelling this host stores, so two spellings of one
        // folder can never become two rows.
        let path = crate::import::watched_folder::canonical_absolute_root(path)?;
        if self.watched_import_roots().await?.contains(&path) {
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

    pub async fn set_import_candidate_skipped(
        &self,
        watched_folder_path: &str,
        relative_candidate_path: &str,
        skipped: bool,
    ) -> Result<bool, DbError> {
        crate::import::watched_folder::validate_relative_path(relative_candidate_path)?;
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
    /// Store the reading a scan settled on for one folder, under the grouping
    /// key the scan gave it, without disturbing the scan that produced it: no
    /// generation bump, no re-scan. Refused when the folder already reads
    /// some stored way — the scan proposes only where nothing is stored.
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

    /// How every folder below one watched root reads, where a reading is
    /// stored.
    pub async fn load_folder_release_decisions(
        &self,
        watched_folder_path: &str,
    ) -> Result<FolderReleaseDecisions, DbError> {
        let watched_folder_path = watched_folder_path.to_string();
        self.read(move |sql| {
            let rows = sql.query(
                "SELECT anchor_relative_path, combined, author, key \
                 FROM release_grouping \
                 WHERE watched_folder_path = ? AND anchor_relative_path IS NOT NULL",
                [watched_folder_path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?;
            let mut readings = HashMap::with_capacity(rows.len());
            for (path, combined, author, grouping) in rows {
                crate::import::watched_folder::validate_relative_path(&path)?;
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
        self.read(move |sql| load_states_rows_on(&sql, None))
            .process(|process| {
                Ok(StoredCandidateEdits::new(
                    process()?
                        .into_iter()
                        .map(|(hash, state)| (hash, state.file_edits))
                        .collect(),
                ))
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
            .process(|process| process())
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
        self.read(move |sql| load_states_rows_on(&sql, None))
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
                load_states_rows_on(&sql, Some(&content_hash))?,
                content_hash,
            ))
        })
        .process(|(process, content_hash)| Ok(process()?.remove(&content_hash)))
        .await
    }
}

/// Apply source decisions to every candidate sharing the preparation: each
/// takes its settled files, and every grouping built from one of them is
/// rebuilt. Every row must still have the revision the caller prepared
/// against.
pub(super) fn settle_scanned_candidates(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    expected_revision: i64,
    next_revision: i64,
    settled_by_key: &HashMap<String, crate::import::folder_scanner::CategorizedFiles>,
    observed_at: i64,
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
                // The file-tag reading names the files it was read from, so
                // taking the file rows away takes it too. The decision
                // changed which files are tracks, not the files, so the
                // reading is laid back down over the settled rows as it was —
                // with the revision it was taken at, which is no longer the
                // candidate's.
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
        let stored =
            super::release_groupings::load_candidate_on(sql, &path)?.ok_or_else(|| {
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
    // A release a grouping built from one of these is built again, with the
    // files it now holds.
    let settled: Vec<String> = updated_folders.into_iter().collect();
    let regrouped = super::release_groupings::rebuild_groupings(sql, &settled, observed_at)?;
    updated_candidates.extend(regrouped.written.into_iter().filter_map(|item| match item {
        crate::import::folder_scanner::ScanItem::Valid(candidate) => Some(candidate),
        _ => None,
    }));
    Ok(updated_candidates)
}
