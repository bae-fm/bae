//! Found's filter: one entry chosen at a time, each holding the rows in the
//! states it covers, and how many rows each holds.
//!
//! Every Found row is in exactly one state, and every state is under exactly
//! one entry past All — the placement's `pending_entry` names it — so those
//! entries split Found's rows between them and All holds them all. The
//! counts kept beside the placement (`import_list_count`) give each entry's
//! count.

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
