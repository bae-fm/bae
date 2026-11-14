//! A window as a range of the list's order: the rows from one key up to
//! another, each bound optional, read as a run of index seeks that each hold
//! a prefix of the order's terms equal and compare the next. A window depends
//! on its own stretch of the list alone, so coven reruns its live query only
//! for a write inside that stretch, and two windows sharing a bound hold
//! every row between them exactly once.

use super::{
    compare_values, from_seeks, key_parameter, until_parameter, Bound, Keyed, ListOrder, OrderKey,
    Past, Seek, Side,
};
use rusqlite::types::Value;
use std::cmp::Ordering;

/// A window of a list: a range of its order, bounded by keys that need not
/// belong to a row that still exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListWindow {
    /// The rows at or after `from` (the list's top when `None`) and before
    /// `until` (no end when `None`), at most `limit` of them, read forward.
    Forward { from: Option<OrderKey>, until: Option<OrderKey>, limit: u64 },
    /// The `limit` rows nearest before `until`, read backward through the
    /// index and delivered in the list's order.
    Backward { until: OrderKey, limit: u64 },
}

/// A window as one read found it.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowRead<R> {
    /// The request this answers, so a screen that has replaced it drops the
    /// value.
    pub window: ListWindow,
    pub rows: Vec<R>,
    /// The key of the first row.
    pub first: Option<OrderKey>,
    /// The key of the next row past the window's far end, read as one row
    /// more than `limit`: below the last row of a `Forward` window the limit
    /// cut short, above the first row of a `Backward` one. `None` when the
    /// window reached `until`, the list's end, or the list's top.
    pub beyond: Option<OrderKey>,
}

impl ListOrder {
    /// How the term at `at` ranks two values, the way the order runs.
    fn compare_term(&self, at: usize, left: &Value, right: &Value) -> Ordering {
        let term = &self.terms[at];
        let ordering = match (term.collation, left, right) {
            (Some(compare), Value::Text(left), Value::Text(right)) => compare(left, right),
            _ => compare_values(left, right),
        };
        if term.descending {
            ordering.reverse()
        } else {
            ordering
        }
    }

    /// The first term two keys put in a different place, and how the order
    /// ranks the keys there; `None` for keys at the same place.
    fn first_difference(&self, left: &OrderKey, right: &OrderKey) -> Option<(usize, Ordering)> {
        (0..self.terms.len())
            .map(|at| (at, self.compare_term(at, &left.0[at], &right.0[at])))
            .find(|(_, ordering)| ordering.is_ne())
    }

    /// The seeks that read the rows of `[from, until)`, in the order's own
    /// run, each an index range: the rows from `from` that share a longer
    /// prefix with it than with `until`, one seek on the first term where the
    /// two keys differ, bounded on both sides, then the rows before `until`
    /// that share a longer prefix with it.
    pub fn range_seeks(&self, from: Option<&OrderKey>, until: Option<&OrderKey>) -> Vec<Seek> {
        match (from, until) {
            (None, None) => vec![self.everything()],
            (Some(from), None) => self.seeks(from, Side::After, true, false),
            (None, Some(until)) => {
                let mut seeks = self.seeks(until, Side::Before, false, false);
                seeks.reverse();
                seeks
            }
            (Some(from), Some(until)) => {
                let Some((at, Ordering::Less)) = self.first_difference(from, until) else {
                    return Vec::new();
                };
                let mut seeks: Vec<Seek> = self
                    .seeks(from, Side::After, true, false)
                    .into_iter()
                    .filter(|seek| seek.equal > at)
                    .collect();
                seeks.push(self.between(from, until, at));
                let mut before_until: Vec<Seek> = self
                    .seeks(until, Side::Before, false, false)
                    .into_iter()
                    .filter(|seek| seek.equal > at)
                    .collect();
                before_until.reverse();
                seeks.extend(before_until);
                seeks
            }
        }
    }

    /// The seek of the rows holding the first `at` terms as the two keys do,
    /// which they share, and the term `at` between theirs, `from`'s first.
    /// Nothing follows the last term to tie on, so a row at `from`'s value
    /// there is within.
    fn between(&self, from: &OrderKey, until: &OrderKey, at: usize) -> Seek {
        // A NULL bound is left out: a comparison leaves NULL out already.
        let is_set = |key: &OrderKey| !matches!(key.0[at], Value::Null);
        let from_bound = is_set(from)
            .then(|| Bound { inclusive: at + 1 == self.terms.len(), parameter: key_parameter(at) });
        let until_bound =
            is_set(until).then(|| Bound { inclusive: false, parameter: until_parameter(at) });
        let mut binds: Vec<_> =
            (0..at).map(|held| (key_parameter(held), from.0[held].clone())).collect();
        if from_bound.is_some() {
            binds.push((key_parameter(at), from.0[at].clone()));
        }
        if until_bound.is_some() {
            binds.push((until_parameter(at), until.0[at].clone()));
        }
        // A descending term has `until` below `from` in value.
        let (lower, upper) = if self.terms[at].descending {
            (until_bound, from_bound)
        } else {
            (from_bound, until_bound)
        };
        Seek {
            equal: at,
            past: Some(Past::Between { lower, upper }),
            from: at,
            reversed: false,
            binds,
        }
    }
}

/// `window`'s rows, each with its key, from `read`, which runs the list's
/// statement under a seek for at most the limit it is given from the offset
/// it is given. One row past the window's limit is read to find where the
/// window's far end leaves off.
pub fn read_range<R, E>(
    order: &ListOrder,
    window: &ListWindow,
    read: &mut impl FnMut(&Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<WindowRead<R>, E> {
    let (seeks, limit, backward) = match window {
        ListWindow::Forward { from, until, limit } => {
            (order.range_seeks(from.as_ref(), until.as_ref()), *limit, false)
        }
        ListWindow::Backward { until, limit } => {
            (order.seeks(until, Side::Before, false, true), *limit, true)
        }
    };
    let mut rows = from_seeks(seeks, limit.saturating_add(1), read)?;
    let beyond = if rows.len() as u64 > limit { rows.pop().map(|(_, key)| key) } else { None };
    if backward {
        rows.reverse();
    }
    let first = rows.first().map(|(_, key)| key.clone());
    Ok(WindowRead {
        window: window.clone(),
        rows: rows.into_iter().map(|(row, _)| row).collect(),
        first,
        beyond,
    })
}

/// How many rows the list has before the row at `key`, as `count` totals
/// them over the seeks that read those rows: a count over index ranges, never
/// a numbering of the list.
pub fn rows_before<E>(
    order: &ListOrder,
    key: &OrderKey,
    mut count: impl FnMut(&Seek) -> Result<u64, E>,
) -> Result<u64, E> {
    order.seeks_before(key).iter().map(&mut count).sum()
}

/// The key of the row at `position`, read by offset, or of the last row when
/// the list ends before it; `None` for a list with no rows. A jump to a
/// position reads this once and starts a chain of windows there.
pub fn key_at<R, E>(
    order: &ListOrder,
    position: u64,
    read: &mut impl FnMut(&Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<Option<OrderKey>, E> {
    if let Some((_, key)) = read(&order.everything(), 1, position)?.pop() {
        return Ok(Some(key));
    }
    Ok(read(&order.everything_reversed(), 1, 0)?.pop().map(|(_, key)| key))
}

impl crate::Store {
    /// A live query of one window of the list `order` sorts, whose request
    /// moves in place when the screen moves the window's bounds. `read` runs
    /// the list's statement under a seek, for at most a limit from an offset,
    /// and gives each row with its key. Coven reruns the query for a write
    /// inside the window's range alone.
    pub fn subscribe_list_window<R, E>(
        &self,
        initial: ListWindow,
        order: ListOrder,
        read: impl Fn(&coven::SqlReadContext<'_>, &Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>
            + Send
            + Sync
            + 'static,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<R>, E>
    where
        R: Send + 'static,
        E: From<coven::DbError> + coven::task::CarriesBug + Send + 'static,
    {
        self.subscribe_reconfigurable(initial, move |window, sql| {
            read_range(&order, window, &mut |seek, limit, offset| read(&sql, seek, limit, offset))
        })
    }
}
