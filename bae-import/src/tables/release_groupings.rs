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
//! (`crate::grouping::shared_parent`); at most one grouping reads a
//! folder's files (see `parent_files_on`). Their tables are
//! `release_groupings.sql`.

use super::folder_scans::{self, EntrySource, RowSources};
use super::*;
use crate::folder_scanner::{
    AudioDamage, CandidateFile, FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey,
    InvalidCandidate, ProposedRole, ScanItem, SidecarFiles, SkippedFile,
};
use crate::grouping::{CombineRefusal, GroupingBlock};
use crate::CandidateKey;
use bae_store::WriteStatements;
pub(crate) use folder_readings::folder_release_decisions;
pub(super) use folder_readings::{store_folder_reading, validate_decision_key_ownership};
use std::path::{Path, PathBuf};

pub(super) mod entry;
mod facts;
pub(super) mod folder_readings;
pub(crate) mod fragments;
pub mod row_fragments;
pub(crate) use entry::{
    listed_entry_of, load_candidate_entry_on, require_editable_files_on, require_workable_on,
    CandidateEntry, ScannedEntry,
};
pub(crate) use facts::GroupingFacts;
pub use folder_readings::FolderReadingRefusal;

coven::write!(
    DeleteGrouping(key: &'p crate::CandidateKey),
    r#"DELETE FROM release_grouping WHERE key = :key"#
);

// The grouping `:key` sits in `:parent_folder` from now on, and reads none of
// its files yet.
coven::write!(
    MoveGrouping(parent_folder: Option<&'p [u8]>, key: &'p crate::CandidateKey),
    r#"
    UPDATE release_grouping SET parent_folder = :parent_folder, reads_parent_files = 0
    WHERE key = :key
"#
);

coven::write!(
    UnblockGrouping(reads_parent_files: bool, key: &'p crate::CandidateKey),
    r#"
    UPDATE release_grouping
    SET blocked = NULL, blocked_subject = NULL, blocked_holder = NULL,
        reads_parent_files = :reads_parent_files
    WHERE key = :key
"#
);

coven::write!(
    BlockGrouping(
        kind: &'static str,
        subject: &'p str,
        holder: Option<&'p str>,
        key: &'p crate::CandidateKey,
    ),
    r#"
    UPDATE release_grouping
    SET blocked = :kind, blocked_subject = :subject, blocked_holder = :holder
    WHERE key = :key
"#
);

coven::write!(
    SkipGrouping(skipped: bool, key: &'p crate::CandidateKey),
    r#"UPDATE release_grouping SET skipped = :skipped WHERE key = :key AND skipped != :skipped"#
);

coven::write!(
    InsertPickedGrouping(key: &'p crate::CandidateKey),
    r#"
    INSERT INTO release_grouping (key, anchor_folder, combined, author)
    VALUES (:key, NULL, 1, 'user')
"#
);

coven::write!(
    InsertGroupingMember(
        grouping: &'p crate::CandidateKey,
        position: i64,
        member: &'p crate::CandidateKey,
        folder: &'p [u8],
    ),
    r#"
    INSERT INTO release_grouping_member (grouping_key, position, member_key, member_folder)
    VALUES (:grouping, :position, :member, :folder)
"#
);

coven::write!(
    DeletePickedGrouping(key: &'p crate::CandidateKey),
    r#"DELETE FROM release_grouping WHERE key = :key AND anchor_folder IS NULL"#
);

/// Why reading a release of folders picked together apart again was
/// refused. Nothing of it was written.
#[derive(Debug, thiserror::Error)]
pub enum SeparationRefusal {
    /// The key names no grouping of picked folders: it was read apart
    /// already.
    #[error("{0} is not a release read from folders picked together")]
    NotPicked(CandidateKey),
}

/// What rebuilding groupings changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupingChanges {
    pub written: Vec<ScanItem>,
    /// The keys of `written` that are new releases: only a combine makes one.
    pub found: Vec<CandidateKey>,
    pub removed: Vec<CandidateKey>,
}

impl GroupingChanges {
    fn extend(&mut self, other: GroupingChanges) {
        self.written.extend(other.written);
        self.found.extend(other.found);
        self.removed.extend(other.removed);
    }
}

// The groupings anchored at, or taking a release from, the folder `root` or
// a folder below it, each with whether the scan lists its release.
// How many groupings and grouping members are stored.
#[cfg(test)]
coven::read!(
    writer GroupingRowCounts { groupings: i64, members: i64 },
    r#"
    SELECT (SELECT COUNT(*) FROM release_grouping AS grouping) AS groupings,
        (SELECT COUNT(*) FROM release_grouping_member AS member) AS members
"#
);

coven::read!(
    writer GroupingsAtOrBelow(root: Vec<u8>, below_first: Vec<u8>, below_past: Vec<u8>) {
        key: CandidateKey,
        listed: bool
    },
    r#"
    SELECT grouping.key,
           EXISTS (SELECT 1
                   FROM @crate::tables::folder_scans::row_fragments::HeldCandidates AS held
                   WHERE held.path = grouping.key) AS listed
    FROM (SELECT anchored.key
          FROM release_grouping AS anchored
          WHERE anchored.anchor_folder = :root
             OR (anchored.anchor_folder >= :below_first AND anchored.anchor_folder < :below_past)
          UNION
          SELECT member.grouping_key AS key
          FROM release_grouping_member AS member
          WHERE member.member_folder = :root
             OR (member.member_folder >= :below_first AND member.member_folder < :below_past))
        AS grouping
    ORDER BY grouping.key
"#
);

// The groupings that take in one of the releases `touched` names, or sit in
// one of the folders `sidecars` names, each a JSON array of bytes in hex.
coven::read!(
    writer GroupingsTouched(touched: String, sidecars: String) { key: CandidateKey },
    r#"
    SELECT member.grouping_key AS key
    FROM release_grouping_member AS member
    WHERE member.member_key IN (SELECT unhex(wanted.value) FROM json_each(:touched) AS wanted)
    UNION
    SELECT grouping.key
    FROM release_grouping AS grouping
    WHERE grouping.parent_folder IN (SELECT unhex(wanted.value) FROM json_each(:sidecars) AS wanted)
    ORDER BY 1
"#
);

// The releases the grouping `key` takes in, in order.
coven::read!(
    writer GroupingMembers(key: CandidateKey) { member_key: CandidateKey },
    r#"
    SELECT member.member_key
    FROM release_grouping_member AS member
    WHERE member.grouping_key = :key
    ORDER BY member.position
"#
);

// The folder the grouping `key` sits in, and whether it reads that folder's
// files.
coven::read!(
    writer GroupingSeat(key: CandidateKey) { reads_parent_files: bool },
    r#"
    SELECT grouping.parent_folder, grouping.reads_parent_files
    FROM release_grouping AS grouping
    WHERE grouping.key = :key
"#
);

// The blocked groupings but `key` that sit in the folder `folder`.
coven::read!(
    writer BlockedInFolder(folder: Vec<u8>, key: CandidateKey) { key: CandidateKey },
    r#"
    SELECT grouping.key
    FROM release_grouping AS grouping
    WHERE grouping.parent_folder = :folder AND grouping.blocked IS NOT NULL
      AND grouping.key != :key
    ORDER BY grouping.key
"#
);

// Why the grouping `key` cannot be worked on, as its columns say.
coven::read!(
    writer StoredGroupingBlock(key: CandidateKey),
    r#"
    SELECT grouping.blocked, grouping.blocked_subject, grouping.blocked_holder
    FROM release_grouping AS grouping
    WHERE grouping.key = :key
"#
);

// Whether a scanned release, or a release this device imported, reads the
// files that hash to `content_hash`.
coven::read!(
    writer ContentInUse(content_hash: String) { in_use: bool },
    r#"
    SELECT EXISTS (SELECT 1
                   FROM @crate::tables::folder_scans::row_fragments::HeldCandidates AS held
                   WHERE held.content_hash = :content_hash)
        OR EXISTS (SELECT 1
                   FROM @crate::tables::import_origin::fragments::ReleaseOrigin AS origin
                   WHERE origin.origin_hash = :content_hash) AS in_use
"#
);

/// Forget every grouping anchored at or taking a release from a folder at or
/// below `folders`. Returns the keys of the grouping releases that were listed
/// and went with them.
pub(super) fn forget_at_or_below(
    sql: &WriteContext<'_>,
    folders: &super::watched_folders::FoldersAtOrBelow,
) -> Result<Vec<CandidateKey>, DbError> {
    let groupings = bae_store::statement_rows(
        sql,
        &GroupingsAtOrBelowParams {
            root: folders.folder.clone(),
            below_first: folders.below_first.clone(),
            below_past: folders.below_past.clone(),
        },
    )?;
    let mut removed = Vec::new();
    for grouping in groupings {
        sql.run(&DeleteGroupingParams { key: &grouping.key })?;
        if grouping.listed {
            removed.push(grouping.key);
        }
    }
    Ok(removed)
}

/// Rebuild the release of every grouping with no anchor that takes in one of
/// `touched` (releases just written or removed) or sits in one of `sidecars`
/// (folders whose sidecar was just written or removed).
pub(super) fn rebuild_groupings(
    sql: &WriteContext<'_>,
    touched: &[CandidateKey],
    sidecars: &[PathBuf],
) -> Result<GroupingChanges, DbError> {
    let touched = crate::tables::key_hex_list(touched.iter());
    let sidecars = path_column::hex_list(sidecars.iter().map(PathBuf::as_path));
    let groupings: Vec<CandidateKey> =
        bae_store::statement_rows(sql, &GroupingsTouchedParams { touched, sidecars })?
            .into_iter()
            .map(|grouping| grouping.key)
            .collect();
    let mut changes = GroupingChanges::default();
    for grouping in &groupings {
        changes.extend(rebuild_grouping(sql, grouping)?);
    }
    Ok(changes)
}

/// Build and store the release of the grouping `key` from the releases it
/// takes in, or record on the grouping why it cannot be built and leave its
/// release as last built.
fn rebuild_grouping(
    sql: &WriteContext<'_>,
    key: &CandidateKey,
) -> Result<GroupingChanges, DbError> {
    let member_keys: Vec<CandidateKey> =
        bae_store::statement_rows(sql, &GroupingMembersParams { key: key.clone() })?
            .into_iter()
            .map(|member| member.member_key)
            .collect();
    let mut members = Vec::with_capacity(member_keys.len());
    for member in &member_keys {
        match folder_scans::load_item_by_key(sql, member, RowSources::Any)? {
            Some((_, stored)) => match stored.item {
                ScanItem::Valid(candidate) => members.push(candidate),
                ScanItem::Discovered(candidate) => {
                    return blocked(
                        sql,
                        key,
                        &GroupingBlock::SourceChanged { folder: candidate.name },
                    );
                }
                ScanItem::Invalid(candidate) => {
                    return blocked(
                        sql,
                        key,
                        &GroupingBlock::SourceChanged { folder: candidate.name },
                    );
                }
            },
            None => {
                return blocked(sql, key, &GroupingBlock::SourceGone { folder: member.to_string() })
            }
        }
    }
    // Its release is listed under the watched folder its first member is.
    let root =
        members.first().map(|member| member.watched_folder_path.clone()).ok_or_else(|| {
            DbError::unreadable(format!("the release_grouping_member rows of {key}"), 0)
        })?;
    // Record where the grouping sits before anything can block it, so other
    // groupings in that folder find it; leaving a folder rebuilds the ones
    // blocked there.
    let sits_in = crate::grouping::shared_parent(
        members
            .iter()
            .map(|member| (member.watched_folder_path.as_path(), member.file_root.as_path())),
    );
    let sits_in_bytes = sits_in.as_deref().map(coven::path_bytes);
    let seat = bae_store::statement_row(sql, &GroupingSeatParams { key: key.clone() })?;
    let (sat_in, was_reading) = (seat.parent_folder, seat.reads_parent_files);
    let mut changes = GroupingChanges::default();
    if sat_in != sits_in_bytes {
        sql.run(&MoveGroupingParams { parent_folder: sits_in_bytes.as_deref(), key })?;
        if let Some(left) = sat_in.clone() {
            changes.extend(rebuild_blocked_in(sql, key, &path_column::decode(left)?)?);
        }
    }
    let reading = sat_in == sits_in_bytes && was_reading;
    let parent = match parent_files_on(sql, key, sits_in.as_deref(), reading)? {
        Ok(parent) => parent,
        Err(conflict) => {
            changes.extend(blocked(sql, key, &conflict)?);
            return Ok(changes);
        }
    };
    let composed = crate::grouping::compose(
        key,
        &root,
        &members
            .into_iter()
            .map(|member| (member, crate::folder_scanner::CandidateFileEdits::default()))
            .collect::<Vec<_>>(),
        parent.valid(),
    );
    let mut candidate = match composed {
        Ok((candidate, _)) => candidate,
        Err(error) => {
            changes.extend(blocked(
                sql,
                key,
                &GroupingBlock::Unbuildable { detail: error.to_string() },
            )?);
            return Ok(changes);
        }
    };
    let reads_parent_files = !matches!(parent, ParentFiles::None);
    let item = match parent {
        ParentFiles::Invalid(parent_files) => {
            let mut files: Vec<SkippedFile> = candidate
                .files
                .files
                .iter()
                .map(SkippedFile::of_release_file)
                .chain(parent_files)
                .collect();
            files.sort_by(|a, b| {
                bae_util::natural_order::compare(&a.relative_path, &b.relative_path)
            });
            invalid(candidate, files)
        }
        // Its folders' names show escaped, so two of its files can show one
        // name, which a release can't hold (`InvalidReason::SameName`).
        ParentFiles::None | ParentFiles::Files(_)
            if crate::folder_scanner::shared_name(
                candidate.files.files.iter().map(|entry| entry.file.relative_path.as_str()),
            )
            .is_some() =>
        {
            let files = candidate.files.files.iter().map(SkippedFile::of_release_file).collect();
            invalid(candidate, files)
        }
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
                Err(unreadable) => {
                    let files = candidate
                        .files
                        .files
                        .iter()
                        .map(|entry| {
                            let mut file = SkippedFile::of_release_file(entry);
                            if file.relative_path == unreadable.path {
                                file.role =
                                    ProposedRole::DamagedAudio { damage: AudioDamage::Unreadable };
                            }
                            file
                        })
                        .collect();
                    invalid(candidate, files)
                }
            }
        }
    };
    sql.run(&UnblockGroupingParams { reads_parent_files, key })?;
    // Its members are entries of the root's scan, which a root holds only
    // once it was read.
    let generation = folder_scans::root_generation_on(sql, &root)?.ok_or_else(|| {
        DbError::unreadable(
            format!(
                "folder_scan_roots.generation of {}, which holds scanned entries",
                coven::path_text(&root)
            ),
            None::<i64>,
        )
    })?;
    let written = folder_scans::write_entry(
        sql,
        &root,
        generation,
        &folder_scans::ScanItemToWrite {
            item: item.clone(),
            seed: folder_scans::ScanItemSeed::Tags(None),
            folder_date: None,
        },
        EntrySource::Grouping,
    )?;
    match written {
        folder_scans::EntryWrite::Stored { .. } => changes.written.push(item),
        folder_scans::EntryWrite::Unchanged => {}
        // Only an item seeded as unchanged finds its entry moved.
        folder_scans::EntryWrite::Moved => {
            panic!("the grouping {key}, stored with no seed taken, found its entry moved")
        }
    }
    Ok(changes)
}

/// The release `candidate` names, as one that cannot be imported, holding
/// `files`. Its tags are not read: a grouping is built from stored rows.
fn invalid(candidate: FolderCandidate, files: Vec<SkippedFile>) -> ScanItem {
    ScanItem::Invalid(InvalidCandidate {
        path: candidate.path,
        name: candidate.name,
        watched_folder_path: candidate.watched_folder_path,
        relative_folder: candidate.relative_folder,
        grouping: candidate.grouping,
        files,
        tags: None,
    })
}

/// The files of the folder a grouping sits in, as its release reads them.
enum ParentFiles {
    /// It sits in no one folder, or that folder has no sidecar.
    None,
    Files(Vec<CandidateFile>),
    /// One of the folder's files is broken, or two of them show one name, so
    /// the release cannot be imported: every one of them, as the scan read it.
    Invalid(Vec<SkippedFile>),
}

impl ParentFiles {
    fn valid(&self) -> &[CandidateFile] {
        match self {
            Self::Files(files) => files,
            Self::None | Self::Invalid(_) => &[],
        }
    }
}

// The other release reading the files of the folder `folder` that a grouping
// sits in, by its name, found by the index of each folder's one reader.
coven::read!(
    writer ParentFilesReader(folder: Vec<u8>, key: CandidateKey) { reader: String },
    r#"
    SELECT COALESCE(candidate.name, coven_path_text(other.key)) AS reader
    FROM release_grouping AS other
    LEFT JOIN @crate::tables::folder_scans::row_fragments::HeldCandidates AS candidate
        ON candidate.path = other.key
    WHERE other.parent_folder = :folder AND other.reads_parent_files = 1 AND other.key != :key
"#
);

// Whether a grouping but `key` sits in the folder `folder`.
coven::read!(
    writer FolderContested(folder: Vec<u8>, key: CandidateKey) { contested: bool },
    r#"
    SELECT EXISTS (
        SELECT 1
        FROM release_grouping AS other
        WHERE other.parent_folder = :folder AND other.key != :key
    ) AS contested
"#
);

/// What the grouping `key`, sitting in `folder`, reads of that folder's files,
/// or why it cannot read them yet; `reading` means it already reads them.
///
/// At most one release reads a folder's files: while another grouping sits in
/// a folder with a sidecar, only the one already reading it may. Files still
/// downloading are read by no release until the download ends.
fn parent_files_on(
    sql: &impl bae_store::WriterRead,
    key: &CandidateKey,
    folder: Option<&Path>,
    reading: bool,
) -> Result<Result<ParentFiles, GroupingBlock>, DbError> {
    let Some(folder) = folder else {
        return Ok(Ok(ParentFiles::None));
    };
    let Some(sidecar) = folder_scans::load_sidecar(sql, folder)? else {
        return Ok(Ok(ParentFiles::None));
    };
    let named = bae_util::path_text::name_text(folder);
    if !reading {
        let reader =
            ParentFilesReaderParams { folder: coven::path_bytes(folder), key: key.clone() };
        let reader = bae_store::statement_optional(sql, &reader)?.map(|other| other.reader);
        if let Some(reader) = reader {
            return Ok(Err(GroupingBlock::FolderFilesTaken { folder: named, release: reader }));
        }
        let contested =
            FolderContestedParams { folder: coven::path_bytes(folder), key: key.clone() };
        let contested = bae_store::statement_row(sql, &contested)?.contested;
        if contested {
            return Ok(Err(GroupingBlock::FolderFilesContested { folder: named }));
        }
    }
    Ok(match sidecar.files {
        SidecarFiles::Downloading => Err(GroupingBlock::FolderFilesDownloading { folder: named }),
        SidecarFiles::Valid(files) => Ok(ParentFiles::Files(files)),
        SidecarFiles::Invalid(files) => Ok(ParentFiles::Invalid(files)),
    })
}

/// Rebuild each grouping but `key` sitting in `folder` that is blocked, now
/// that `key` no longer stands in the way of reading the folder's files.
fn rebuild_blocked_in(
    sql: &WriteContext<'_>,
    key: &CandidateKey,
    folder: &Path,
) -> Result<GroupingChanges, DbError> {
    let blocked: Vec<CandidateKey> = bae_store::statement_rows(
        sql,
        &BlockedInFolderParams { folder: coven::path_bytes(folder), key: key.clone() },
    )?
    .into_iter()
    .map(|grouping| grouping.key)
    .collect();
    let mut changes = GroupingChanges::default();
    for blocked in blocked {
        changes.extend(rebuild_grouping(sql, &blocked)?);
    }
    Ok(changes)
}

/// Say why grouping `key`'s release cannot be worked on as it stands.
fn blocked(
    sql: &WriteContext<'_>,
    key: &CandidateKey,
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
    sql.run(&BlockGroupingParams { kind, subject, holder: holder.map(String::as_str), key })?;
    Ok(GroupingChanges::default())
}

/// How an anchored grouping stored as `combined` reads its folder.
fn decision_of(combined: bool) -> FolderReleaseDecision {
    if combined {
        FolderReleaseDecision::CombineAsOneRelease
    } else {
        FolderReleaseDecision::KeepAsSeparateReleases
    }
}

/// Refuse to take `member`, as the caller read it, into a new grouping,
/// read inside the write that takes it: the scan must still store it as read,
/// no grouping may hold it, it may not be in the library, and nothing may be
/// importing or identifying it.
fn require_combinable_on(
    sql: &WriteContext<'_>,
    member: &FolderCandidate,
) -> Result<(), crate::ImportError> {
    let key = member.key();
    let entry = load_candidate_entry_on(sql, &key)?
        .filter(|entry| entry.settled && entry.candidate == *member)
        .ok_or_else(|| CombineRefusal::MemberChanged(member.name.clone()))?;
    if !entry.listed() || entry.imported.is_some() {
        return Err(CombineRefusal::MemberTaken(member.name.clone()).into());
    }
    require_combination_idle(&key, &entry.runtime)
}

/// Refuse to combine or separate the release at `key` while an import owns it
/// or it is waiting for or running an identification, as `runtime` says.
pub(crate) fn require_combination_idle(
    key: &CandidateKey,
    runtime: &crate::TriageRuntimeFacts,
) -> Result<(), crate::ImportError> {
    if runtime.importing() {
        return Err(crate::ImportError::CandidateImportInProgress);
    }
    if matches!(
        runtime.identification,
        Some(crate::IdentificationStatus::Queued | crate::IdentificationStatus::Running)
    ) {
        return Err(crate::ImportError::Internal {
            detail: format!("identification is still running for {key}"),
        });
    }
    Ok(())
}

/// Set the release a grouping reads as one aside, or back; `false` when it
/// already was, or no grouping has `key`.
fn set_grouping_skipped_on(
    sql: &WriteContext<'_>,
    key: &CandidateKey,
    skipped: bool,
) -> Result<bool, DbError> {
    let changed = sql.run(&SkipGroupingParams { skipped, key })? == 1;
    if changed && skipped {
        super::import_state::change_identification_on(
            sql,
            key,
            &crate::candidate_runtime::IdentificationChange::Ended,
        )?;
    }
    Ok(changed)
}

/// Why grouping `key`'s release cannot be worked on, when something says so.
pub(super) fn load_block_on(
    sql: &impl bae_store::WriterRead,
    key: &CandidateKey,
) -> Result<Option<GroupingBlock>, DbError> {
    let columns =
        bae_store::statement_optional(sql, &StoredGroupingBlockParams { key: key.clone() })?
            .map(|grouping| (grouping.blocked, grouping.blocked_subject, grouping.blocked_holder));
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
        DbError::unreadable(
            format!("release_grouping.blocked_subject of a {kind} block"),
            None::<String>,
        )
    })?;
    Ok(Some(match (kind.as_str(), holder) {
        ("source_changed", None) => GroupingBlock::SourceChanged { folder: subject },
        ("source_gone", None) => GroupingBlock::SourceGone { folder: subject },
        ("folder_files_taken", Some(release)) => {
            GroupingBlock::FolderFilesTaken { folder: subject, release }
        }
        ("folder_files_contested", None) => GroupingBlock::FolderFilesContested { folder: subject },
        ("folder_files_downloading", None) => {
            GroupingBlock::FolderFilesDownloading { folder: subject }
        }
        ("unbuildable", None) => GroupingBlock::Unbuildable { detail: subject },
        (other, holder) => {
            return Err(DbError::unreadable(
                "release_grouping.blocked and blocked_holder",
                (other, holder),
            ))
        }
    }))
}

impl ImportTables {
    /// Set the release at `key` aside, or back; `false` when it already was.
    /// A release a grouping reads as one carries the skip on its grouping's
    /// row, any other in the skip table. Set aside, the release is not
    /// identification's to answer: its identification ends with the skip.
    /// Refused unless the release may be worked on, read inside the write.
    pub(crate) async fn set_candidate_skipped(
        &self,
        key: &CandidateKey,
        skipped: bool,
    ) -> Result<bool, crate::ImportError> {
        let key = key.clone();
        self.store
            .write(move |sql| {
                let sql = &sql;
                let entry = require_workable_on(sql, &key)?;
                if entry.skipped == skipped {
                    return Ok::<_, crate::ImportError>(false);
                }
                Ok(match &entry.candidate.grouping {
                    Some(_) => set_grouping_skipped_on(sql, &key, skipped)?,
                    None => super::skipped_candidates::set_folder_skipped_on(
                        sql,
                        &entry.candidate.path,
                        skipped,
                    )?,
                })
            })
            .await
    }

    /// [`set_grouping_skipped_on`] in a transaction of its own, whatever the
    /// grouping's release stands at.
    #[cfg(test)]
    pub(crate) async fn set_grouping_skipped(
        &self,
        key: &CandidateKey,
        skipped: bool,
    ) -> Result<bool, DbError> {
        let key = key.clone();
        self.store.call(move |sql| set_grouping_skipped_on(sql, &key, skipped)).await
    }

    /// The release stored at `key`, whatever its standing: a candidate being
    /// imported or already imported still shows its folder's files.
    pub(crate) async fn load_folder_candidate(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<FolderCandidate>, DbError> {
        Ok(self.load_candidate_entry(key).await?.map(|entry| entry.candidate))
    }

    /// The release stored at `key`, when it may be worked on — or why a
    /// grouping's release cannot be.
    pub(crate) async fn load_release_candidate(
        &self,
        key: &CandidateKey,
    ) -> Result<Result<Option<FolderCandidate>, GroupingBlock>, DbError> {
        Ok(match self.load_candidate_entry(key).await?.map(CandidateEntry::workable) {
            None => Ok(None),
            Some(workable) => workable.map(|entry| entry.map(|entry| entry.candidate)),
        })
    }

    /// Combine `members`, as the caller read them, into one release under the
    /// new grouping `key`, listed under the first member's watched folder;
    /// they leave the queue in the same write. An error, writing nothing,
    /// when the folders cannot be read as one release, the files of the
    /// folder they all sit in cannot be read, a member changed or is already
    /// grouped or imported, or the combined release already exists.
    pub(crate) async fn combine_releases(
        &self,
        key: CandidateKey,
        members: Vec<FolderCandidate>,
    ) -> Result<GroupingChanges, crate::ImportError> {
        if members.len() < 2 {
            return Err(CombineRefusal::TooFewFolders.into());
        }
        let root_for_compose = members[0].watched_folder_path.clone();
        let key_for_compose = key.clone();
        self.store
            .write(move |sql| {
                let sql = &sql;
                // Each folder's file decisions, which the release starts from.
                let mut with_edits = Vec::with_capacity(members.len());
                for member in &members {
                    let edits = super::import_state::load_candidate_file_edits_on(
                        sql,
                        &member.files.content_hash(),
                    )?()?;
                    with_edits.push((member.clone(), edits));
                }
                let sits_in = crate::grouping::shared_parent(members.iter().map(|member| {
                    (member.watched_folder_path.as_path(), member.file_root.as_path())
                }));
                let parent = parent_files_on(sql, &key_for_compose, sits_in.as_deref(), false)?
                    .map_err(|reason| crate::ImportError::GroupingBlocked { reason })?;
                let (composed, inherited) = crate::grouping::compose(
                    &key_for_compose,
                    &root_for_compose,
                    &with_edits,
                    parent.valid(),
                )?;
                // A release reading a broken folder file is invalid: it holds no
                // files another release could share and no layout to start from.
                if !matches!(parent, ParentFiles::Invalid(_)) {
                    let content_hash = composed.files.content_hash();
                    let in_use = ContentInUseParams { content_hash: content_hash.clone() };
                    let in_use = bae_store::statement_row(sql, &in_use)?.in_use;
                    if in_use {
                        return Err(CombineRefusal::AlreadyPresent.into());
                    }
                    super::import_state::restart_state_on(sql, &content_hash, &key, &inherited)?;
                }
                for member in &members {
                    require_combinable_on(sql, member)?;
                }
                sql.run(&InsertPickedGroupingParams { key: &key })?;
                for (position, member) in members.iter().enumerate() {
                    sql.run(&InsertGroupingMemberParams {
                        grouping: &key,
                        position: position as i64,
                        member: &member.key(),
                        folder: &coven::path_bytes(&member.path),
                    })?;
                    // A folder read into the combined release leaves the queue, and
                    // its identification ends with it.
                    super::import_state::change_identification_on(
                        sql,
                        &member.key(),
                        &crate::candidate_runtime::IdentificationChange::Ended,
                    )?;
                }
                let mut changes = rebuild_grouping(sql, &key)?;
                if changes
                    .written
                    .iter()
                    .any(|item| matches!(item, ScanItem::Valid(release) if release.key() == key))
                {
                    changes.found.push(key.clone());
                }
                if changes.written.is_empty() {
                    // Nothing of it is kept: the whole write rolls back.
                    return Err(match load_block_on(sql, &key)? {
                        Some(reason) => crate::ImportError::GroupingBlocked { reason },
                        None => CombineRefusal::BuiltNothing(key.clone()).into(),
                    });
                }
                Ok(changes)
            })
            .await
    }

    /// Undo the grouping with no anchor at `key`, removing its release. Returns
    /// the releases it took in, as stored, and what rebuilding the groupings
    /// blocked in its folder changed.
    pub(crate) async fn separate_picked_grouping(
        &self,
        key: &CandidateKey,
    ) -> Result<(Vec<ScanItem>, GroupingChanges), crate::ImportError> {
        let key = key.clone();
        self.store
            .write(move |sql| {
                let sql = &sql;
                require_combination_idle(
                    &key,
                    &crate::tables::import_state::load_standing_on(sql, &key)?.facts(),
                )?;
                let members: Vec<CandidateKey> =
                    bae_store::statement_rows(sql, &GroupingMembersParams { key: key.clone() })?
                        .into_iter()
                        .map(|member| member.member_key)
                        .collect();
                let sat_in =
                    bae_store::statement_optional(sql, &GroupingSeatParams { key: key.clone() })?
                        .and_then(|seat| seat.parent_folder)
                        .map(path_column::decode)
                        .transpose()?;
                let removed = sql.run(&DeletePickedGroupingParams { key: &key })?;
                if removed == 0 {
                    return Err(SeparationRefusal::NotPicked(key).into());
                }
                // The release the grouping read leaves the scan with it, and its
                // identification with it.
                super::import_state::end_file_work_unless_of_on(sql, &key, None)?;
                let mut returned = Vec::with_capacity(members.len());
                for member in members {
                    if let Some((_, stored)) =
                        folder_scans::load_item_by_key(sql, &member, RowSources::Any)?
                    {
                        returned.push(stored.item);
                    }
                }
                let regrouped = match sat_in {
                    Some(folder) => rebuild_blocked_in(sql, &key, &folder)?,
                    None => GroupingChanges::default(),
                };
                Ok((returned, regrouped))
            })
            .await
    }
}

#[cfg(test)]
#[path = "release_groupings_tests.rs"]
pub(crate) mod tests;
