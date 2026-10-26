//! Found's filter: one entry chosen at a time, each holding the rows in the
//! states it covers, and how many rows each holds.
//!
//! Every Found row is in exactly one [`PendingState`], and every state is
//! under exactly one entry past All, so those entries split Found's rows
//! between them and All holds them all.
//!
//! The counts are worked out in the read that places the rows, each row in
//! the state its standing and the tables put it in, so they never disagree
//! with the rows the read shows.

use crate::import::triage::PendingState;
use std::collections::HashMap;

/// Which of Found's rows the list shows: one entry of the filter menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PendingFilter {
    /// Every row.
    #[default]
    All,
    /// Every row waiting on the person: the lookup left the answer to them,
    /// or a catalog, bae itself or the import failed.
    NeedsYou,
    /// A run or an import is queued or going.
    InProgress,
    Identified,
    Unmatched,
    NotLookedUp,
}

impl PendingFilter {
    /// Every entry, in the menu's order.
    pub const ENTRIES: [Self; 6] = [
        Self::All,
        Self::NeedsYou,
        Self::InProgress,
        Self::Identified,
        Self::Unmatched,
        Self::NotLookedUp,
    ];

    /// The one entry past All that holds a row in `state`.
    fn covering(state: PendingState) -> Self {
        match state {
            PendingState::NeedsYou
            | PendingState::LookupError
            | PendingState::Error
            | PendingState::ImportError => Self::NeedsYou,
            PendingState::Identifying | PendingState::Importing => Self::InProgress,
            PendingState::Identified => Self::Identified,
            PendingState::Unmatched => Self::Unmatched,
            PendingState::NotLookedUp => Self::NotLookedUp,
        }
    }

    /// Whether this entry holds a row in `state`.
    pub fn holds(self, state: PendingState) -> bool {
        self == Self::All || Self::covering(state) == self
    }
}

/// One entry of the filter menu and how many of Found's rows it holds. An
/// entry holding none cannot be chosen; one already chosen stays chosen, and
/// the list shows it empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingFilterEntry {
    pub filter: PendingFilter,
    pub count: u32,
    pub selectable: bool,
}

/// Every entry of the menu, in its order, with how many of Found's rows it
/// holds, from the state each of them is in.
pub(crate) fn pending_filter_entries(
    states: impl IntoIterator<Item = PendingState>,
) -> Vec<PendingFilterEntry> {
    let mut counts: HashMap<PendingState, u32> = HashMap::new();
    for state in states {
        *counts.entry(state).or_default() += 1;
    }
    PendingFilter::ENTRIES
        .into_iter()
        .map(|filter| {
            let count = counts
                .iter()
                .filter(|(&state, _)| filter.holds(state))
                .map(|(_, count)| count)
                .sum();
            PendingFilterEntry {
                filter,
                count,
                selectable: count > 0,
            }
        })
        .collect()
}
