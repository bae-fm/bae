//! One action a selection offers: what a selection of candidates can be told
//! to do is decided in SQL, from what each member offers, for every surface
//! that acts on a selection (`tables::import_list::selection`).

use super::CandidateAction;

/// One action a selection offers, how many members it applies to, and whether
/// it can run as the selection stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionOffer {
    pub action: CandidateAction,
    pub count: u64,
    pub enabled: bool,
}
