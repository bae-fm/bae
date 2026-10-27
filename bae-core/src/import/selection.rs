//! The import list's selection: what it holds, how a person changes it, and
//! what it can be told to do.
//!
//! Each list holds its selection in a table beside its view
//! (`import_list_selection`), and it only ever holds rows the list shows: the
//! write that stops the list showing a row, by a change to the row or to the
//! view, takes the row out of the selection with it. What it offers is read
//! from the selected rows' placement and what is running for each, and a bulk
//! action runs in core over the selected keys that offer it.

use super::triage::{CandidateActionBasis, SelectionMember, SelectionOffer, TriageRuntimeFacts};
use std::collections::HashMap;

/// How a person changed the selection by pointing at rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionChange {
    /// Select exactly these rows, and nothing else.
    Replace { keys: Vec<String> },
    /// Add these rows and take those out, leaving the rest as it is.
    Toggle {
        add: Vec<String>,
        remove: Vec<String>,
    },
    /// Add every row the list shows from `from` to `to`, both included.
    Extend { from: String, to: String },
}

/// One selected candidate as the tables place it: what its commands are
/// decided from, and the name a report of a bulk action gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCandidate {
    pub candidate_key: String,
    pub name: String,
    pub basis: CandidateActionBasis,
}

/// Every selected row as the tables place it: the candidates, and the folders
/// that failed validation, which the person can select to look into but which
/// offer nothing to do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SelectedRows {
    pub candidates: Vec<SelectedCandidate>,
    /// The keys of the selected folders that failed validation.
    pub invalid: Vec<String>,
}

/// The one selected row, when exactly one is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SingleSelection {
    Candidate {
        key: String,
    },
    /// A folder that failed validation.
    InvalidCandidate {
        key: String,
    },
}

/// What the selection holds and can be told to do, as it stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SelectionSummary {
    pub count: u64,
    pub single: Option<SingleSelection>,
    pub offers: Vec<SelectionOffer>,
}

/// The selected candidates with the actions each offers now.
pub(crate) fn members(
    selected: &[SelectedCandidate],
    facts: &HashMap<String, TriageRuntimeFacts>,
) -> Vec<SelectionMember> {
    selected
        .iter()
        .map(|candidate| SelectionMember {
            candidate_key: candidate.candidate_key.clone(),
            actions: candidate.basis.actions(
                &facts
                    .get(&candidate.candidate_key)
                    .cloned()
                    .unwrap_or_default(),
            ),
        })
        .collect()
}
