//! Groupings, and the releases the ones with no anchor build.
//!
//! A grouping anchored at a folder is how the scan reads that folder; the scan
//! stores its release like any other ([`super::folder_scans`]). A grouping with
//! no anchor takes in releases the scans stored, from anywhere, and its release
//! is rebuilt here in every transaction that writes or removes one of them.
//! While it stands, the releases it takes in stay stored and leave the queue.
//!
//! When those releases all sit directly in one folder, the grouping's release
//! also reads that folder's sidecar files
//! (`crate::import::grouping::shared_parent`); at most one grouping reads a
//! folder's files (see `parent_files_on`).

use super::folder_scans::{self, EntrySource, RowSources};
use super::*;
use crate::import::folder_scanner::{
    CandidateFile, FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey,
    InvalidCandidate, InvalidReason, ScanItem, SidecarFiles,
};
use crate::import::grouping::GroupingBlock;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A stored release, whether it may be worked on, and why not when it is a
/// grouping's release that is blocked.
pub(super) struct StoredReleaseCandidate {
    pub candidate: FolderCandidate,
    pub generation: u64,
    /// Valid, not taken into a grouping, and not blocked.
    pub actionable: bool,
    pub error: Option<GroupingBlock>,
}

/// What rebuilding groupings changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupingChanges {
    pub written: Vec<ScanItem>,
    /// The keys of `written` that are new releases: only a combine makes one.
    pub found: Vec<String>,
    pub removed: Vec<String>,
}

impl GroupingChanges {
    fn extend(&mut self, other: GroupingChanges) {
        self.written.extend(other.written);
        self.found.extend(other.found);
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
        None => Ok(sql.query_row(
            "SELECT EXISTS(SELECT 1 FROM skipped_import_candidates WHERE candidate_path = ?)",
            [candidate.path.to_string_lossy()],
            |row| row.get(0),
        )?),
    }
}

/// Rebuild the release of every grouping with no anchor that takes in one of
/// `touched` (releases just written or removed) or sits in one of `sidecars`
/// (folders whose sidecar was just written or removed).
pub(super) fn rebuild_groupings(
    sql: &SqlContext<'_, '_>,
    touched: &[String],
    sidecars: &[PathBuf],
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
        changes.extend(rebuild_grouping(sql, &grouping)?);
    }
    Ok(changes)
}

/// Build and store the release of the grouping `key` from the releases it
/// takes in, or record on the grouping why it cannot be built and leave its
/// release as last built.
fn rebuild_grouping(sql: &SqlContext<'_, '_>, key: &str) -> Result<GroupingChanges, DbError> {
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
    // Its release is listed under the watched folder its first member is.
    let root = members
        .first()
        .map(|member| member.watched_folder_path.clone())
        .ok_or_else(|| DbError::Message(format!("grouping {key} takes in no release")))?;
    // Record where the grouping sits before anything can block it, so other
    // groupings in that folder find it; leaving a folder rebuilds the ones
    // blocked there.
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
            changes.extend(rebuild_blocked_in(sql, key, Path::new(left))?);
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
            // Keyed by the release's own files; the decisions it took over
            // from its folders were stored that way in `combine_releases`.
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
        EntrySource::Grouping,
    )?;
    if let folder_scans::EntryWrite::Stored { .. } = written {
        changes.written.push(item);
    }
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
    /// It sits in no one folder, or that folder has no sidecar.
    None,
    Files(Vec<CandidateFile>),
    /// One of the folder's files is broken, so the release cannot be imported.
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

/// What the grouping `key`, sitting in `folder`, reads of that folder's files,
/// or why it cannot read them yet; `reading` means it already reads them.
///
/// At most one release reads a folder's files: while another grouping sits in
/// a folder with a sidecar, only the one already reading it may. Files still
/// downloading are read by no release until the download ends.
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
) -> Result<GroupingChanges, DbError> {
    let blocked: Vec<String> = sql.query(
        "SELECT key FROM release_grouping \
         WHERE parent_folder = ? AND blocked IS NOT NULL AND key != ? ORDER BY key",
        params![folder.to_string_lossy(), key],
        |row| row.get(0),
    )?;
    let mut changes = GroupingChanges::default();
    for blocked in blocked {
        changes.extend(rebuild_grouping(sql, &blocked)?);
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

/// The folder a grouping is anchored at, addressed below the watched folder
/// covering it.
fn anchored_at(
    sql: &(impl QueryOne + QueryRows),
    anchor: &str,
) -> Result<FolderReleaseDecisionKey, DbError> {
    let roots: Vec<String> = sql.query(
        "SELECT path FROM watched_import_folders",
        [],
        |row| row.get(0),
    )?;
    let root = crate::import::watched_folder::covering_root(&roots, anchor)
        .ok_or_else(|| DbError::Message(format!("{anchor} is under no watched folder")))?;
    Ok(FolderReleaseDecisionKey {
        watched_folder_path: root.to_string(),
        relative_folder_path: super::import_state::relative_below(root, anchor)?,
    })
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
            let stored: Option<(Option<String>, bool)> = sql
                .query_row(
                    "SELECT anchor_folder, combined FROM release_grouping WHERE key = ?",
                    [&key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((anchor, combined)) = stored else {
                return Ok(None);
            };
            Ok(Some(match anchor {
                Some(anchor) => GroupingFacts::Anchored {
                    folder: anchored_at(&sql, &anchor)?,
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

    /// Combine `members`, as the caller read them, into one release under the
    /// new grouping `key`, listed under the first member's watched folder;
    /// they leave the queue in the same write. `Err(block)` when the files of
    /// the folder they all sit in cannot be read; an error, writing nothing,
    /// when a member changed or is already grouped or imported, or the
    /// combined release already exists.
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
        let root_for_compose = members[0].watched_folder_path.clone();
        let key_for_compose = key.clone();
        // Check for a refusal on the read connection; the write checks again
        // and fails if the store changed in between.
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
            // Each folder's file decisions, which the release starts from.
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
            // A release reading a broken folder file is invalid: it holds no
            // files another release could share and no layout to start from.
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
                // Drop any state an undone grouping of the same files left.
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
                "INSERT INTO release_grouping (key, anchor_folder, combined, author) \
                 VALUES (?, NULL, 1, 'user')",
                [&key],
            )?;
            for (position, member) in members.iter().enumerate() {
                sql.execute(
                    "INSERT INTO release_grouping_member \
                         (grouping_key, position, member_key, member_folder) \
                     VALUES (?, ?, ?, ?)",
                    params![
                        key,
                        position as i64,
                        member.key(),
                        member.path.to_string_lossy()
                    ],
                )?;
            }
            let mut changes = rebuild_grouping(sql, &key)?;
            if changes.written.iter().any(
                |item| matches!(item, ScanItem::Valid(release) if release.key() == key),
            ) {
                changes.found.push(key.clone());
            }
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

    /// Undo the grouping with no anchor at `key`, removing its release. Returns
    /// the releases it took in, as stored, and what rebuilding the groupings
    /// blocked in its folder changed.
    pub(crate) async fn separate_picked_grouping(
        &self,
        key: &str,
    ) -> Result<(Vec<ScanItem>, GroupingChanges), DbError> {
        let key = key.to_string();
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
                "DELETE FROM release_grouping WHERE key = ? AND anchor_folder IS NULL",
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
                Some(folder) => rebuild_blocked_in(sql, &key, Path::new(&folder))?,
                None => GroupingChanges::default(),
            };
            Ok((returned, regrouped))
        })
        .await
    }
}
