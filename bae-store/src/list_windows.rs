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
//! Each table a list's order reads keeps a version, in the versions table of
//! the domain whose list reads it (`library_list_versions`,
//! `release_queue_versions`, `import_list_versions`), that any write
//! inserting, deleting or reordering one of its rows advances, in that write
//! (Appendix A of the spec). A read carries the places its windows' first and
//! last rows sat at, with the versions it read them at, and the next read
//! starts from them only while those versions still stand.

use coven::SqlReadContext;
use rusqlite::types::Value;
use rusqlite::{Row, ToSql};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

/// Which way a list runs its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryPageWindow {
    pub offset: u64,
    pub limit: u64,
}

pub type LibraryPageWindows = BTreeSet<LibraryPageWindow>;

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryBrowseWindow<Row> {
    pub window: LibraryPageWindow,
    pub rows: Vec<Row>,
}

/// What a surface has been told of a list's windows, so a delivery names only
/// the windows whose rows differ from what it holds.
///
/// A window the surface has just asked for is never skipped, whatever rows an
/// earlier delivery held under the same offset and limit: [`request`](Self::request)
/// forgets every window that was not in the request before it or is not in
/// the request now, and [`changed`](Self::changed) keeps nothing for a window
/// outside the request standing.
#[derive(Debug)]
pub struct WindowsTold<Row> {
    state: Mutex<ToldState<Row>>,
}

#[derive(Debug)]
struct ToldState<Row> {
    requested: LibraryPageWindows,
    told: BTreeMap<LibraryPageWindow, Vec<Row>>,
}

impl<Row: Clone + PartialEq> WindowsTold<Row> {
    pub fn new(requested: LibraryPageWindows) -> Self {
        Self { state: Mutex::new(ToldState { requested, told: BTreeMap::new() }) }
    }

    /// The surface now asks for `requested`.
    pub fn request(&self, requested: &LibraryPageWindows) {
        let mut state = self.state.lock().expect("windows told mutex poisoned");
        let before = std::mem::replace(&mut state.requested, requested.clone());
        state.told.retain(|window, _| before.contains(window) && requested.contains(window));
    }

    /// `windows` less those the surface already holds with these rows. A
    /// window outside the request standing is dropped: nothing there is
    /// listening.
    pub fn changed(&self, windows: Vec<LibraryBrowseWindow<Row>>) -> Vec<LibraryBrowseWindow<Row>> {
        let mut guard = self.state.lock().expect("windows told mutex poisoned");
        let ToldState { requested, told } = &mut *guard;
        windows
            .into_iter()
            .filter(|window| {
                if !requested.contains(&window.window) {
                    return false;
                }
                if told.get(&window.window) == Some(&window.rows) {
                    return false;
                }
                told.insert(window.window.clone(), window.rows.clone());
                true
            })
            .collect()
    }
}

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
#[derive(Debug, Clone, PartialEq)]
pub struct OrderKey(Vec<Value>);

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
        let past = seek.past.map(|past| match past {
            Past::Greater(at) => format!("({}) > {}", term(at), key_parameter(at)),
            Past::Less(at) => format!("({}) < {}", term(at), key_parameter(at)),
            Past::NotNull(at) => format!("({}) IS NOT NULL", term(at)),
            Past::Null(at) => format!("({}) IS NULL", term(at)),
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
        self.seeks(key, Side::Before)
    }

    /// The seeks reading the rows after the row at `key`, then those reading
    /// the rows before it: what a test holds to the order's index.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn seeks_beside(&self, key: &OrderKey) -> Vec<Seek> {
        let mut seeks = self.seeks(key, Side::After);
        seeks.extend(self.seeks_before(key));
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
pub type Places = BTreeMap<u64, OrderKey>;

/// Where some of a list's rows sat when one live query last read it, and what
/// placed them then: the versions of the tables the list's order reads, and
/// anything else that orders or narrows the list, such as a sort or a filter.
/// `Kept` is what a read keeps of where its rows sat: their places, and
/// whatever else a list keeps of them.
pub struct KnownPlaces<Standing, Kept = Places> {
    last: Mutex<Option<(Standing, Kept)>>,
}

impl<Standing: PartialEq, Kept: Clone + Default> KnownPlaces<Standing, Kept> {
    pub fn new() -> Self {
        Self { last: Mutex::new(None) }
    }

    /// What the last read kept, if the list stands where it stood for that
    /// read; nothing otherwise.
    pub fn under(&self, standing: &Standing) -> Kept {
        match &*self.last.lock().expect("known places lock poisoned") {
            Some((kept_under, kept)) if kept_under == standing => kept.clone(),
            _ => Kept::default(),
        }
    }

    /// Keep what a read found, under what placed its rows.
    pub fn keep(&self, standing: Standing, kept: Kept) {
        *self.last.lock().expect("known places lock poisoned") = Some((standing, kept));
    }
}

/// The versions of the tables `sources` names, as the versions table
/// `versions` holds them in this read.
pub fn list_versions_on(
    sql: &SqlReadContext<'_>,
    versions: &str,
    sources: &[&str],
) -> rusqlite::Result<Vec<i64>> {
    let statement = format!("SELECT version FROM {versions} WHERE source = ?");
    sources.iter().map(|source| sql.query_row(&statement, [source], |row| row.get(0))).collect()
}

/// Where one window's read starts.
enum Start<'k> {
    Offset(u64),
    After(&'k OrderKey),
    Before(&'k OrderKey),
}

/// What one read of a list's windows found: each window's rows, and where
/// each window's first and last rows sit, which a later read may start from.
pub struct WindowsRead<R> {
    pub windows: Vec<(LibraryPageWindow, Vec<R>)>,
    pub found: Places,
}

/// The known rows beside `window`: the one just before it and the one just
/// after it.
fn known_beside<'k>(
    window: &LibraryPageWindow,
    known: &'k Places,
) -> (Option<&'k OrderKey>, Option<&'k OrderKey>) {
    (
        window.offset.checked_sub(1).and_then(|before| known.get(&before)),
        window.offset.checked_add(window.limit).and_then(|after| known.get(&after)),
    )
}

/// Whether `window` is read from a known row beside it, and not by its
/// offset: the way `read_windows` reads it.
pub fn is_read_beside_known(window: &LibraryPageWindow, known: &Places) -> bool {
    window.limit > 0 && matches!(known_beside(window, known), (Some(_), _) | (_, Some(_)))
}

/// Read each of `windows`, in offset order, from the known row beside it
/// when there is one and by its offset otherwise. `read` runs the list's
/// statement under a seek, for at most `limit` rows from `offset`, and gives
/// each row with its key.
pub fn read_windows<R, E>(
    order: &ListOrder,
    windows: &LibraryPageWindows,
    mut known: Places,
    mut read: impl FnMut(&Seek, u64, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<WindowsRead<R>, E> {
    let mut found = Places::new();
    let mut read_windows = Vec::with_capacity(windows.len());
    for window in windows {
        let start = match known_beside(window, &known) {
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
        read_windows.push((window.clone(), rows.into_iter().map(|(row, _)| row).collect()));
    }
    Ok(WindowsRead { windows: read_windows, found })
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
