//! A list's items in its order, as the compiled fragments and reads that find
//! them: which rows a request's list shows, the order each of its sorts runs
//! them in, the place of each, the key at a position and the number of items
//! before a key.
//!
//! The list is a join of the rows that others sort under (`u`: a group's
//! header, and each row in no group) with the rows sorting under each
//! (`unit_entry`). An order sorts by terms of `u`, which a group's header and
//! members therefore hold alike, so they sort together, and then by terms of
//! the row itself, the header first. Every term is a column that is never
//! NULL, and an order's terms all run the same way, so a row comparison
//! against a key reads what follows it or comes before it. Each order has two
//! indexes, one for each side of the join, so SQLite reads the list in its
//! order with no sort. A folded group's join reads its header alone: the bound
//! on the first of the row's own terms keeps the header, the one row of its
//! unit the bound holds, so no read passes a folded group's members. Only
//! Found has groups. Done lists first the releases with files in coven's
//! upload queue, the first queued first.
//!
//! An order's SQL names its own columns, so each order has its own fragment of
//! the list's items and its own statement for each read that binds a key, and
//! Rust picks one by the request. A read that numbers the items spreads
//! [`ShownReleases`], whose arms name the four orders. A window (`window_reads`)
//! joins the placement itself, since coven narrows a window's reruns only when
//! the statement that has its ORDER BY and LIMIT is the join.

use super::*;
use crate::ImportListOrder;
use bae_store::list_windows::{OrderKey, SortValue};

// Whether the list shows none of the members of the group of the placement
// row: its folds fold the group, and it is not the group of the candidate at
// `opened`, which a reveal shows open whatever the folds say.
sqlive_compose::fragment!(
    Shut(folds_only: bool, folded: String, opened: Option<crate::CandidateKey>)
        on import_list_placement { shut: bool },
    r#"
    SELECT @crate::tables::import_list::row_fragments::Folded(
               folds_only: :folds_only, folded: :folded, parent: unit)
           AND NOT EXISTS (
               SELECT 1 FROM import_list_placement AS revealed
               WHERE revealed.key = :opened AND revealed.item = 'candidate'
                 AND revealed.watched_folder_path = unit.watched_folder_path
                 AND revealed.group_folder = unit.group_folder) AS shut
    FROM import_list_placement AS unit
    WHERE unit.entry = @parent.entry
"#
);

// The items of the list newest first, each with its key.
sqlive_compose::fragment!(
    NewestFirstItems(
        tab: Option<String>,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        opened: Option<crate::CandidateKey>,
    ),
    r#"
    SELECT u.sort_queued AS order_key_0, u.sort_queue AS order_key_1,
        u.sort_undated AS order_key_2, u.sort_newest AS order_key_3,
        u.root_position AS order_key_4, u.watched_folder_path AS order_key_5,
        u.path_text AS order_key_6, u.entry AS order_key_7,
        w.sort_member AS order_key_8, w.sort_undated AS order_key_9,
        w.sort_newest AS order_key_10, w.path_text AS order_key_11,
        w.key AS order_key_12, w.entry AS order_key_13, w.entry AS item_entry,
        w.item AS item_kind, w.key AS item_key
    FROM import_list_placement AS u
    CROSS JOIN import_list_placement AS w
        ON w.unit_entry = u.entry
       AND w.sort_member <= CASE WHEN u.item = 'group' AND @crate::tables::import_list::list_items::Shut(
              folds_only: :folds_only, folded: :folded, opened: :opened, parent: u)
              THEN 0 ELSE 1 END
    WHERE u.tab = COALESCE(:tab, (
              SELECT shown.tab FROM import_list_placement AS shown
              WHERE shown.key = :opened AND shown.item = 'candidate'))
      AND u.unit_entry = u.entry
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: w)
"#
);

// The items of the list oldest first, each with its key.
sqlive_compose::fragment!(
    OldestFirstItems(
        tab: Option<String>,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        opened: Option<crate::CandidateKey>,
    ),
    r#"
    SELECT u.sort_queued AS order_key_0, u.sort_queue AS order_key_1,
        u.sort_undated AS order_key_2, u.sort_oldest AS order_key_3,
        u.root_position AS order_key_4, u.watched_folder_path AS order_key_5,
        u.path_text AS order_key_6, u.entry AS order_key_7,
        w.sort_member AS order_key_8, w.sort_undated AS order_key_9,
        w.sort_oldest AS order_key_10, w.path_text AS order_key_11,
        w.key AS order_key_12, w.entry AS order_key_13, w.entry AS item_entry,
        w.item AS item_kind, w.key AS item_key
    FROM import_list_placement AS u
    CROSS JOIN import_list_placement AS w
        ON w.unit_entry = u.entry
       AND w.sort_member <= CASE WHEN u.item = 'group' AND @crate::tables::import_list::list_items::Shut(
              folds_only: :folds_only, folded: :folded, opened: :opened, parent: u)
              THEN 0 ELSE 1 END
    WHERE u.tab = COALESCE(:tab, (
              SELECT shown.tab FROM import_list_placement AS shown
              WHERE shown.key = :opened AND shown.item = 'candidate'))
      AND u.unit_entry = u.entry
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: w)
"#
);

// The items of the list by folder, A to Z, each with its key.
sqlive_compose::fragment!(
    PathAscendingItems(
        tab: Option<String>,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        opened: Option<crate::CandidateKey>,
    ),
    r#"
    SELECT u.sort_queued AS order_key_0, u.sort_queue AS order_key_1,
        u.root_position AS order_key_2, u.watched_folder_path AS order_key_3,
        u.path_text AS order_key_4, u.entry AS order_key_5,
        w.sort_member AS order_key_6, w.path_text AS order_key_7, w.key AS order_key_8,
        w.entry AS order_key_9, w.entry AS item_entry, w.item AS item_kind,
        w.key AS item_key
    FROM import_list_placement AS u
    CROSS JOIN import_list_placement AS w
        ON w.unit_entry = u.entry
       AND w.sort_member <= CASE WHEN u.item = 'group' AND @crate::tables::import_list::list_items::Shut(
              folds_only: :folds_only, folded: :folded, opened: :opened, parent: u)
              THEN 0 ELSE 1 END
    WHERE u.tab = COALESCE(:tab, (
              SELECT shown.tab FROM import_list_placement AS shown
              WHERE shown.key = :opened AND shown.item = 'candidate'))
      AND u.unit_entry = u.entry
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: w)
"#
);

// The items of the list by folder, Z to A, each with its key.
sqlive_compose::fragment!(
    PathDescendingItems(
        tab: Option<String>,
        filter: Option<String>,
        filter_text: Option<String>,
        folds_only: bool,
        folded: String,
        opened: Option<crate::CandidateKey>,
    ),
    r#"
    SELECT u.sort_queued_back AS order_key_0, u.sort_queue_back AS order_key_1,
        u.root_position AS order_key_2, u.watched_folder_path AS order_key_3,
        u.path_text AS order_key_4, u.entry AS order_key_5,
        w.sort_header AS order_key_6, w.path_text AS order_key_7, w.key AS order_key_8,
        w.entry AS order_key_9, w.entry AS item_entry, w.item AS item_kind,
        w.key AS item_key
    FROM import_list_placement AS u
    CROSS JOIN import_list_placement AS w
        ON w.unit_entry = u.entry
       AND w.sort_header >= CASE WHEN u.item = 'group' AND @crate::tables::import_list::list_items::Shut(
              folds_only: :folds_only, folded: :folded, opened: :opened, parent: u)
              THEN 1 ELSE 0 END
    WHERE u.tab = COALESCE(:tab, (
              SELECT shown.tab FROM import_list_placement AS shown
              WHERE shown.key = :opened AND shown.item = 'candidate'))
      AND u.unit_entry = u.entry
      AND @crate::tables::import_list::row_fragments::Kept(
          filter: :filter, filter_text: :filter_text, parent: w)
"#
);

// The releases a list shows, each with its place among the list's items, which
// count its groups' headers: the list `list_order` names, of which the one
// arm that names it reads a row.
sqlive_compose::fragment!(
    ShownReleases(
        list_order: String, tab: Option<String>, filter: Option<String>,
        filter_text: Option<String>, folds_only: bool, folded: String,
        opened: Option<crate::CandidateKey>
    ) { release_key: Vec<u8>, release_position: i64 },
    r#"
    WITH numbered(item_kind, item_key, position) AS (
        SELECT i.item_kind, i.item_key,
            row_number() OVER (ORDER BY i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7,
                i.order_key_8, i.order_key_9, i.order_key_10, i.order_key_11,
                i.order_key_12, i.order_key_13) - 1
        FROM @crate::tables::import_list::list_items::NewestFirstItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
        WHERE :list_order = 'newest_first'
        UNION ALL
        SELECT i.item_kind, i.item_key,
            row_number() OVER (ORDER BY i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7,
                i.order_key_8, i.order_key_9, i.order_key_10, i.order_key_11,
                i.order_key_12, i.order_key_13) - 1
        FROM @crate::tables::import_list::list_items::OldestFirstItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
        WHERE :list_order = 'oldest_first'
        UNION ALL
        SELECT i.item_kind, i.item_key,
            row_number() OVER (ORDER BY i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7,
                i.order_key_8, i.order_key_9) - 1
        FROM @crate::tables::import_list::list_items::PathAscendingItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
        WHERE :list_order = 'path_ascending'
        UNION ALL
        SELECT i.item_kind, i.item_key,
            row_number() OVER (ORDER BY i.order_key_0 DESC, i.order_key_1 DESC, i.order_key_2 DESC,
                i.order_key_3 DESC, i.order_key_4 DESC, i.order_key_5 DESC,
                i.order_key_6 DESC, i.order_key_7 DESC, i.order_key_8 DESC,
                i.order_key_9 DESC) - 1
        FROM @crate::tables::import_list::list_items::PathDescendingItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
        WHERE :list_order = 'path_descending'
    )
    SELECT numbered.item_key AS release_key, numbered.position AS release_position
    FROM numbered
    WHERE numbered.item_kind <> 'group'
"#
);

// How many items of the list newest first sit before the one at a key.
sqlive_compose::read!(
    writer NewestFirstPosition(
        tab: Option<String>, filter: Option<String>, filter_text: Option<String>,
        folds_only: bool, folded: String, opened: Option<crate::CandidateKey>,
        order_key_0: bae_store::list_windows::SortValue,
        order_key_1: bae_store::list_windows::SortValue,
        order_key_2: bae_store::list_windows::SortValue,
        order_key_3: bae_store::list_windows::SortValue,
        order_key_4: bae_store::list_windows::SortValue,
        order_key_5: bae_store::list_windows::SortValue,
        order_key_6: bae_store::list_windows::SortValue,
        order_key_7: bae_store::list_windows::SortValue,
        order_key_8: bae_store::list_windows::SortValue,
        order_key_9: bae_store::list_windows::SortValue,
        order_key_10: bae_store::list_windows::SortValue,
        order_key_11: bae_store::list_windows::SortValue,
        order_key_12: bae_store::list_windows::SortValue,
        order_key_13: bae_store::list_windows::SortValue
    ) { before: i64 },
    r#"
    SELECT count(*) AS before
    FROM @crate::tables::import_list::list_items::NewestFirstItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
    WHERE (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3, i.order_key_4,
          i.order_key_5, i.order_key_6, i.order_key_7)
          <= (:order_key_0, :order_key_1, :order_key_2, :order_key_3, :order_key_4,
              :order_key_5, :order_key_6, :order_key_7)
      AND (i.order_key_8, i.order_key_9, i.order_key_10, i.order_key_11, i.order_key_12,
           i.order_key_13)
          < (
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_8 ELSE 2 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_9 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_10 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_11 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_12 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_13 ELSE 0 END)
"#
);

// The key of the item of the list newest first at `position`, read by offset, then
// that of its last item: the first row is the one a jump takes.
sqlive_compose::read!(
    writer NewestFirstKeyAt(
        tab: Option<String>, filter: Option<String>, filter_text: Option<String>,
        folds_only: bool, folded: String, opened: Option<crate::CandidateKey>, position: i64
    ) {
        order_key_0: Option<bae_store::list_windows::SortValue>,
        order_key_1: Option<bae_store::list_windows::SortValue>,
        order_key_2: Option<bae_store::list_windows::SortValue>,
        order_key_3: Option<bae_store::list_windows::SortValue>,
        order_key_4: Option<bae_store::list_windows::SortValue>,
        order_key_5: Option<bae_store::list_windows::SortValue>,
        order_key_6: Option<bae_store::list_windows::SortValue>,
        order_key_7: Option<bae_store::list_windows::SortValue>,
        order_key_8: Option<bae_store::list_windows::SortValue>,
        order_key_9: Option<bae_store::list_windows::SortValue>,
        order_key_10: Option<bae_store::list_windows::SortValue>,
        order_key_11: Option<bae_store::list_windows::SortValue>,
        order_key_12: Option<bae_store::list_windows::SortValue>,
        order_key_13: Option<bae_store::list_windows::SortValue>
    },
    r#"
    SELECT at_position.order_key_0, at_position.order_key_1, at_position.order_key_2, at_position.order_key_3,
        at_position.order_key_4, at_position.order_key_5, at_position.order_key_6, at_position.order_key_7,
        at_position.order_key_8, at_position.order_key_9, at_position.order_key_10, at_position.order_key_11,
        at_position.order_key_12, at_position.order_key_13
    FROM (SELECT i.* FROM @crate::tables::import_list::list_items::NewestFirstItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
          ORDER BY i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3, i.order_key_4,
              i.order_key_5, i.order_key_6, i.order_key_7, i.order_key_8, i.order_key_9,
              i.order_key_10, i.order_key_11, i.order_key_12, i.order_key_13
          LIMIT 1 OFFSET :position) AS at_position
    UNION ALL
    SELECT last.order_key_0, last.order_key_1, last.order_key_2, last.order_key_3,
        last.order_key_4, last.order_key_5, last.order_key_6, last.order_key_7,
        last.order_key_8, last.order_key_9, last.order_key_10, last.order_key_11,
        last.order_key_12, last.order_key_13
    FROM (SELECT i.* FROM @crate::tables::import_list::list_items::NewestFirstItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
          ORDER BY i.order_key_0 DESC, i.order_key_1 DESC, i.order_key_2 DESC,
              i.order_key_3 DESC, i.order_key_4 DESC, i.order_key_5 DESC,
              i.order_key_6 DESC, i.order_key_7 DESC, i.order_key_8 DESC,
              i.order_key_9 DESC, i.order_key_10 DESC, i.order_key_11 DESC,
              i.order_key_12 DESC, i.order_key_13 DESC
          LIMIT 1) AS last
    LIMIT 1
"#
);

// How many items of the list oldest first sit before the one at a key.
sqlive_compose::read!(
    writer OldestFirstPosition(
        tab: Option<String>, filter: Option<String>, filter_text: Option<String>,
        folds_only: bool, folded: String, opened: Option<crate::CandidateKey>,
        order_key_0: bae_store::list_windows::SortValue,
        order_key_1: bae_store::list_windows::SortValue,
        order_key_2: bae_store::list_windows::SortValue,
        order_key_3: bae_store::list_windows::SortValue,
        order_key_4: bae_store::list_windows::SortValue,
        order_key_5: bae_store::list_windows::SortValue,
        order_key_6: bae_store::list_windows::SortValue,
        order_key_7: bae_store::list_windows::SortValue,
        order_key_8: bae_store::list_windows::SortValue,
        order_key_9: bae_store::list_windows::SortValue,
        order_key_10: bae_store::list_windows::SortValue,
        order_key_11: bae_store::list_windows::SortValue,
        order_key_12: bae_store::list_windows::SortValue,
        order_key_13: bae_store::list_windows::SortValue
    ) { before: i64 },
    r#"
    SELECT count(*) AS before
    FROM @crate::tables::import_list::list_items::OldestFirstItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
    WHERE (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3, i.order_key_4,
          i.order_key_5, i.order_key_6, i.order_key_7)
          <= (:order_key_0, :order_key_1, :order_key_2, :order_key_3, :order_key_4,
              :order_key_5, :order_key_6, :order_key_7)
      AND (i.order_key_8, i.order_key_9, i.order_key_10, i.order_key_11, i.order_key_12,
           i.order_key_13)
          < (
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_8 ELSE 2 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_9 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_10 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_11 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_12 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5, i.order_key_6, i.order_key_7)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5, :order_key_6, :order_key_7)
             THEN :order_key_13 ELSE 0 END)
"#
);

// The key of the item of the list oldest first at `position`, read by offset, then
// that of its last item: the first row is the one a jump takes.
sqlive_compose::read!(
    writer OldestFirstKeyAt(
        tab: Option<String>, filter: Option<String>, filter_text: Option<String>,
        folds_only: bool, folded: String, opened: Option<crate::CandidateKey>, position: i64
    ) {
        order_key_0: Option<bae_store::list_windows::SortValue>,
        order_key_1: Option<bae_store::list_windows::SortValue>,
        order_key_2: Option<bae_store::list_windows::SortValue>,
        order_key_3: Option<bae_store::list_windows::SortValue>,
        order_key_4: Option<bae_store::list_windows::SortValue>,
        order_key_5: Option<bae_store::list_windows::SortValue>,
        order_key_6: Option<bae_store::list_windows::SortValue>,
        order_key_7: Option<bae_store::list_windows::SortValue>,
        order_key_8: Option<bae_store::list_windows::SortValue>,
        order_key_9: Option<bae_store::list_windows::SortValue>,
        order_key_10: Option<bae_store::list_windows::SortValue>,
        order_key_11: Option<bae_store::list_windows::SortValue>,
        order_key_12: Option<bae_store::list_windows::SortValue>,
        order_key_13: Option<bae_store::list_windows::SortValue>
    },
    r#"
    SELECT at_position.order_key_0, at_position.order_key_1, at_position.order_key_2, at_position.order_key_3,
        at_position.order_key_4, at_position.order_key_5, at_position.order_key_6, at_position.order_key_7,
        at_position.order_key_8, at_position.order_key_9, at_position.order_key_10, at_position.order_key_11,
        at_position.order_key_12, at_position.order_key_13
    FROM (SELECT i.* FROM @crate::tables::import_list::list_items::OldestFirstItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
          ORDER BY i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3, i.order_key_4,
              i.order_key_5, i.order_key_6, i.order_key_7, i.order_key_8, i.order_key_9,
              i.order_key_10, i.order_key_11, i.order_key_12, i.order_key_13
          LIMIT 1 OFFSET :position) AS at_position
    UNION ALL
    SELECT last.order_key_0, last.order_key_1, last.order_key_2, last.order_key_3,
        last.order_key_4, last.order_key_5, last.order_key_6, last.order_key_7,
        last.order_key_8, last.order_key_9, last.order_key_10, last.order_key_11,
        last.order_key_12, last.order_key_13
    FROM (SELECT i.* FROM @crate::tables::import_list::list_items::OldestFirstItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
          ORDER BY i.order_key_0 DESC, i.order_key_1 DESC, i.order_key_2 DESC,
              i.order_key_3 DESC, i.order_key_4 DESC, i.order_key_5 DESC,
              i.order_key_6 DESC, i.order_key_7 DESC, i.order_key_8 DESC,
              i.order_key_9 DESC, i.order_key_10 DESC, i.order_key_11 DESC,
              i.order_key_12 DESC, i.order_key_13 DESC
          LIMIT 1) AS last
    LIMIT 1
"#
);

// How many items of the list by folder, A to Z sit before the one at a key.
sqlive_compose::read!(
    writer PathAscendingPosition(
        tab: Option<String>, filter: Option<String>, filter_text: Option<String>,
        folds_only: bool, folded: String, opened: Option<crate::CandidateKey>,
        order_key_0: bae_store::list_windows::SortValue,
        order_key_1: bae_store::list_windows::SortValue,
        order_key_2: bae_store::list_windows::SortValue,
        order_key_3: bae_store::list_windows::SortValue,
        order_key_4: bae_store::list_windows::SortValue,
        order_key_5: bae_store::list_windows::SortValue,
        order_key_6: bae_store::list_windows::SortValue,
        order_key_7: bae_store::list_windows::SortValue,
        order_key_8: bae_store::list_windows::SortValue,
        order_key_9: bae_store::list_windows::SortValue
    ) { before: i64 },
    r#"
    SELECT count(*) AS before
    FROM @crate::tables::import_list::list_items::PathAscendingItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
    WHERE (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3, i.order_key_4,
          i.order_key_5)
          <= (:order_key_0, :order_key_1, :order_key_2, :order_key_3, :order_key_4,
              :order_key_5)
      AND (i.order_key_6, i.order_key_7, i.order_key_8, i.order_key_9)
          < (
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5)
             THEN :order_key_6 ELSE 2 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5)
             THEN :order_key_7 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5)
             THEN :order_key_8 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5)
             THEN :order_key_9 ELSE 0 END)
"#
);

// The key of the item of the list by folder, A to Z at `position`, read by offset, then
// that of its last item: the first row is the one a jump takes.
sqlive_compose::read!(
    writer PathAscendingKeyAt(
        tab: Option<String>, filter: Option<String>, filter_text: Option<String>,
        folds_only: bool, folded: String, opened: Option<crate::CandidateKey>, position: i64
    ) {
        order_key_0: Option<bae_store::list_windows::SortValue>,
        order_key_1: Option<bae_store::list_windows::SortValue>,
        order_key_2: Option<bae_store::list_windows::SortValue>,
        order_key_3: Option<bae_store::list_windows::SortValue>,
        order_key_4: Option<bae_store::list_windows::SortValue>,
        order_key_5: Option<bae_store::list_windows::SortValue>,
        order_key_6: Option<bae_store::list_windows::SortValue>,
        order_key_7: Option<bae_store::list_windows::SortValue>,
        order_key_8: Option<bae_store::list_windows::SortValue>,
        order_key_9: Option<bae_store::list_windows::SortValue>
    },
    r#"
    SELECT at_position.order_key_0, at_position.order_key_1, at_position.order_key_2, at_position.order_key_3,
        at_position.order_key_4, at_position.order_key_5, at_position.order_key_6, at_position.order_key_7,
        at_position.order_key_8, at_position.order_key_9
    FROM (SELECT i.* FROM @crate::tables::import_list::list_items::PathAscendingItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
          ORDER BY i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3, i.order_key_4,
              i.order_key_5, i.order_key_6, i.order_key_7, i.order_key_8, i.order_key_9
          LIMIT 1 OFFSET :position) AS at_position
    UNION ALL
    SELECT last.order_key_0, last.order_key_1, last.order_key_2, last.order_key_3,
        last.order_key_4, last.order_key_5, last.order_key_6, last.order_key_7,
        last.order_key_8, last.order_key_9
    FROM (SELECT i.* FROM @crate::tables::import_list::list_items::PathAscendingItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
          ORDER BY i.order_key_0 DESC, i.order_key_1 DESC, i.order_key_2 DESC,
              i.order_key_3 DESC, i.order_key_4 DESC, i.order_key_5 DESC,
              i.order_key_6 DESC, i.order_key_7 DESC, i.order_key_8 DESC,
              i.order_key_9 DESC
          LIMIT 1) AS last
    LIMIT 1
"#
);

// How many items of the list by folder, Z to A sit before the one at a key.
sqlive_compose::read!(
    writer PathDescendingPosition(
        tab: Option<String>, filter: Option<String>, filter_text: Option<String>,
        folds_only: bool, folded: String, opened: Option<crate::CandidateKey>,
        order_key_0: bae_store::list_windows::SortValue,
        order_key_1: bae_store::list_windows::SortValue,
        order_key_2: bae_store::list_windows::SortValue,
        order_key_3: bae_store::list_windows::SortValue,
        order_key_4: bae_store::list_windows::SortValue,
        order_key_5: bae_store::list_windows::SortValue,
        order_key_6: bae_store::list_windows::SortValue,
        order_key_7: bae_store::list_windows::SortValue,
        order_key_8: bae_store::list_windows::SortValue,
        order_key_9: bae_store::list_windows::SortValue
    ) { before: i64 },
    r#"
    SELECT count(*) AS before
    FROM @crate::tables::import_list::list_items::PathDescendingItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
    WHERE (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3, i.order_key_4,
          i.order_key_5)
          >= (:order_key_0, :order_key_1, :order_key_2, :order_key_3, :order_key_4,
              :order_key_5)
      AND (i.order_key_6, i.order_key_7, i.order_key_8, i.order_key_9)
          > (
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5)
             THEN :order_key_6 ELSE -1 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5)
             THEN :order_key_7 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5)
             THEN :order_key_8 ELSE 0 END,
             CASE WHEN (i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3,
                 i.order_key_4, i.order_key_5)
                 = (:order_key_0, :order_key_1, :order_key_2, :order_key_3,
                    :order_key_4, :order_key_5)
             THEN :order_key_9 ELSE 0 END)
"#
);

// The key of the item of the list by folder, Z to A at `position`, read by offset, then
// that of its last item: the first row is the one a jump takes.
sqlive_compose::read!(
    writer PathDescendingKeyAt(
        tab: Option<String>, filter: Option<String>, filter_text: Option<String>,
        folds_only: bool, folded: String, opened: Option<crate::CandidateKey>, position: i64
    ) {
        order_key_0: Option<bae_store::list_windows::SortValue>,
        order_key_1: Option<bae_store::list_windows::SortValue>,
        order_key_2: Option<bae_store::list_windows::SortValue>,
        order_key_3: Option<bae_store::list_windows::SortValue>,
        order_key_4: Option<bae_store::list_windows::SortValue>,
        order_key_5: Option<bae_store::list_windows::SortValue>,
        order_key_6: Option<bae_store::list_windows::SortValue>,
        order_key_7: Option<bae_store::list_windows::SortValue>,
        order_key_8: Option<bae_store::list_windows::SortValue>,
        order_key_9: Option<bae_store::list_windows::SortValue>
    },
    r#"
    SELECT at_position.order_key_0, at_position.order_key_1, at_position.order_key_2, at_position.order_key_3,
        at_position.order_key_4, at_position.order_key_5, at_position.order_key_6, at_position.order_key_7,
        at_position.order_key_8, at_position.order_key_9
    FROM (SELECT i.* FROM @crate::tables::import_list::list_items::PathDescendingItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
          ORDER BY i.order_key_0 DESC, i.order_key_1 DESC, i.order_key_2 DESC,
              i.order_key_3 DESC, i.order_key_4 DESC, i.order_key_5 DESC,
              i.order_key_6 DESC, i.order_key_7 DESC, i.order_key_8 DESC,
              i.order_key_9 DESC
          LIMIT 1 OFFSET :position) AS at_position
    UNION ALL
    SELECT last.order_key_0, last.order_key_1, last.order_key_2, last.order_key_3,
        last.order_key_4, last.order_key_5, last.order_key_6, last.order_key_7,
        last.order_key_8, last.order_key_9
    FROM (SELECT i.* FROM @crate::tables::import_list::list_items::PathDescendingItems(
            tab: :tab, filter: :filter, filter_text: :filter_text,
            folds_only: :folds_only, folded: :folded, opened: :opened) AS i
          ORDER BY i.order_key_0, i.order_key_1, i.order_key_2, i.order_key_3, i.order_key_4,
              i.order_key_5, i.order_key_6, i.order_key_7, i.order_key_8, i.order_key_9
          LIMIT 1) AS last
    LIMIT 1
"#
);

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

/// What every statement over a request's list binds.
#[derive(Debug, Clone)]
pub(super) struct ListBinding {
    pub(super) tab: String,
    /// Found's filter entry, while it narrows the list.
    pub(super) filter: Option<String>,
    pub(super) filter_text: Option<String>,
    /// Whether the folds name the groups folded, rather than those left open.
    pub(super) folds_only: bool,
    /// The groups the folds name, each a JSON array of its watched folder's
    /// path and its folder, as hex of the bytes the placement holds.
    pub(super) folded: String,
}

impl ListBinding {
    pub(super) fn of(request: &ImportListRequest) -> Self {
        let (folds_only, ImportListFolds::Only(named) | ImportListFolds::AllBut(named)) =
            (matches!(request.folds, ImportListFolds::Only(_)), &request.folds);
        let groups: Vec<[String; 2]> = named
            .iter()
            .map(|group| {
                [
                    hex::encode(coven::path_bytes(&group.watched_folder_path)),
                    hex::encode(group.relative_folder.as_bytes()),
                ]
            })
            .collect();
        Self {
            tab: tab_column(request.view.tab).to_string(),
            filter: request.entry().map(|filter| pending_filter_column(filter).to_string()),
            filter_text: request.view.search.as_ref().map(|text| text.as_str().to_string()),
            folds_only,
            folded: serde_json::to_string(&groups).expect("a list of text pairs serializes"),
        }
    }
}

/// The name [`ShownReleases`]'s arm for `order` is picked by.
pub(super) fn order_name(order: ImportListOrder) -> String {
    match order {
        ImportListOrder::NewestFirst => "newest_first",
        ImportListOrder::OldestFirst => "oldest_first",
        ImportListOrder::PathAscending => "path_ascending",
        ImportListOrder::PathDescending => "path_descending",
    }
    .to_string()
}

/// A key column of a row as the value a key holds.
pub(super) trait IntoKeyValue {
    fn into_key_value(self) -> SortValue;
}

impl IntoKeyValue for i64 {
    fn into_key_value(self) -> SortValue {
        SortValue::Integer(self)
    }
}

impl IntoKeyValue for String {
    fn into_key_value(self) -> SortValue {
        SortValue::Text(self)
    }
}

impl IntoKeyValue for Vec<u8> {
    fn into_key_value(self) -> SortValue {
        SortValue::Blob(self)
    }
}

impl IntoKeyValue for SortValue {
    fn into_key_value(self) -> SortValue {
        self
    }
}

impl<T: IntoKeyValue> IntoKeyValue for Option<T> {
    fn into_key_value(self) -> SortValue {
        self.map_or(SortValue::Null, IntoKeyValue::into_key_value)
    }
}

/// The key a row of a statement holds in its columns `order_key_*`.
macro_rules! key_of {
    ($row:ident, $($column:ident)*) => {
        bae_store::list_windows::OrderKey::new(vec![
            $($crate::tables::import_list::list_items::IntoKeyValue::into_key_value($row.$column)),*
        ])
    };
}
pub(super) use key_of;

/// The parameters of a statement that binds a key: the fields given, and the
/// key's values, one for each of the order's terms.
macro_rules! keyed_params {
    ($params:ident { $($field:ident : $value:expr),* $(,)? }, $values:expr, [$($key:ident)*]) => {{
        let [$($key),*] = $values;
        $params { $($field: $value,)* $($key),* }
    }};
}
pub(super) use keyed_params;

/// The values of `key`, one for each of its order's `N` terms. A key with any
/// other number of values is one read under another view, which the list
/// refuses.
pub(super) fn key_values<const N: usize>(key: &OrderKey) -> Result<[SortValue; N], DbError> {
    key.values()
        .to_vec()
        .try_into()
        .map_err(|_values| DbError::unreadable("a key of an import list", format!("{key:?}")))
}

/// The statement counting the items before a key: the read of the list's
/// order, with what it binds.
pub(super) enum PositionStatement {
    NewestFirst(NewestFirstPositionParams),
    OldestFirst(OldestFirstPositionParams),
    PathAscending(PathAscendingPositionParams),
    PathDescending(PathDescendingPositionParams),
}

impl PositionStatement {
    pub(super) fn of(request: &ImportListRequest, key: &OrderKey) -> Result<Self, DbError> {
        let ListBinding { tab, filter, filter_text, folds_only, folded } = ListBinding::of(request);
        let tab = Some(tab);
        macro_rules! bound {
            ($params:ident, [$($key:ident)*]) => {
                keyed_params!(
                    $params {
                        tab: tab,
                        filter: filter,
                        filter_text: filter_text,
                        folds_only: folds_only,
                        folded: folded,
                        opened: None,
                    },
                    key_values(key)?,
                    [$($key)*]
                )
            };
        }
        Ok(match request.view.order {
            ImportListOrder::NewestFirst => Self::NewestFirst(bound!(
                NewestFirstPositionParams,
                [
                    order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                    order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                    order_key_10 order_key_11 order_key_12 order_key_13
                ]
            )),
            ImportListOrder::OldestFirst => Self::OldestFirst(bound!(
                OldestFirstPositionParams,
                [
                    order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                    order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                    order_key_10 order_key_11 order_key_12 order_key_13
                ]
            )),
            ImportListOrder::PathAscending => Self::PathAscending(bound!(
                PathAscendingPositionParams,
                [
                    order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                    order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                ]
            )),
            ImportListOrder::PathDescending => Self::PathDescending(bound!(
                PathDescendingPositionParams,
                [
                    order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                    order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                ]
            )),
        })
    }

    /// The read of how many items sit before the statement's key.
    pub(super) fn reads(self) -> coven::ReadAs<i64> {
        match self {
            Self::NewestFirst(params) => coven::ReadAs::new(params, |row| row.before),
            Self::OldestFirst(params) => coven::ReadAs::new(params, |row| row.before),
            Self::PathAscending(params) => coven::ReadAs::new(params, |row| row.before),
            Self::PathDescending(params) => coven::ReadAs::new(params, |row| row.before),
        }
    }
}

/// How many items sit before a key, as a position statement's row holds it.
pub(super) fn position_of(before: i64) -> Result<u64, DbError> {
    to_u64(before, "an import list position")
}

/// The statement reading the key at a position: the read of the list's order,
/// with what it binds.
pub(super) enum KeyAtStatement {
    NewestFirst(NewestFirstKeyAtParams),
    OldestFirst(OldestFirstKeyAtParams),
    PathAscending(PathAscendingKeyAtParams),
    PathDescending(PathDescendingKeyAtParams),
}

impl KeyAtStatement {
    pub(super) fn of(request: &ImportListRequest, position: u64) -> Self {
        let ListBinding { tab, filter, filter_text, folds_only, folded } = ListBinding::of(request);
        let tab = Some(tab);
        let position = bae_store::list_windows::sql_bound(position);
        macro_rules! bound {
            ($params:ident) => {
                $params { tab, filter, filter_text, folds_only, folded, opened: None, position }
            };
        }
        match request.view.order {
            ImportListOrder::NewestFirst => Self::NewestFirst(bound!(NewestFirstKeyAtParams)),
            ImportListOrder::OldestFirst => Self::OldestFirst(bound!(OldestFirstKeyAtParams)),
            ImportListOrder::PathAscending => Self::PathAscending(bound!(PathAscendingKeyAtParams)),
            ImportListOrder::PathDescending => {
                Self::PathDescending(bound!(PathDescendingKeyAtParams))
            }
        }
    }

    /// The key of the item at the statement's position, or of the list's last
    /// item when the list ends before it; `None` for a list with no items.
    pub(super) fn read(
        &self,
        sql: &impl bae_store::WriterRead,
    ) -> Result<Option<OrderKey>, DbError> {
        macro_rules! first_key {
            ($params:ident, [$($column:ident)*]) => {
                statement_optional(sql, $params)?.map(|row| key_of!(row, $($column)*))
            };
        }
        Ok(match self {
            Self::NewestFirst(params) => first_key!(params, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                order_key_10 order_key_11 order_key_12 order_key_13
            ]),
            Self::OldestFirst(params) => first_key!(params, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
                order_key_10 order_key_11 order_key_12 order_key_13
            ]),
            Self::PathAscending(params) => first_key!(params, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
            ]),
            Self::PathDescending(params) => first_key!(params, [
                order_key_0 order_key_1 order_key_2 order_key_3 order_key_4
                order_key_5 order_key_6 order_key_7 order_key_8 order_key_9
            ]),
        })
    }
}

#[cfg(test)]
#[path = "list_items_tests.rs"]
mod tests;
