//! A paged list's windows, each read from where the list's order puts it
//! (`range`: the windows a screen holds, each a range of the order).
//!
//! A window carries a cursor when the screen holds a row beside it: the sort
//! values of the row at the screen's edge, which the screen read from a window
//! it was delivered. The window after the row starts just past it, the window
//! before the row ends just short of it, and the read seeks to the row through
//! the order's index rather than stepping over the rows before it. A window
//! with no cursor, a jump to a position, is read by its offset, which steps
//! over every row before it. Nothing about where rows sit is kept between
//! reads.

use rusqlite::types::Value;
use rusqlite::{Row, ToSql};
use std::cmp::Ordering;
use std::collections::BTreeSet;

mod range;
pub use range::{key_at, read_range, rows_before, ListWindow, WindowRead};

/// Which way a list runs its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// One sort value of a row: what an order term evaluates to for it.
pub use rusqlite::types::Value as SortValue;

/// A window of a list: where the screen counts it from, how many rows it
/// holds, and, when the screen holds the row beside it, the cursor the read
/// starts from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryPageWindow {
    pub offset: u64,
    pub limit: u64,
    pub cursor: Option<Cursor>,
}

impl LibraryPageWindow {
    /// The window of `limit` rows from `offset`, read by its offset.
    pub fn at(offset: u64, limit: u64) -> Self {
        Self { offset, limit, cursor: None }
    }

    /// The window read from `cursor`, labelled `offset`.
    pub fn from_cursor(offset: u64, limit: u64, cursor: Cursor) -> Self {
        Self { offset, limit, cursor: Some(cursor) }
    }
}

/// The row beside a window, by its sort values, and which side the window
/// lies on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cursor {
    /// The window holds the rows after the row at the key.
    After(OrderKey),
    /// The window holds the rows before the row at the key.
    Before(OrderKey),
}

pub type LibraryPageWindows = BTreeSet<LibraryPageWindow>;

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryBrowseWindow<Row> {
    pub window: LibraryPageWindow,
    pub rows: Vec<Row>,
    /// The sort values of the window's first row and of its last, which the
    /// screen sends back as the cursor of the window beside it. Both `None`
    /// for a list that pages by offset alone.
    pub first: Option<OrderKey>,
    pub last: Option<OrderKey>,
}

impl<Row> LibraryBrowseWindow<Row> {
    /// The window with each row made into another by `convert`.
    pub fn map_rows<Other>(self, convert: impl FnMut(Row) -> Other) -> LibraryBrowseWindow<Other> {
        LibraryBrowseWindow {
            window: self.window,
            rows: self.rows.into_iter().map(convert).collect(),
            first: self.first,
            last: self.last,
        }
    }

    /// The window with its rows made into others all at once by `convert`.
    pub fn try_convert_rows<Other, E>(
        self,
        convert: impl FnOnce(Vec<Row>) -> Result<Vec<Other>, E>,
    ) -> Result<LibraryBrowseWindow<Other>, E> {
        Ok(LibraryBrowseWindow {
            window: self.window,
            rows: convert(self.rows)?,
            first: self.first,
            last: self.last,
        })
    }

    /// The window with each row made into another by `convert`, or the first
    /// error it gives.
    pub fn try_map_rows<Other, E>(
        self,
        convert: impl FnMut(Row) -> Result<Other, E>,
    ) -> Result<LibraryBrowseWindow<Other>, E> {
        Ok(LibraryBrowseWindow {
            window: self.window,
            rows: self.rows.into_iter().map(convert).collect::<Result<_, E>>()?,
            first: self.first,
            last: self.last,
        })
    }

    /// A window of a list that pages by offset alone.
    pub fn unkeyed(window: LibraryPageWindow, rows: Vec<Row>) -> Self {
        Self { window, rows, first: None, last: None }
    }
}

/// One term of a list's order: an SQL expression over the list's row, the
/// way it runs, and whether it can be NULL, which sorts below every value.
#[derive(Debug, Clone)]
pub struct SortTerm {
    expression: String,
    descending: bool,
    nullable: bool,
    /// How the term's text compares, when its expression collates it: two
    /// values this calls equal are one value to the order, whatever their
    /// bytes.
    collation: Option<fn(&str, &str) -> Ordering>,
}

impl SortTerm {
    pub fn ascending(expression: impl Into<String>) -> Self {
        Self { expression: expression.into(), descending: false, nullable: false, collation: None }
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

    /// The same term, whose expression collates its text as `compare` does.
    pub fn collated(self, compare: fn(&str, &str) -> Ordering) -> Self {
        Self { collation: Some(compare), ..self }
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

    /// How many terms the key holds values of.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A value's rank among SQLite's storage classes, which sort NULL, numbers,
/// text, then blobs.
fn rank(value: &Value) -> u8 {
    match value {
        Value::Null => 0,
        Value::Integer(_) | Value::Real(_) => 1,
        Value::Text(_) => 2,
        Value::Blob(_) => 3,
    }
}

fn compare_values(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => left.cmp(right),
        (Value::Integer(left), Value::Real(right)) => (*left as f64).total_cmp(right),
        (Value::Real(left), Value::Integer(right)) => left.total_cmp(&(*right as f64)),
        (Value::Real(left), Value::Real(right)) => left.total_cmp(right),
        (Value::Text(left), Value::Text(right)) => left.cmp(right),
        (Value::Blob(left), Value::Blob(right)) => left.cmp(right),
        (left, right) => rank(left).cmp(&rank(right)),
    }
}

/// Keys compare value by value, which only a request set needs, to hold two
/// windows apart.
impl Ord for OrderKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .iter()
            .zip(&other.0)
            .map(|(left, right)| compare_values(left, right))
            .find(|ordering| ordering.is_ne())
            .unwrap_or_else(|| self.0.len().cmp(&other.0.len()))
    }
}

impl PartialOrd for OrderKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for OrderKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
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

/// The parameter a range seek binds term `at`'s value of its upper bound to.
fn until_parameter(at: usize) -> String {
    format!(":until_key_{at}")
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
}

/// What a seek holds the term at its place to.
#[derive(Debug, Clone)]
enum Past {
    /// Above the lower bound and below the upper one, whichever are set. NULL
    /// is neither.
    Between {
        lower: Option<Bound>,
        upper: Option<Bound>,
    },
    NotNull,
    Null,
}

/// One end of a [`Past::Between`]: the parameter holding the value, and
/// whether a term equal to it is within.
#[derive(Debug, Clone)]
struct Bound {
    inclusive: bool,
    parameter: String,
}

impl Seek {
    /// The parameters the seek's condition names, as a statement binds them.
    pub(super) fn params(&self) -> Vec<(&str, &dyn ToSql)> {
        self.binds.iter().map(|(name, value)| (name.as_str(), value as &dyn ToSql)).collect()
    }

    /// Run `run` with the seek's parameters and a window's `:limit` and
    /// `:offset`.
    pub fn bound<T>(
        &self,
        limit: u64,
        offset: u64,
        run: impl FnOnce(&[(&str, &dyn ToSql)]) -> T,
    ) -> T {
        let (limit, offset) = (sql_bound(limit), sql_bound(offset));
        let mut params = self.params();
        params.push((":limit", &limit));
        params.push((":offset", &offset));
        run(&params)
    }

    /// How many of the order's leading terms the seek holds equal.
    pub fn held_equal(&self) -> usize {
        self.equal
    }

    /// Whether the seek compares its term with a bound at each end.
    #[cfg(test)]
    pub(super) fn bounds_both_ends(&self) -> bool {
        matches!(&self.past, Some(Past::Between { lower: Some(_), upper: Some(_) }))
    }

    /// Whether the seek reads every row in the order's own run: the seek of
    /// a read by offset.
    #[cfg(test)]
    pub(super) fn reads_everything(&self) -> bool {
        self.past.is_none()
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

    /// Every row of the list, in its order: the seek a window read by its
    /// offset runs.
    pub fn everything(&self) -> Seek {
        Seek { equal: 0, past: None, from: 0, reversed: false, binds: Vec::new() }
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
                Past::Between { lower, upper } => {
                    let lower = lower.iter().map(|bound| {
                        let op = if bound.inclusive { ">=" } else { ">" };
                        format!("({term}) {op} {}", bound.parameter)
                    });
                    let upper = upper.iter().map(|bound| {
                        let op = if bound.inclusive { "<=" } else { "<" };
                        format!("({term}) {op} {}", bound.parameter)
                    });
                    lower.chain(upper).collect::<Vec<_>>().join(" AND ")
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
                    Past::Between { .. } => at + 1,
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
                (_, false) => {
                    seeks.push(seek(Past::Between { lower: Some(bound()), upper: None }, at));
                }
                (_, true) => {
                    seeks.push(seek(Past::Between { lower: None, upper: Some(bound()) }, at));
                    if term.nullable {
                        seeks.push(seek(Past::Null, at + 1));
                    }
                }
            }
        }
        seeks
    }
}

/// One window's rows as one read found them, with the sort values of its
/// first row and of its last.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorWindowRead<R> {
    pub window: LibraryPageWindow,
    pub rows: Vec<R>,
    pub first: Option<OrderKey>,
    pub last: Option<OrderKey>,
}

impl<R> CursorWindowRead<R> {
    /// The window as a delivery carries it.
    pub fn into_browse(self) -> LibraryBrowseWindow<R> {
        LibraryBrowseWindow {
            window: self.window,
            rows: self.rows,
            first: self.first,
            last: self.last,
        }
    }
}

/// Up to `window.limit` rows of `window`, each with its key: from its cursor
/// through the order's index when it has one, by its offset otherwise. `read`
/// runs the list's statement under a seek, for at most `limit` rows from
/// `offset`, and gives each row with its key.
pub fn read_window<R, E>(
    order: &ListOrder,
    window: &LibraryPageWindow,
    read: &mut impl FnMut(&Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<Vec<Keyed<R>>, E> {
    if window.limit == 0 {
        return Ok(Vec::new());
    }
    match &window.cursor {
        None => read(&order.everything(), window.limit, window.offset),
        Some(Cursor::After(key)) => {
            from_seeks(order.seeks(key, Side::After, false, false), window.limit, read)
        }
        Some(Cursor::Before(key)) => {
            let mut rows =
                from_seeks(order.seeks(key, Side::Before, false, true), window.limit, read)?;
            rows.reverse();
            Ok(rows)
        }
    }
}

/// Read each of `windows`, in order, with [`read_window`].
pub fn read_windows<R, E>(
    order: &ListOrder,
    windows: &LibraryPageWindows,
    mut read: impl FnMut(&Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<Vec<CursorWindowRead<R>>, E> {
    windows
        .iter()
        .map(|window| {
            let rows = read_window(order, window, &mut read)?;
            let first = rows.first().map(|(_, key)| key.clone());
            let last = rows.last().map(|(_, key)| key.clone());
            Ok(CursorWindowRead {
                window: window.clone(),
                rows: rows.into_iter().map(|(row, _)| row).collect(),
                first,
                last,
            })
        })
        .collect()
}

/// Up to `limit` rows from `seeks`, run in turn until they hold that many.
fn from_seeks<R, E>(
    seeks: Vec<Seek>,
    limit: u64,
    read: &mut impl FnMut(&Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<Vec<Keyed<R>>, E> {
    let mut rows = Vec::new();
    for seek in seeks {
        let wanted = limit - rows.len() as u64;
        if wanted == 0 {
            break;
        }
        rows.extend(read(&seek, wanted, 0)?);
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "list_windows_tests.rs"]
mod tests;
