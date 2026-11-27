//! A list's items in its order: which rows it shows, the order each of its
//! sorts runs them in, and the reads that take a window of them or find where
//! one sits.
//!
//! The list is a join of the rows that others sort under (`u`: a group's
//! header, and each row in no group) with the rows sorting under each
//! (`unit_entry`). An order sorts by terms of `u`, which a group's header and
//! members therefore hold alike, so they sort together, and then by terms of
//! the row itself, the header first. Each order has two indexes, one for each
//! side of the join, so SQLite reads the list in its order with no sort: a
//! window is one SELECT
//! that starts after its key by comparing row values, and a change to what a
//! group sorts by writes its header alone. A position is the count of the
//! items before it in the same order. Done lists first the releases with files
//! in coven's upload queue, the first queued first.

use super::*;
use crate::{ImportListOrder, TriageTab};
use bae_store::list_windows::{OrderKey, SortValue};
use rusqlite::ToSql;

/// What one item of the list is, by the placement's `item` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Item {
    Candidate,
    Invalid,
    Group,
}

impl Item {
    pub(super) fn of(column: &str) -> Result<Self, DbError> {
        match column {
            "candidate" => Ok(Self::Candidate),
            "invalid" => Ok(Self::Invalid),
            "group" => Ok(Self::Group),
            other => Err(DbError::unreadable("an import list's stored item", other)),
        }
    }
}

/// One order's terms, every one a column that is never NULL and all run the
/// same way, so a row comparison against a key reads what follows it.
struct OrderTerms {
    /// The columns of the row an item sorts under, its number last.
    unit: &'static [&'static str],
    /// The columns of the item itself, which put a group's header before its
    /// members and order the members.
    within: &'static [&'static str],
    descending: bool,
}

const NEWEST_FIRST: OrderTerms = OrderTerms {
    unit: &[
        "sort_queued",
        "sort_queue",
        "sort_undated",
        "sort_newest",
        "root_position",
        "watched_folder_path",
        "path_text",
        "entry",
    ],
    within: &["sort_member", "sort_undated", "sort_newest", "path_text", "key"],
    descending: false,
};

const OLDEST_FIRST: OrderTerms = OrderTerms {
    unit: &[
        "sort_queued",
        "sort_queue",
        "sort_undated",
        "sort_oldest",
        "root_position",
        "watched_folder_path",
        "path_text",
        "entry",
    ],
    within: &["sort_member", "sort_undated", "sort_oldest", "path_text", "key"],
    descending: false,
};

const PATH_ASCENDING: OrderTerms = OrderTerms {
    unit: &[
        "sort_queued",
        "sort_queue",
        "root_position",
        "watched_folder_path",
        "path_text",
        "entry",
    ],
    within: &["sort_member", "path_text", "key"],
    descending: false,
};

const PATH_DESCENDING: OrderTerms = OrderTerms {
    unit: &[
        "sort_queued_back",
        "sort_queue_back",
        "root_position",
        "watched_folder_path",
        "path_text",
        "entry",
    ],
    within: &["sort_header", "path_text", "key"],
    descending: true,
};

fn terms(order: ImportListOrder) -> &'static OrderTerms {
    match order {
        ImportListOrder::NewestFirst => &NEWEST_FIRST,
        ImportListOrder::OldestFirst => &OLDEST_FIRST,
        ImportListOrder::PathAscending => &PATH_ASCENDING,
        ImportListOrder::PathDescending => &PATH_DESCENDING,
    }
}

/// The values the first of an item's own terms, 0 or 1, lies between: what
/// every item of a unit other than a key's compares past.
const BELOW_EVERY_ITEM: i64 = -1;
const ABOVE_EVERY_ITEM: i64 = 2;

/// Which of the list's rows a comparison with a key keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// The row at the key and those after it.
    From,
    /// Those before it.
    Before,
}

impl OrderTerms {
    /// `columns` of the row aliased `t`, separated by commas.
    fn columns(columns: &[&str], t: &str) -> String {
        columns.iter().map(|column| format!("{t}.{column}")).collect::<Vec<_>>().join(", ")
    }

    /// The order's terms over the rows aliased `u` and `i`, each with the
    /// alias it reads.
    fn aliased<'a>(&'a self, u: &'a str, i: &'a str) -> impl Iterator<Item = String> + 'a {
        let unit = self.unit.iter().map(move |column| format!("{u}.{column}"));
        unit.chain(self.within.iter().map(move |column| format!("{i}.{column}")))
    }

    fn count(&self) -> usize {
        self.unit.len() + self.within.len()
    }

    /// The ORDER BY terms of the list over `u` and `i`, run `backwards` or
    /// not.
    fn ordering(&self, u: &str, i: &str, backwards: bool) -> String {
        let direction = if self.descending != backwards { " DESC" } else { "" };
        let terms = self.aliased(u, i).map(|term| format!("{term}{direction}"));
        terms.collect::<Vec<_>>().join(", ")
    }

    /// The condition holding the rows `u` and `i` read to those on `side` of
    /// the key whose value of the term numbered `at` the SQL `value(at)`
    /// reads. The first comparison starts SQLite's read of the units at the
    /// key's unit. The second holds that unit's items to the key's side of
    /// it, and, comparing every other unit's items with a value outside
    /// their range, keeps them all, from the start of the index SQLite reads
    /// them by.
    fn on_side(&self, u: &str, i: &str, side: Side, value: impl Fn(usize) -> String) -> String {
        let unit = Self::columns(self.unit, u);
        let unit_key = (0..self.unit.len()).map(&value).collect::<Vec<_>>().join(", ");
        let ascending = (side == Side::From) != self.descending;
        let open = if ascending { BELOW_EVERY_ITEM } else { ABOVE_EVERY_ITEM };
        let within_key = (0..self.within.len())
            .map(|at| {
                let outside = if at == 0 { open } else { 0 };
                format!(
                    "CASE WHEN ({unit}) = ({unit_key}) THEN {key} ELSE {outside} END",
                    key = value(self.unit.len() + at),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        // Which way each comparison runs: the units on the key's side of it,
        // the key's own included, and its items from the key or strictly
        // before it.
        let (units, items) = match (side, self.descending) {
            (Side::From, false) => (">=", ">="),
            (Side::From, true) => ("<=", "<="),
            (Side::Before, false) => ("<=", "<"),
            (Side::Before, true) => (">=", ">"),
        };
        format!(
            "({unit}) {units} ({unit_key}) AND ({within}) {items} ({within_key})",
            within = Self::columns(self.within, i),
        )
    }
}

/// The ORDER BY terms the list's `order` sorts the row aliased `i` by, under
/// the row aliased `u` it sorts under.
pub(super) fn ordering(order: ImportListOrder, u: &str, i: &str) -> String {
    terms(order).ordering(u, i, false)
}

/// The name of the result column and parameter holding a key's value of the
/// term numbered `at`.
fn key_column(at: usize) -> String {
    format!("order_key_{at}")
}

/// A stream of one list's items in one tab, in its order: the rows it shows
/// and what each statement over it binds.
pub(super) struct Items {
    request: ImportListRequest,
    /// The statement selecting the group a reveal opens whatever the
    /// request's folds say, for the items a reveal counts.
    opened: Option<&'static str>,
}

/// The condition holding a statement's rows to those from a window's first
/// key on, and the values it binds; for a window from the list's top, `1`
/// and none.
pub(super) struct WindowStart {
    pub(super) condition: String,
    values: Vec<(String, SortValue)>,
}

impl WindowStart {
    pub(super) fn params(&self) -> impl Iterator<Item = (&str, &dyn ToSql)> {
        self.values.iter().map(|(name, value)| (name.as_str(), value as &dyn ToSql))
    }
}

impl Items {
    pub(super) fn new(request: ImportListRequest) -> Self {
        Self { request, opened: None }
    }

    /// The items a reveal counts in the list `request` shows: the group the
    /// statement `opened` selects open, whatever the request's folds say.
    pub(super) fn revealing(request: ImportListRequest, opened: &'static str) -> Self {
        Self { request, opened: Some(opened) }
    }

    /// The request's fragments, with the group a reveal opens open.
    fn fragments(&self) -> Fragments<'_> {
        self.request.fragments_opening(self.opened)
    }

    fn tab(&self) -> TriageTab {
        self.request.view.tab
    }

    fn terms(&self) -> &'static OrderTerms {
        terms(self.request.view.order)
    }

    /// The stream's tab as an SQL literal.
    fn tab_sql(&self) -> String {
        tab_literal(self.tab())
    }

    /// The FROM clause reading the stream's rows aliased `i`, each under the
    /// row aliased `u` it sorts under. CROSS JOIN keeps SQLite reading `u`
    /// first, in the order. On Found, a folded group's join reads its header
    /// alone: the header is the one row of its unit the bound on the first of
    /// the order's own terms keeps, so no read passes a folded group's
    /// members.
    pub(super) fn from(&self, i: &str) -> String {
        let folded = match self.tab() {
            TriageTab::Pending => {
                let terms = self.terms();
                let first = terms.within[0];
                // A header is the first of its unit: the least of the
                // ascending orders' first term, the greatest of the
                // descending one's.
                let (bound, header, any) = match terms.descending {
                    false => ("<=", 0, 1),
                    true => (">=", 1, 0),
                };
                format!(
                    " AND {i}.{first} {bound} CASE WHEN u.item = 'group' AND {folds} \
                                               THEN {header} ELSE {any} END",
                    folds = self.fragments().folds_sql("u.watched_folder_path", "u.group_folder"),
                )
            }
            TriageTab::Done | TriageTab::Skipped => String::new(),
        };
        format!(
            "import_list_placement AS u \
             CROSS JOIN import_list_placement AS {i} ON {i}.unit_entry = u.entry{folded}"
        )
    }

    /// The WHERE condition keeping the stream's rows of [`Self::from`].
    pub(super) fn condition(&self, i: &str) -> String {
        let mut condition = format!("u.tab = {} AND u.unit_entry = u.entry", self.tab_sql());
        if let Some(kept) = self.fragments().holds_kept_sql(i) {
            condition.push_str(&format!(" AND {kept}"));
        }
        condition
    }

    /// The order's terms of a row, as the result columns a key is read from.
    pub(super) fn key_columns(&self, i: &str) -> String {
        let terms = self.terms().aliased("u", i);
        let columns = terms.enumerate().map(|(at, term)| format!("{term} AS {}", key_column(at)));
        columns.collect::<Vec<_>>().join(", ")
    }

    /// The ORDER BY terms the stream's rows aliased `i` run in.
    pub(super) fn ordering(&self, i: &str) -> String {
        self.terms().ordering("u", i, false)
    }

    /// The list key the row of a statement selecting [`Self::key_columns`]
    /// holds.
    pub(super) fn key_of(&self, row: &Row<'_>) -> rusqlite::Result<OrderKey> {
        (0..self.terms().count())
            .map(|at| row.get::<_, SortValue>(key_column(at).as_str()))
            .collect::<rusqlite::Result<_>>()
            .map(OrderKey::new)
    }

    /// The values of `key`, each by the name of the parameter that binds it.
    /// A key holding any number of values but one for each of the order's
    /// terms is one read under another view, which the list refuses.
    fn bound_key(&self, key: &OrderKey) -> Result<Vec<(String, SortValue)>, DbError> {
        if key.values().len() != self.terms().count() {
            return Err(DbError::unreadable("a key of an import list", format!("{key:?}")));
        }
        let values = key.values().iter().cloned().enumerate();
        Ok(values.map(|(at, value)| (format!(":{}", key_column(at)), value)).collect())
    }

    /// The condition holding the rows aliased `i` to those from the key
    /// `from` on.
    pub(super) fn starting_at(
        &self,
        i: &str,
        from: Option<&OrderKey>,
    ) -> Result<WindowStart, DbError> {
        let Some(key) = from else {
            return Ok(WindowStart { condition: "1".to_string(), values: Vec::new() });
        };
        let values = self.bound_key(key)?;
        let condition =
            self.terms().on_side("u", i, Side::From, |at| format!(":{}", key_column(at)));
        Ok(WindowStart { condition, values })
    }

    /// An expression over the row aliased `w`: for a member of a group,
    /// whether the list shows no member of the group after it, so the row is
    /// the group's last; NULL for every other row.
    pub(super) fn ends_group(&self, w: &str) -> String {
        let terms = self.terms();
        let after = if terms.descending { "<" } else { ">" };
        let mut kept = String::new();
        if let Some(test) = self.fragments().holds_kept_sql("m") {
            kept = format!(" AND {test}");
        }
        format!(
            "CASE WHEN {w}.item <> 'group' AND {w}.group_folder IS NOT NULL \
                  THEN NOT EXISTS (SELECT 1 FROM import_list_placement AS m \
                                   WHERE m.unit_entry = {w}.unit_entry \
                                     AND ({later}) {after} ({own}){kept}) END",
            later = OrderTerms::columns(terms.within, "m"),
            own = OrderTerms::columns(terms.within, w),
        )
    }

    /// Run `statement` with each parameter it names, of the request's and
    /// `extra`.
    pub(super) fn query<T>(
        &self,
        sql: &impl coven::SqlRead,
        statement: &str,
        extra: &[(&str, &dyn ToSql)],
        read: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, DbError> {
        let filter = pending_filter_column(self.request.view.pending_filter);
        let folded = self.request.folded_json();
        let text = self.request.view.search.as_ref().map(crate::SearchText::as_str);
        let mut named: Vec<(&str, &dyn ToSql)> =
            vec![(":filter", &filter), (":filter_text", &text), (":folded", &folded)];
        named.extend_from_slice(extra);
        let bound: Vec<(&str, &dyn ToSql)> =
            named.into_iter().filter(|(name, _)| names(statement, name)).collect();
        Ok(sql.query(statement, bound.as_slice(), read)?)
    }

    /// An expression counting the stream's rows before the key whose values
    /// `value` reads.
    fn held_before(&self, value: impl Fn(usize) -> String) -> String {
        format!(
            "(SELECT count(*) FROM {from} WHERE {condition} AND {before})",
            from = self.from("i"),
            condition = self.condition("i"),
            before = self.terms().on_side("u", "i", Side::Before, value),
        )
    }

    /// The arm of a reveal's statement for the stream's tab, with the CTE it
    /// reads the candidate's key in the list's order from: when the candidate
    /// the CTE `revealed` holds is on this tab, the arm selects its tab, its
    /// group and how many items sit before it, a count off the order's
    /// indexes.
    pub(super) fn reveal_arm(&self) -> (String, String) {
        let keys = format!("revealed_{}", tab_column(self.tab()));
        let cte = format!(
            "{keys} AS (SELECT {columns} FROM revealed AS i \
                        JOIN import_list_placement AS u ON u.entry = i.unit_entry)",
            columns = self.key_columns("i"),
        );
        let arm = format!(
            "SELECT r.tab, r.watched_folder_path, r.group_folder, {before} AS position \
             FROM revealed AS r, {keys} AS rk WHERE r.tab = {tab}",
            before = self.held_before(|at| format!("rk.{}", key_column(at))),
            tab = self.tab_sql(),
        );
        (cte, arm)
    }

    /// The statement reading the stream's rows from `from` on, each its
    /// item's columns and its key, at most `:limit`.
    #[cfg(test)]
    fn window_statement(&self, from: &WindowStart) -> String {
        format!(
            "SELECT i.watched_folder_path, i.item, i.key, {keys} FROM {source} \
             WHERE {condition} AND {after} ORDER BY {ordering} LIMIT :limit",
            keys = self.key_columns("i"),
            source = self.from("i"),
            condition = self.condition("i"),
            after = from.condition,
            ordering = self.ordering("i"),
        )
    }

    /// The plans SQLite reads the list's windows by, one line per step: from
    /// the list's top, then from its first item. What a test holds each order
    /// to its indexes with.
    #[cfg(test)]
    pub(super) fn window_plans(
        &self,
        sql: &impl coven::SqlRead,
    ) -> Result<Vec<Vec<String>>, DbError> {
        let limit = 50i64;
        let top = self.starting_at("i", None)?;
        let first = self
            .query(sql, &self.window_statement(&top), &[(":limit", &limit)], |row| {
                self.key_of(row)
            })?
            .into_iter()
            .next();
        let mut froms = vec![top];
        if first.is_some() {
            froms.push(self.starting_at("i", first.as_ref())?);
        }
        let mut plans = Vec::new();
        for from in &froms {
            let statement = format!("EXPLAIN QUERY PLAN {}", self.window_statement(from));
            let mut params: Vec<(&str, &dyn ToSql)> = from.params().collect();
            params.push((":limit", &limit));
            plans.push(self.query(sql, &statement, &params, |row| row.get::<_, String>(3))?);
        }
        Ok(plans)
    }

    /// The key of the item at `position`, read by offset, or of the last
    /// item when the list ends before it; `None` for a list with no items.
    /// One statement selects the item at the offset, then the list's last
    /// item, and `key_at` takes the first row.
    pub(super) fn key_at(
        &self,
        sql: &impl coven::SqlRead,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let arm = |backwards: bool, tail: &str| {
            format!(
                "SELECT * FROM (SELECT {keys} FROM {from} WHERE {condition} \
                                ORDER BY {ordering} {tail})",
                keys = self.key_columns("i"),
                from = self.from("i"),
                condition = self.condition("i"),
                ordering = self.terms().ordering("u", "i", backwards),
            )
        };
        let statement = format!(
            "{at} UNION ALL {last} LIMIT 1",
            at = arm(false, "LIMIT 1 OFFSET :position"),
            last = arm(true, "LIMIT 1"),
        );
        let position = bae_store::list_windows::sql_bound(position);
        Ok(self
            .query(sql, &statement, &[(":position", &position)], |row| self.key_of(row))?
            .into_iter()
            .next())
    }

    /// How many items sit before the one at `key`: a count over the order's
    /// indexes.
    pub(super) fn rows_before(
        &self,
        sql: &impl coven::SqlRead,
        key: &OrderKey,
    ) -> Result<u64, DbError> {
        let values = self.bound_key(key)?;
        let statement = format!("SELECT {}", self.held_before(|at| format!(":{}", key_column(at))));
        let params: Vec<(&str, &dyn ToSql)> =
            values.iter().map(|(name, value)| (name.as_str(), value as &dyn ToSql)).collect();
        let count = self
            .query(sql, &statement, &params, |row| row.get::<_, i64>(0))?
            .into_iter()
            .next()
            .expect("a count answers one row");
        to_u64(count, "an import list position")
    }

    /// Each item of the list, with its place in it as `position`: one
    /// statement numbers the items in the list's order. Its columns are the
    /// stream's `watched_folder_path`, `item` and `key`, then `position`.
    fn numbered(&self) -> String {
        format!(
            "SELECT i.watched_folder_path, i.item, i.key, \
                    row_number() OVER (ORDER BY {ordering}) - 1 AS position \
             FROM {from} WHERE {condition}",
            ordering = self.ordering("i"),
            from = self.from("i"),
            condition = self.condition("i"),
        )
    }

    /// Where the first item meeting `condition` (over the stream's columns
    /// `watched_folder_path`, `item` and `key`, and the alias `s` for the
    /// row) sits in the list; `None` when no item does.
    fn position_where(
        &self,
        sql: &impl coven::SqlRead,
        condition: &str,
        extra: &[(&'static str, &dyn ToSql)],
    ) -> Result<Option<u64>, DbError> {
        let statement = format!(
            "SELECT s.position FROM ({numbered}) AS s WHERE {condition} \
             ORDER BY s.position LIMIT 1",
            numbered = self.numbered(),
        );
        self.query(sql, &statement, extra, |row| row.get::<_, i64>("position"))?
            .into_iter()
            .next()
            .map(|position| to_u64(position, "an import list position"))
            .transpose()
    }

    /// The releases the list shows from the release `from` to the release
    /// `to`, both included, in the list's order, its groups' headers left
    /// out: what a Shift gesture selects. The one the list shows when it
    /// shows only one of the two, and none when it shows neither. One
    /// statement numbers the list's items and keeps the releases between the
    /// two ends' numbers.
    pub(super) fn keys_between(
        &self,
        sql: &impl coven::SqlRead,
        from: &crate::CandidateKey,
        to: &crate::CandidateKey,
    ) -> Result<Vec<crate::CandidateKey>, DbError> {
        let statement = format!(
            "WITH items AS ({items}), \
                  ends AS (SELECT MIN(position) AS low, MAX(position) AS high FROM items \
                           WHERE item <> 'group' AND key IN (:from, :to)) \
             SELECT items.key FROM items, ends \
             WHERE items.item <> 'group' AND items.position BETWEEN ends.low AND ends.high \
             ORDER BY items.position",
            items = self.numbered(),
        );
        self.query(sql, &statement, &[(":from", from), (":to", to)], |row| row.get(0))
    }

    /// Where the first item of `keys` the list shows sits and its key;
    /// `None` when the list shows none of them.
    pub(super) fn position_of_first(
        &self,
        sql: &impl coven::SqlRead,
        keys: &[crate::CandidateKey],
    ) -> Result<Option<u64>, DbError> {
        let keys = crate::tables::key_hex_list(keys.iter());
        self.position_where(
            sql,
            "s.item <> 'group' AND s.key IN (SELECT unhex(value) FROM json_each(:keys))",
            &[(":keys", &keys)],
        )
    }

    /// Where the release at `key` sits in the list; `None` when the list
    /// does not show it.
    pub(super) fn position_of(
        &self,
        sql: &impl coven::SqlRead,
        key: &crate::CandidateKey,
    ) -> Result<Option<u64>, DbError> {
        self.position_where(sql, "s.item <> 'group' AND s.key = :key", &[(":key", key)])
    }
}

/// Whether `statement` names the parameter `name`, as a whole name.
fn names(statement: &str, name: &str) -> bool {
    statement.match_indices(name).any(|(at, _)| {
        statement[at + name.len()..]
            .chars()
            .next()
            .is_none_or(|next| !(next.is_alphanumeric() || next == '_'))
    })
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
