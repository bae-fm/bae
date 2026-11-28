//! The candidates the import list lists, read whole for automation: each
//! candidate as the pane reads it, with what is in flight for it and the
//! library statuses of the releases its verdicts offer, and each folder that
//! failed validation as its row shows it. A read answers with two statements,
//! the pane's head beside the rest and the pane's draft, each over every
//! listed candidate it reads, in key order.

use super::pane::{detail_rows, CandidateDetailRows};
use super::pane_read::PaneDraftRows;
use super::*;
use crate::candidate_runtime::{compose, CandidateRuntimeSnapshot};
use crate::tables::import_state::run_rows::runtime_of;
use bae_records::{LibraryCheck, LibraryStatus};

// Each placement of `keys` that the list lists as a candidate or as a folder
// that failed validation: the candidate's pane head, its runtime and the
// library statuses of its stored verdict and of its run in flight, or the
// folder's `ListedRow` document.
coven::fragment!(
    ListedCandidateRows with keys(candidate_key BLOB) {
        listed_candidate_key: crate::CandidateKey,
        listed_root: Vec<u8>,
        listed_item: String,
        invalid_row: Option<String>
    },
    r#"
    SELECT p.key AS listed_candidate_key, p.watched_folder_path AS listed_root,
           p.item AS listed_item,
           CASE WHEN p.item = 'invalid'
               THEN @crate::tables::import_list::row_fragments::ListedRow(parent: p) END
               AS invalid_row,
           head.*, runtime.*, stored_statuses.*, run_statuses.*
    FROM keys
    JOIN import_list_placement AS p
        ON p.key = keys.candidate_key AND p.item IN ('candidate', 'invalid')
    LEFT JOIN @crate::tables::import_list::pane_read::PaneHeadRows(keys: (SELECT * FROM keys))
        AS head ON head.listed_key = p.key
    LEFT JOIN @crate::tables::import_state::run_rows::RuntimeRows(keys: (SELECT * FROM keys))
        AS runtime ON runtime.runtime_key = p.key
    LEFT JOIN @crate::tables::import_state::current_owner::CurrentOwner(
        keys: (SELECT * FROM keys)) AS owner ON owner.owner_key = p.key
    LEFT JOIN @crate::tables::import_state::card_fragments::VerdictLibraryStatuses(
        owner: (SELECT owner.content_hash, owner.identify_run
            FROM @crate::tables::import_state::current_owner::CurrentOwner(
                keys: (SELECT * FROM keys)) AS owner))
        AS stored_statuses ON stored_statuses.statuses_row = owner.content_hash
    LEFT JOIN @crate::tables::import_state::card_fragments::VerdictLibraryStatuses(
        owner: (SELECT NULL AS content_hash, run.identify_run
            FROM @crate::tables::import_state::run_rows::ChosenRun(
                keys: (SELECT * FROM keys)) AS run
            WHERE run.identify_run IS NOT NULL))
        AS run_statuses ON run_statuses.statuses_row = 'run ' || runtime.runtime_run
"#);

coven::read!(
    writer ListedCandidates,
    r#"
    SELECT listed.*
    FROM @crate::tables::import_list::listed_candidates::ListedCandidateRows(keys: (
        SELECT p.key AS candidate_key FROM import_list_placement AS p
        WHERE p.item IN ('candidate', 'invalid'))) AS listed
    ORDER BY listed.listed_candidate_key
"#
);

coven::read!(
    writer ListedCandidateOf(key: crate::CandidateKey),
    r#"
    SELECT listed.*
    FROM @crate::tables::import_list::listed_candidates::ListedCandidateRows(
        keys: (SELECT :key AS candidate_key)) AS listed
"#
);

coven::read!(
    writer ListedCandidateDrafts,
    r#"
    SELECT draft.*
    FROM @crate::tables::import_list::pane_read::PaneDraftRows(keys: (
        SELECT p.key AS candidate_key FROM import_list_placement AS p
        WHERE p.item = 'candidate')) AS draft
    ORDER BY draft.pane_key
"#
);

/// A release the import list lists, as automation reads it.
pub enum ListedCandidate {
    Folder(Box<ListedFolder>),
    /// A folder that failed validation.
    Invalid(crate::triage::InvalidRow),
}

/// A candidate the import list lists, read whole.
pub struct ListedFolder {
    /// The candidate as the pane reads it.
    pub detail: crate::ImportCandidateDetail,
    /// What is in flight for it; `None` when nothing is.
    pub runtime: Option<CandidateRuntimeSnapshot>,
    /// The library statuses of the releases its stored verdict offers or
    /// sets aside.
    pub stored_statuses: VerdictStatuses,
    /// The library statuses of the releases its run in flight offers or sets
    /// aside.
    pub run_statuses: VerdictStatuses,
}

/// The library status of each release a verdict offers or sets aside, with
/// the catalog that names it.
pub struct VerdictStatuses(Vec<(String, LibraryStatus)>);

impl VerdictStatuses {
    /// The status of each of `checks`, in their order, found by catalog and
    /// release id; an error for a check the verdict holds no status for.
    pub fn of(&self, checks: &[LibraryCheck]) -> Result<Vec<LibraryStatus>, crate::ImportError> {
        checks
            .iter()
            .map(|check| {
                self.0
                    .iter()
                    .find(|(source, status)| {
                        source == check.source.as_str() && status.release_id == check.release_id
                    })
                    .map(|(_, status)| status.clone())
                    .ok_or_else(|| {
                        DbError::unreadable(
                            "the library status of a release a verdict offers",
                            format!("{} {}", check.source.as_str(), check.release_id),
                        )
                        .into()
                    })
            })
            .collect()
    }

    /// The statuses of a `VerdictLibraryStatuses` document; none for an
    /// owner the read holds no verdict of.
    fn of_document(document: Option<String>) -> Result<Self, DbError> {
        #[derive(serde::Deserialize)]
        struct Entry {
            source: String,
            release_id: String,
            albums: serde_json::Value,
        }
        let Some(document) = document else {
            return Ok(Self(Vec::new()));
        };
        let entries: Vec<Entry> = serde_json::from_str(&document).map_err(|error| {
            DbError::unreadable_json("a verdict's library statuses", &document, error)
        })?;
        entries
            .into_iter()
            .map(|entry| {
                let status = bae_records::records::fragments::library_status_of_columns(
                    entry.release_id,
                    &entry.albums.to_string(),
                )?;
                Ok((entry.source, status))
            })
            .collect::<Result<_, DbError>>()
            .map(Self)
    }
}

/// The listed candidate of one row of the list's read, with the draft the
/// draft read returned for it, which a candidate not imported here holds.
fn listed_of(
    row: ListedCandidateRows,
    draft: Option<PaneDraftRows>,
) -> Result<ListedCandidate, crate::ImportError> {
    let key = row.listed_candidate_key;
    if row.listed_item == "invalid" {
        let document = row.invalid_row.ok_or_else(|| {
            DbError::unreadable(format!("the listed row of {key}"), None::<String>)
        })?;
        let columns = super::row_columns::release_columns(&document)?;
        return Ok(ListedCandidate::Invalid(super::window::invalid_row_of(
            row.listed_root,
            key,
            columns,
        )?));
    }
    let head = row.head.ok_or_else(|| {
        DbError::unreadable(format!("the pane head of listed {key}"), None::<String>)
    })?;
    let detail = CandidateDetailRows::process(detail_rows(head, draft)?)?;
    let runtime =
        row.runtime.map(runtime_of).transpose()?.and_then(|(key, stored)| compose(&key, &stored));
    Ok(ListedCandidate::Folder(Box::new(ListedFolder {
        detail,
        runtime,
        stored_statuses: VerdictStatuses::of_document(
            row.stored_statuses.map(|statuses| statuses.library_statuses),
        )?,
        run_statuses: VerdictStatuses::of_document(
            row.run_statuses.map(|statuses| statuses.library_statuses),
        )?,
    })))
}

impl ImportTables {
    /// Every release the import list lists, in key order, read with two
    /// statements in one snapshot.
    pub(crate) async fn load_listed_candidates(
        &self,
    ) -> Result<Vec<ListedCandidate>, crate::ImportError> {
        self.store
            .read(|sql| {
                Ok((
                    statement_rows(&sql, &ListedCandidatesParams {})?,
                    statement_rows(&sql, &ListedCandidateDraftsParams {})?,
                ))
            })
            .process(|(rows, drafts)| {
                let mut drafts = drafts.into_iter().map(|row| row.draft).peekable();
                rows.into_iter()
                    .map(|row| {
                        let row = row.listed;
                        let draft = drafts.next_if(|draft| {
                            draft.pane_key.as_ref() == Some(&row.listed_candidate_key)
                        });
                        listed_of(row, draft)
                    })
                    .collect()
            })
            .await
    }

    /// The release the import list lists at `key`; `None` when it lists none
    /// there.
    pub(crate) async fn load_listed_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<ListedCandidate>, crate::ImportError> {
        let key = key.clone();
        self.store
            .read(move |sql| {
                Ok((
                    statement_optional(&sql, &ListedCandidateOfParams { key: key.clone() })?,
                    statement_optional(
                        &sql,
                        &super::pane_read::PaneDraftParams { key: Some(key) },
                    )?,
                ))
            })
            .process(|(row, draft)| {
                row.map(|row| listed_of(row.listed, draft.map(|draft| draft.draft))).transpose()
            })
            .await
    }
}

#[cfg(test)]
#[path = "listed_candidates_tests.rs"]
mod tests;
