//! A paged list's windows, each read from where the list's order puts it.
//!
//! A window beside a row whose place in the list is known is read from that
//! row's sort key through the order's index: the window after the row starts
//! just past it, the window before the row ends just short of it, and the read
//! seeks to the row rather than stepping over the rows before it. Every other
//! window is read by its offset, which steps over every row before it.
//!
//! A row's place is known when an earlier window of the same read held it, or
//! when the list's last read held it and no row of the list has moved since.
//! Each table a list's order reads keeps a version in `list_order_versions`
//! that any write inserting, deleting or reordering one of its rows advances,
//! in that write (Appendix A of the spec). A read carries the places its
//! windows' first and last rows sat at, with the versions it read them at, and
//! the next read starts from them only while those versions still stand.

use super::*;
use crate::library::{LibraryPageWindow, LibraryPageWindows};
use rusqlite::types::Value;
use rusqlite::ToSql;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// One term of a list's order: an SQL expression over the list's row, the
/// way it runs, and whether it can be NULL, which sorts below every value.
#[derive(Debug, Clone)]
pub(super) struct SortTerm {
    expression: String,
    descending: bool,
    nullable: bool,
}

impl SortTerm {
    pub(super) fn ascending(expression: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
            descending: false,
            nullable: false,
        }
    }

    pub(super) fn descending(expression: impl Into<String>) -> Self {
        Self {
            descending: true,
            ..Self::ascending(expression)
        }
    }

    pub(super) fn running(expression: impl Into<String>, direction: SortDirection) -> Self {
        match direction {
            SortDirection::Ascending => Self::ascending(expression),
            SortDirection::Descending => Self::descending(expression),
        }
    }

    /// The same term over an expression that can be NULL.
    pub(super) fn nullable(self) -> Self {
        Self {
            nullable: true,
            ..self
        }
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
pub(super) struct ListOrder {
    terms: Vec<SortTerm>,
}

/// The values a row has for its list's order terms, which say where it sits.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct OrderKey(Vec<Value>);

/// A list's row with its key.
pub(super) type Keyed<R> = (R, OrderKey);

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
pub(super) struct Seek {
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
#[derive(Debug, Clone, Copy)]
enum Past {
    Greater(usize),
    Less(usize),
    NotNull(usize),
    Null(usize),
}

impl Seek {
    /// The parameters the seek's condition names, as a statement binds them.
    pub(super) fn params(&self) -> Vec<(&str, &dyn ToSql)> {
        self.binds
            .iter()
            .map(|(name, value)| (name.as_str(), value as &dyn ToSql))
            .collect()
    }

    /// Run `run` with the seek's parameters and a window's `:limit` and
    /// `:offset`.
    pub(super) fn bound<T>(
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
    #[cfg(test)]
    pub(super) fn held_equal(&self) -> usize {
        self.equal
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
pub(super) fn sql_bound(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl ListOrder {
    pub(super) fn new(terms: Vec<SortTerm>) -> Self {
        assert!(
            terms.last().is_some_and(|last| !last.nullable),
            "a list's order ends with a term that is never NULL"
        );
        Self { terms }
    }

    /// The order as an ORDER BY clause.
    pub(super) fn clause(&self) -> String {
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
    pub(super) fn key_columns(&self) -> String {
        self.terms
            .iter()
            .enumerate()
            .map(|(at, term)| format!("{} AS {}", term.expression, key_column(at)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Where `row` sits, from the columns [`Self::key_columns`] names.
    pub(super) fn key_of(&self, row: &Row<'_>) -> rusqlite::Result<OrderKey> {
        (0..self.terms.len())
            .map(|at| row.get::<_, Value>(key_column(at).as_str()))
            .collect::<rusqlite::Result<_>>()
            .map(OrderKey)
    }

    /// Every row of the list, in its order: the seek a window read by its
    /// offset runs.
    pub(super) fn everything(&self) -> Seek {
        Seek {
            equal: 0,
            past: None,
            from: 0,
            reversed: false,
            binds: Vec::new(),
        }
    }

    /// The condition `seek` holds this order's rows to.
    pub(super) fn condition(&self, seek: &Seek) -> String {
        let term = |at: usize| &self.terms[at].expression;
        let past = seek.past.map(|past| match past {
            Past::Greater(at) => format!("({}) > {}", term(at), key_parameter(at)),
            Past::Less(at) => format!("({}) < {}", term(at), key_parameter(at)),
            Past::NotNull(at) => format!("({}) IS NOT NULL", term(at)),
            Past::Null(at) => format!("({}) IS NULL", term(at)),
        });
        let conditions = (0..seek.equal)
            .map(|at| format!("({}) IS {}", term(at), key_parameter(at)))
            .chain(past)
            .collect::<Vec<_>>();
        if conditions.is_empty() {
            "1".to_string()
        } else {
            conditions.join(" AND ")
        }
    }

    /// The ORDER BY clause `seek` reads this order's rows in.
    pub(super) fn ordering(&self, seek: &Seek) -> String {
        Self::ordered(&self.terms[seek.from..], seek.reversed)
    }

    /// The ORDER BY clause `seek` reads a compound statement in whose result
    /// columns hold this order's terms from column `first`, counted from one.
    #[cfg(desktop)]
    pub(super) fn column_ordering(&self, seek: &Seek, first: usize) -> String {
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
    pub(super) fn key_ordering(&self, t: &str) -> String {
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

    /// The seeks reading the rows after the row at `key`, then those reading
    /// the rows before it: what a test holds to the order's index.
    #[cfg(test)]
    pub(super) fn seeks_beside(&self, key: &OrderKey) -> Vec<Seek> {
        let mut seeks = self.seeks(key, Side::After);
        seeks.extend(self.seeks(key, Side::Before));
        seeks
    }

    /// The rows on `side` of the row at `key`, nearest first, as seeks to
    /// run in turn, each an index seek: the rows that tie the row on every
    /// term but the last and pass it on that, then those tying it on every
    /// term but the last two, and so on out to the first term. Within each,
    /// the terms the seek holds equal are left out of its order, which an
    /// index on an expression needs to read them in order.
    fn seeks(&self, key: &OrderKey, side: Side) -> Vec<Seek> {
        let reversed = side == Side::Before;
        let mut seeks = Vec::new();
        for at in (0..self.terms.len()).rev() {
            let term = &self.terms[at];
            // The seek binds the values it holds equal, and the one it passes
            // when it compares with it.
            let seek = |past: Past, from: usize| {
                let compared = match past {
                    Past::Greater(_) | Past::Less(_) => at + 1,
                    Past::NotNull(_) | Past::Null(_) => at,
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
            let downward = term.descending != reversed;
            match (&key.0[at], downward) {
                // NULL sorts first: everything else on this term is past it
                // going up, and nothing is going down.
                (Value::Null, false) => seeks.push(seek(Past::NotNull(at), at)),
                (Value::Null, true) => {}
                (_, false) => seeks.push(seek(Past::Greater(at), at)),
                (_, true) => {
                    seeks.push(seek(Past::Less(at), at));
                    if term.nullable {
                        seeks.push(seek(Past::Null(at), at + 1));
                    }
                }
            }
        }
        seeks
    }
}

/// Where rows of a list sit: each one's key by its place.
pub(super) type Places = BTreeMap<u64, OrderKey>;

/// Where some of a list's rows sat when one live query last read it, and what
/// placed them then: the versions of the tables the list's order reads, and
/// anything else that orders or narrows the list, such as a sort or a filter.
/// `Kept` is what a read keeps of where its rows sat: their places, and
/// whatever else a list keeps of them.
pub(super) struct KnownPlaces<Standing, Kept = Places> {
    last: Mutex<Option<(Standing, Kept)>>,
}

impl<Standing: PartialEq, Kept: Clone + Default> KnownPlaces<Standing, Kept> {
    pub(super) fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }

    /// What the last read kept, if the list stands where it stood for that
    /// read; nothing otherwise.
    pub(super) fn under(&self, standing: &Standing) -> Kept {
        match &*self.last.lock().expect("known places lock poisoned") {
            Some((kept_under, kept)) if kept_under == standing => kept.clone(),
            _ => Kept::default(),
        }
    }

    /// Keep what a read found, under what placed its rows.
    pub(super) fn keep(&self, standing: Standing, kept: Kept) {
        *self.last.lock().expect("known places lock poisoned") = Some((standing, kept));
    }
}

/// The versions of the tables `sources` names, as `list_order_versions`
/// holds them in this read.
pub(super) fn list_order_versions_on(
    sql: &SqlReadContext<'_>,
    sources: &[&str],
) -> rusqlite::Result<Vec<i64>> {
    sources
        .iter()
        .map(|source| {
            sql.query_row(
                "SELECT version FROM list_order_versions WHERE source = ?",
                [source],
                |row| row.get(0),
            )
        })
        .collect()
}

/// Where one window's read starts.
enum Start<'k> {
    Offset(u64),
    After(&'k OrderKey),
    Before(&'k OrderKey),
}

/// What one read of a list's windows found: each window's rows, and where
/// each window's first and last rows sit, which a later read may start from.
pub(super) struct WindowsRead<R> {
    pub(super) windows: Vec<(LibraryPageWindow, Vec<R>)>,
    pub(super) found: Places,
}

/// Read each of `windows`, in offset order, from the known row beside it
/// when there is one and by its offset otherwise. `read` runs the list's
/// statement under a seek, for at most `limit` rows from `offset`, and gives
/// each row with its key.
pub(super) fn read_windows<R, E>(
    order: &ListOrder,
    windows: &LibraryPageWindows,
    mut known: Places,
    mut read: impl FnMut(&Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<WindowsRead<R>, E> {
    let mut found = Places::new();
    let mut read_windows = Vec::with_capacity(windows.len());
    for window in windows {
        let start = match (
            window
                .offset
                .checked_sub(1)
                .and_then(|before| known.get(&before)),
            window
                .offset
                .checked_add(window.limit)
                .and_then(|after| known.get(&after)),
        ) {
            _ if window.limit == 0 => {
                read_windows.push((window.clone(), Vec::new()));
                continue;
            }
            (Some(before), _) => Start::After(before),
            (None, Some(after)) => Start::Before(after),
            (None, None) => Start::Offset(window.offset),
        };
        let rows = match start {
            Start::Offset(offset) => read(&order.everything(), window.limit, offset)?,
            Start::After(key) => {
                from_seeks(order.seeks(key, Side::After), window.limit, &mut read)?
            }
            Start::Before(key) => {
                let mut rows = from_seeks(order.seeks(key, Side::Before), window.limit, &mut read)?;
                rows.reverse();
                rows
            }
        };
        if let (Some((_, first)), Some((_, last))) = (rows.first(), rows.last()) {
            let end = window.offset + rows.len() as u64 - 1;
            for (place, key) in [(window.offset, first), (end, last)] {
                known.insert(place, key.clone());
                found.insert(place, key.clone());
            }
        }
        read_windows.push((
            window.clone(),
            rows.into_iter().map(|(row, _)| row).collect(),
        ));
    }
    Ok(WindowsRead {
        windows: read_windows,
        found,
    })
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
