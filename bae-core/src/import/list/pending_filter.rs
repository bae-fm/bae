//! Found's filter: one entry chosen at a time, each holding the rows in the
//! states it covers, and how many rows each holds.
//!
//! Every Found row is in exactly one state, and every state is under exactly
//! one entry past All — the placement's `pending_entry` names it — so those
//! entries split Found's rows between them and All holds them all. The counts
//! are read in the same read as the rows, so they never disagree with them.

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
/// holds, from how many each entry past All holds alone.
pub(crate) fn pending_filter_entries(held: &[(PendingFilter, u32)]) -> Vec<PendingFilterEntry> {
    PendingFilter::ENTRIES
        .into_iter()
        .map(|filter| {
            let count = held
                .iter()
                .filter(|(entry, _)| filter == PendingFilter::All || *entry == filter)
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
