//! A list's items in its order: which rows it shows, the order each of its
//! sorts runs them in, and the reads that take a window of them or find where
//! one sits.
//!
//! Every order is read off the placement's order indexes, so a window costs
//! about as many index entries as lie between it and the nearest start a read
//! can take, the list's top, its end or an item an earlier read placed, plus
//! its limit (`list_windows`), and a position is the count of the items
//! before it in the same order. Found's members sort in their header's place, so Found is
//! read in runs off three of them. A search that finds few rows has its
//! windows read from those rows instead, sorted, each member by its header's
//! terms. Done runs in two parts: the releases coven still has uploads queued
//! for (`coven_uploads`), first to go first, and then the rest in the index's
//! order. The first part is the outbox's handful; the second is read by its
//! index like any other tab.

use super::views::tab_column;
use super::*;
use crate::{FolderReleaseDecisionKey, ImportListOrder, TriageTab};
use bae_store::list_windows::{OrderKey, SortValue};
use bae_store::ListWindow;
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

/// One item of a list, by its placement row's key, and the group it is
/// listed under: a header's own, a member's group, or none. Each is the bytes
/// its column holds — the watched folder's path, a release's `CandidateKey`
/// or a header's `RelativeFolder`, and the group's `RelativeFolder` — which
/// the window's reads look its rows up by.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ItemKey {
    pub(super) watched_folder_path: Vec<u8>,
    pub(super) item: Item,
    pub(super) key: Vec<u8>,
    pub(super) group_folder: Option<Vec<u8>>,
}

type ItemKeyColumns = (Vec<u8>, String, Vec<u8>, Option<Vec<u8>>);

impl ItemKey {
    fn read(row: &Row<'_>) -> rusqlite::Result<ItemKeyColumns> {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    }

    fn of((watched_folder_path, item, key, group_folder): ItemKeyColumns) -> Result<Self, DbError> {
        Ok(Self { watched_folder_path, item: Item::of(&item)?, key, group_folder })
    }

    /// The key as text for the log.
    pub(super) fn shown(&self) -> String {
        crate::CandidateKey::from(self.key.clone()).to_string()
    }
}

/// One term an order sorts placement rows by: a column or an expression over
/// the row, whether it runs in reverse, and whether it can be NULL.
struct Term {
    expression: &'static str,
    descending: bool,
    nullable: bool,
    /// Whether the expression's text sorts in natural order (the
    /// `natural_order` collation), so two names it calls equal are one place.
    natural: bool,
}

const fn up(expression: &'static str) -> Term {
    Term { expression, descending: false, nullable: false, natural: false }
}

const fn down(expression: &'static str) -> Term {
    Term { expression, descending: true, nullable: false, natural: false }
}

impl Term {
    /// The same term over a column a row can hold NULL in: an undated row's
    /// date, a header's display path.
    const fn nullable(self) -> Self {
        Term { nullable: true, ..self }
    }

    /// The same term with its text sorted in natural order.
    const fn natural(self) -> Self {
        Term { natural: true, ..self }
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
        up("{t}.display_path").nullable().natural(),
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
        up("{t}.display_path").nullable().natural(),
        up("{t}.key"),
    ],
);

const PATH_ASCENDING: (&[Term], &[Term]) = (
    &[up("{t}.root_position"), up("{t}.watched_folder_path"), up("{t}.unit_path")],
    &[down("{t}.item = 'group'"), up("{t}.display_path").nullable().natural(), up("{t}.key")],
);

const PATH_DESCENDING: (&[Term], &[Term]) = (
    &[down("{t}.root_position"), down("{t}.watched_folder_path"), down("{t}.unit_path")],
    &[down("{t}.item = 'group'"), down("{t}.display_path").nullable().natural(), down("{t}.key")],
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
        let expression = self.expression.replace("{t}", t);
        match self.natural {
            true => format!("{expression} COLLATE natural_order"),
            false => expression,
        }
    }

    /// The term over the alias `t`, as a list order's term.
    fn sort_term(&self, t: &str) -> SortTerm {
        let term = match self.descending {
            true => SortTerm::descending(self.over(t)),
            false => SortTerm::ascending(self.over(t)),
        };
        let term = match self.nullable {
            true => term.nullable(),
            false => term,
        };
        match self.natural {
            true => term.collated(bae_util::natural_order::compare),
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
            .chain(std::iter::once(SortTerm::ascending(format!("{head}.entry"))))
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
             WHEN {t}.item <> 'group' AND {t}.group_folder IS NOT NULL \
             THEN (SELECT header.entry FROM import_list_placement AS header \
                   WHERE header.watched_folder_path = {t}.watched_folder_path \
                     AND header.item = 'group' AND header.key = {t}.group_folder) \
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
        "SELECT p.watched_folder_path, p.item, p.key, p.group_folder, {unfolded_keys} \
         FROM {rows} AND (p.item = 'group' OR p.group_folder IS NULL) AND {unfolded_condition} \
         UNION ALL \
         SELECT m.watched_folder_path, m.item, m.key, m.group_folder, {grouped_keys} \
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
        releases = bae_records::records::query::UPLOADING_RELEASES,
    )
}

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
    Revealing { opened: Option<&'a FolderReleaseDecisionKey> },
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

/// The items of one window, in order, with the key of the first and the key
/// of the item beyond its far end: what the windows beside it start and end
/// at.
pub(super) struct WindowItems {
    pub(super) keys: Vec<ItemKey>,
    pub(super) first: Option<OrderKey>,
    pub(super) beyond: Option<OrderKey>,
}

/// The stretch of one part a window reaches: the part, and the offset and
/// limit the window reads it at.
struct PartWindow {
    part: Part,
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
        Self { list, tab, order, shown }
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
                 AND (i.item = 'group' OR i.group_folder IS NULL \
                      OR (i.watched_folder_path IS :opened_root AND i.group_folder IS :opened_group) \
                      OR NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                                     WHERE c.list_id = :list \
                                       AND c.watched_folder_path = i.watched_folder_path \
                                       AND c.group_folder = i.group_folder))",
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
                    AND m.group_folder = h.key AND m.item <> 'group' \
                 WHERE h.list_id = :list AND h.tab = :tab AND h.item = 'group' AND NOT h.folded",
            )),
            Shown::Revealing { .. } => Some((
                "import_list_placement AS p WHERE p.tab = :tab",
                "import_list_placement AS h \
                 CROSS JOIN import_list_placement AS m \
                     ON m.watched_folder_path = h.watched_folder_path \
                    AND m.group_folder = h.key AND m.item <> 'group' \
                 WHERE h.tab = :tab AND h.item = 'group' \
                   AND ((h.watched_folder_path IS :opened_root AND h.key IS :opened_group) \
                        OR NOT EXISTS (SELECT 1 FROM import_list_collapsed AS c \
                                       WHERE c.list_id = :list \
                                         AND c.watched_folder_path = h.watched_folder_path \
                                         AND c.group_folder = h.key))",
            )),
            Shown::Found => None,
        }
    }

    /// A statement over `part`'s rows, aliased `i`, sorted by
    /// [`Self::part_order`]: Found's each joined to the row it sorts in the
    /// place of as `unit`.
    fn part(&self, part: Part) -> PartSql {
        let (source, condition) = self.source();
        // Done's and Skipped's rows are read off the order indexes, which hold
        // no Found row; naming that lets SQLite read them, since it can't tell
        // from the bound tab.
        let (join, condition) = match self.tab {
            TriageTab::Pending => (unit_join("i", "unit"), condition.to_string()),
            TriageTab::Done | TriageTab::Skipped => {
                (String::new(), format!("{condition} AND i.tab <> 'pending'"))
            }
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
            Shown::Revealing { opened: Some(opened) } => (
                Some(coven::path_bytes(&opened.watched_folder_path)),
                Some(opened.relative_folder.as_bytes().to_vec()),
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
        let bound: Vec<(&str, &dyn ToSql)> =
            named.into_iter().filter(|(name, _)| names(statement, name)).collect();
        run(bound.as_slice())
    }

    /// Run `statement` with each parameter it names.
    fn query<T>(
        &self,
        sql: &impl coven::SqlRead,
        statement: &str,
        extra: &[(&str, &dyn ToSql)],
        read: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, DbError> {
        self.bound(statement, extra, |bound| Ok(sql.query(statement, bound, read)?))
    }

    fn count(&self, sql: &impl coven::SqlRead, part: Part) -> Result<u64, DbError> {
        let PartSql { with, from, .. } = self.part(part);
        let count = self
            .query(sql, &format!("{with} SELECT count(*) FROM {from}"), &[], |row| {
                row.get::<_, i64>(0)
            })?
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
            "{with} SELECT i.watched_folder_path, i.item, i.key, i.group_folder, {keys} \
             FROM {from} AND {condition} \
             ORDER BY {ordering} LIMIT :limit OFFSET :offset",
            keys = order.key_columns(),
            condition = order.condition(seek),
            ordering = order.ordering(seek),
        )
    }

    fn window_of(
        &self,
        sql: &impl coven::SqlRead,
        part: Part,
        order: &ListOrder,
        seek: &Seek,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<Keyed<ItemKeyColumns>>, DbError> {
        let statement = self.window_statement(part, seek);
        seek.bound(limit, offset, |window| {
            self.query(sql, &statement, window, |row| Ok((ItemKey::read(row)?, order.key_of(row)?)))
        })
    }

    /// The plans SQLite reads each part's windows by, one line per step: by
    /// its offset, then by each seek from the part's first item. What a test
    /// holds each order to its index with.
    #[cfg(test)]
    pub(super) fn window_plans(
        &self,
        sql: &impl coven::SqlRead,
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

    /// Each part the rows from `offset` on reach into, with the offset and
    /// limit each is read at, clamped to what the list holds.
    fn window_parts(
        &self,
        sql: &impl coven::SqlRead,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<PartWindow>, DbError> {
        let mut reached = Vec::new();
        let mut offset = offset;
        let mut limit = limit;
        let parts = self.parts();
        for (at, &part) in parts.iter().enumerate() {
            if limit == 0 {
                break;
            }
            if at + 1 == parts.len() {
                reached.push(PartWindow { part, offset, limit });
                break;
            }
            let held = self.count(sql, part)?;
            if offset >= held {
                offset -= held;
                continue;
            }
            let read = limit.min(held - offset);
            reached.push(PartWindow { part, offset, limit: read });
            limit -= read;
            offset = 0;
        }
        Ok(reached)
    }

    /// `key` as the list's keys are made: the number of the part the row is in
    /// (its place in [`Self::parts`]), then the values of that part's order
    /// terms. One order across the parts: every row of an earlier part
    /// sorts before every row of a later one.
    fn led(at: usize, key: &OrderKey) -> OrderKey {
        let lead = SortValue::Integer(i64::try_from(at).expect("a tab has few parts"));
        OrderKey::new(std::iter::once(lead).chain(key.values().iter().cloned()).collect())
    }

    /// The part a list key is in and the values of its order's terms. A key
    /// that names no part of this tab, or holds the wrong number of values for
    /// its part, is one read under another view, which the list refuses.
    fn unled(&self, key: &OrderKey) -> Result<(usize, OrderKey), DbError> {
        let unreadable = || DbError::unreadable("a key of an import list", format!("{key:?}"));
        let Some((SortValue::Integer(at), tail)) = key.values().split_first() else {
            return Err(unreadable());
        };
        let at = usize::try_from(*at).map_err(|_negative| unreadable())?;
        let part = *self.parts().get(at).ok_or_else(unreadable)?;
        if self.part_order(part).term_count() != tail.len() {
            return Err(unreadable());
        }
        Ok((at, OrderKey::new(tail.to_vec())))
    }

    /// The seeks that read `window`, in the order they run, each with the
    /// part it reads: a run of index seeks per part from the part the range
    /// starts in to the part it ends in.
    fn range_plan(&self, window: &ListWindow) -> Result<Vec<(usize, Seek)>, DbError> {
        let parts = self.parts();
        let mut plan = Vec::new();
        match window {
            ListWindow::Forward { from, until, .. } => {
                let from = from.as_ref().map(|key| self.unled(key)).transpose()?;
                let until = until.as_ref().map(|key| self.unled(key)).transpose()?;
                let first = from.as_ref().map_or(0, |(at, _)| *at);
                let last = until.as_ref().map_or(parts.len() - 1, |(at, _)| *at);
                for (at, &part) in parts.iter().enumerate().take(last + 1).skip(first) {
                    let order = self.part_order(part);
                    let start = from.as_ref().filter(|(from_at, _)| *from_at == at);
                    let end = until.as_ref().filter(|(until_at, _)| *until_at == at);
                    plan.extend(
                        order
                            .range_seeks(start.map(|(_, key)| key), end.map(|(_, key)| key))
                            .into_iter()
                            .map(|seek| (at, seek)),
                    );
                }
            }
            ListWindow::Backward { until, .. } => {
                let (at, key) = self.unled(until)?;
                plan.extend(
                    self.part_order(parts[at])
                        .seeks_before(&key)
                        .into_iter()
                        .map(|seek| (at, seek)),
                );
                for before in (0..at).rev() {
                    plan.push((before, self.part_order(parts[before]).everything_reversed()));
                }
            }
        }
        Ok(plan)
    }

    /// The items of `window`, in the list's order, with the key of the first
    /// and of the item beyond the window's far end: one item more than the
    /// limit is read to find it. The list's windows are ranges of its order,
    /// each read from the index of the part it is in and running on into the
    /// next part, or back into the one before, when its part ends.
    pub(super) fn range(
        &self,
        sql: &impl coven::SqlRead,
        window: &ListWindow,
    ) -> Result<WindowItems, DbError> {
        let (limit, backward) = match window {
            ListWindow::Forward { limit, .. } => (*limit, false),
            ListWindow::Backward { limit, .. } => (*limit, true),
        };
        let wanted = limit.saturating_add(1);
        let mut rows: Vec<(ItemKeyColumns, OrderKey)> = Vec::new();
        for (at, seek) in self.range_plan(window)? {
            let remaining = wanted - rows.len() as u64;
            if remaining == 0 {
                break;
            }
            let part = self.parts()[at];
            let order = self.part_order(part);
            rows.extend(
                self.window_of(sql, part, &order, &seek, remaining, 0)?
                    .into_iter()
                    .map(|(columns, key)| (columns, Self::led(at, &key))),
            );
        }
        let beyond = if rows.len() as u64 > limit { rows.pop().map(|(_, key)| key) } else { None };
        if backward {
            rows.reverse();
        }
        let first = rows.first().map(|(_, key)| key.clone());
        let keys = rows
            .into_iter()
            .map(|(columns, _)| ItemKey::of(columns))
            .collect::<Result<_, DbError>>()?;
        Ok(WindowItems { keys, first, beyond })
    }

    /// The key of the item at `position`, read by offset, or of the last
    /// item when the list ends before it; `None` for a list with no items.
    pub(super) fn key_at(
        &self,
        sql: &impl coven::SqlRead,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let parts = self.parts();
        for PartWindow { part, offset, .. } in self.window_parts(sql, position, 1)? {
            let order = self.part_order(part);
            let at = parts.iter().position(|&of| of == part).expect("a part of the tab");
            if let Some((_, key)) =
                self.window_of(sql, part, &order, &order.everything(), 1, offset)?.pop()
            {
                return Ok(Some(Self::led(at, &key)));
            }
        }
        for (at, &part) in parts.iter().enumerate().rev() {
            let order = self.part_order(part);
            if let Some((_, key)) =
                self.window_of(sql, part, &order, &order.everything_reversed(), 1, 0)?.pop()
            {
                return Ok(Some(Self::led(at, &key)));
            }
        }
        Ok(None)
    }

    /// How many items sit before the one at `key`: every item of the parts
    /// before its part, and the rows the seeks before it in its part read, a
    /// count over index ranges.
    pub(super) fn rows_before(
        &self,
        sql: &impl coven::SqlRead,
        key: &OrderKey,
    ) -> Result<u64, DbError> {
        let (at, tail) = self.unled(key)?;
        let parts = self.parts();
        let mut counted = 0;
        for &earlier in &parts[..at] {
            counted += self.count(sql, earlier)?;
        }
        Ok(counted + self.count_before(sql, parts[at], &self.part_order(parts[at]), &tail)?)
    }

    /// Add to the list's selection every release the `limit` items from
    /// `offset` cover, its groups' headers left out.
    pub(super) fn select_window(
        &self,
        sql: &CovenWrite<'_>,
        offset: u64,
        limit: u64,
    ) -> Result<(), DbError> {
        for PartWindow { part, offset, limit, .. } in self.window_parts(sql, offset, limit)? {
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

    /// Where the first item meeting `condition` (over the stream's columns
    /// `watched_folder_path`, `item` and `key`, and the alias `s` for the
    /// row) sits in the list and its key, reading the items before it and no
    /// further; `None` when no item does.
    pub(super) fn position_where(
        &self,
        sql: &impl coven::SqlRead,
        condition: &str,
        extra: &[(&'static str, &dyn ToSql)],
    ) -> Result<Option<(u64, OrderKey)>, DbError> {
        if let Some(sources) = self.run_sources() {
            // Found runs in one part, its runs numbered in their order.
            let order = self.part_order(Part::Whole);
            let found = self
                .query(
                    sql,
                    &format!(
                        "SELECT s.* FROM ( \
                             SELECT r.*, row_number() OVER (ORDER BY {ordering}) - 1 AS position \
                             FROM ({run}) AS r) AS s \
                         WHERE {condition} LIMIT 1",
                        ordering = order.key_ordering("r"),
                        run = run(self.order, sources, &order.everything()),
                    ),
                    extra,
                    |row| Ok((row.get::<_, i64>("position")?, order.key_of(row)?)),
                )?
                .into_iter()
                .next();
            return found
                .map(|(position, key)| {
                    Ok((to_u64(position, "an import list position")?, Self::led(0, &key)))
                })
                .transpose();
        }
        let mut before = 0;
        let parts = self.parts();
        for (at, &part) in parts.iter().enumerate() {
            let PartSql { with, from, order: ordering } = self.part(part);
            let order = self.part_order(part);
            let found = self
                .query(
                    sql,
                    &format!(
                        "{with} SELECT s.* FROM ( \
                             SELECT i.watched_folder_path, i.item, i.key, {keys}, \
                                    row_number() OVER (ORDER BY {ordering}) - 1 AS position \
                             FROM {from}) AS s \
                         WHERE {condition} LIMIT 1",
                        keys = order.key_columns(),
                    ),
                    extra,
                    |row| Ok((row.get::<_, i64>("position")?, order.key_of(row)?)),
                )?
                .into_iter()
                .next();
            if let Some((position, key)) = found {
                let position = before + to_u64(position, "an import list position")?;
                return Ok(Some((position, Self::led(at, &key))));
            }
            if at + 1 < parts.len() {
                before += self.count(sql, part)?;
            }
        }
        Ok(None)
    }

    /// Where the list's first selected item sits and its key: found among the
    /// selected items alone, in the list's order, and placed by counting the
    /// items before it through the seeks a window is read beside a row by.
    /// `None` when no selected item is shown.
    pub(super) fn position_of_first_selected(
        &self,
        sql: &impl coven::SqlRead,
    ) -> Result<Option<(u64, OrderKey)>, DbError> {
        let selected = self
            .query(
                sql,
                "SELECT 1 FROM import_list_selection WHERE list_id = :list LIMIT 1",
                &[],
                |row| row.get::<_, i64>(0),
            )?
            .into_iter()
            .next();
        if selected.is_none() {
            return Ok(None);
        }
        let mut before = 0;
        let parts = self.parts();
        for (at, &part) in parts.iter().enumerate() {
            let order = self.part_order(part);
            let PartSql { with, from, .. } = self.part(part);
            let first = self
                .query(
                    sql,
                    &format!(
                        "{with} SELECT {keys} FROM {from} AND i.item <> 'group' \
                         AND i.key IN (SELECT chosen.key FROM import_list_selection AS chosen \
                                       WHERE chosen.list_id = :list) \
                         ORDER BY {ordering} LIMIT 1",
                        keys = order.key_columns(),
                        ordering = order.clause(),
                    ),
                    &[],
                    |row| order.key_of(row),
                )?
                .into_iter()
                .next();
            if let Some(key) = first {
                let position = before + self.count_before(sql, part, &order, &key)?;
                return Ok(Some((position, Self::led(at, &key))));
            }
            if at + 1 < parts.len() {
                before += self.count(sql, part)?;
            }
        }
        Ok(None)
    }

    /// How many of `part`'s items sit before the one at `key` in the part's
    /// order, `order`: the sum of the rows each seek before it reads.
    fn count_before(
        &self,
        sql: &impl coven::SqlRead,
        part: Part,
        order: &ListOrder,
        key: &OrderKey,
    ) -> Result<u64, DbError> {
        let mut counted = 0;
        for seek in order.seeks_before(key) {
            counted += self.count_seek(sql, part, order, &seek, None)?;
        }
        Ok(counted)
    }

    /// How many of `part`'s items `seek` reads in the part's order, `order`,
    /// counting no further than `cap`.
    fn count_seek(
        &self,
        sql: &impl coven::SqlRead,
        part: Part,
        order: &ListOrder,
        seek: &Seek,
        cap: Option<u64>,
    ) -> Result<u64, DbError> {
        let rows = match self.run_sources() {
            Some(sources) => format!("SELECT 1 FROM ({})", run(self.order, sources, seek)),
            None => {
                let PartSql { with, from, .. } = self.part(part);
                format!("{with} SELECT 1 FROM {from} AND {}", order.condition(seek))
            }
        };
        let statement = match cap {
            Some(_) => format!("SELECT count(*) FROM ({rows} LIMIT :limit)"),
            None => format!("SELECT count(*) FROM ({rows})"),
        };
        let count = seek.bound(cap.unwrap_or(0), 0, |bound| {
            self.query(sql, &statement, bound, |row| row.get::<_, i64>(0))
        })?;
        to_u64(
            count.into_iter().next().expect("a count answers one row"),
            "an import list position",
        )
    }

    /// Where the release at `key` sits in the list; `None` when the list
    /// does not show it.
    pub(super) fn position_of(
        &self,
        sql: &impl coven::SqlRead,
        key: &crate::CandidateKey,
    ) -> Result<Option<(u64, OrderKey)>, DbError> {
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
