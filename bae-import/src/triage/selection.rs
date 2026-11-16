//! One action a selection offers. The query `tables::import_list::selection`
//! decides, from what each member offers, which actions a selection offers on
//! every surface.

use super::CandidateAction;

/// One action a selection offers, how many members it applies to, and whether
/// it can run as the selection stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionOffer {
    pub action: CandidateAction,
    pub count: u64,
    pub enabled: bool,
}
