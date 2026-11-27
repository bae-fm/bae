//! A list's sort as its compiled statements bind it: the person's criteria in
//! the order they stack, at most one for each field the list sorts by, and the
//! keys of the rows in its order.

use bae_store::list_windows::{OrderKey, SortValue};
use bae_store::{DbError, SortDirection};

/// A field a list sorts by, as the list's statements name it.
pub(crate) trait ListSortField: Copy + PartialEq {
    /// The names, which a sort of no criteria leads with, ascending.
    const NAME: Self;

    /// What the statements call the field's term running `direction`.
    fn term(self, direction: SortDirection) -> &'static str;
}

/// A sort of up to `N` criteria, in the order they stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListSort<F, const N: usize> {
    criteria: [Option<(F, SortDirection)>; N],
}

impl<F: ListSortField, const N: usize> ListSort<F, N> {
    /// The sort by `criteria`, the first foremost. A field named again orders
    /// no two rows its first naming left tied, so only that one counts, and a
    /// list with `N` fields has a place for each.
    pub(crate) fn of(criteria: impl IntoIterator<Item = (F, SortDirection)>) -> Self {
        let mut sort = [None; N];
        let mut stacked = 0;
        for (field, direction) in criteria {
            if sort.iter().flatten().any(|(held, _)| *held == field) {
                continue;
            }
            sort[stacked] = Some((field, direction));
            stacked += 1;
        }
        Self { criteria: sort }
    }

    /// The field and direction the sort leads with.
    pub(crate) fn first(&self) -> (F, SortDirection) {
        self.criteria[0].unwrap_or((F::NAME, SortDirection::Ascending))
    }

    /// The term of the criterion at `place`, `None` past the last criterion.
    pub(crate) fn later(&self, place: usize) -> Option<String> {
        self.criteria[place].map(|(field, direction)| field.term(direction).to_string())
    }
}

/// The key a window from the top binds, which every row comes after: no term
/// is as low as the least integer, which SQLite also sorts before every blob.
pub(crate) fn before_every_row<const N: usize>() -> ([SortValue; N], SortValue, String) {
    let sorts =
        std::array::from_fn(|place| SortValue::Integer(if place == 0 { i64::MIN } else { 0 }));
    (sorts, SortValue::Integer(0), String::new())
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
