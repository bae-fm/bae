//! Found's filter: one entry chosen at a time, each holding the rows in the
//! states it covers, and how many rows each holds.
//!
//! Every Found row is in exactly one [`PendingState`], and every state is
//! under exactly one entry past All, so those entries split Found's rows
//! between them and All holds them all.
//!
//! The counts are worked out when the menu opens rather than on every read of
//! the list: what is running for a candidate decides its state over the
//! tables, and carrying it in the list's request for the counts' sake would
//! read the whole list again each time a run or an import starts or ends.

use crate::import::triage::{LiveStanding, PendingState, TriageRow, TriageRuntimeFacts};
use std::collections::{BTreeMap, HashMap};

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

    /// The state what is running puts each candidate in, from every
    /// candidate's runtime facts, while this entry narrows the rows; empty
    /// under All, which shows every row whatever state it is in.
    pub(crate) fn live_standings<'a>(
        self,
        facts: impl IntoIterator<Item = (&'a String, &'a TriageRuntimeFacts)>,
    ) -> BTreeMap<String, LiveStanding> {
        if self == Self::All {
            return BTreeMap::new();
        }
        LiveStanding::of_each(facts)
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

/// The state the tables put each of Found's rows in, by candidate key, as one
/// read of the list placed them: what the filter's counts start from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FoundStates(HashMap<String, PendingState>);

impl FoundStates {
    /// The stored state of each of `rows` placed on Found.
    pub(crate) fn of<'a>(rows: impl IntoIterator<Item = &'a TriageRow>) -> Self {
        Self(
            rows.into_iter()
                .filter_map(|row| {
                    let standing = row.action_basis.standing.as_ref()?;
                    Some((row.candidate_key.clone(), standing.state()))
                })
                .collect(),
        )
    }

    /// Every entry of the menu, in its order, with how many of Found's rows
    /// it holds: each row in the state what is running for its candidate puts
    /// it in, by key in `live_standings`, or else in the one the tables put it
    /// in.
    pub fn entries(&self, live_standings: &BTreeMap<String, LiveStanding>) -> Vec<PendingFilterEntry> {
        let mut states: HashMap<PendingState, u32> = HashMap::new();
        for (key, stored) in &self.0 {
            let state = live_standings.get(key).map_or(*stored, LiveStanding::state);
            *states.entry(state).or_default() += 1;
        }
        PendingFilter::ENTRIES
            .into_iter()
            .map(|filter| {
                let count = states
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
}
