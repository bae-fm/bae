//! The composer list's reads: its windows, pages, positions and jumps under
//! the person's sort of up to 3 criteria, and its length.
//!
//! After the criteria, rows go by name, then id, which tells every two apart.
//! Every term of the order sorts ascending and none is NULL. The first
//! criterion's term is a column: a descending one is generated to sort
//! ascending (`composer.sql`), a count negated, and the name's key written in
//! hex, which sorts ascending in the `descending` collation. Each later place
//! has two terms, one compared by its bytes and one in that collation, each a
//! CASE over `field_N`, which names the Nth criterion's term. The term a
//! place does not use, and both past the sort's last criterion, hold one value
//! in every row. A key is a row's value for each of those terms, its name key
//! and its id. The CASEs write out the generated columns' expressions, because
//! coven narrows an expression over stored columns alone.
//!
//! SQLite fixes the column a statement's index leads with, so each read has
//! one statement for each term the sort can lead with, and a window has two:
//! one that reads from a key (`window_reads`), and one with no key that reads
//! from the order's first row, for a window from the list's top
//! (`top_reads`). Each statement writes
//! the order over the list table's own columns, and what coven reruns a window
//! or a count for depends on the term the sort leads with. Under an ascending
//! one, only a write on their side of the key reruns them, and a full window
//! only for a write at or before its last row. coven narrows a WHERE by stored
//! columns alone, so under a descending count, whose column is generated, a
//! write before a window's start reruns it too, and any write reruns the
//! count. Under the name descending, whose generated column is in a collation
//! of its own, coven narrows neither bound, so any write to the list reruns
//! them. No index covers the terms after the first, so SQLite sorts the rows
//! that tie on the first term, and a window reads all of them: few when the
//! sort leads with the name, and every composer with one count when it leads
//! with a count.

use super::top_reads::{
    ComposerTopByNameDescendingParams, ComposerTopByNameParams,
    ComposerTopByReleasesDescendingParams, ComposerTopByReleasesParams,
    ComposerTopByWorksDescendingParams, ComposerTopByWorksParams,
};
use super::window_reads::{
    ComposerWindowByNameDescendingParams, ComposerWindowByNameParams,
    ComposerWindowByReleasesDescendingParams, ComposerWindowByReleasesParams,
    ComposerWindowByWorksDescendingParams, ComposerWindowByWorksParams,
};
use super::*;
use crate::list_sort::{key, key_values, ListSort, ListSortField};
use bae_store::list_windows::{sql_bound, SortValue};
use bae_store::statement_optional;

impl ListSortField for ComposerSortField {
    const NAME: Self = Self::Name;

    fn term(self, direction: SortDirection) -> &'static str {
        match (self, direction) {
            (ComposerSortField::Name, SortDirection::Ascending) => "name",
            (ComposerSortField::Name, SortDirection::Descending) => "name_descending",
            (ComposerSortField::WorkCount, SortDirection::Ascending) => "works",
            (ComposerSortField::WorkCount, SortDirection::Descending) => "works_descending",
            (ComposerSortField::LinkedReleaseCount, SortDirection::Ascending) => "releases",
            (ComposerSortField::LinkedReleaseCount, SortDirection::Descending) => {
                "releases_descending"
            }
        }
    }
}

// The composer list's rows at the places from `offset`, at most `limit` of
// them, for each first term.
coven::read!(
    writer ComposerRowsByName(field_2: Option<String>, field_3: Option<String>, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.name_sort_key,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

coven::read!(
    writer ComposerRowsByNameDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.sort_name_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

coven::read!(
    writer ComposerRowsByWorks(field_2: Option<String>, field_3: Option<String>, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.work_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

coven::read!(
    writer ComposerRowsByWorksDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.sort_works_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

coven::read!(
    writer ComposerRowsByReleases(
        field_2: Option<String>,
        field_3: Option<String>,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.linked_release_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

coven::read!(
    writer ComposerRowsByReleasesDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.sort_releases_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

// How many composers come before the row at a key, for each first term.
coven::read!(
    writer ComposerBeforeByName(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
    WHERE (listing.name_sort_key,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
"#
);

coven::read!(
    writer ComposerBeforeByNameDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
    WHERE (listing.sort_name_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
"#
);

coven::read!(
    writer ComposerBeforeByWorks(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
    WHERE (listing.work_count,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
"#
);

coven::read!(
    writer ComposerBeforeByWorksDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
    WHERE (listing.sort_works_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
"#
);

coven::read!(
    writer ComposerBeforeByReleases(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
    WHERE (listing.linked_release_count,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
"#
);

coven::read!(
    writer ComposerBeforeByReleasesDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
    WHERE (listing.sort_releases_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
"#
);

// The key of the row at the place `index`, or of the last row when the list
// ends before it, for each first term. SQLite reads the offset -1 of an empty
// list as 0, where there is no row.
coven::read!(
    writer ComposerKeyAtByName(
        field_2: Option<String>,
        field_3: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue,
        order_5: SortValue
    },
    r#"
    SELECT listing.name_sort_key AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_4,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_5,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM composer_listing AS listing
    ORDER BY listing.name_sort_key,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM composer_listing AS counted) - 1)
"#
);

coven::read!(
    writer ComposerKeyAtByNameDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue,
        order_5: SortValue
    },
    r#"
    SELECT listing.sort_name_back AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_4,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_5,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM composer_listing AS listing
    ORDER BY listing.sort_name_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM composer_listing AS counted) - 1)
"#
);

coven::read!(
    writer ComposerKeyAtByWorks(
        field_2: Option<String>,
        field_3: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue,
        order_5: SortValue
    },
    r#"
    SELECT listing.work_count AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_4,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_5,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM composer_listing AS listing
    ORDER BY listing.work_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM composer_listing AS counted) - 1)
"#
);

coven::read!(
    writer ComposerKeyAtByWorksDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue,
        order_5: SortValue
    },
    r#"
    SELECT listing.sort_works_back AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_4,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_5,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM composer_listing AS listing
    ORDER BY listing.sort_works_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM composer_listing AS counted) - 1)
"#
);

coven::read!(
    writer ComposerKeyAtByReleases(
        field_2: Option<String>,
        field_3: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue,
        order_5: SortValue
    },
    r#"
    SELECT listing.linked_release_count AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_4,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_5,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM composer_listing AS listing
    ORDER BY listing.linked_release_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM composer_listing AS counted) - 1)
"#
);

coven::read!(
    writer ComposerKeyAtByReleasesDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue,
        order_5: SortValue
    },
    r#"
    SELECT listing.sort_releases_back AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END AS order_4,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_5,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM composer_listing AS listing
    ORDER BY listing.sort_releases_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM composer_listing AS counted) - 1)
"#
);

// How many composers the list has.
coven::read!(
    writer ComposerListLength { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
"#
);

/// The composer list's `window` under `sort`, each row with its key.
#[cfg(test)]
pub(crate) fn composer_window_on(
    sql: &ReadContext<'_>,
    sort: &ListSort<ComposerSortField, 3>,
    window: &ListWindow,
) -> Result<WindowRead<DbComposerSummary>, DbError> {
    composer_window_of(window, sql.reads(&composer_window_read(sort, window)?)?)
}

/// The composer list's `window` from the rows its read returned.
pub(crate) fn composer_window_of(
    window: &ListWindow,
    rows: Vec<bae_store::list_windows::Keyed<crate::composer::ListedComposer>>,
) -> Result<WindowRead<DbComposerSummary>, DbError> {
    let rows = rows
        .into_iter()
        .map(|(shown, key)| Ok((shown.into_summary()?, key)))
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// The read of the composer list's `window` under `sort`, each row with its
/// key.
pub(crate) fn composer_window_read(
    sort: &ListSort<ComposerSortField, 3>,
    window: &ListWindow,
) -> Result<coven::ReadAs<bae_store::list_windows::Keyed<crate::composer::ListedComposer>>, DbError>
{
    let (field_2, field_3) = (sort.later(1), sort.later(2));
    let limit = sql_bound(window.limit);
    macro_rules! rows {
        ($statement:expr) => {
            coven::ReadAs::new($statement, |row| {
                let key = key(
                    [row.order_1, row.order_2, row.order_3, row.order_4, row.order_5],
                    row.order_name,
                    row.order_id,
                );
                (row.shown, key)
            })
        };
    }
    // A window from the top reads from the order's first row, and one from a
    // key from the key's row.
    let Some(from) = &window.from else {
        macro_rules! top {
            ($params:ident) => {
                rows!($params { field_2, field_3, limit })
            };
        }
        return Ok(match sort.first() {
            (ComposerSortField::Name, SortDirection::Ascending) => top!(ComposerTopByNameParams),
            (ComposerSortField::Name, SortDirection::Descending) => {
                top!(ComposerTopByNameDescendingParams)
            }
            (ComposerSortField::WorkCount, SortDirection::Ascending) => {
                top!(ComposerTopByWorksParams)
            }
            (ComposerSortField::WorkCount, SortDirection::Descending) => {
                top!(ComposerTopByWorksDescendingParams)
            }
            (ComposerSortField::LinkedReleaseCount, SortDirection::Ascending) => {
                top!(ComposerTopByReleasesParams)
            }
            (ComposerSortField::LinkedReleaseCount, SortDirection::Descending) => {
                top!(ComposerTopByReleasesDescendingParams)
            }
        });
    };
    let ([key_1, key_2, key_3, key_4, key_5], key_name, key_id) = key_values(from)?;
    macro_rules! from_key {
        ($params:ident) => {
            rows!($params {
                field_2,
                field_3,
                key_1,
                key_2,
                key_3,
                key_4,
                key_5,
                key_name,
                key_id,
                limit
            })
        };
    }
    Ok(match sort.first() {
        (ComposerSortField::Name, SortDirection::Ascending) => {
            from_key!(ComposerWindowByNameParams)
        }
        (ComposerSortField::Name, SortDirection::Descending) => {
            from_key!(ComposerWindowByNameDescendingParams)
        }
        (ComposerSortField::WorkCount, SortDirection::Ascending) => {
            from_key!(ComposerWindowByWorksParams)
        }
        (ComposerSortField::WorkCount, SortDirection::Descending) => {
            from_key!(ComposerWindowByWorksDescendingParams)
        }
        (ComposerSortField::LinkedReleaseCount, SortDirection::Ascending) => {
            from_key!(ComposerWindowByReleasesParams)
        }
        (ComposerSortField::LinkedReleaseCount, SortDirection::Descending) => {
            from_key!(ComposerWindowByReleasesDescendingParams)
        }
    })
}

/// The composer list's rows at `page`'s places under `sort`.
#[cfg(test)]
pub(crate) fn composer_page_on(
    sql: &ReadContext<'_>,
    sort: &ListSort<ComposerSortField, 3>,
    page: ListPage,
) -> Result<Vec<DbComposerSummary>, DbError> {
    sql.reads(&composer_page_read(sort, page))?
        .into_iter()
        .map(crate::composer::ListedComposer::into_summary)
        .collect()
}

/// The read of the composer list's rows at `page`'s places under `sort`.
pub(crate) fn composer_page_read(
    sort: &ListSort<ComposerSortField, 3>,
    page: ListPage,
) -> coven::ReadAs<crate::composer::ListedComposer> {
    let (offset, limit) = (sql_bound(page.offset), sql_bound(page.limit));
    macro_rules! rows {
        ($params:ident) => {{
            let statement =
                $params { field_2: sort.later(1), field_3: sort.later(2), offset, limit };
            coven::ReadAs::new(statement, |row| row.shown)
        }};
    }
    match sort.first() {
        (ComposerSortField::Name, SortDirection::Ascending) => rows!(ComposerRowsByNameParams),
        (ComposerSortField::Name, SortDirection::Descending) => {
            rows!(ComposerRowsByNameDescendingParams)
        }
        (ComposerSortField::WorkCount, SortDirection::Ascending) => {
            rows!(ComposerRowsByWorksParams)
        }
        (ComposerSortField::WorkCount, SortDirection::Descending) => {
            rows!(ComposerRowsByWorksDescendingParams)
        }
        (ComposerSortField::LinkedReleaseCount, SortDirection::Ascending) => {
            rows!(ComposerRowsByReleasesParams)
        }
        (ComposerSortField::LinkedReleaseCount, SortDirection::Descending) => {
            rows!(ComposerRowsByReleasesDescendingParams)
        }
    }
}

/// How many composers come before the row at `at` under `sort`.
#[cfg(test)]
pub(crate) fn composers_before_on(
    sql: &ReadContext<'_>,
    sort: &ListSort<ComposerSortField, 3>,
    at: &OrderKey,
) -> Result<u64, DbError> {
    list_count(exactly_one::<coven::ReadAs<i64>, _>(sql.reads(&composers_before_read(sort, at)?)?)?)
}

/// The read of how many composers come before the row at `at` under `sort`.
pub(crate) fn composers_before_read(
    sort: &ListSort<ComposerSortField, 3>,
    at: &OrderKey,
) -> Result<coven::ReadAs<i64>, DbError> {
    let ([key_1, key_2, key_3, key_4, key_5], key_name, key_id) = key_values(at)?;
    macro_rules! entries {
        ($params:ident) => {{
            let statement = $params {
                field_2: sort.later(1),
                field_3: sort.later(2),
                key_1,
                key_2,
                key_3,
                key_4,
                key_5,
                key_name,
                key_id,
            };
            coven::ReadAs::new(statement, |row| row.entries)
        }};
    }
    Ok(match sort.first() {
        (ComposerSortField::Name, SortDirection::Ascending) => entries!(ComposerBeforeByNameParams),
        (ComposerSortField::Name, SortDirection::Descending) => {
            entries!(ComposerBeforeByNameDescendingParams)
        }
        (ComposerSortField::WorkCount, SortDirection::Ascending) => {
            entries!(ComposerBeforeByWorksParams)
        }
        (ComposerSortField::WorkCount, SortDirection::Descending) => {
            entries!(ComposerBeforeByWorksDescendingParams)
        }
        (ComposerSortField::LinkedReleaseCount, SortDirection::Ascending) => {
            entries!(ComposerBeforeByReleasesParams)
        }
        (ComposerSortField::LinkedReleaseCount, SortDirection::Descending) => {
            entries!(ComposerBeforeByReleasesDescendingParams)
        }
    })
}

/// The key of the composer at `position` under `sort`, or of the last one.
pub(crate) fn composer_key_at_on(
    sql: &ReadContext<'_>,
    sort: &ListSort<ComposerSortField, 3>,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let index = sql_bound(position);
    macro_rules! key_at {
        ($params:ident) => {{
            let statement = $params { field_2: sort.later(1), field_3: sort.later(2), index };
            statement_optional(sql, &statement)?.map(|row| {
                key(
                    [row.order_1, row.order_2, row.order_3, row.order_4, row.order_5],
                    row.order_name,
                    row.order_id,
                )
            })
        }};
    }
    Ok(match sort.first() {
        (ComposerSortField::Name, SortDirection::Ascending) => key_at!(ComposerKeyAtByNameParams),
        (ComposerSortField::Name, SortDirection::Descending) => {
            key_at!(ComposerKeyAtByNameDescendingParams)
        }
        (ComposerSortField::WorkCount, SortDirection::Ascending) => {
            key_at!(ComposerKeyAtByWorksParams)
        }
        (ComposerSortField::WorkCount, SortDirection::Descending) => {
            key_at!(ComposerKeyAtByWorksDescendingParams)
        }
        (ComposerSortField::LinkedReleaseCount, SortDirection::Ascending) => {
            key_at!(ComposerKeyAtByReleasesParams)
        }
        (ComposerSortField::LinkedReleaseCount, SortDirection::Descending) => {
            key_at!(ComposerKeyAtByReleasesDescendingParams)
        }
    })
}

/// The composer list's sort as its statements bind it.
pub(crate) fn composer_sort(sort: &[ComposerSortCriterion]) -> ListSort<ComposerSortField, 3> {
    ListSort::of(sort.iter().map(|criterion| (criterion.field, criterion.direction)))
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
