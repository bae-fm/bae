//! A list's items in its order: which rows it shows, the order each of its
//! sorts runs them in, and the reads that take a window of them or find where
//! one sits.
//!
//! Every order is read off the placement's order indexes, so a window read
//! by its offset reads about as many index entries as its offset and limit,
//! one read from an item beside it about as many as its limit
//! (`list_windows`), and a position is the count of the items before it in
//! the same order. Found's members sort in their header's place, so Found is
//! read in runs off three of them. A search that finds few rows has its
//! windows read from those rows instead, sorted, each member by its header's
//! terms. Done runs in two parts: the releases coven still has uploads queued
//! for (`coven_uploads`), first to go first, and then the rest in the index's
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
/// the row, whether it runs in reverse, and whether it can be NULL.
struct Term {
    expression: &'static str,
    descending: bool,
    nullable: bool,
}

const fn up(expression: &'static str) -> Term {
    Term {
        expression,
        descending: false,
        nullable: false,
    }
}

const fn down(expression: &'static str) -> Term {
    Term {
        expression,
        descending: true,
        nullable: false,
    }
}

impl Term {
    /// The same term over a column a row can hold NULL in: an undated row's
    /// date, a header's display path.
    const fn nullable(self) -> Self {
        Term {
            nullable: true,
            ..self
        }
    }
}

const NEWEST_FIRST: (&[Term], &[Term]) = (
    &[
        up("{t}.unit_date IS NULL"),
        down("{t}.unit_date").nullable(),
        up("{t}.root_position"),
        up("{t}.watched_folder_path"),
        up("{t}.unit_path"),
    ],
    &[
        down("{t}.item = 'group'"),
        up("{t}.own_date IS NULL"),
        down("{t}.own_date").nullable(),
        up("{t}.display_path COLLATE natural_order").nullable(),
        up("{t}.key"),
    ],
);

const OLDEST_FIRST: (&[Term], &[Term]) = (
    &[
        up("{t}.unit_date IS NULL"),
        up("{t}.unit_date").nullable(),
        up("{t}.root_position"),
        up("{t}.watched_folder_path"),
        up("{t}.unit_path"),
    ],
    &[
        down("{t}.item = 'group'"),
        up("{t}.own_date IS NULL"),
        up("{t}.own_date").nullable(),
        up("{t}.display_path COLLATE natural_order").nullable(),
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
        up("{t}.display_path COLLATE natural_order").nullable(),
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
        down("{t}.display_path COLLATE natural_order").nullable(),
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

    /// The term over the alias `t`, as a list order's term.
    fn sort_term(&self, t: &str) -> SortTerm {
        let term = match self.descending {
            true => SortTerm::descending(self.over(t)),
            false => SortTerm::ascending(self.over(t)),
        };
        match self.nullable {
            true => term.nullable(),
            false => term,
        }
    }
}

/// `order`'s terms over the alias `t`, as a list order: what a window of
/// Done's or Skipped's rows, which hold no group, is read from a row beside
/// it by, exactly the order index's (`import_list_newest_first` and its
/// siblings), so SQLite reads the rows in order from it.
fn list_order(order: ImportListOrder, lead: Option<&str>, t: &str) -> ListOrder {
    let (prefix, suffix) = terms(order);
    ListOrder::new(
        lead.map(SortTerm::ascending)
            .into_iter()
            .chain(prefix.iter().chain(suffix).map(|term| term.sort_term(t)))
            .collect(),
    )
}

/// `order` with each member in its header's place: its terms up to the folder
/// over the row aliased `head` — a member's header, every other row itself —
/// then that row's number, which breaks the tie between rows that are not a
/// header and its members, then its terms after the folder over the row
/// aliased `row`. What Found's items, the only ones in groups, run in.
fn unit_order(order: ImportListOrder, head: &str, row: &str) -> ListOrder {
    let (prefix, suffix) = terms(order);
    ListOrder::new(
        prefix
            .iter()
            .map(|term| term.sort_term(head))
            .chain(std::iter::once(SortTerm::ascending(format!(
                "{head}.entry"
            ))))
            .chain(suffix.iter().map(|term| term.sort_term(row)))
            .collect(),
    )
}

/// The placement row whose terms up to the folder the row aliased `t` sorts
/// by in [`unit_order`], joined as `u`: a group's member sorts in its
/// header's place, every other row in its own. For reads that sort the few
/// rows they take.
pub(super) fn unit_join(t: &str, u: &str) -> String {
    format!(
        "JOIN import_list_placement AS {u} ON {u}.entry = CASE \
             WHEN {t}.item <> 'group' AND {t}.group_name IS NOT NULL \
             THEN (SELECT header.entry FROM import_list_placement AS header \
                   WHERE header.watched_folder_path = {t}.watched_folder_path \
                     AND header.item = 'group' AND header.key = {t}.group_name) \
             ELSE {t}.entry END"
    )
}

/// The ORDER BY terms the row aliased `t` sorts by in the list's `order`,
/// its terms up to the folder taken from the row [`unit_join`] joins as `u`:
/// the order the list shows, on any tab.
pub(super) fn sorted_terms(order: ImportListOrder, t: &str, u: &str) -> String {
    unit_order(order, u, t).clause()
}

/// Found's run of `order` in two parts, each read off its own indexes
/// (`import_list_newest_first_unfolded`, `_headers` and `_members`, and their
/// siblings) in [`unit_order`]: the headers and the rows outside every group,
/// from `rows` aliased `p`; and each open group's members under their header,
/// from `members` aliased `m` under headers aliased `h`. Each part selects
/// the row's key and the group it is listed under, and then the order's
/// terms as key columns, held to `seek`. An ORDER BY over those columns
/// after it sorts the whole run, which SQLite does by merging the two parts.
fn run(order: ImportListOrder, (rows, members): (&str, &str), seek: &Seek) -> String {
    let (unfolded, grouped) = (unit_order(order, "p", "p"), unit_order(order, "h", "m"));
    format!(
        "SELECT p.watched_folder_path, p.item, p.key, p.group_name, {unfolded_keys} \
         FROM {rows} AND (p.item = 'group' OR p.group_name IS NULL) AND {unfolded_condition} \
         UNION ALL \
         SELECT m.watched_folder_path, m.item, m.key, m.group_name, {grouped_keys} \
         FROM {members} AND {grouped_condition}",
        unfolded_keys = unfolded.key_columns(),
        unfolded_condition = unfolded.condition(seek),
        grouped_keys = grouped.key_columns(),
        grouped_condition = grouped.condition(seek),
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

/// Where the headers of a list's groups sit, by watched folder and group.
pub(super) type HeaderPlaces = std::collections::BTreeMap<(String, String), u64>;

/// Which rows a stream of a list's items runs over.
#[derive(Debug, Clone, Copy)]
pub(super) enum Shown<'a> {
    /// What the list shows under its view, its tab's rows read in order and
    /// each tested; Found's in runs that pass no folded group's members.
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

/// The stretch of one part a window reaches: the part, the places in the
/// list it runs over (to the list's end for the last part), and the offset
/// and limit the window reads it at.
struct PartWindow {
    part: Part,
    start: u64,
    end: Option<u64>,
    offset: u64,
    limit: u64,
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

    /// Found's rows read in runs, each member in its header's place, by the
    /// rows and the members under their headers each run reads: a header's
    /// members only while the list does not fold it, or while it is the
    /// group being opened. `None` for a search's rows, which are sorted, and
    /// for Done and Skipped, which hold no group.
    fn run_sources(&self) -> Option<(&'static str, &'static str)> {
        if self.tab != TriageTab::Pending {
            return None;
        }
        match self.shown {
            Shown::View => Some((
                "import_list_item AS p WHERE p.list_id = :list AND p.tab = :tab",
                "import_list_judged AS h \
                 CROSS JOIN import_list_item AS m \
                     ON m.list_id = h.list_id AND m.watched_folder_path = h.watched_folder_path \
                    AND m.group_name = h.key AND m.item <> 'group' \
                 WHERE h.list_id = :list AND h.tab = :tab AND h.item = 'group' AND NOT h.folded",
            )),
            Shown::Revealing { .. } => Some((
                "import_list_placement AS p WHERE p.tab = :tab",
                "import_list_placement AS h \
                 CROSS JOIN import_list_placement AS m \
                     ON m.watched_folder_path = h.watched_folder_path \
                    AND m.group_name = h.key AND m.item <> 'group' \
                 WHERE h.tab = :tab AND h.item = 'group' \
                   AND ((h.watched_folder_path IS :opened_root AND h.key IS :opened_group) \
                        OR NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                                       WHERE c.list_id = :list \
                                         AND c.watched_folder_path = h.watched_folder_path \
                                         AND c.group_name = h.key))",
            )),
            Shown::Found => None,
        }
    }

    /// A statement over `part`'s rows, aliased `i`, sorted by
    /// [`Self::part_order`]: Found's each joined to the row it sorts in the
    /// place of as `unit`.
    fn part(&self, part: Part) -> PartSql {
        let (source, condition) = self.source();
        let join = match self.tab {
            TriageTab::Pending => unit_join("i", "unit"),
            TriageTab::Done | TriageTab::Skipped => String::new(),
        };
        let order = self.part_order(part).clause();
        match part {
            Part::Whole => PartSql {
                with: String::new(),
                from: format!("{source} {join} WHERE {condition}"),
                order,
            },
            Part::Uploading => PartSql {
                with: format!("WITH {}", uploading_cte()),
                from: format!(
                    "uploading AS u JOIN {source} ON i.key = u.key {join} WHERE {condition}"
                ),
                order,
            },
            Part::Settled => PartSql {
                with: format!("WITH {}", uploading_cte()),
                from: format!(
                    "{source} {join} WHERE {condition} AND i.key NOT IN (SELECT key FROM uploading)"
                ),
                order,
            },
        }
    }

    /// The order `part`'s rows run in, over the alias `i`: Done's uploads
    /// first to go first, and every part then in the list's order, Found's
    /// with each member in its header's place, the row [`Self::part`] joins
    /// as `unit`. Found's runs read the same terms over their own aliases.
    fn part_order(&self, part: Part) -> ListOrder {
        let lead = match part {
            Part::Uploading => Some("u.position"),
            Part::Whole | Part::Settled => None,
        };
        match self.tab {
            TriageTab::Pending => unit_order(self.order, "unit", "i"),
            TriageTab::Done | TriageTab::Skipped => list_order(self.order, lead, "i"),
        }
    }

    /// Run `run` with each parameter `statement` names, bound.
    fn bound<T>(
        &self,
        statement: &str,
        extra: &[(&str, &dyn ToSql)],
        run: impl FnOnce(&[(&str, &dyn ToSql)]) -> Result<T, DbError>,
    ) -> Result<T, DbError> {
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
        run(bound.as_slice())
    }

    /// Run `statement` with each parameter it names.
    fn query<T>(
        &self,
        sql: &(impl QueryOne + QueryRows),
        statement: &str,
        extra: &[(&str, &dyn ToSql)],
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

    /// The statement reading the rows of `part` that `seek` reads, each
    /// item's key and group then its key in the part's order, at most
    /// `:limit` from `:offset`.
    fn window_statement(&self, part: Part, seek: &Seek) -> String {
        if let Some(sources) = self.run_sources() {
            // Each term sorts its result column, the first four being the
            // key and its group; a column keeps its expression's collation.
            return format!(
                "{run} ORDER BY {ordering} LIMIT :limit OFFSET :offset",
                run = run(self.order, sources, seek),
                ordering = self.part_order(part).column_ordering(seek, 5),
            );
        }
        let PartSql { with, from, .. } = self.part(part);
        let order = self.part_order(part);
        format!(
            "{with} SELECT i.watched_folder_path, i.item, i.key, i.group_name, {keys} \
             FROM {from} AND {condition} \
             ORDER BY {ordering} LIMIT :limit OFFSET :offset",
            keys = order.key_columns(),
            condition = order.condition(seek),
            ordering = order.ordering(seek),
        )
    }

    fn window_of(
        &self,
        sql: &(impl QueryOne + QueryRows),
        part: Part,
        order: &ListOrder,
        seek: &Seek,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<Keyed<ItemKeyColumns>>, DbError> {
        let statement = self.window_statement(part, seek);
        seek.bound(limit, offset, |window| {
            self.query(sql, &statement, window, |row| {
                Ok((ItemKey::read(row)?, order.key_of(row)?))
            })
        })
    }

    /// The plans SQLite reads each part's windows by, one line per step: by
    /// its offset, then by each seek from the part's first item. What a test
    /// holds each order to its index with.
    #[cfg(test)]
    pub(super) fn window_plans(
        &self,
        sql: &(impl QueryOne + QueryRows),
    ) -> Result<Vec<Vec<Vec<String>>>, DbError> {
        let mut parts = Vec::new();
        for &part in self.parts() {
            let order = self.part_order(part);
            let mut seeks = vec![order.everything()];
            let first = self.window_of(sql, part, &order, &order.everything(), 1, 0)?;
            if let Some((_, key)) = first.first() {
                seeks.extend(order.seeks_beside(key));
            }
            let mut plans = Vec::new();
            for seek in seeks {
                let statement =
                    format!("EXPLAIN QUERY PLAN {}", self.window_statement(part, &seek));
                plans.push(seek.bound(50, 0, |window| {
                    self.query(sql, &statement, window, |row| row.get::<_, String>(3))
                })?);
            }
            parts.push(plans);
        }
        Ok(parts)
    }

    /// Each part `window` reaches into, with the offset and limit it reads
    /// that part at, clamped to what the list holds.
    fn window_parts(
        &self,
        sql: &(impl QueryOne + QueryRows),
        window: &LibraryPageWindow,
    ) -> Result<Vec<PartWindow>, DbError> {
        let mut reached = Vec::new();
        let mut start = 0;
        let mut offset = window.offset;
        let mut limit = window.limit;
        let parts = self.parts();
        for (at, &part) in parts.iter().enumerate() {
            if limit == 0 {
                break;
            }
            if at + 1 == parts.len() {
                reached.push(PartWindow {
                    part,
                    start,
                    end: None,
                    offset,
                    limit,
                });
                break;
            }
            let held = self.count(sql, part)?;
            if offset >= held {
                offset -= held;
                start += held;
                continue;
            }
            let read = limit.min(held - offset);
            reached.push(PartWindow {
                part,
                start,
                end: Some(start + held),
                offset,
                limit: read,
            });
            limit -= read;
            offset = 0;
            start += held;
        }
        Ok(reached)
    }

    /// The items `window` covers, in order, clamped to what the list holds,
    /// each part read from a row beside the window whose place `known`
    /// holds when there is one (`list_windows`), and the places of the
    /// window's first and last items in each part it reaches.
    pub(super) fn window(
        &self,
        sql: &(impl QueryOne + QueryRows),
        window: &LibraryPageWindow,
        known: &Places,
    ) -> Result<(Vec<ItemKey>, Places), DbError> {
        let mut items = Vec::new();
        let mut found = Places::new();
        for PartWindow {
            part,
            start,
            end,
            offset,
            limit,
        } in self.window_parts(sql, window)?
        {
            let order = self.part_order(part);
            // The part's places, in its own count from its first row.
            let in_part = known
                .range(start..)
                .take_while(|(place, _)| end.is_none_or(|end| **place < end))
                .map(|(place, key)| (place - start, key.clone()))
                .collect();
            let WindowsRead {
                windows: read,
                found: places,
            } = read_windows(
                &order,
                &crate::library::LibraryPageWindows::from([LibraryPageWindow { offset, limit }]),
                in_part,
                |seek, limit, offset| self.window_of(sql, part, &order, seek, limit, offset),
            )?;
            found.extend(places.into_iter().map(|(place, key)| (place + start, key)));
            for (_, keys) in read {
                for columns in keys {
                    items.push(ItemKey::of(columns)?);
                }
            }
        }
        Ok((items, found))
    }

    /// Add to the list's selection every release `window` covers, its
    /// groups' headers left out.
    pub(super) fn select_window(
        &self,
        sql: &SqlContext<'_, '_>,
        window: &LibraryPageWindow,
    ) -> Result<(), DbError> {
        for PartWindow {
            part,
            offset,
            limit,
            ..
        } in self.window_parts(sql, window)?
        {
            let order = self.part_order(part);
            let everything = order.everything();
            let statement = format!(
                "INSERT INTO import_list_selection (list_id, key) \
                 SELECT :list, w.key FROM ({}) AS w WHERE w.item <> 'group' \
                 ON CONFLICT (list_id, key) DO NOTHING",
                self.window_statement(part, &everything)
            );
            everything.bound(limit, offset, |window| {
                self.bound(&statement, window, |bound| {
                    sql.execute(&statement, bound)?;
                    Ok(())
                })
            })?;
        }
        Ok(())
    }

    /// Where the header of the group each of `keys` is listed under sits in
    /// the list, for the items of the window at `offset`: `None` for a header
    /// and for an item in no group. A group's header leads its members, so a
    /// member's header is the last header of its group in the window before
    /// it, or, for the members a window starts with, the one before the
    /// window: where `headers` says it sits, else found by reading the items
    /// up to it. Every header the window holds or finds goes into `headers`.
    pub(super) fn header_positions(
        &self,
        sql: &(impl QueryOne + QueryRows),
        offset: u64,
        keys: &[ItemKey],
        headers: &mut HeaderPlaces,
    ) -> Result<Vec<Option<u64>>, DbError> {
        let mut leading: Option<(&str, &str, u64)> = None;
        let mut positions = Vec::with_capacity(keys.len());
        for (index, key) in (0_u64..).zip(keys) {
            let Some(group) = key.group_name.as_deref() else {
                positions.push(None);
                continue;
            };
            let root = key.watched_folder_path.as_str();
            let group_key = (root.to_string(), group.to_string());
            if key.item == Item::Group {
                leading = Some((root, group, offset + index));
                headers.insert(group_key, offset + index);
                positions.push(None);
                continue;
            }
            let header = match (leading, headers.get(&group_key)) {
                (Some((at_root, name, position)), _) if at_root == root && name == group => {
                    position
                }
                (_, Some(&position)) => position,
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
            headers.insert(group_key, header);
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
        if let Some(sources) = self.run_sources() {
            // Found runs in one part, its runs numbered in their order.
            let order = self.part_order(Part::Whole);
            let found = self
                .query(
                    sql,
                    &format!(
                        "SELECT s.position FROM ( \
                             SELECT r.watched_folder_path, r.item, r.key, \
                                    row_number() OVER (ORDER BY {ordering}) - 1 AS position \
                             FROM ({run}) AS r) AS s \
                         WHERE {condition} LIMIT 1",
                        ordering = order.key_ordering("r"),
                        run = run(self.order, sources, &order.everything()),
                    ),
                    extra,
                    |row| row.get::<_, i64>(0),
                )?
                .into_iter()
                .next();
            return found
                .map(|position| to_u64(position, "an import list position"))
                .transpose();
        }
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
