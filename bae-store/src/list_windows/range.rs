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
    /// The `limit` rows nearest before `until`. `read_range` reads these rows
    /// backward through the index and returns them in the list's order.
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

impl<R> WindowRead<R> {
    /// The window with its rows made into others all at once by `convert`.
    pub fn try_convert_rows<Other, E>(
        self,
        convert: impl FnOnce(Vec<R>) -> Result<Vec<Other>, E>,
    ) -> Result<WindowRead<Other>, E> {
        Ok(WindowRead {
            window: self.window,
            rows: convert(self.rows)?,
            first: self.first,
            beyond: self.beyond,
        })
    }

    /// The window with each row made into another by `convert`, or the first
    /// error it gives.
    pub fn try_map_rows<Other, E>(
        self,
        convert: impl FnMut(R) -> Result<Other, E>,
    ) -> Result<WindowRead<Other>, E> {
        self.try_convert_rows(|rows| rows.into_iter().map(convert).collect())
    }
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

    /// The seeks that read the rows of `[from, until)`, in the list's sort
    /// order, each an index range: the rows from `from` that share a longer
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

    /// The seek for rows whose first `at` terms equal the terms both keys
    /// share, and whose term `at` lies between the two keys' values, `from`'s
    /// value first. When `at` is the last term, no later term breaks a tie,
    /// so a row equal to `from` is in the range.
    fn between(&self, from: &OrderKey, until: &OrderKey, at: usize) -> Seek {
        // The seek skips a NULL bound, because an SQL comparison with NULL
        // matches no row.
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
/// it is given. `read_range` reads one row more than the limit to find the
/// key of the next row past the window.
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

/// One statement that counts the rows of `from`, a statement's FROM clause,
/// that `filter` keeps (every row when it is `None`) and that come before the
/// row at `key` in `order`. The statement adds one COUNT for each seek before
/// the key, so each COUNT reads its own index range, and it returns the sum
/// in its one column, `count`.
pub struct RowsBefore {
    pub statement: String,
    seeks: Vec<Seek>,
}

impl RowsBefore {
    pub fn new(order: &ListOrder, key: &OrderKey, from: &str, filter: Option<&str>) -> Self {
        let seeks = order.seeks_before(key);
        // The sum starts at 0, which is the count when no seek reads a row
        // before the key.
        let counts: String = seeks
            .iter()
            .map(|seek| {
                let condition = order.condition(seek);
                let condition = match filter {
                    Some(filter) => format!("({filter}) AND ({condition})"),
                    None => condition,
                };
                format!(" + (SELECT COUNT(*) {from} WHERE {condition})")
            })
            .collect();
        Self { statement: format!("SELECT 0{counts} AS count"), seeks }
    }

    /// The parameters the statement's seeks name. Every seek before a key
    /// binds the key's values under the same names, so a name bound twice
    /// holds one value.
    pub fn params(&self) -> Vec<(&str, &dyn rusqlite::ToSql)> {
        self.seeks.iter().flat_map(Seek::params).collect()
    }
}

/// How many rows of `from` that `filter` keeps come before the row at `key`
/// in `order`, counted by one [`RowsBefore`] statement. `params` binds the
/// named parameters `filter` names.
pub fn rows_before_on(
    sql: &coven::SqlReadContext<'_>,
    from: &str,
    filter: Option<&str>,
    params: &[(&str, &dyn rusqlite::ToSql)],
    order: &ListOrder,
    key: &OrderKey,
) -> Result<u64, crate::DbError> {
    let before = RowsBefore::new(order, key, from, filter);
    let mut bound = before.params();
    bound.extend_from_slice(params);
    Ok(sql.query_row(&before.statement, bound.as_slice(), |row| {
        crate::read::count_column::<u64>(row, "count")
    })?)
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

/// The window of the `limit` rows from `position`, bounded by the keys that
/// two reads by offset find: from the row at `position` up to the row at
/// `position + limit`, or to the list's end when no row is there.
/// `None` when the list ends before `position`. A caller that names a page of
/// a list by its offset reads this once and follows the window live, so coven
/// reruns the window for a write inside that page alone.
pub fn page_window<R, E>(
    order: &ListOrder,
    position: u64,
    limit: u64,
    read: &mut impl FnMut(&Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<Option<ListWindow>, E> {
    let mut key_at_offset =
        |offset| Ok(read(&order.everything(), 1, offset)?.into_iter().next().map(|(_, key)| key));
    let Some(from) = key_at_offset(position)? else {
        return Ok(None);
    };
    let until = key_at_offset(position.saturating_add(limit))?;
    Ok(Some(ListWindow::Forward { from: Some(from), until, limit }))
}

impl crate::Store {
    /// A live query of one window of the list `order` sorts. When the screen
    /// changes the window's bounds, coven reruns this same live query with
    /// the new bounds. `read` runs the list's statement under a seek, for at
    /// most a limit from an offset, and gives each row with its key. Coven
    /// reruns the query for a write inside the window's range alone.
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
