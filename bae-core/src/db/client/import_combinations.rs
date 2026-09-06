//! Explicit folder selections and their immutable, reviewed source snapshots.

use super::*;
use crate::import::combination::{CandidateCombination, CombinationPart, CombinationTrackOrder};
use crate::import::folder_scanner::{CategorizedFiles, FolderCandidate, ScanItem};
use crate::import::release_candidate::{CombinedCandidate, ReleaseCandidate};

pub(super) struct StoredReleaseCandidate {
    pub candidate: ReleaseCandidate,
    pub generation: u64,
    pub actionable: bool,
    pub error: Option<String>,
}

pub(super) fn load_candidate_on(
    sql: &(impl QueryOne + QueryRows),
    key: &str,
) -> Result<Option<StoredReleaseCandidate>, DbError> {
    let source: Option<String> = sql
        .query_row(
            "SELECT source_kind FROM scan_candidate WHERE path = ?",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    match source.as_deref() {
        None => Ok(None),
        Some("folder") => {
            let Some((root, stored)) = folder_scans::load_item_by_key(sql, key)? else {
                return Err(DbError::Message(format!(
                    "candidate {key} disappeared during its read"
                )));
            };
            folder_scans::validate_scan_item_ownership(&root, &stored.key, &stored.item)?;
            let (candidate, actionable) = match stored.item {
                ScanItem::Valid(candidate) => (candidate, true),
                ScanItem::Discovered(candidate) => (candidate, false),
                ScanItem::Invalid(_) | ScanItem::Decided { .. } => return Ok(None),
            };
            let combined = sql.query_row(
                "SELECT EXISTS(SELECT 1 FROM candidate_combination_member WHERE candidate_key = ?)",
                [key],
                |row| row.get::<_, bool>(0),
            )?;
            Ok(Some(StoredReleaseCandidate {
                candidate: candidate.into(),
                generation: stored.generation,
                actionable: actionable && !combined,
                error: None,
            }))
        }
        Some("combination") => {
            let (root, name, order, revision, generation, error): (String, String, String, i64, i64, Option<String>) = sql.query_row(
                "SELECT g.watched_folder_path, g.name, g.track_order, c.file_edit_revision, c.generation, g.error \
                 FROM candidate_combination g JOIN scan_candidate c ON c.path = g.candidate_key \
                 WHERE g.candidate_key = ?", [key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )?;
            let order = order_of(&order)?;
            let parts = sql.query(
                "SELECT candidate_key, folder_name, file_prefix, first_disc, disc_count, track_count \
                 FROM candidate_combination_member WHERE combination_key = ? ORDER BY position", [key],
                |row| Ok(CombinationPart { candidate_key: row.get(0)?, folder_name: row.get(1)?, file_prefix: row.get(2)?, first_disc: row.get(3)?, disc_count: row.get(4)?, track_count: row.get(5)? }),
            )?;
            let files = folder_scans::read::load_files(sql, &root, Some(key))?
                .remove(key)
                .ok_or_else(|| {
                    DbError::Message(format!("combined candidate {key} has no stored files"))
                })?;
            let combination =
                CandidateCombination::from_stored(parts, CategorizedFiles { files }, order)
                    .map_err(|error| DbError::Message(error.to_string()))?;
            Ok(Some(StoredReleaseCandidate {
                candidate: ReleaseCandidate::Combined(CombinedCandidate {
                    key: key.into(),
                    name,
                    watched_folder_path: root,
                    order,
                    combination,
                    file_edit_revision: folder_scans::columns::to_u64(
                        revision,
                        "combination file revision",
                    )?,
                }),
                generation: folder_scans::columns::to_u64(generation, "combination generation")?,
                actionable: error.is_none(),
                error,
            }))
        }
        Some(other) => Err(DbError::Message(format!(
            "unknown candidate source {other}"
        ))),
    }
}

fn order_of(value: &str) -> Result<CombinationTrackOrder, DbError> {
    match value {
        "separate_discs" => Ok(CombinationTrackOrder::SeparateDiscs),
        "continuous" => Ok(CombinationTrackOrder::Continuous),
        _ => Err(DbError::Message(format!(
            "unknown combination track order {value}"
        ))),
    }
}

pub(super) fn skipped_on(
    sql: &(impl QueryOne + QueryRows),
    candidate: &ReleaseCandidate,
) -> Result<bool, DbError> {
    match candidate {
        ReleaseCandidate::Combined(candidate) => Ok(sql.query_row(
            "SELECT skipped FROM candidate_combination WHERE candidate_key = ?",
            [&candidate.key],
            |row| row.get(0),
        )?),
        ReleaseCandidate::Folder(candidate) => {
            let relative = crate::import::folder_registry::candidate_relative_path(
                &candidate.watched_folder_path,
                &candidate.path,
            )
            .map_err(|error| DbError::Message(error.to_string()))?;
            Ok(sql.query_row(
                "SELECT EXISTS(SELECT 1 FROM skipped_import_candidates WHERE watched_folder_path = ? AND relative_candidate_path = ?)",
                params![candidate.watched_folder_path, relative], |row| row.get(0),
            )?)
        }
    }
}

impl Database {
    pub(crate) async fn set_combined_candidate_skipped(
        &self,
        key: &str,
        skipped: bool,
    ) -> Result<bool, DbError> {
        let key = key.to_string();
        self.call(move |sql| {
            Ok(sql.execute("UPDATE candidate_combination SET skipped = ? WHERE candidate_key = ? AND skipped != ?", params![skipped, key, skipped])? == 1)
        }).await
    }

    pub(crate) async fn load_release_candidate(
        &self,
        key: &str,
    ) -> Result<Option<ReleaseCandidate>, DbError> {
        let key = key.to_string();
        self.read(move |sql| {
            let Some(stored) = load_candidate_on(&sql, &key)? else {
                return Ok(None);
            };
            if let Some(error) = stored.error {
                return Err(DbError::Message(error));
            }
            Ok(stored.actionable.then_some(stored.candidate))
        })
        .await
    }

    /// The reviewed files must still be the current files of every selected
    /// candidate. Membership, the source snapshot, and its draft commit together.
    pub(crate) async fn combine_candidates(
        &self,
        key: String,
        name: String,
        candidates: Vec<FolderCandidate>,
        order: CombinationTrackOrder,
    ) -> Result<(), DbError> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(DbError::Message("a combined release needs a name".into()));
        }
        let combination = CandidateCombination::prepare(&candidates, order)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let created_at = self.inner.clock.now().timestamp_millis();
        self.call(move |sql| {
            let content_hash = combination.files.content_hash();
            let in_use: bool = sql.query_row(
                "SELECT EXISTS(SELECT 1 FROM scan_candidate WHERE content_hash = ?1) OR EXISTS(SELECT 1 FROM releases WHERE content_hash = ?1)",
                [&content_hash], |row| row.get(0),
            )?;
            if in_use { return Err(DbError::Message("this combined release is already present in the queue or library".into())); }
            // A newly reviewed selection starts with its reviewed layout, not
            // an abandoned combination's draft under the same file hash.
            sql.execute("DELETE FROM import_candidate_state WHERE content_hash = ?", [&content_hash])?;
            for candidate in &candidates {
                let source_key = candidate.path.to_string_lossy();
                if folder_scans::load_scan_item_on(sql, &source_key)? != Some(ScanItem::Valid(candidate.clone())) {
                    return Err(DbError::Message(format!("{} changed while the combination was being reviewed", candidate.name)));
                }
                let unavailable = sql.query_row(
                    "SELECT EXISTS(SELECT 1 FROM candidate_combination_member WHERE candidate_key = ?1) \
                     OR EXISTS(SELECT 1 FROM releases WHERE content_hash = ?2)",
                    params![source_key, candidate.files.content_hash()], |row| row.get::<_, bool>(0),
                )?;
                if unavailable { return Err(DbError::Message(format!("{} is already combined or imported", candidate.name))); }
            }
            let root = &candidates[0].watched_folder_path;
            let generation: i64 = sql.query_row("SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?", [root], |row| row.get(0))?;
            let order_text = match order { CombinationTrackOrder::SeparateDiscs => "separate_discs", CombinationTrackOrder::Continuous => "continuous" };
            sql.execute("INSERT INTO candidate_combination (candidate_key, watched_folder_path, name, track_order, created_at) VALUES (?, ?, ?, ?, ?)", params![key, root, name, order_text, created_at])?;
            for (position, (part, candidate)) in combination.parts.iter().zip(&candidates).enumerate() {
                sql.execute("INSERT INTO candidate_combination_member \
                    (combination_key, position, candidate_key, watched_folder_path, folder_name, file_prefix, first_disc, disc_count, track_count) \
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    params![key, position as i64, part.candidate_key, candidate.watched_folder_path, part.folder_name, part.file_prefix, part.first_disc, part.disc_count, part.track_count])?;
            }
            sql.execute("INSERT INTO scan_candidate \
                (watched_folder_path, path, generation, kind, name, display_path, content_hash, file_edit_revision, initial_metadata_source, source_kind, first_seen_at) \
                VALUES (?, ?, ?, 'valid', ?, ?, ?, 0, 'none', 'combination', ?)",
                params![root, key, generation, name, name, combination.files.content_hash(), created_at])?;
            let mut draft = crate::import::pane::blank_source_for_tracks(combination.tracks);
            draft.draft.album_title = name;
            folder_scans::write::ensure_candidate_state(sql, &key, root, &combination.files, &draft)?;
            folder_scans::insert_candidate_files(sql, root, &key, &combination.files)?;
            Ok(())
        }).await
    }

    pub(crate) async fn separate_combined_candidate(&self, key: &str) -> Result<(), DbError> {
        let key = key.to_string();
        self.call(move |sql| {
            let removed = sql.execute(
                "DELETE FROM candidate_combination WHERE candidate_key = ?",
                [&key],
            )?;
            if removed != 1 {
                return Err(DbError::Message(format!("{key} is not a combined release")));
            }
            Ok(())
        })
        .await
    }
}
