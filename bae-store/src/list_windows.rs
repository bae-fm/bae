//! A paged list's window. A list's screen reads one window of it (`range`):
//! the rows from one key on, at most a limit of them, which the read finds by
//! seeking through the order's index rather than stepping over the rows
//! before them.
//!
//! A jump to a position reads the key of the row at that position by offset
//! once (`key_at`), and the window starts at that key. A page of a list
//! (`ListPage`) follows the key of the row at its offset, read live the same
//! way, and takes its rows from there as a window does. `RowsBefore` counts
//! the rows before a key over index ranges in one statement, without
//! numbering the list. The module keeps nothing about where rows sit between
//! reads.

use rusqlite::types::Value;
use rusqlite::{Row, ToSql};

mod range;
pub use range::{key_at, read_range, rows_before_on, ListPage, ListWindow, RowsBefore, WindowRead};

/// Which way a list runs its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// One sort value of a row: what an order term evaluates to for it.
pub use rusqlite::types::Value as SortValue;

/// One term of a list's order: an SQL expression over the list's row, the
/// way it runs, and whether it can be NULL, which sorts below every value.
#[derive(Debug, Clone)]
pub struct SortTerm {
    expression: String,
    descending: bool,
    nullable: bool,
}

impl SortTerm {
    pub fn ascending(expression: impl Into<String>) -> Self {
        Self { expression: expression.into(), descending: false, nullable: false }
    }

    pub fn descending(expression: impl Into<String>) -> Self {
        Self { descending: true, ..Self::ascending(expression) }
    }

    pub fn running(expression: impl Into<String>, direction: SortDirection) -> Self {
        match direction {
            SortDirection::Ascending => Self::ascending(expression),
            SortDirection::Descending => Self::descending(expression),
        }
    }

    /// The same term over an expression that can be NULL.
    pub fn nullable(self) -> Self {
        Self { nullable: true, ..self }
    }

    /// The term as an ORDER BY term, run `descending` or the other way.
    fn ordering(&self, descending: bool) -> String {
        if descending {
            format!("{} DESC", self.expression)
        } else {
            self.expression.clone()
        }
    }
}

/// A list's order: terms that together tell any two of its rows apart, so
/// each row has one place in it.
#[derive(Debug, Clone)]
pub struct ListOrder {
    terms: Vec<SortTerm>,
}

impl ListOrder {
    /// The order of a list of named rows: `terms`, then `name` ascending
    /// unless a term already orders by it (`ordered_by_name`), then `id`, the
    /// tie-break a window needs because a name is not unique.
    pub fn named(mut terms: Vec<SortTerm>, ordered_by_name: bool, name: &str, id: &str) -> Self {
        if !ordered_by_name {
            terms.push(SortTerm::ascending(name));
        }
        terms.push(SortTerm::ascending(id));
        Self::new(terms)
    }
}

/// The values a row has for its list's order terms, which say where it sits.
#[derive(Debug, Clone)]
pub struct OrderKey(Vec<Value>);

impl OrderKey {
    pub fn new(values: Vec<SortValue>) -> Self {
        Self(values)
    }

    pub fn values(&self) -> &[SortValue] {
        &self.0
    }
}

/// Two keys are equal when they hold the same values: a window's request
/// and the request a read answers compare so.
impl PartialEq for OrderKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for OrderKey {}

/// A list's row with its key.
pub type Keyed<R> = (R, OrderKey);

/// The name of the result column holding a row's value for term `at`.
fn key_column(at: usize) -> String {
    format!("order_key_{at}")
}

/// The parameter a seek binds term `at`'s value of its row to.
fn key_parameter(at: usize) -> String {
    format!(":order_key_{at}")
}

/// Which way a read runs from its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    After,
    Before,
}

/// One statement of a window's read: which rows past the list's own
/// condition it reads, and the order it reads them in. A seek names the
/// order's terms by place, so any statement sorting by the same terms — over
/// other aliases, or by result column — can run it.
#[derive(Debug, Clone)]
pub struct Seek {
    /// How many of the order's leading terms hold the row's values.
    equal: usize,
    /// What the term after them is held to; nothing for every row.
    past: Option<Past>,
    /// The first term the read sorts by: the terms before it are held equal.
    from: usize,
    /// Whether the read runs against the order, from the row back.
    reversed: bool,
    binds: Vec<(String, Value)>,
    /// How many of the seek's rows the read skips, for a read by offset; a
    /// window's read skips none and has no OFFSET.
    offset: Option<u64>,
}

/// What a seek holds the term at its place to.
#[derive(Debug, Clone)]
enum Past {
    /// Above the bound's value. NULL is not.
    Above(Bound),
    /// Below the bound's value. NULL is not.
    Below(Bound),
    NotNull,
    Null,
}

/// The value a [`Past::Above`] or [`Past::Below`] compares its term with: the
/// parameter holding the value, and whether a term equal to it passes.
#[derive(Debug, Clone)]
struct Bound {
    inclusive: bool,
    parameter: String,
}

impl Seek {
    /// The parameters the seek's condition names, as a statement binds them.
    pub fn params(&self) -> Vec<(&str, &dyn ToSql)> {
        self.binds.iter().map(|(name, value)| (name.as_str(), value as &dyn ToSql)).collect()
    }

    /// The statement's LIMIT clause: `LIMIT :limit`, with `OFFSET :offset`
    /// for a read by offset. A window's statement has no OFFSET, so coven
    /// reruns it only for a write at or before its last row.
    pub fn limit_clause(&self) -> &'static str {
        match self.offset {
            Some(_) => "LIMIT :limit OFFSET :offset",
            None => "LIMIT :limit",
        }
    }

    /// Run `run` with the seek's parameters, `limit` as `:limit`, and for a
    /// read by offset, the offset as `:offset`.
    pub fn bound<T>(&self, limit: u64, run: impl FnOnce(&[(&str, &dyn ToSql)]) -> T) -> T {
        let limit = sql_bound(limit);
        let offset = self.offset.map(sql_bound);
        let mut params = self.params();
        params.push((":limit", &limit));
        if let Some(offset) = &offset {
            params.push((":offset", offset));
        }
        run(&params)
    }

    /// Whether the read skips rows by offset.
    pub fn by_offset(&self) -> bool {
        self.offset.is_some()
    }

    /// How many of the order's leading terms the seek holds equal.
    pub fn held_equal(&self) -> usize {
        self.equal
    }
}

/// A window's bound as SQLite's LIMIT and OFFSET take it: a bound past the
/// largest is no bound at all, since no list holds that many rows.
pub fn sql_bound(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl ListOrder {
    pub fn new(terms: Vec<SortTerm>) -> Self {
        assert!(
            terms.last().is_some_and(|last| !last.nullable),
            "a list's order ends with a term that is never NULL"
        );
        Self { terms }
    }

    /// How many terms the order has, and so how many values its keys hold.
    pub fn term_count(&self) -> usize {
        self.terms.len()
    }

    /// The order as an ORDER BY clause.
    pub fn clause(&self) -> String {
        Self::ordered(&self.terms, false)
    }

    fn ordered(terms: &[SortTerm], reversed: bool) -> String {
        terms
            .iter()
            .map(|term| term.ordering(term.descending != reversed))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The order's terms as result columns, which [`Self::key_of`] reads.
    pub fn key_columns(&self) -> String {
        self.terms
            .iter()
            .enumerate()
            .map(|(at, term)| format!("{} AS {}", term.expression, key_column(at)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Where `row` sits, from the columns [`Self::key_columns`] names.
    pub fn key_of(&self, row: &Row<'_>) -> rusqlite::Result<OrderKey> {
        (0..self.terms.len())
            .map(|at| row.get::<_, Value>(key_column(at).as_str()))
            .collect::<rusqlite::Result<_>>()
            .map(OrderKey)
    }

    /// Every row of the list, in its order: the seek a window from the
    /// list's top runs.
    pub fn everything(&self) -> Seek {
        Seek { equal: 0, past: None, from: 0, reversed: false, binds: Vec::new(), offset: None }
    }

    /// Every row of the list from `position` on, read by offset: the seek
    /// that finds the key of the row at a position.
    pub fn at_offset(&self, position: u64) -> Seek {
        Seek { offset: Some(position), ..self.everything() }
    }

    /// Every row of the list, from its last to its first: the seek a read of
    /// the list's end runs.
    pub fn everything_reversed(&self) -> Seek {
        Seek { reversed: true, ..self.everything() }
    }

    /// The condition `seek` holds this order's rows to. A term held to its
    /// row's value is compared with `=`, and with `IS` only when that value
    /// is NULL, which `=` never matches. SQLite takes a column held with `IS`
    /// to repeat, so a read whose order goes on from one table into another,
    /// as the album grid grouped by artist goes from the artists into each
    /// one's albums, would sort the second table's rows instead of reading
    /// them in its index's order.
    pub fn condition(&self, seek: &Seek) -> String {
        let term = |at: usize| &self.terms[at].expression;
        let held = |at: usize| match &seek.binds[at].1 {
            Value::Null => format!("({}) IS {}", term(at), key_parameter(at)),
            _ => format!("({}) = {}", term(at), key_parameter(at)),
        };
        let past = seek.past.as_ref().map(|past| {
            let term = term(seek.equal);
            match past {
                Past::Above(bound) => {
                    let op = if bound.inclusive { ">=" } else { ">" };
                    format!("({term}) {op} {}", bound.parameter)
                }
                Past::Below(bound) => {
                    let op = if bound.inclusive { "<=" } else { "<" };
                    format!("({term}) {op} {}", bound.parameter)
                }
                Past::NotNull => format!("({term}) IS NOT NULL"),
                Past::Null => format!("({term}) IS NULL"),
            }
        });
        let conditions = (0..seek.equal).map(held).chain(past).collect::<Vec<_>>();
        if conditions.is_empty() {
            "1".to_string()
        } else {
            conditions.join(" AND ")
        }
    }

    /// The ORDER BY clause `seek` reads this order's rows in.
    pub fn ordering(&self, seek: &Seek) -> String {
        Self::ordered(&self.terms[seek.from..], seek.reversed)
    }

    /// The ORDER BY clause `seek` reads a compound statement in whose result
    /// columns hold this order's terms from column `first`, counted from one.
    #[cfg(desktop)]
    pub fn column_ordering(&self, seek: &Seek, first: usize) -> String {
        (seek.from..self.terms.len())
            .map(|at| {
                let column = first + at;
                if self.terms[at].descending != seek.reversed {
                    format!("{column} DESC")
                } else {
                    column.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The order's key columns ([`Self::key_columns`]) of a subquery aliased
    /// `t`, then NULL columns up to `width` of them: what each SELECT of one
    /// compound statement selects when their orders have different numbers of
    /// terms.
    #[cfg(desktop)]
    pub fn key_columns_padded(&self, t: &str, width: usize) -> String {
        (0..width)
            .map(|at| match at < self.terms.len() {
                true => format!("{t}.{0} AS {0}", key_column(at)),
                false => format!("NULL AS {}", key_column(at)),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The order as an ORDER BY clause over the key columns
    /// ([`Self::key_columns`]) of a subquery aliased `t`; a column keeps its
    /// expression's collation.
    #[cfg(desktop)]
    pub fn key_ordering(&self, t: &str) -> String {
        self.terms
            .iter()
            .enumerate()
            .map(|(at, term)| {
                let column = format!("{t}.{}", key_column(at));
                if term.descending {
                    format!("{column} DESC")
                } else {
                    column
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The seeks that together read every row before the row at `key`, each
    /// an index seek: counting what they read counts those rows without
    /// numbering the list from its start.
    pub fn seeks_before(&self, key: &OrderKey) -> Vec<Seek> {
        self.seeks(key, Side::Before, false, true)
    }

    /// The seeks reading the rows after the row at `key`, then those reading
    /// the rows before it: what a test holds to the order's index.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seeks_beside(&self, key: &OrderKey) -> Vec<Seek> {
        let mut seeks = self.seeks(key, Side::After, false, false);
        seeks.extend(self.seeks_before(key));
        seeks
    }

    /// The rows on `side` of the row at `key`, nearest first, as seeks to run
    /// in turn, each read against the order when `reversed`, each an index
    /// seek: the rows that tie the row on every term but the last and
    /// pass it on that, then those tying it on every term but the last two,
    /// and so on out to the first term. Within each, the terms the seek holds
    /// equal are left out of its order, which an index on an expression needs
    /// to read them in order. With `inclusive`, the row itself is among the
    /// rows after it.
    fn seeks(&self, key: &OrderKey, side: Side, inclusive: bool, reversed: bool) -> Vec<Seek> {
        let last = self.terms.len() - 1;
        let mut seeks = Vec::new();
        for at in (0..self.terms.len()).rev() {
            let term = &self.terms[at];
            // The seek binds the values it holds equal, and the one it passes
            // when it compares with it.
            let seek = |past: Past, from: usize| {
                let compared = match past {
                    Past::Above(_) | Past::Below(_) => at + 1,
                    Past::NotNull | Past::Null => at,
                };
                Seek {
                    equal: at,
                    past: Some(past),
                    from,
                    reversed,
                    binds: (0..compared)
                        .map(|bound| (key_parameter(bound), key.0[bound].clone()))
                        .collect(),
                    offset: None,
                }
            };
            let bound =
                || Bound { inclusive: inclusive && at == last, parameter: key_parameter(at) };
            let downward = term.descending != (side == Side::Before);
            match (&key.0[at], downward) {
                // NULL sorts first: everything else on this term is past it
                // going up, and nothing is going down.
                (Value::Null, false) => seeks.push(seek(Past::NotNull, at)),
                (Value::Null, true) => {}
                (_, false) => seeks.push(seek(Past::Above(bound()), at)),
                (_, true) => {
                    seeks.push(seek(Past::Below(bound()), at));
                    if term.nullable {
                        seeks.push(seek(Past::Null, at + 1));
                    }
                }
            }
        }
        seeks
    }
}

/// Up to `limit` rows from `seeks`, run in turn until they hold that many.
fn from_seeks<R, E>(
    seeks: Vec<Seek>,
    limit: u64,
    read: &mut impl FnMut(&Seek, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<Vec<Keyed<R>>, E> {
    let mut rows = Vec::new();
    for seek in seeks {
        let wanted = limit - rows.len() as u64;
        if wanted == 0 {
            break;
        }
        rows.extend(read(&seek, wanted)?);
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "list_windows_tests.rs"]
mod tests;
