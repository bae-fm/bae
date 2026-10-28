//! A list's items in its order: which rows it shows, the order each of its
//! sorts runs them in, and the reads that take a window of them or find where
//! one sits.
//!
//! Every order is one of the placement's order indexes, so a window reads
//! about as many index entries as its offset and limit, and a position is the
//! count of the items before it in the same scan. A search that finds few
//! rows has its windows read from those rows instead, sorted. Done runs in
//! two parts: the releases coven still has uploads queued for
//! (`coven_uploads`), first to go first, and then the rest in the index's
//! order. The first part is the outbox's handful; the second is read by its
//! index like any other tab.

use super::super::query::{QueryOne, QueryRows};
use super::*;
use crate::import::{FolderReleaseDecisionKey, ImportListOrder, TriageTab};
use crate::library::LibraryPageWindow;
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
            other => Err(unreadable("an import list item", other)),
        }
    }
}

/// One item of a list, by its placement row's key, and the group it is
/// listed under: a header's own, a member's group, or none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ItemKey {
    pub(super) watched_folder_path: String,
    pub(super) item: Item,
    pub(super) key: String,
    pub(super) group_name: Option<String>,
}

type ItemKeyColumns = (String, String, String, Option<String>);

impl ItemKey {
    fn read(row: &Row<'_>) -> rusqlite::Result<ItemKeyColumns> {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    }

    fn of((watched_folder_path, item, key, group_name): ItemKeyColumns) -> Result<Self, DbError> {
        Ok(Self {
            watched_folder_path,
            item: Item::of(&item)?,
            key,
            group_name,
        })
    }
}

/// The tab as the placement's `tab` column holds it.
pub(super) fn tab_column(tab: TriageTab) -> &'static str {
    match tab {
        TriageTab::Pending => "pending",
        TriageTab::Done => "done",
        TriageTab::Skipped => "skipped",
    }
}

pub(super) fn tab_of(column: &str) -> Result<TriageTab, DbError> {
    match column {
        "pending" => Ok(TriageTab::Pending),
        "done" => Ok(TriageTab::Done),
        "skipped" => Ok(TriageTab::Skipped),
        other => Err(unreadable("an import list tab", other)),
    }
}

/// The order as the list's `list_order` column holds it.
pub(super) fn order_column(order: ImportListOrder) -> &'static str {
    match order {
        ImportListOrder::NewestFirst => "newest_first",
        ImportListOrder::OldestFirst => "oldest_first",
        ImportListOrder::PathAscending => "path_ascending",
        ImportListOrder::PathDescending => "path_descending",
    }
}

pub(super) fn order_of(column: &str) -> Result<ImportListOrder, DbError> {
    match column {
        "newest_first" => Ok(ImportListOrder::NewestFirst),
        "oldest_first" => Ok(ImportListOrder::OldestFirst),
        "path_ascending" => Ok(ImportListOrder::PathAscending),
        "path_descending" => Ok(ImportListOrder::PathDescending),
        other => Err(unreadable("an import list order", other)),
    }
}

/// One term an order sorts placement rows by: a column or an expression over
/// the row, and whether it runs in reverse.
struct Term {
    expression: &'static str,
    descending: bool,
}

const fn up(expression: &'static str) -> Term {
    Term {
        expression,
        descending: false,
    }
}

const fn down(expression: &'static str) -> Term {
    Term {
        expression,
        descending: true,
    }
}

const NEWEST_FIRST: (&[Term], &[Term]) = (
    &[
        up("{t}.unit_date IS NULL"),
        down("{t}.unit_date"),
        up("{t}.root_position"),
        up("{t}.watched_folder_path"),
        up("{t}.unit_path"),
    ],
    &[
        down("{t}.item = 'group'"),
        up("{t}.own_date IS NULL"),
        down("{t}.own_date"),
        up("{t}.display_path COLLATE natural_order"),
        up("{t}.key"),
    ],
);

const OLDEST_FIRST: (&[Term], &[Term]) = (
    &[
        up("{t}.unit_date IS NULL"),
        up("{t}.unit_date"),
        up("{t}.root_position"),
        up("{t}.watched_folder_path"),
        up("{t}.unit_path"),
    ],
    &[
        down("{t}.item = 'group'"),
        up("{t}.own_date IS NULL"),
        up("{t}.own_date"),
        up("{t}.display_path COLLATE natural_order"),
        up("{t}.key"),
    ],
);

const PATH_ASCENDING: (&[Term], &[Term]) = (
    &[
        up("{t}.root_position"),
        up("{t}.watched_folder_path"),
        up("{t}.unit_path"),
    ],
    &[
        down("{t}.item = 'group'"),
        up("{t}.display_path COLLATE natural_order"),
        up("{t}.key"),
    ],
);

const PATH_DESCENDING: (&[Term], &[Term]) = (
    &[
        down("{t}.root_position"),
        down("{t}.watched_folder_path"),
        down("{t}.unit_path"),
    ],
    &[
        down("{t}.item = 'group'"),
        down("{t}.display_path COLLATE natural_order"),
        down("{t}.key"),
    ],
);

/// The terms `order` sorts by up to a row's folder, where only a header and
/// its members tie, and the terms after it, which order a group's header
/// before its members and the members among themselves. A term's expression
/// names the row's columns as `{t}.`.
fn terms(order: ImportListOrder) -> (&'static [Term], &'static [Term]) {
    match order {
        ImportListOrder::NewestFirst => NEWEST_FIRST,
        ImportListOrder::OldestFirst => OLDEST_FIRST,
        ImportListOrder::PathAscending => PATH_ASCENDING,
        ImportListOrder::PathDescending => PATH_DESCENDING,
    }
}

impl Term {
    /// The term over the alias `t`, as an expression.
    fn over(&self, t: &str) -> String {
        self.expression.replace("{t}", t)
    }

    /// The term over the alias `t`, as an ORDER BY term.
    fn ordering(&self, t: &str) -> String {
        match self.descending {
            true => format!("{} DESC", self.over(t)),
            false => self.over(t),
        }
    }
}

/// The terms `order` sorts placement rows by, over the alias `t`: exactly the
/// order index's (`import_list_newest_first` and its siblings), so SQLite
/// reads the rows in order from it.
pub(super) fn order_terms(order: ImportListOrder, t: &str) -> String {
    let (prefix, suffix) = terms(order);
    prefix
        .iter()
        .chain(suffix)
        .map(|term| term.ordering(t))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A folded list's run of `order` in three parts, each read off its own
/// index (`import_list_newest_first_unfolded`, `_headers` and `_members`,
/// and their siblings): the headers and the rows outside every group, and
/// each open group's members under its header. Each part selects the row's
/// key and the group it is listed under, and then the order's terms, the row's number breaking the tie after
/// its folder, which a group's members take from their header; SQLite merges
/// the two parts in that sequence. Every row is one of the list's items
/// (`import_list_item`), and only open groups' members are read.
fn folded_run(order: ImportListOrder) -> String {
    let (prefix, suffix) = terms(order);
    let columns = |head: &str, row: &str| {
        prefix
            .iter()
            .map(|term| term.over(head))
            .chain(std::iter::once(format!("{head}.entry")))
            .chain(suffix.iter().map(|term| term.over(row)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    // Each term sorts its result column, the first four being the key and
    // its group; a column keeps its expression's collation.
    let ordering = prefix
        .iter()
        .chain(std::iter::once(&up("{t}.entry")))
        .chain(suffix)
        .enumerate()
        .map(|(at, term)| match term.descending {
            true => format!("{} DESC", at + 5),
            false => (at + 5).to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT p.watched_folder_path, p.item, p.key, p.group_name, {unfolded} \
         FROM import_list_item AS p \
         WHERE p.list_id = :list AND p.tab = :tab \
           AND (p.item = 'group' OR p.group_name IS NULL) \
         UNION ALL \
         SELECT m.watched_folder_path, m.item, m.key, m.group_name, {members} \
         FROM import_list_judged AS h \
         CROSS JOIN import_list_item AS m \
             ON m.list_id = h.list_id AND m.watched_folder_path = h.watched_folder_path \
            AND m.group_name = h.key AND m.item <> 'group' \
         WHERE h.list_id = :list AND h.tab = :tab AND h.item = 'group' AND NOT h.folded \
         ORDER BY {ordering}",
        unfolded = columns("p", "p"),
        members = columns("h", "m"),
    )
}

/// The releases with uploads coven still has queued, by key, and where the
/// first of each release's files stands in the queue: what Done runs first.
/// Only a release's own files count, as the Storage Manager's upload counts
/// count them.
pub(super) fn uploading_cte() -> String {
    format!(
        "uploading (key, position) AS ( \
             SELECT c.path, MIN(released.position) \
             FROM ({releases}) AS released \
             JOIN local_release_origin AS origin ON origin.release_id = released.release_id \
             JOIN scan_candidate AS c ON c.content_hash = origin.content_hash \
             GROUP BY c.path)",
        releases = super::super::query::UPLOADING_RELEASES,
    )
}

/// Which rows a stream of a list's items runs over.
#[derive(Debug, Clone, Copy)]
pub(super) enum Shown<'a> {
    /// What the list shows under its view, its tab's rows read in order and
    /// each tested.
    View,
    /// What a searching list shows, read from the rows its search finds and
    /// sorted: the same items as `View`.
    Found,
    /// What a list that folds groups shut shows on Found, read in runs that
    /// pass no folded group's members: the same items as `View`.
    Folded,
    /// What `tab` shows with the filters cleared and the group `opened`
    /// folded open: the view a release is revealed in, its other folds kept.
    Revealing {
        opened: Option<&'a FolderReleaseDecisionKey>,
    },
}

/// One part of a tab's run of items: Done's uploads, Done's rest, or a whole
/// tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Whole,
    Uploading,
    Settled,
}

/// A stream of one list's items in one tab, in its order: the parts it runs
/// in, the rows it shows, and what each statement over it binds.
pub(super) struct Items<'a> {
    list: ImportListId,
    tab: TriageTab,
    order: ImportListOrder,
    shown: Shown<'a>,
}

/// A statement over one part of a stream: what it starts with, its rows, and
/// their order.
struct PartSql {
    with: String,
    from: String,
    order: String,
}

impl<'a> Items<'a> {
    pub(super) fn new(
        list: ImportListId,
        tab: TriageTab,
        order: ImportListOrder,
        shown: Shown<'a>,
    ) -> Self {
        Self {
            list,
            tab,
            order,
            shown,
        }
    }

    /// The parts the tab runs in, in order.
    fn parts(&self) -> &'static [Part] {
        match self.tab {
            TriageTab::Done => &[Part::Uploading, Part::Settled],
            TriageTab::Pending | TriageTab::Skipped => &[Part::Whole],
        }
    }

    /// The rows the stream shows, over the alias `i`, and the condition on
    /// them past its source.
    fn source(&self) -> (&'static str, &'static str) {
        match self.shown {
            Shown::View => (
                "import_list_item AS i",
                "i.list_id = :list AND i.tab = :tab",
            ),
            Shown::Found => (
                "import_list_found_item AS i",
                "i.list_id = :list AND i.tab = :tab",
            ),
            // Only a window is read in runs; a position or a part's count
            // reads the folded list's items as `View` does.
            Shown::Folded => (
                "import_list_item AS i",
                "i.list_id = :list AND i.tab = :tab",
            ),
            Shown::Revealing { .. } => (
                "import_list_placement AS i",
                "i.tab = :tab \
                 AND (i.item = 'group' OR i.group_name IS NULL \
                      OR (i.watched_folder_path IS :opened_root AND i.group_name IS :opened_group) \
                      OR NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                                     WHERE c.list_id = :list \
                                       AND c.watched_folder_path = i.watched_folder_path \
                                       AND c.group_name = i.group_name))",
            ),
        }
    }

    fn part(&self, part: Part) -> PartSql {
        let (source, condition) = self.source();
        let terms = order_terms(self.order, "i");
        match part {
            Part::Whole => PartSql {
                with: String::new(),
                from: format!("{source} WHERE {condition}"),
                order: terms,
            },
            Part::Uploading => PartSql {
                with: format!("WITH {}", uploading_cte()),
                from: format!("uploading AS u JOIN {source} ON i.key = u.key WHERE {condition}"),
                order: format!("u.position, {terms}"),
            },
            Part::Settled => PartSql {
                with: format!("WITH {}", uploading_cte()),
                from: format!(
                    "{source} WHERE {condition} AND i.key NOT IN (SELECT key FROM uploading)"
                ),
                order: terms,
            },
        }
    }

    /// Run `run` with each parameter `statement` names, bound.
    fn bound<T>(
        &self,
        statement: &str,
        extra: &[(&'static str, &dyn ToSql)],
        run: impl FnOnce(&[(&str, &dyn ToSql)]) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
        let (opened_root, opened_group) = match self.shown {
            Shown::Revealing {
                opened: Some(opened),
            } => (
                Some(opened.watched_folder_path.as_str()),
                Some(opened.relative_folder_path.as_str()),
            ),
            Shown::Revealing { opened: None } | Shown::View | Shown::Found | Shown::Folded => {
                (None, None)
            }
        };
        let tab = tab_column(self.tab);
        let mut named: Vec<(&str, &dyn ToSql)> = vec![
            (":list", &self.list.0),
            (":tab", &tab),
            (":opened_root", &opened_root),
            (":opened_group", &opened_group),
        ];
        named.extend_from_slice(extra);
        let bound: Vec<(&str, &dyn ToSql)> = named
            .into_iter()
            .filter(|(name, _)| names(statement, name))
            .collect();
        run(bound.as_slice())
    }

    /// Run `statement` with each parameter it names.
    fn query<T>(
        &self,
        sql: &(impl QueryOne + QueryRows),
        statement: &str,
        extra: &[(&'static str, &dyn ToSql)],
        read: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, DbError> {
        self.bound(statement, extra, |bound| {
            Ok(sql.query(statement, bound, read)?)
        })
    }

    fn count(&self, sql: &(impl QueryOne + QueryRows), part: Part) -> Result<u64, DbError> {
        let PartSql { with, from, .. } = self.part(part);
        let count = self
            .query(
                sql,
                &format!("{with} SELECT count(*) FROM {from}"),
                &[],
                |row| row.get::<_, i64>(0),
            )?
            .into_iter()
            .next()
            .expect("a count answers one row");
        to_u64(count, "an import list part's count")
    }

    /// The statement reading a window of `part`.
    fn window_statement(&self, part: Part) -> String {
        if let (Shown::Folded, Part::Whole) = (self.shown, part) {
            return format!("{} LIMIT :limit OFFSET :offset", folded_run(self.order));
        }
        let PartSql { with, from, order } = self.part(part);
        format!(
            "{with} SELECT i.watched_folder_path, i.item, i.key, i.group_name FROM {from} \
             ORDER BY {order} LIMIT :limit OFFSET :offset"
        )
    }

    fn window_of(
        &self,
        sql: &(impl QueryOne + QueryRows),
        part: Part,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<ItemKey>, DbError> {
        let (offset, limit) = (sql_bound(offset), sql_bound(limit));
        self.query(
            sql,
            &self.window_statement(part),
            &[(":limit", &limit), (":offset", &offset)],
            ItemKey::read,
        )?
        .into_iter()
        .map(ItemKey::of)
        .collect()
    }

    /// The plan SQLite reads each part's window by, one line per step: what a
    /// test holds each order to its index with.
    #[cfg(test)]
    pub(super) fn window_plans(
        &self,
        sql: &(impl QueryOne + QueryRows),
    ) -> Result<Vec<Vec<String>>, DbError> {
        self.parts()
            .iter()
            .map(|&part| {
                self.query(
                    sql,
                    &format!("EXPLAIN QUERY PLAN {}", self.window_statement(part)),
                    &[(":limit", &50_i64), (":offset", &0_i64)],
                    |row| row.get::<_, String>(3),
                )
            })
            .collect()
    }

    /// Each part `window` reaches into, with the offset and limit it reads
    /// that part at, clamped to what the list holds.
    fn window_parts(
        &self,
        sql: &(impl QueryOne + QueryRows),
        window: &LibraryPageWindow,
    ) -> Result<Vec<(Part, u64, u64)>, DbError> {
        let mut reached = Vec::new();
        let mut offset = window.offset;
        let mut limit = window.limit;
        let parts = self.parts();
        for (at, &part) in parts.iter().enumerate() {
            if limit == 0 {
                break;
            }
            if at + 1 == parts.len() {
                reached.push((part, offset, limit));
                break;
            }
            let held = self.count(sql, part)?;
            if offset >= held {
                offset -= held;
                continue;
            }
            let read = limit.min(held - offset);
            reached.push((part, offset, read));
            limit -= read;
            offset = 0;
        }
        Ok(reached)
    }

    /// The items `window` covers, in order, clamped to what the list holds.
    pub(super) fn window(
        &self,
        sql: &(impl QueryOne + QueryRows),
        window: &LibraryPageWindow,
    ) -> Result<Vec<ItemKey>, DbError> {
        let mut items = Vec::new();
        for (part, offset, limit) in self.window_parts(sql, window)? {
            items.extend(self.window_of(sql, part, offset, limit)?);
        }
        Ok(items)
    }

    /// Add to the list's selection every release `window` covers, its
    /// groups' headers left out.
    pub(super) fn select_window(
        &self,
        sql: &SqlContext<'_, '_>,
        window: &LibraryPageWindow,
    ) -> Result<(), DbError> {
        for (part, offset, limit) in self.window_parts(sql, window)? {
            let (offset, limit) = (sql_bound(offset), sql_bound(limit));
            let statement = format!(
                "INSERT INTO import_list_selection (list_id, key) \
                 SELECT :list, w.key FROM ({}) AS w WHERE w.item <> 'group' \
                 ON CONFLICT (list_id, key) DO NOTHING",
                self.window_statement(part)
            );
            self.bound(
                &statement,
                &[(":limit", &limit), (":offset", &offset)],
                |bound| {
                    sql.execute(&statement, bound)?;
                    Ok(())
                },
            )?;
        }
        Ok(())
    }

    /// Where the header of the group each of `keys` is listed under sits in
    /// the list, for the items of the window at `offset`: `None` for a header
    /// and for an item in no group. A group's header leads its members, so a
    /// member's header is the last header of its group in the window before
    /// it, or, for the members a window starts with, the one before the
    /// window, found by reading the items up to it.
    pub(super) fn header_positions(
        &self,
        sql: &(impl QueryOne + QueryRows),
        offset: u64,
        keys: &[ItemKey],
    ) -> Result<Vec<Option<u64>>, DbError> {
        let mut leading: Option<(&str, &str, u64)> = None;
        let mut positions = Vec::with_capacity(keys.len());
        for (index, key) in (0_u64..).zip(keys) {
            let Some(group) = key.group_name.as_deref() else {
                positions.push(None);
                continue;
            };
            let root = key.watched_folder_path.as_str();
            if key.item == Item::Group {
                leading = Some((root, group, offset + index));
                positions.push(None);
                continue;
            }
            let header = match leading {
                Some((at_root, name, position)) if at_root == root && name == group => position,
                _ => self
                    .position_where(
                        sql,
                        "s.item = 'group' AND s.watched_folder_path = :root AND s.key = :group",
                        &[(":root", &root), (":group", &group)],
                    )?
                    .ok_or_else(|| {
                        DbError::Message(format!(
                            "the header of the group {group} listing {} is not in the list",
                            key.key
                        ))
                    })?,
            };
            leading = Some((root, group, header));
            positions.push(Some(header));
        }
        Ok(positions)
    }

    /// Where the first item meeting `condition` (over the stream's columns
    /// `watched_folder_path`, `item` and `key`, and the alias `s` for the
    /// row) sits in the list, reading the items before it and no further;
    /// `None` when no item does.
    pub(super) fn position_where(
        &self,
        sql: &(impl QueryOne + QueryRows),
        condition: &str,
        extra: &[(&'static str, &dyn ToSql)],
    ) -> Result<Option<u64>, DbError> {
        let mut before = 0;
        for &part in self.parts() {
            let PartSql { with, from, order } = self.part(part);
            let found = self
                .query(
                    sql,
                    &format!(
                        "{with} SELECT s.position FROM ( \
                             SELECT i.watched_folder_path, i.item, i.key, \
                                    row_number() OVER (ORDER BY {order}) - 1 AS position \
                             FROM {from}) AS s \
                         WHERE {condition} LIMIT 1"
                    ),
                    extra,
                    |row| row.get::<_, i64>(0),
                )?
                .into_iter()
                .next();
            if let Some(position) = found {
                return Ok(Some(before + to_u64(position, "an import list position")?));
            }
            if part != *self.parts().last().expect("a tab runs in a part") {
                before += self.count(sql, part)?;
            }
        }
        Ok(None)
    }

    /// Where the release at `key` sits in the list; `None` when the list
    /// does not show it.
    pub(super) fn position_of(
        &self,
        sql: &(impl QueryOne + QueryRows),
        key: &str,
    ) -> Result<Option<u64>, DbError> {
        self.position_where(sql, "s.item <> 'group' AND s.key = :key", &[(":key", &key)])
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

/// A window's bound as SQLite's LIMIT and OFFSET take it: a bound past the
/// largest is no bound at all, since no list holds that many rows.
fn sql_bound(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
