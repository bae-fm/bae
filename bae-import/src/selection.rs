//! The import list's selection: what it holds and what the surface can ask it
//! to do.
//!
//! The surface keeps the keys of the rows the person selected, and passes
//! them to each read and action on them. Core reads what the selected rows
//! hold and offer from their rows' placement and from what is running for
//! each, including rows the list does not show. A bulk action runs in core
//! over the selected rows the list shows that offer it.

use super::triage::SelectionOffer;
use super::CandidateKey;

/// One selected candidate, and the name a report of a bulk action gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedCandidate {
    pub candidate_key: CandidateKey,
    pub name: String,
}

/// Every selected row as the tables place it: the candidates, and the folders
/// that failed validation, which the person can select to look into but which
/// offer nothing to do.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SelectedRows {
    pub candidates: Vec<SelectedCandidate>,
    /// The keys of the selected folders that failed validation.
    pub invalid: Vec<CandidateKey>,
}

/// The one selected row, when exactly one is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SingleSelection {
    Candidate {
        key: CandidateKey,
    },
    /// A folder that failed validation.
    InvalidCandidate {
        key: CandidateKey,
    },
}

/// What the selection holds and can be told to do, as it stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SelectionSummary {
    pub count: u64,
    pub single: Option<SingleSelection>,
    pub offers: Vec<SelectionOffer>,
}
