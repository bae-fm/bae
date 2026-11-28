//! The composer list's window from its top as one compiled read for each
//! term its sort can lead with: [`super::window_reads`] with no key to start
//! from.

use bae_store::list_windows::SortValue;

// [`super::window_reads::ComposerWindowByName`] from the list's top.
sqlive_compose::read!(
    writer ComposerTopByName(
        field_2: Option<String>,
        field_3: Option<String>,
        limit: i64
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
           listing.artist_id AS order_id,
           shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.name_sort_key,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

// [`super::window_reads::ComposerWindowByNameDescending`] from the list's top.
sqlive_compose::read!(
    writer ComposerTopByNameDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        limit: i64
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
           listing.artist_id AS order_id,
           shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.sort_name_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

// [`super::window_reads::ComposerWindowByWorks`] from the list's top.
sqlive_compose::read!(
    writer ComposerTopByWorks(
        field_2: Option<String>,
        field_3: Option<String>,
        limit: i64
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
           listing.artist_id AS order_id,
           shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.work_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

// [`super::window_reads::ComposerWindowByWorksDescending`] from the list's top.
sqlive_compose::read!(
    writer ComposerTopByWorksDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        limit: i64
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
           listing.artist_id AS order_id,
           shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.sort_works_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

// [`super::window_reads::ComposerWindowByReleases`] from the list's top.
sqlive_compose::read!(
    writer ComposerTopByReleases(
        field_2: Option<String>,
        field_3: Option<String>,
        limit: i64
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
           listing.artist_id AS order_id,
           shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.linked_release_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

// [`super::window_reads::ComposerWindowByReleasesDescending`] from the list's top.
sqlive_compose::read!(
    writer ComposerTopByReleasesDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        limit: i64
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
           listing.artist_id AS order_id,
           shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.sort_releases_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
             CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);
