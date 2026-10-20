//! The import list's selection: what it holds, how a person changes it, and
//! what it can be told to do.
//!
//! The list subscription owns it beside the rows it shows
//! ([`ImportListSubscription`](super::ImportListSubscription)), so it only
//! ever holds rows the list shows: each read of the list that shows fewer rows
//! takes the others out of it in the same step that delivers them. What it
//! offers is read from the selected rows as that read placed them and what is
//! running for each, and a bulk action runs in core over the selected keys
//! that offer it.

use super::triage::{
    selection_offers, CandidateActionBasis, CandidateLiveState, SelectionMember, SelectionOffer,
    TriageRuntimeFacts,
};
use std::collections::{BTreeSet, HashMap};

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

impl SelectionChange {
    /// The selection `selected` becomes, with `shown` the keys of the rows
    /// the list last showed, in its order; `None` when it names a run whose
    /// ends the list does not show. The list's next read takes out any key
    /// it does not show, before anything reads the selection.
    pub(crate) fn applied(
        self,
        selected: &BTreeSet<String>,
        shown: &[String],
    ) -> Option<BTreeSet<String>> {
        let mut next = selected.clone();
        match self {
            Self::Replace { keys } => next = keys.into_iter().collect(),
            Self::Toggle { add, remove } => {
                for key in remove {
                    next.remove(&key);
                }
                next.extend(add);
            }
            Self::Extend { from, to } => {
                let position = |key: &str| shown.iter().position(|shown| shown == key);
                let (from, to) = (position(&from)?, position(&to)?);
                next.extend(shown[from.min(to)..=from.max(to)].iter().cloned());
            }
        }
        Some(next)
    }
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
    Candidate { key: String },
    /// A folder that failed validation.
    InvalidCandidate { key: String },
}

/// What the selection holds and can be told to do, as it stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SelectionSummary {
    pub count: u64,
    pub single: Option<SingleSelection>,
    pub offers: Vec<SelectionOffer>,
}

impl SelectionSummary {
    /// The selection of `selected`, the rows as the list placed them, with
    /// what is running for each.
    pub(crate) fn of(
        selected: &SelectedRows,
        facts: &HashMap<String, TriageRuntimeFacts>,
    ) -> Self {
        let single = match (selected.candidates.as_slice(), selected.invalid.as_slice()) {
            ([only], []) => Some(SingleSelection::Candidate {
                key: only.candidate_key.clone(),
            }),
            ([], [only]) => Some(SingleSelection::InvalidCandidate { key: only.clone() }),
            _ => None,
        };
        // A folder that failed validation takes part in the selection — it
        // holds back combining it — with no action of its own.
        let mut members = members(&selected.candidates, facts);
        members.extend(selected.invalid.iter().map(|key| SelectionMember {
            candidate_key: key.clone(),
            actions: Vec::new(),
        }));
        Self {
            count: (selected.candidates.len() + selected.invalid.len()) as u64,
            single,
            offers: selection_offers(&members),
        }
    }
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
            actions: CandidateLiveState::of(
                &candidate.basis,
                facts
                    .get(&candidate.candidate_key)
                    .cloned()
                    .unwrap_or_default(),
            )
            .actions,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::triage::{CandidateAction, StoredLookup, TriagePlacement};

    fn selected(key: &str, lookup: Option<StoredLookup>) -> SelectedCandidate {
        SelectedCandidate {
            candidate_key: key.to_string(),
            name: key.to_string(),
            basis: CandidateActionBasis::of(
                true,
                &TriagePlacement::Pending,
                true,
                lookup,
                false,
                None,
            ),
        }
    }

    fn identify_count(selected: &[SelectedCandidate]) -> Option<u64> {
        SelectionSummary::of(
            &SelectedRows {
                candidates: selected.to_vec(),
                invalid: Vec::new(),
            },
            &HashMap::new(),
        )
            .offers
            .into_iter()
            .find(|offer| offer.action == CandidateAction::Identify)
            .map(|offer| offer.count)
    }

    /// A selection of identified rows offers no Identify at all.
    #[test]
    fn identified_rows_offer_no_identify() {
        assert_eq!(
            identify_count(&[
                selected("Album A", Some(StoredLookup::Answered)),
                selected("Album B", Some(StoredLookup::Answered)),
            ]),
            None
        );
    }

    /// A mixed selection counts only the rows no lookup is stored for.
    #[test]
    fn a_mixed_selection_counts_only_the_unidentified_rows() {
        assert_eq!(
            identify_count(&[
                selected("Album A", Some(StoredLookup::Answered)),
                selected("Album B", None),
                selected("Album C", Some(StoredLookup::Failed)),
                selected("Album D", None),
            ]),
            Some(2)
        );
    }
}
