//! Groupings, and the releases the ones with no anchor build.
//!
//! A grouping anchored at a folder is how the scan reads that folder: the
//! scan builds its release from the disk and stores it like any other
//! (see [`super::folder_scans`]). A grouping with no anchor takes in releases
//! the scans stored, from anywhere, and its release is built here from theirs.
//! It is rebuilt in every transaction that writes or removes one of them, so
//! it never describes files its releases no longer hold. While it stands, the
//! releases it takes in stay stored and leave the queue.
//!
//! When the releases it takes in all sit directly in one folder, the release
//! is that folder's and reads the folder's sidecar files too — the cover
//! beside the disc folders (see `crate::import::grouping::shared_parent`).
//! One grouping at most reads a folder's files: each grouping records the
//! folder it sits in and whether it reads its files, the store lets one read
//! them, and while several sit in a folder that has files, a new one is
//! refused and a rebuilt one other than the reader is blocked. The sidecar's
//! own writes rebuild the groupings sitting in its folder, like their
//! releases' do.

use super::folder_scans::{self, EntrySource, RowSources};
use super::*;
use crate::import::folder_scanner::{
    CandidateFile, FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey,
    InvalidCandidate, InvalidReason, ScanItem, SidecarFiles,
};
use crate::import::grouping::GroupingBlock;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// One stored release as a reader of one key takes it: whether it may be
/// worked on, and why not when it is a grouping that cannot be built.
pub(super) struct StoredReleaseCandidate {
    pub candidate: FolderCandidate,
    pub generation: u64,
    /// Settled, not taken into a grouping, and — for a grouping — built from
    /// every release it takes in.
    pub actionable: bool,
    pub error: Option<GroupingBlock>,
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
        ScanItem::Invalid(_) | ScanItem::Decided { .. } | ScanItem::Sidecar(_) => return Ok(None),
    };
    let taken_in = sql.query_row(
        "SELECT EXISTS(SELECT 1 FROM release_grouping_member WHERE member_key = ?)",
        [key],
        |row| row.get::<_, bool>(0),
    )?;
    let error = match &candidate.grouping {
        Some(grouping) => load_block_on(sql, grouping)?,
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
/// `touched` — releases just written, or just removed — or reads the sidecar
/// files of one of `sidecars` — folders whose sidecar was just written or
/// removed.
pub(super) fn rebuild_groupings(
    sql: &SqlContext<'_, '_>,
    touched: &[String],
    sidecars: &[PathBuf],
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
    for folder in sidecars {
        groupings.extend(sql.query(
            "SELECT key FROM release_grouping WHERE parent_folder = ?",
            [folder.to_string_lossy()],
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
                    return blocked(
                        sql,
                        key,
                        &GroupingBlock::SourceChanged {
                            folder: candidate.name,
                        },
                    );
                }
                ScanItem::Invalid(candidate) => {
                    return blocked(
                        sql,
                        key,
                        &GroupingBlock::SourceChanged {
                            folder: candidate.name,
                        },
                    );
                }
                ScanItem::Decided { .. } | ScanItem::Sidecar(_) => {
                    return blocked(
                        sql,
                        key,
                        &GroupingBlock::SourceGone {
                            folder: member.clone(),
                        },
                    );
                }
            },
            None => {
                return blocked(
                    sql,
                    key,
                    &GroupingBlock::SourceGone {
                        folder: member.clone(),
                    },
                )
            }
        }
    }
    // Where the grouping sits is recorded first, whatever the rebuild comes
    // to: that is how another grouping sitting in the same folder finds it.
    // Leaving a folder frees its files for a grouping blocked on them.
    let sits_in = crate::import::grouping::shared_parent(
        members
            .iter()
            .map(|member| (member.watched_folder_path.as_str(), member.file_root.as_path())),
    );
    let sits_in_text = sits_in
        .as_ref()
        .map(|folder| folder.to_string_lossy().into_owned());
    let (sat_in, was_reading): (Option<String>, bool) = sql.query_row(
        "SELECT parent_folder, reads_parent_files FROM release_grouping WHERE key = ?",
        [key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut changes = GroupingChanges::default();
    if sat_in != sits_in_text {
        sql.execute(
            "UPDATE release_grouping SET parent_folder = ?, reads_parent_files = 0 WHERE key = ?",
            params![sits_in_text, key],
        )?;
        if let Some(left) = &sat_in {
            changes.extend(rebuild_blocked_in(sql, key, Path::new(left), observed_at)?);
        }
    }
    let reading = sat_in == sits_in_text && was_reading;
    let parent = match parent_files_on(sql, key, sits_in.as_deref(), reading)? {
        Ok(parent) => parent,
        Err(conflict) => {
            changes.extend(blocked(sql, key, &conflict)?);
            return Ok(changes);
        }
    };
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
        parent.valid(),
    )
    .map_err(|error| DbError::Message(error.to_string()));
    let mut candidate = match composed {
        Ok((candidate, _)) => candidate,
        Err(error) => {
            changes.extend(blocked(
                sql,
                key,
                &GroupingBlock::Unbuildable {
                    detail: error.to_string(),
                },
            )?);
            return Ok(changes);
        }
    };
    let reads_parent_files = !matches!(parent, ParentFiles::None);
    let item = match parent {
        ParentFiles::Invalid(reason) => invalid(candidate, reason),
        ParentFiles::None | ParentFiles::Files(_) => {
            // The release's decisions are its own, keyed by its files like
            // any release's: the ones it took over from its folders were
            // stored as its own when it was made.
            let edits = super::import_state::load_candidate_file_edits_on(
                sql,
                &candidate.files.content_hash(),
            )?()?;
            match candidate.files.apply_candidate_file_edits(&edits) {
                Ok(()) => {
                    candidate.file_edit_revision = edits.revision;
                    ScanItem::Valid(candidate)
                }
                Err(reason) => invalid(candidate, reason),
            }
        }
    };
    sql.execute(
        "UPDATE release_grouping \
         SET blocked = NULL, blocked_subject = NULL, blocked_holder = NULL, \
             reads_parent_files = ? \
         WHERE key = ?",
        params![reads_parent_files, key],
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
    changes.written.extend(written.map(|_| item));
    Ok(changes)
}

/// The release `candidate` names, as one that cannot be imported.
fn invalid(candidate: FolderCandidate, reason: InvalidReason) -> ScanItem {
    ScanItem::Invalid(InvalidCandidate {
        path: candidate.path,
        name: candidate.name,
        watched_folder_path: candidate.watched_folder_path,
        display_path: candidate.display_path,
        grouping: candidate.grouping,
        reason,
    })
}

/// The files of the folder a grouping sits in, as its release reads them.
enum ParentFiles {
    /// None: it sits in no one folder, or the folder has no files no release
    /// there owns.
    None,
    Files(Vec<CandidateFile>),
    /// The folder's files hold a broken one, so the release cannot be
    /// imported.
    Invalid(InvalidReason),
}

impl ParentFiles {
    fn valid(&self) -> &[CandidateFile] {
        match self {
            Self::Files(files) => files,
            Self::None | Self::Invalid(_) => &[],
        }
    }
}

/// What the grouping `key`, sitting in `folder`, reads of that folder's files
/// — `reading` when it is the grouping that already does — or why it cannot
/// read them yet.
///
/// A folder's files go with one release at most. While another grouping sits
/// in a folder that has files, only the one already reading them may, and
/// none does when several came to them at once; a folder with no files of
/// its own has nothing to go with anyone, so any number may sit in it. Files
/// still downloading are not known yet, so no release reads them until the
/// download ends.
fn parent_files_on(
    sql: &(impl QueryOne + QueryRows),
    key: &str,
    folder: Option<&Path>,
    reading: bool,
) -> Result<Result<ParentFiles, GroupingBlock>, DbError> {
    let Some(folder) = folder else {
        return Ok(Ok(ParentFiles::None));
    };
    let Some(sidecar) = folder_scans::load_sidecar(sql, folder)? else {
        return Ok(Ok(ParentFiles::None));
    };
    let named = folder
        .file_name()
        .map_or_else(|| folder.to_string_lossy(), |name| name.to_string_lossy())
        .into_owned();
    if !reading {
        let others: Vec<(bool, String)> = sql.query(
            "SELECT other.reads_parent_files, COALESCE(candidate.name, other.key) \
             FROM release_grouping AS other \
             LEFT JOIN scan_candidate AS candidate ON candidate.path = other.key \
             WHERE other.parent_folder = ? AND other.key != ? ORDER BY other.key",
            params![folder.to_string_lossy(), key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if let Some((_, reader)) = others.into_iter().find(|(reads, _)| *reads) {
            return Ok(Err(GroupingBlock::FolderFilesTaken {
                folder: named,
                release: reader,
            }));
        }
        let contested: bool = sql.query_row(
            "SELECT EXISTS(SELECT 1 FROM release_grouping WHERE parent_folder = ? AND key != ?)",
            params![folder.to_string_lossy(), key],
            |row| row.get(0),
        )?;
        if contested {
            return Ok(Err(GroupingBlock::FolderFilesContested { folder: named }));
        }
    }
    Ok(match sidecar.files {
        SidecarFiles::Downloading => Err(GroupingBlock::FolderFilesDownloading { folder: named }),
        SidecarFiles::Valid(files) => Ok(ParentFiles::Files(files)),
        SidecarFiles::Invalid(reason) => Ok(ParentFiles::Invalid(reason)),
    })
}

/// Rebuild each grouping but `key` sitting in `folder` that is blocked, now
/// that `key` no longer stands in the way of reading the folder's files.
fn rebuild_blocked_in(
    sql: &SqlContext<'_, '_>,
    key: &str,
    folder: &Path,
    observed_at: i64,
) -> Result<GroupingChanges, DbError> {
    let blocked: Vec<String> = sql.query(
        "SELECT key FROM release_grouping \
         WHERE parent_folder = ? AND blocked IS NOT NULL AND key != ? ORDER BY key",
        params![folder.to_string_lossy(), key],
        |row| row.get(0),
    )?;
    let mut changes = GroupingChanges::default();
    for blocked in blocked {
        changes.extend(rebuild_grouping(sql, &blocked, observed_at)?);
    }
    Ok(changes)
}

/// Say why grouping `key`'s release cannot be worked on as it stands.
fn blocked(
    sql: &SqlContext<'_, '_>,
    key: &str,
    block: &GroupingBlock,
) -> Result<GroupingChanges, DbError> {
    let (kind, subject, holder) = match block {
        GroupingBlock::SourceChanged { folder } => ("source_changed", folder, None),
        GroupingBlock::SourceGone { folder } => ("source_gone", folder, None),
        GroupingBlock::FolderFilesTaken { folder, release } => {
            ("folder_files_taken", folder, Some(release))
        }
        GroupingBlock::FolderFilesContested { folder } => ("folder_files_contested", folder, None),
        GroupingBlock::FolderFilesDownloading { folder } => {
            ("folder_files_downloading", folder, None)
        }
        GroupingBlock::Unbuildable { detail } => ("unbuildable", detail, None),
    };
    sql.execute(
        "UPDATE release_grouping \
         SET blocked = ?, blocked_subject = ?, blocked_holder = ? WHERE key = ?",
        params![kind, subject, holder, key],
    )?;
    Ok(GroupingChanges::default())
}

/// Why grouping `key`'s release cannot be worked on, when something says so.
pub(super) fn load_block_on(
    sql: &(impl QueryOne + QueryRows),
    key: &str,
) -> Result<Option<GroupingBlock>, DbError> {
    let columns: Option<(Option<String>, Option<String>, Option<String>)> = sql
        .query_row(
            "SELECT blocked, blocked_subject, blocked_holder FROM release_grouping WHERE key = ?",
            [key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match columns {
        Some((kind, subject, holder)) => block_of(kind, subject, holder),
        None => Ok(None),
    }
}

/// A grouping's block from the columns [`blocked`] writes.
pub(super) fn block_of(
    kind: Option<String>,
    subject: Option<String>,
    holder: Option<String>,
) -> Result<Option<GroupingBlock>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    let subject = subject.ok_or_else(|| {
        DbError::Message(format!("grouping block {kind} names no folder or detail"))
    })?;
    Ok(Some(match (kind.as_str(), holder) {
        ("source_changed", None) => GroupingBlock::SourceChanged { folder: subject },
        ("source_gone", None) => GroupingBlock::SourceGone { folder: subject },
        ("folder_files_taken", Some(release)) => GroupingBlock::FolderFilesTaken {
            folder: subject,
            release,
        },
        ("folder_files_contested", None) => GroupingBlock::FolderFilesContested { folder: subject },
        ("folder_files_downloading", None) => {
            GroupingBlock::FolderFilesDownloading { folder: subject }
        }
        ("unbuildable", None) => GroupingBlock::Unbuildable { detail: subject },
        (other, _) => {
            return Err(DbError::Message(format!(
                "release_grouping.blocked holds {other:?} with a holder it does not name"
            )))
        }
    }))
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

    /// The release stored at `key`, when it may be worked on — or why a
    /// grouping's release cannot be.
    pub(crate) async fn load_release_candidate(
        &self,
        key: &str,
    ) -> Result<Result<Option<FolderCandidate>, GroupingBlock>, DbError> {
        let key = key.to_string();
        self.read(move |sql| {
            let Some(stored) = load_candidate_on(&sql, &key)? else {
                return Ok(Ok(None));
            };
            if let Some(block) = stored.error {
                return Ok(Err(block));
            }
            Ok(Ok(stored.actionable.then_some(stored.candidate)))
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
    /// When they all sit directly in one folder, the release reads that
    /// folder's sidecar files too.
    ///
    /// Refused, writing nothing, when any of them changed since the caller
    /// read it, is already taken into a grouping, or is already in the
    /// library, when the release they make already is, or when another
    /// grouping already reads the files of the folder they sit in.
    pub(crate) async fn combine_releases(
        &self,
        key: String,
        members: Vec<FolderCandidate>,
    ) -> Result<Result<GroupingChanges, GroupingBlock>, DbError> {
        if members.len() < 2 {
            return Err(DbError::Message(
                "combining a release requires at least two folders".into(),
            ));
        }
        let root = members[0].watched_folder_path.clone();
        let root_for_compose = root.clone();
        let key_for_compose = key.clone();
        let observed_at = self.inner.clock.now().timestamp_millis();
        // A refusal writes nothing, so it is found by a read. The write below
        // asks again, and refuses as a fault a store that changed in between.
        let sits_in = crate::import::grouping::shared_parent(
            members
                .iter()
                .map(|member| (member.watched_folder_path.as_str(), member.file_root.as_path())),
        );
        let refusal_key = key.clone();
        let refused = self
            .read(move |sql| {
                Ok(parent_files_on(&sql, &refusal_key, sits_in.as_deref(), false)?.err())
            })
            .await?;
        if let Some(block) = refused {
            return Ok(Err(block));
        }
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
            let sits_in = crate::import::grouping::shared_parent(members.iter().map(|member| {
                (member.watched_folder_path.as_str(), member.file_root.as_path())
            }));
            let parent = parent_files_on(sql, &key_for_compose, sits_in.as_deref(), false)?
                .map_err(|block| {
                    DbError::Message(format!("{key_for_compose} could not be combined: {block}"))
                })?;
            let (composed, inherited) = crate::import::grouping::compose(
                &key_for_compose,
                &root_for_compose,
                &with_edits,
                parent.valid(),
            )
            .map_err(|error| DbError::Message(error.to_string()))?;
            // A release that reads a broken file of its folder cannot be
            // imported: it holds no files, so no other release can be the
            // same one, and it has no layout to start from.
            if !matches!(parent, ParentFiles::Invalid(_)) {
                let content_hash = composed.files.content_hash();
                let in_use: bool = sql.query_row(
                    "SELECT EXISTS(SELECT 1 FROM scan_candidate WHERE content_hash = ?1) \
                     OR EXISTS(SELECT 1 FROM releases WHERE content_hash = ?1)",
                    [&content_hash],
                    |row| row.get(0),
                )?;
                if in_use {
                    return Err(DbError::Message(
                        "this combined release is already present in the queue or library"
                            .into(),
                    ));
                }
                // A new grouping starts with the layout it was just given, not
                // a draft an abandoned grouping of the same files left behind.
                sql.execute(
                    "DELETE FROM import_candidate_state WHERE content_hash = ?",
                    [&content_hash],
                )?;
                if !inherited.is_empty() {
                    sql.execute(
                        "INSERT INTO import_candidate_state (content_hash, folder_path) \
                         VALUES (?, ?)",
                        params![content_hash, key],
                    )?;
                    sql.execute(
                        "INSERT INTO import_candidate_asset_preparation (content_hash) VALUES (?)",
                        [&content_hash],
                    )?;
                    super::import_state::store_file_edits(sql, &content_hash, &inherited)?;
                }
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
                // Nothing of it is kept: the whole write rolls back.
                return match load_block_on(sql, &key)? {
                    Some(block) => Err(DbError::Message(format!(
                        "{key} could not be combined: {block}"
                    ))),
                    None => Err(DbError::Message(format!(
                        "{key} built no release from the folders it takes in"
                    ))),
                };
            }
            Ok(Ok(changes))
        })
        .await
    }

    /// Undo the grouping with no anchor at `key`: its release leaves the queue
    /// and the releases it took in return, as they are stored. Returns them,
    /// and the releases of groupings rebuilt because the files of the folder
    /// it read are free again.
    pub(crate) async fn separate_picked_grouping(
        &self,
        key: &str,
    ) -> Result<(Vec<ScanItem>, GroupingChanges), DbError> {
        let key = key.to_string();
        let observed_at = self.inner.clock.now().timestamp_millis();
        self.call(move |sql| {
            let members: Vec<String> = sql.query(
                "SELECT member_key FROM release_grouping_member \
                 WHERE grouping_key = ? ORDER BY position",
                [&key],
                |row| row.get(0),
            )?;
            let sat_in: Option<String> = sql
                .query_row(
                    "SELECT parent_folder FROM release_grouping WHERE key = ?",
                    [&key],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
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
            let regrouped = match sat_in {
                Some(folder) => rebuild_blocked_in(sql, &key, Path::new(&folder), observed_at)?,
                None => GroupingChanges::default(),
            };
            Ok((returned, regrouped))
        })
        .await
    }
}
