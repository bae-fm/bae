//! One action a selection offers. The query `tables::import_list::selection`
//! decides, from what each member offers, which actions a selection offers on
//! every surface.

use super::CandidateAction;

/// One action a selection or a row offers, the number every surface shows
/// beside it, and whether it can run as the selection stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionOffer {
    pub action: CandidateAction,
    /// How many rows the action runs on: the rows that offer it, or, for
    /// Combine, every selected row while every one can join one release and
    /// none otherwise.
    pub count: u64,
    pub enabled: bool,
}
