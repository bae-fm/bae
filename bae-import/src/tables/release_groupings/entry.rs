//! A scanned candidate's entry and where it stands, read as one statement
//! ([`EnteredCandidate`]): the release the scan or a grouping stores at a
//! key, whether a grouping took it in or is blocked, whether the person set
//! it aside, the release this device imported from its files, and what is
//! running for it. A read outside a write and a write that depends on these
//! rows run the same statement.

use super::fragments::{EnteredCandidate, GroupingOfCandidate};
use super::*;
use crate::tables::import_origin::fragments::ImportedRelease;
use crate::tables::import_state::fragments::Standing;
use crate::tables::PreparationRefusal;
use crate::{CandidateStanding, ImportError, TriageRuntimeFacts};
use bae_store::statement_optional;

coven::read!(
    writer CandidateEntryAt(key: crate::CandidateKey),
    r#"
    SELECT entry.*
    FROM @crate::tables::release_groupings::fragments::EnteredCandidate(key: :key) AS entry
"#
);

/// A release stored at a key, and where it stands.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateEntry {
    pub candidate: FolderCandidate,
    /// The scan generation that wrote the entry.
    pub generation: u64,
    /// Whether the scan settled on the release; one found before the folder
    /// around it was known is tentative.
    pub settled: bool,
    /// Whether a grouping took the release in.
    taken_in: bool,
    /// Why the release cannot be worked on, when it is a grouping's release
    /// that is blocked.
    pub block: Option<GroupingBlock>,
    /// Whether the person set it aside.
    pub skipped: bool,
    /// The release this device imported from its files, if it did.
    pub imported: Option<String>,
    /// What is running for it, an import's claim among it.
    pub runtime: TriageRuntimeFacts,
}

/// What the scan stores at a key.
#[derive(Debug)]
pub(crate) enum ScannedEntry {
    Release(Box<CandidateEntry>),
    /// A folder that failed validation, which stands nowhere in the queue.
    InvalidFolder,
}

impl CandidateEntry {
    /// The entry a read's fragments hold, or `None` for a folder that failed
    /// validation.
    pub(crate) fn of(
        scan: crate::tables::folder_scans::fragments::ScanItems,
        grouping: Option<GroupingOfCandidate>,
        standing: Option<Standing>,
        imported: Option<ImportedRelease>,
    ) -> Result<Option<Self>, DbError> {
        let key = scan.path.clone();
        let grouping = grouping.ok_or_else(|| {
            DbError::unreadable(format!("whether a grouping took {key} in"), None::<String>)
        })?;
        let scan = folder_scans::CandidateRow::of(scan)?;
        let root = scan.root().to_path_buf();
        let stored = scan.item()?;
        let generation = stored.generation;
        folder_scans::stored_entry_belongs(&root, &stored.key, &stored.item)?;
        let (candidate, settled) = match stored.item {
            ScanItem::Valid(candidate) => (candidate, true),
            ScanItem::Discovered(candidate) => (candidate, false),
            ScanItem::Invalid(_) => return Ok(None),
        };
        let skipped = grouping.skipped.ok_or_else(|| {
            DbError::unreadable(format!("the release_grouping row of {key}"), &candidate.grouping)
        })?;
        let block = block_of(grouping.blocked, grouping.blocked_subject, grouping.blocked_holder)?;
        let runtime = crate::tables::import_state::standing_of(standing, &key)?.facts();
        Ok(Some(Self {
            candidate,
            generation,
            settled,
            taken_in: grouping.taken_in,
            block,
            skipped,
            imported: imported.map(ImportedRelease::into_release_id),
            runtime,
        }))
    }

    /// Where the release stands in the queue.
    pub(crate) fn standing(&self) -> CandidateStanding {
        CandidateStanding {
            skipped: self.skipped,
            imported: self.imported.clone(),
            claimed: self.runtime.importing(),
        }
    }

    /// Whether the import list shows the release: the scan settled on it and
    /// no grouping took it in.
    pub(crate) fn listed(&self) -> bool {
        self.settled && !self.taken_in
    }

    /// Whether the release may be worked on: listed, and not blocked.
    pub(crate) fn actionable(&self) -> bool {
        self.listed() && self.block.is_none()
    }

    /// The entry when its release may be worked on, `None` when it may not,
    /// or why a grouping's release cannot be.
    pub(crate) fn workable(self) -> Result<Option<Self>, GroupingBlock> {
        match self.block {
            Some(block) => Err(block),
            None => Ok(self.actionable().then_some(self)),
        }
    }
}

/// What the scan stores at `key`: a release and where it stands, or a folder
/// that failed validation; `None` when it stores nothing there. Watched roots
/// never overlap and keys are absolute paths, so at most one root holds it.
fn load_scanned_entry_on(
    sql: &impl bae_store::WriterRead,
    key: &CandidateKey,
) -> Result<Option<ScannedEntry>, DbError> {
    let row = statement_optional(sql, &CandidateEntryAtParams { key: key.clone() })?;
    row.map(|row| scanned_entry_of(row.entry)).transpose()
}

/// What a read of [`EnteredCandidate`] returned holds.
fn scanned_entry_of(row: EnteredCandidate) -> Result<ScannedEntry, DbError> {
    let EnteredCandidate { scan, grouping, standing, imported } = row;
    Ok(match CandidateEntry::of(scan, grouping, standing, imported)? {
        Some(entry) => ScannedEntry::Release(Box::new(entry)),
        None => ScannedEntry::InvalidFolder,
    })
}

/// The release at `key` that the import list shows, from what a read of
/// [`EnteredCandidate`] returned there; refused when the list shows none
/// there.
pub(crate) fn listed_entry_of(
    key: &CandidateKey,
    row: Option<EnteredCandidate>,
) -> Result<CandidateEntry, ImportError> {
    match row.map(scanned_entry_of).transpose()? {
        Some(ScannedEntry::Release(entry)) if entry.listed() => Ok(*entry),
        _ => Err(ImportError::Internal {
            detail: format!("{key} is not a scanned folder candidate"),
        }),
    }
}

/// The release stored at `key` and where it stands, or `None` when the scan
/// stores no release there: nothing, or a folder that failed validation.
pub(crate) fn load_candidate_entry_on(
    sql: &impl bae_store::WriterRead,
    key: &CandidateKey,
) -> Result<Option<CandidateEntry>, DbError> {
    Ok(match load_scanned_entry_on(sql, key)? {
        Some(ScannedEntry::Release(entry)) => Some(*entry),
        Some(ScannedEntry::InvalidFolder) | None => None,
    })
}

/// The release at `key` for a write that works on it, read inside that
/// write. Refused when a grouping's release is blocked, or when the key names
/// no release that may be worked on.
pub(crate) fn require_workable_on(
    sql: &impl bae_store::WriterRead,
    key: &CandidateKey,
) -> Result<CandidateEntry, ImportError> {
    load_candidate_entry_on(sql, key)?
        .map(CandidateEntry::workable)
        .transpose()
        .map_err(|reason| ImportError::GroupingBlocked { reason })?
        .flatten()
        .ok_or_else(|| ImportError::Internal {
            detail: format!("{key} is not a scanned candidate"),
        })
}

/// The release at `key` for a write prepared against the files hashed
/// `content_hash` at file decision revision `file_edit_revision`, read inside
/// that write. Refused as [`PreparationRefusal::Changed`] when the scan no
/// longer lists the release there with those files, and otherwise as a
/// release is that is blocked, imported or claimed by an import.
pub(crate) fn require_editable_files_on(
    sql: &impl bae_store::WriterRead,
    key: &CandidateKey,
    content_hash: &str,
    file_edit_revision: u64,
) -> Result<CandidateEntry, ImportError> {
    let entry = load_candidate_entry_on(sql, key)?
        .filter(|entry| {
            entry.listed()
                && entry.candidate.file_edit_revision == file_edit_revision
                && entry.candidate.files.content_hash() == content_hash
        })
        .ok_or_else(|| PreparationRefusal::Changed(key.clone()))?;
    if let Some(reason) = entry.block {
        return Err(ImportError::GroupingBlocked { reason });
    }
    entry.standing().editable()?;
    Ok(entry)
}

impl ImportTables {
    /// What the scan stores at `key`; see [`load_scanned_entry_on`].
    pub(crate) async fn load_scanned_entry(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<ScannedEntry>, DbError> {
        let key = key.clone();
        self.store.read(move |sql| load_scanned_entry_on(&sql, &key)).await
    }

    /// The release at `key` that may be worked on; see
    /// [`require_workable_on`].
    pub(crate) async fn require_workable(
        &self,
        key: &CandidateKey,
    ) -> Result<CandidateEntry, ImportError> {
        let key = key.clone();
        self.store.read(move |sql| Ok(require_workable_on(&sql, &key))).await?
    }

    /// The release stored at `key` and where it stands; see
    /// [`load_candidate_entry_on`].
    pub(crate) async fn load_candidate_entry(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<CandidateEntry>, DbError> {
        let key = key.clone();
        self.store.read(move |sql| load_candidate_entry_on(&sql, &key)).await
    }
}

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;
