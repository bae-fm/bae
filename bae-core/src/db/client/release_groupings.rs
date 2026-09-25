//! Groupings, and the releases the ones with no anchor build.
//!
//! A grouping anchored at a folder is how the scan reads that folder: the
//! scan builds its release from the disk and stores it like any other
//! (see [`super::folder_scans`]). A grouping with no anchor takes in releases
//! the scans stored, from anywhere, and its release is built here from theirs.
//! It is rebuilt in every transaction that writes or removes one of them, so
//! it never describes files its releases no longer hold. While it stands, the
//! releases it takes in stay stored and leave the queue.

use super::folder_scans::{self, EntrySource, RowSources};
use super::*;
use crate::import::folder_scanner::{
    FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate, ScanItem,
};
use std::collections::BTreeSet;

/// One stored release as a reader of one key takes it: whether it may be
/// worked on, and why not when it is a grouping that cannot be built.
pub(super) struct StoredReleaseCandidate {
    pub candidate: FolderCandidate,
    pub generation: u64,
    /// Settled, not taken into a grouping, and — for a grouping — built from
    /// every release it takes in.
    pub actionable: bool,
    pub error: Option<String>,
}

/// What rebuilding groupings changed: the releases written, and the keys of
/// releases no longer stored.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupingChanges {
    pub written: Vec<ScanItem>,
    pub removed: Vec<String>,
}

impl GroupingChanges {
    fn extend(&mut self, other: GroupingChanges) {
        self.written.extend(other.written);
        self.removed.extend(other.removed);
    }
}

/// How a grouping reads, as the actions on its release need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupingFacts {
    /// A folder's reading: the folder, and whether its releases are one.
    Anchored {
        folder: FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
    },
    /// Releases the person read as one, by key, in play order.
    Picked { members: Vec<String> },
}

pub(super) fn load_candidate_on(
    sql: &(impl QueryOne + QueryRows),
    key: &str,
) -> Result<Option<StoredReleaseCandidate>, DbError> {
    let Some((root, stored)) = folder_scans::load_item_by_key(sql, key, RowSources::Any)? else {
        return Ok(None);
    };
    folder_scans::validate_scan_item_ownership(&root, &stored.key, &stored.item)?;
    let (candidate, settled) = match stored.item {
        ScanItem::Valid(candidate) => (candidate, true),
        ScanItem::Discovered(candidate) => (candidate, false),
        ScanItem::Invalid(_) | ScanItem::Decided { .. } => return Ok(None),
    };
    let taken_in = sql.query_row(
        "SELECT EXISTS(SELECT 1 FROM release_grouping_member WHERE member_key = ?)",
        [key],
        |row| row.get::<_, bool>(0),
    )?;
    let error = match &candidate.grouping {
        Some(grouping) => sql
            .query_row(
                "SELECT error FROM release_grouping WHERE key = ?",
                [grouping],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten(),
        None => None,
    };
    Ok(Some(StoredReleaseCandidate {
        actionable: settled && !taken_in && error.is_none(),
        candidate,
        generation: stored.generation,
        error,
    }))
}

/// Whether the person set this release aside: a grouping's own flag, or a
/// folder's row in the skip table.
pub(super) fn skipped_on(
    sql: &(impl QueryOne + QueryRows),
    candidate: &FolderCandidate,
) -> Result<bool, DbError> {
    match &candidate.grouping {
        Some(grouping) => sql
            .query_row(
                "SELECT skipped FROM release_grouping WHERE key = ?",
                [grouping],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| DbError::Message(format!("release {grouping} has no grouping"))),
        None => {
            let relative = crate::import::watched_folder::candidate_relative_path(
                &candidate.watched_folder_path,
                &candidate.path,
            )
            .map_err(|error| DbError::Message(error.to_string()))?;
            Ok(sql.query_row(
                "SELECT EXISTS(SELECT 1 FROM skipped_import_candidates \
                 WHERE watched_folder_path = ? AND relative_candidate_path = ?)",
                params![candidate.watched_folder_path, relative],
                |row| row.get(0),
            )?)
        }
    }
}

/// Rebuild the release of every grouping with no anchor that takes in one of
/// `touched` — releases just written, or just removed.
pub(super) fn rebuild_groupings(
    sql: &SqlContext<'_, '_>,
    touched: &[String],
    observed_at: i64,
) -> Result<GroupingChanges, DbError> {
    let mut groupings = BTreeSet::new();
    for key in touched {
        groupings.extend(sql.query(
            "SELECT grouping_key FROM release_grouping_member WHERE member_key = ?",
            [key],
            |row| row.get::<_, String>(0),
        )?);
    }
    let mut changes = GroupingChanges::default();
    for grouping in groupings {
        changes.extend(rebuild_grouping(sql, &grouping, observed_at)?);
    }
    Ok(changes)
}

/// Build the release of the grouping `key` from the releases it takes in, and
/// store it — or, when one of them is gone or no longer valid, say so on the
/// grouping and leave its release as it was last built.
fn rebuild_grouping(
    sql: &SqlContext<'_, '_>,
    key: &str,
    observed_at: i64,
) -> Result<GroupingChanges, DbError> {
    let root: String = sql.query_row(
        "SELECT watched_folder_path FROM release_grouping \
         WHERE key = ? AND anchor_relative_path IS NULL",
        [key],
        |row| row.get(0),
    )?;
    let member_keys: Vec<String> = sql.query(
        "SELECT member_key FROM release_grouping_member WHERE grouping_key = ? ORDER BY position",
        [key],
        |row| row.get(0),
    )?;
    let mut members = Vec::with_capacity(member_keys.len());
    for member in &member_keys {
        match folder_scans::load_item_by_key(sql, member, RowSources::Any)? {
            Some((_, stored)) => match stored.item {
                ScanItem::Valid(candidate) => members.push(candidate),
                ScanItem::Discovered(candidate) => {
                    return blocked(sql, key, &format!("Source folder changed: {}", candidate.name));
                }
                ScanItem::Invalid(candidate) => {
                    return blocked(sql, key, &format!("Source folder changed: {}", candidate.name));
                }
                ScanItem::Decided { .. } => {
                    return blocked(sql, key, &format!("Source folder disappeared: {member}"));
                }
            },
            None => {
                return blocked(
                    sql,
                    key,
                    &format!("Source folder changed or disappeared: {member}"),
                )
            }
        }
    }
    let composed = crate::import::grouping::compose(
        key,
        &root,
        &members
            .into_iter()
            .map(|member| {
                (
                    member,
                    crate::import::folder_scanner::CandidateFileEdits::default(),
                )
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|error| DbError::Message(error.to_string()));
    let mut candidate = match composed {
        Ok((candidate, _)) => candidate,
        Err(error) => return blocked(sql, key, &error.to_string()),
    };
    // The release's decisions are its own, keyed by its files like any
    // release's: the ones it took over from its folders were stored as its
    // own when it was made.
    let edits =
        super::import_state::load_candidate_file_edits_on(sql, &candidate.files.content_hash())?(
        )?;
    let item = match candidate.files.apply_candidate_file_edits(&edits) {
        Ok(()) => {
            candidate.file_edit_revision = edits.revision;
            ScanItem::Valid(candidate)
        }
        Err(reason) => ScanItem::Invalid(InvalidCandidate {
            path: candidate.path,
            name: candidate.name,
            watched_folder_path: candidate.watched_folder_path,
            display_path: candidate.display_path,
            grouping: candidate.grouping,
            reason,
        }),
    };
    sql.execute(
        "UPDATE release_grouping SET error = NULL WHERE key = ?",
        [key],
    )?;
    let generation: i64 = sql
        .query_row(
            "SELECT generation FROM folder_scan_roots WHERE watched_folder_path = ?",
            [&root],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            DbError::Message(format!(
                "{root} has not been read, so no release can be listed under it"
            ))
        })?;
    let written = folder_scans::write_entry(
        sql,
        &root,
        generation,
        &folder_scans::ScanItemToWrite {
            item: item.clone(),
            file_metadata: None,
            folder_date: None,
        },
        observed_at,
        EntrySource::Grouping,
    )?;
    Ok(GroupingChanges {
        written: written.map(|_| item).into_iter().collect(),
        removed: Vec::new(),
    })
}

/// Say why grouping `key`'s release cannot be built as it stands.
fn blocked(
    sql: &SqlContext<'_, '_>,
    key: &str,
    error: &str,
) -> Result<GroupingChanges, DbError> {
    sql.execute(
        "UPDATE release_grouping SET error = ? WHERE key = ?",
        params![error, key],
    )?;
    Ok(GroupingChanges::default())
}

impl Database {
    pub(crate) async fn set_grouping_skipped(
        &self,
        key: &str,
        skipped: bool,
    ) -> Result<bool, DbError> {
        let key = key.to_string();
        self.call(move |sql| {
            Ok(sql.execute(
                "UPDATE release_grouping SET skipped = ? WHERE key = ? AND skipped != ?",
                params![skipped, key, skipped],
            )? == 1)
        })
        .await
    }

    /// Whether the person set this release aside.
    pub(crate) async fn is_release_candidate_skipped(
        &self,
        candidate: &FolderCandidate,
    ) -> Result<bool, DbError> {
        let candidate = candidate.clone();
        self.read(move |sql| skipped_on(&sql, &candidate)).await
    }

    /// The release stored at `key`, when it may be worked on.
    pub(crate) async fn load_release_candidate(
        &self,
        key: &str,
    ) -> Result<Option<FolderCandidate>, DbError> {
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

    /// How the grouping `key` reads, or `None` when no grouping has that key.
    pub(crate) async fn load_grouping(&self, key: &str) -> Result<Option<GroupingFacts>, DbError> {
        let key = key.to_string();
        self.read(move |sql| {
            let stored: Option<(String, Option<String>, bool)> = sql
                .query_row(
                    "SELECT watched_folder_path, anchor_relative_path, combined \
                     FROM release_grouping WHERE key = ?",
                    [&key],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((watched_folder_path, anchor, combined)) = stored else {
                return Ok(None);
            };
            Ok(Some(match anchor {
                Some(relative_folder_path) => GroupingFacts::Anchored {
                    folder: FolderReleaseDecisionKey {
                        watched_folder_path,
                        relative_folder_path,
                    },
                    decision: if combined {
                        FolderReleaseDecision::CombineAsOneRelease
                    } else {
                        FolderReleaseDecision::KeepAsSeparateReleases
                    },
                },
                None => GroupingFacts::Picked {
                    members: sql.query(
                        "SELECT member_key FROM release_grouping_member \
                         WHERE grouping_key = ? ORDER BY position",
                        [&key],
                        |row| row.get(0),
                    )?,
                },
            }))
        })
        .await
    }

    /// Read `members` — each as the caller read it — as one release under the
    /// new grouping `key`, listed under the first one's watched folder. They
    /// leave the queue in the same write the release joins it.
    ///
    /// Refused, writing nothing, when any of them changed since the caller
    /// read it, is already taken into a grouping, or is already in the
    /// library, or when the release they make already is.
    pub(crate) async fn combine_releases(
        &self,
        key: String,
        members: Vec<FolderCandidate>,
    ) -> Result<GroupingChanges, DbError> {
        if members.len() < 2 {
            return Err(DbError::Message(
                "combining a release requires at least two folders".into(),
            ));
        }
        let root = members[0].watched_folder_path.clone();
        let root_for_compose = root.clone();
        let key_for_compose = key.clone();
        let observed_at = self.inner.clock.now().timestamp_millis();
        self.call(move |sql| {
            // Each folder's own file decisions, which the release takes over
            // as its own starting point.
            let mut with_edits = Vec::with_capacity(members.len());
            for member in &members {
                let edits = super::import_state::load_candidate_file_edits_on(
                    sql,
                    &member.files.content_hash(),
                )?()?;
                with_edits.push((member.clone(), edits));
            }
            let (composed, inherited) = crate::import::grouping::compose(
                &key_for_compose,
                &root_for_compose,
                &with_edits,
            )
            .map_err(|error| DbError::Message(error.to_string()))?;
            let content_hash = composed.files.content_hash();
            let in_use: bool = sql.query_row(
                "SELECT EXISTS(SELECT 1 FROM scan_candidate WHERE content_hash = ?1) \
                 OR EXISTS(SELECT 1 FROM releases WHERE content_hash = ?1)",
                [&content_hash],
                |row| row.get(0),
            )?;
            if in_use {
                return Err(DbError::Message(
                    "this combined release is already present in the queue or library".into(),
                ));
            }
            // A new grouping starts with the layout it was just given, not a
            // draft an abandoned grouping of the same files left behind.
            sql.execute(
                "DELETE FROM import_candidate_state WHERE content_hash = ?",
                [&content_hash],
            )?;
            if !inherited.is_empty() {
                sql.execute(
                    "INSERT INTO import_candidate_state (content_hash, folder_path) VALUES (?, ?)",
                    params![content_hash, key],
                )?;
                sql.execute(
                    "INSERT INTO import_candidate_asset_preparation (content_hash) VALUES (?)",
                    [&content_hash],
                )?;
                super::import_state::store_file_edits(sql, &content_hash, &inherited)?;
            }
            for member in &members {
                let member_key = member.key();
                if folder_scans::load_item_by_key(sql, &member_key, RowSources::Any)?
                    .map(|(_, stored)| stored.item)
                    != Some(ScanItem::Valid(member.clone()))
                {
                    return Err(DbError::Message(format!(
                        "{} changed while the folders were being combined",
                        member.name
                    )));
                }
                let unavailable = sql.query_row(
                    "SELECT EXISTS(SELECT 1 FROM release_grouping_member WHERE member_key = ?1) \
                     OR EXISTS(SELECT 1 FROM releases WHERE content_hash = ?2)",
                    params![member_key, member.files.content_hash()],
                    |row| row.get::<_, bool>(0),
                )?;
                if unavailable {
                    return Err(DbError::Message(format!(
                        "{} is already combined or imported",
                        member.name
                    )));
                }
            }
            sql.execute(
                "INSERT INTO release_grouping \
                     (key, watched_folder_path, anchor_relative_path, combined, author) \
                 VALUES (?, ?, NULL, 1, 'user')",
                params![key, root],
            )?;
            for (position, member) in members.iter().enumerate() {
                sql.execute(
                    "INSERT INTO release_grouping_member \
                         (grouping_key, position, member_key, watched_folder_path) \
                     VALUES (?, ?, ?, ?)",
                    params![
                        key,
                        position as i64,
                        member.key(),
                        member.watched_folder_path
                    ],
                )?;
            }
            let changes = rebuild_grouping(sql, &key, observed_at)?;
            if changes.written.is_empty() {
                let error: Option<String> = sql.query_row(
                    "SELECT error FROM release_grouping WHERE key = ?",
                    [&key],
                    |row| row.get(0),
                )?;
                return Err(DbError::Message(error.unwrap_or_else(|| {
                    format!("{key} built no release from the folders it takes in")
                })));
            }
            Ok(changes)
        })
        .await
    }

    /// Undo the grouping with no anchor at `key`: its release leaves the queue
    /// and the releases it took in return, as they are stored. Returns them.
    pub(crate) async fn separate_picked_grouping(
        &self,
        key: &str,
    ) -> Result<Vec<ScanItem>, DbError> {
        let key = key.to_string();
        self.call(move |sql| {
            let members: Vec<String> = sql.query(
                "SELECT member_key FROM release_grouping_member \
                 WHERE grouping_key = ? ORDER BY position",
                [&key],
                |row| row.get(0),
            )?;
            let removed = sql.execute(
                "DELETE FROM release_grouping WHERE key = ? AND anchor_relative_path IS NULL",
                [&key],
            )?;
            if removed != 1 {
                return Err(DbError::Message(format!(
                    "{key} is not a release read from folders picked together"
                )));
            }
            let mut returned = Vec::with_capacity(members.len());
            for member in members {
                if let Some((_, stored)) =
                    folder_scans::load_item_by_key(sql, &member, RowSources::Any)?
                {
                    returned.push(stored.item);
                }
            }
            Ok(returned)
        })
        .await
    }
}
