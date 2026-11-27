//! A list's sort as its compiled statements bind it: the person's criteria in
//! the order they stack, at most one for each field the list sorts by.

use bae_store::list_windows::{OrderKey, SortValue};
use bae_store::{DbError, SortDirection};

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

    /// The value in a row of the criterion at `position`, from the criterion's
    /// ascending term and its descending one: the term its direction orders
    /// by.
    pub(crate) fn term(
        &self,
        position: usize,
        ascending: SortValue,
        descending: SortValue,
    ) -> SortValue {
        if self.descending(position) {
            descending
        } else {
            ascending
        }
    }
}

/// The key of a row with the value `sorts` for each criterion, the name key
/// `name` and the id `id`.
pub(crate) fn key<const N: usize>(sorts: [SortValue; N], name: Vec<u8>, id: String) -> OrderKey {
    let mut values = Vec::from(sorts);
    values.extend([SortValue::Blob(name), SortValue::Text(id)]);
    OrderKey::new(values)
}

/// The values `key` holds, as the statements bind them: each criterion's, the
/// name key's and the id.
pub(crate) fn key_values<const N: usize>(
    key: &OrderKey,
) -> Result<([SortValue; N], SortValue, String), DbError> {
    let unreadable = || DbError::unreadable("a list key", key.values());
    let [sorts @ .., name, SortValue::Text(id)] = key.values() else {
        return Err(unreadable());
    };
    let Ok(sorts) = <[SortValue; N]>::try_from(sorts.to_vec()) else {
        return Err(unreadable());
    };
    Ok((sorts, name.clone(), id.clone()))
}
