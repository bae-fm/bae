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

/// One item of a list, by its placement row's key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ItemKey {
    pub(super) watched_folder_path: String,
    pub(super) item: Item,
    pub(super) key: String,
}

impl ItemKey {
    fn read(row: &Row<'_>) -> rusqlite::Result<(String, String, String)> {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    }

    fn of((watched_folder_path, item, key): (String, String, String)) -> Result<Self, DbError> {
        Ok(Self {
            watched_folder_path,
            item: Item::of(&item)?,
            key,
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

/// The terms `order` sorts placement rows by, over the alias `t`: exactly the
/// order index's (`import_list_newest_first` and its siblings), so SQLite
/// reads the rows in order from it.
pub(super) fn order_terms(order: ImportListOrder, t: &str) -> String {
    match order {
        ImportListOrder::NewestFirst => format!(
            "{t}.unit_date IS NULL, {t}.unit_date DESC, {t}.root_position, \
             {t}.watched_folder_path, {t}.unit_path, {t}.item = 'group' DESC, \
             {t}.own_date IS NULL, {t}.own_date DESC, \
             {t}.display_path COLLATE natural_order, {t}.key"
        ),
        ImportListOrder::OldestFirst => format!(
            "{t}.unit_date IS NULL, {t}.unit_date, {t}.root_position, \
             {t}.watched_folder_path, {t}.unit_path, {t}.item = 'group' DESC, \
             {t}.own_date IS NULL, {t}.own_date, \
             {t}.display_path COLLATE natural_order, {t}.key"
        ),
        ImportListOrder::PathAscending => format!(
            "{t}.root_position, {t}.watched_folder_path, {t}.unit_path, \
             {t}.item = 'group' DESC, {t}.display_path COLLATE natural_order, {t}.key"
        ),
        ImportListOrder::PathDescending => format!(
            "{t}.root_position DESC, {t}.watched_folder_path DESC, {t}.unit_path DESC, \
             {t}.item = 'group' DESC, {t}.display_path COLLATE natural_order DESC, \
             {t}.key DESC"
        ),
    }
}

/// The releases with uploads coven still has queued, by key, and where the
/// first of each release's files stands in the queue: what Done runs first.
/// Only a release's own files count, as the Storage Manager's Uploading filter
/// counts them.
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

    /// Run `statement` with each parameter it names.
    fn query<T>(
        &self,
        sql: &(impl QueryOne + QueryRows),
        statement: &str,
        extra: &[(&'static str, &dyn ToSql)],
        read: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, DbError> {
        let (opened_root, opened_group) = match self.shown {
            Shown::Revealing {
                opened: Some(opened),
            } => (
                Some(opened.watched_folder_path.as_str()),
                Some(opened.relative_folder_path.as_str()),
            ),
            Shown::Revealing { opened: None } | Shown::View | Shown::Found => (None, None),
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
        Ok(sql.query(statement, bound.as_slice(), read)?)
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
        let PartSql { with, from, order } = self.part(part);
        format!(
            "{with} SELECT i.watched_folder_path, i.item, i.key FROM {from} \
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

    /// The items `window` covers, in order, clamped to what the list holds.
    pub(super) fn window(
        &self,
        sql: &(impl QueryOne + QueryRows),
        window: &LibraryPageWindow,
    ) -> Result<Vec<ItemKey>, DbError> {
        let mut items = Vec::new();
        let mut offset = window.offset;
        let mut limit = window.limit;
        for &part in self.parts() {
            if limit == 0 {
                break;
            }
            let last = part == *self.parts().last().expect("a tab runs in a part");
            let held = match last {
                true => None,
                false => Some(self.count(sql, part)?),
            };
            match held {
                Some(held) if offset >= held => offset -= held,
                _ => {
                    let read = self.window_of(sql, part, offset, limit)?;
                    limit = limit.saturating_sub(read.len() as u64);
                    offset = 0;
                    items.extend(read);
                }
            }
        }
        Ok(items)
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
