//! A paged list's window and pages. A list's screen reads one window of it:
//! the rows from one key of the list's order on, at most a limit of them.
//! Each list reads its window, the rows before a key and the key at a
//! position with generated statements of its own; these types say what such a
//! read is asked for and what it answers.

/// Which way a list runs its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// One sort value of a row: what an order term evaluates to for it.
pub use rusqlite::types::Value as SortValue;

/// The values a row has for its list's order terms, which say where it sits.
/// The terms together tell any two of the list's rows apart.
#[derive(Debug, Clone)]
pub struct OrderKey(Vec<SortValue>);

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

/// A window's bound as SQLite's LIMIT and OFFSET take it: a bound past the
/// largest is no bound at all, since no list holds that many rows.
pub fn sql_bound(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// A window of a list: the `limit` rows at or after `from` in the list's
/// order, or from the list's top when `from` is `None`. `from` need not
/// belong to a row that still exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListWindow {
    pub from: Option<OrderKey>,
    pub limit: u64,
}

impl ListWindow {
    /// The window from the list's top.
    pub fn top(limit: u64) -> Self {
        Self { from: None, limit }
    }
}

/// A window as one read found it.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowRead<R> {
    /// The request this answers, so a screen that has replaced it drops the
    /// value.
    pub window: ListWindow,
    /// The window's rows in the list's order, each with its key, from which
    /// the list's screen moves the window to a row it holds.
    pub rows: Vec<Keyed<R>>,
}

impl<R> WindowRead<R> {
    /// The window with each row made into another by `convert`, or the first
    /// error it gives.
    pub fn try_map_rows<Other, E>(
        self,
        mut convert: impl FnMut(R) -> Result<Other, E>,
    ) -> Result<WindowRead<Other>, E> {
        Ok(WindowRead {
            window: self.window,
            rows: self
                .rows
                .into_iter()
                .map(|(row, key)| Ok((convert(row)?, key)))
                .collect::<Result<_, E>>()?,
        })
    }
}

/// A page of a list: the `limit` rows from the row at `offset` in the list's
/// order. A page that starts past the list's end holds no rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPage {
    pub offset: u64,
    pub limit: u64,
}
