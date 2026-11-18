//! A list's window: the rows from one key on, at most a limit of them, read
//! as a run of index seeks that each hold a prefix of the order's terms equal
//! and compare the next. Each seek's statement orders by the terms it does
//! not hold equal and has a LIMIT and no OFFSET, so once its read returns as
//! many rows as its LIMIT, coven reruns it only for a write at or before its
//! last row. A write after the window reruns nothing.

use super::{from_seeks, Keyed, ListOrder, OrderKey, Seek, Side};

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
    /// The window with its rows made into others all at once by `convert`,
    /// each keeping its key.
    pub fn try_convert_rows<Other, E>(
        self,
        convert: impl FnOnce(Vec<R>) -> Result<Vec<Other>, E>,
    ) -> Result<WindowRead<Other>, E> {
        let (rows, keys): (Vec<R>, Vec<OrderKey>) = self.rows.into_iter().unzip();
        Ok(WindowRead { window: self.window, rows: convert(rows)?.into_iter().zip(keys).collect() })
    }

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

impl ListOrder {
    /// The seeks that read the rows at or after `from` (every row when it is
    /// `None`), in the list's sort order, each an index range.
    pub fn window_seeks(&self, from: Option<&OrderKey>) -> Vec<Seek> {
        match from {
            None => vec![self.everything()],
            Some(from) => self.seeks(from, Side::After, true, false),
        }
    }
}

/// `window`'s rows, each with its key, from `read`, which runs the list's
/// statement under a seek for at most the limit it is given.
pub fn read_range<R, E>(
    order: &ListOrder,
    window: &ListWindow,
    read: &mut impl FnMut(&Seek, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<WindowRead<R>, E> {
    let rows = from_seeks(order.window_seeks(window.from.as_ref()), window.limit, read)?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// A page of a list: the `limit` rows from the row at `offset` in the list's
/// order. A page's live read is one statement, whose subquery finds the row
/// at `offset` the page starts at; a page that starts past the list's end
/// holds no rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListPage {
    pub offset: u64,
    pub limit: u64,
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
/// position with no row the window holds reads this once and starts the
/// window there.
pub fn key_at<R, E>(
    order: &ListOrder,
    position: u64,
    read: &mut impl FnMut(&Seek, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<Option<OrderKey>, E> {
    if let Some(key) = row_key_at(order, position, read)? {
        return Ok(Some(key));
    }
    Ok(read(&order.everything_reversed(), 1)?.pop().map(|(_, key)| key))
}

/// The key of the row at `position`, read by offset; `None` when the list
/// ends at or before it.
fn row_key_at<R, E>(
    order: &ListOrder,
    position: u64,
    read: &mut impl FnMut(&Seek, u64) -> Result<Vec<Keyed<R>>, E>,
) -> Result<Option<OrderKey>, E> {
    Ok(read(&order.at_offset(position), 1)?.pop().map(|(_, key)| key))
}

impl crate::Store {
    /// The live read of one page of the list `order` sorts, its rows made
    /// into a value by `process`: one statement holding the list's rows to
    /// those at or after the row at the page's offset, which a subquery of
    /// the statement reads by offset, at most the page's limit of them. The
    /// subquery's comparison keeps coven from narrowing which writes rerun
    /// it, so coven reruns it for every write to the list. `statement` is the
    /// list's statement under a seek, and `read` runs it, giving each row
    /// with its key.
    pub fn subscribe_list_page<R, T>(
        &self,
        page: ListPage,
        order: ListOrder,
        statement: impl Fn(&ListOrder, &Seek) -> String,
        read: impl Fn(&coven::SqlReadContext<'_>, &Seek, u64) -> Result<Vec<Keyed<R>>, crate::DbError>
            + Send
            + Sync
            + 'static,
        process: impl Fn(Vec<R>) -> Result<T, crate::DbError> + Send + Sync + 'static,
    ) -> coven::LiveQuery<T, crate::DbError>
    where
        R: Clone + PartialEq + Send + 'static,
        T: Send + 'static,
    {
        let seek = order.from_place(page.offset, statement);
        self.subscribe(move |sql| {
            Ok(read(&sql, &seek, page.limit)?.into_iter().map(|(row, _)| row).collect::<Vec<R>>())
        })
        .process(process)
    }

    /// A live query of one window of the list `order` sorts. When the list's
    /// screen moves the window, coven reruns this same live query with the
    /// new window. `read` runs the list's statement under a seek for at most
    /// a limit and gives each row with its key. Coven reruns the query for a
    /// write at or before the window's last row alone.
    pub fn subscribe_list_window<R, E>(
        &self,
        initial: ListWindow,
        order: ListOrder,
        read: impl Fn(&coven::SqlReadContext<'_>, &Seek, u64) -> Result<Vec<Keyed<R>>, E>
            + Send
            + Sync
            + 'static,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<R>, E>
    where
        R: Send + 'static,
        E: From<coven::DbError> + coven::task::CarriesBug + Send + 'static,
    {
        self.subscribe_reconfigurable(initial, move |window, sql| {
            read_range(&order, window, &mut |seek, limit| read(&sql, seek, limit))
        })
    }
}
