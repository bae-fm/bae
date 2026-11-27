//! A list's sort as its compiled statements bind it: the person's criteria in
//! the order they stack, at most one for each field the list sorts by.

use bae_store::list_windows::SortValue;
use bae_store::SortDirection;

/// A criterion's value in a row, from its ascending term and its descending
/// one, of which at most one holds it.
pub(crate) fn term(ascending: SortValue, descending: SortValue) -> SortValue {
    match ascending {
        SortValue::Null => descending,
        value => value,
    }
}

/// A sort of up to `N` criteria: each position's field, as the list's
/// statements name it, and whether it runs descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListSort<const N: usize> {
    criteria: [Option<(&'static str, bool)>; N],
}

impl<const N: usize> ListSort<N> {
    /// The sort by `criteria`, the first foremost. A field named again orders
    /// no two rows its first naming left tied, so only that one counts, and a
    /// list with `N` fields has a position for each.
    pub(crate) fn of(criteria: impl IntoIterator<Item = (&'static str, SortDirection)>) -> Self {
        let mut sort = [None; N];
        let mut stacked = 0;
        for (field, direction) in criteria {
            if sort.iter().flatten().any(|(held, _)| *held == field) {
                continue;
            }
            sort[stacked] = Some((field, direction == SortDirection::Descending));
            stacked += 1;
        }
        Self { criteria: sort }
    }

    /// The field at `position`, `None` past the last criterion.
    pub(crate) fn field(&self, position: usize) -> Option<String> {
        self.criteria[position].map(|(field, _)| field.to_string())
    }

    /// Whether the criterion at `position` runs descending.
    pub(crate) fn descending(&self, position: usize) -> bool {
        self.criteria[position].is_some_and(|(_, descending)| descending)
    }
}
