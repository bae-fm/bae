use super::*;
use bae_db::client::query::CandidateRows;

mod album_pick_rows;
mod draft_store;
mod edit_rows;
pub(super) mod failure_rows;
mod import_commit;
mod ledger_rows;
mod lookup_choice_rows;
mod lookup_failure_columns;
mod match_rows;
mod pane_rows;
mod preparation_rows;
mod prepared_asset_rows;
mod rows;
mod run_rows;
mod search_rows;
mod session_rows;
mod signal_rows;
mod standing_rows;
mod verdict_rows;

pub(super) use album_pick_rows::load_album_pick_on;
use edit_rows::{delete_file_edits, insert_file_edits};
use failure_rows::{delete_failure_on, load_failure_on};
pub use import_commit::ImportCommitError;
pub(crate) use import_commit::{ImportCommitGuard, ImportRows, NewArtistImages};
pub(super) use pane_rows::{cover_read_columns, insert_draft, load_pane_rows_on, read_cover};
pub(crate) use preparation_rows::{
    CandidatePaneWrite, CandidatePreparationRead, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, RunVerdictSaved, ScannedCandidateKey,
};
pub(super) use rows::{
    load_candidate_file_edits_on, load_states_on, read_provenance, read_release_link,
    PROVENANCE_COLUMNS, RELEASE_LINK_COLUMNS,
};
use session_rows::load_session_on;
use signal_rows::{delete_signals, store_signals};
pub(crate) use standing_rows::ImportClaim;
pub(super) use standing_rows::{
    change_identification_on, end_file_work_unless_of_on, load_standing_on, move_import_on,
    RuntimeFactColumns, RUNTIME_FACT_COLUMNS,
};

use super::watched_folders::relative_below;
use crate::folder_scanner::{
    CandidateFileEdits, FolderReleaseDecision, FolderReleaseDecisionAuthor,
    FolderReleaseDecisionKey, FolderReleaseDecisions, StoredCandidateEdits,
};
use crate::CandidateKey;
use rows::{insert_provenance, load_states_rows_on};
use std::collections::HashSet;
use std::path::Path;
use verdict_rows::{delete_verdict, store_verdict};

/// Move the pane inside a candidate write, without splitting the transaction.
pub(crate) fn move_pane_on(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    pane_move: crate::PaneMove,
) -> Result<(), DbError> {
    session_rows::move_pane_on(sql, content_hash, pane_move)
}

/// Replace every file decision held for `content_hash` with `edits`.
pub(super) fn store_file_edits(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    edits: &CandidateFileEdits,
) -> Result<(), DbError> {
    delete_file_edits(sql, content_hash)?;
    insert_file_edits(sql, content_hash, edits)
}

/// Seed a candidate's draft from the folder's file tags, authored by the
/// pre-fill, with provenance naming the tags as its source.
pub(crate) fn insert_file_tags_draft(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &crate::CandidateDraft,
) -> Result<(), DbError> {
    pane_rows::insert_draft(sql, content_hash, draft, crate::MetadataAuthor::Prefill)?;
    insert_provenance(sql, content_hash, &crate::MetadataProvenance::FileMetadata)?;
    Ok(())
}

pub(super) fn require_current_candidate(
    sql: &CovenWrite<'_>,
    watched_folder_path: &Path,
    candidate_path: &CandidateKey,
    content_hash: &str,
    expected_file_edit_revision: u64,
) -> Result<u64, DbError> {
    let current: Option<(String, i64, i64)> = sql
        .query_row(
            "SELECT content_hash, generation, file_edit_revision \
             FROM scan_candidate WHERE watched_folder_path = ? AND path = ? AND kind = 'valid'",
            params![coven::path_bytes(watched_folder_path), candidate_path],
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

/// The next scan generation. An upsert, so a missing seed row is created
/// rather than failing the scan.
pub(super) fn next_folder_scan_generation(sql: &CovenWrite<'_>) -> Result<i64, DbError> {
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

impl ImportTables {}

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
        self.database
            .call(move |sql| {
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
        reading: &crate::folder_scanner::FolderReading,
    ) -> Result<(), DbError> {
        let key = key.clone();
        let reading = reading.clone();
        self.database
            .call(move |sql| {
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
    ) -> Result<Option<CandidateKey>, DbError> {
        let anchor = coven::path_bytes(&bae_db::watched_path::folder_below(
            &folder.watched_folder_path,
            &folder.relative_folder,
        ));
        self.database
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
        let folder =
            bae_db::watched_path::folder_below(&key.watched_folder_path, &key.relative_folder);
        let (below_first, below_past) = bae_db::watched_path::paths_below(&folder);
        let folder = coven::path_bytes(&folder);
        let root = coven::path_bytes(&key.watched_folder_path);
        self.database
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
        let (below_first, below_past) = bae_db::watched_path::paths_below(folder);
        let (root, folder) = (coven::path_bytes(watched_folder_path), coven::path_bytes(folder));
        self.database
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
        let (below_first, below_past) = bae_db::watched_path::paths_below(&watched_folder_path);
        self.database
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
                    bae_db::watched_path::validate_stored_folder(&folder)?;
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

    /// Every candidate's user-set file decisions, keyed by `content_hash`, so a
    /// folder scan reports the roles the user settled.
    pub async fn load_stored_candidate_edits(&self) -> Result<StoredCandidateEdits, DbError> {
        self.database
            .read(move |sql| rows::load_stored_candidate_edits_on(&sql))
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
                                crate::folder_scanner::ScanItem::Discovered(candidate)
                                | crate::folder_scanner::ScanItem::Valid(candidate),
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
        self.database
            .read(move |sql| load_candidate_file_edits_on(&sql, &content_hash))
            .process(|process| process())
            .await
    }

    /// Every stored `import_candidate_state` row, keyed by `content_hash`, for
    /// the tests that read back everything a write stored. A column holding a
    /// value no writer produces fails the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn load_import_candidate_states(
        &self,
    ) -> Result<HashMap<String, DbImportCandidateState>, DbError> {
        self.database
            .read(move |sql| load_states_rows_on(&sql, CandidateRows::All))
            .process(|process| process())
            .await
    }

    pub async fn load_import_candidate_state(
        &self,
        content_hash: &str,
    ) -> Result<Option<DbImportCandidateState>, DbError> {
        let content_hash = content_hash.to_string();
        self.database
            .read(move |sql| {
                Ok((load_states_rows_on(&sql, CandidateRows::One(&content_hash))?, content_hash))
            })
            .process(|(process, content_hash)| Ok(process()?.remove(&content_hash)))
            .await
    }

    /// Whether a verdict is stored for the files at `content_hash` as their
    /// file decisions stand at `edit_revision`.
    pub(crate) async fn verdict_stored_for(
        &self,
        content_hash: &str,
        edit_revision: u64,
    ) -> Result<bool, DbError> {
        let content_hash = content_hash.to_string();
        let edit_revision = to_i64(edit_revision, "a candidate's file edit revision")?;
        self.database
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM import_candidate_state AS state \
                     JOIN import_candidate_verdict AS verdict \
                       ON verdict.content_hash = state.content_hash \
                     WHERE state.content_hash = ? AND state.edit_revision = ?)",
                    params![content_hash, edit_revision],
                    |row| row.get(0),
                )?)
            })
            .await
    }
}

/// Give every candidate sharing the preparation its settled files and rebuild
/// every grouping built from one of them. Every row must still be at the
/// revision the caller prepared against.
pub(super) fn settle_scanned_candidates(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    expected_revision: i64,
    next_revision: i64,
    settled_by_key: &HashMap<CandidateKey, crate::folder_scanner::CategorizedFiles>,
) -> Result<Vec<crate::folder_scanner::FolderCandidate>, DbError> {
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
                    params![coven::path_bytes(&watched_folder_path), path],
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
            other => return Err(DbError::Message(format!("unknown candidate source {other}"))),
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
        // The decision makes the candidate other files than its
        // identification was of, which ends with it.
        end_file_work_unless_of_on(
            sql,
            &path,
            Some(&crate::CandidateFiles {
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
            .map(CandidateKey::to_string)
            .collect();
        return Err(DbError::Message(format!(
            "candidate file decision could not update persisted scan entries: {}",
            missing.join(", ")
        )));
    }
    let settled: Vec<CandidateKey> = updated_folders.into_iter().collect();
    let regrouped = super::release_groupings::rebuild_groupings(sql, &settled, &[])?;
    updated_candidates.extend(regrouped.written.into_iter().filter_map(|item| match item {
        crate::folder_scanner::ScanItem::Valid(candidate) => Some(candidate),
        _ => None,
    }));
    Ok(updated_candidates)
}

/// The stored medium conflict of a verdict row.
pub(super) fn medium_conflict_of(
    kind: Option<String>,
) -> Result<Option<bae_identify::MediumConflict>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    Ok(Some(match kind.as_str() {
        "cd_rip" => bae_identify::MediumConflict::CdRip,
        "not_cd_audio" => bae_identify::MediumConflict::NotCdAudio,
        other => return Err(verdict_rows::unreadable("medium_conflict", other)),
    }))
}

#[cfg(test)]
#[path = "import_state_tests.rs"]
pub(crate) mod tests;
