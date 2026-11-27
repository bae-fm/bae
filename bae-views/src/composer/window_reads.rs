//! The composer list's window as one compiled read for each term its sort can
//! lead with (`composer::list`).

use bae_store::list_windows::SortValue;

// The composer list's rows from a key on, at most `limit` of them, for each
// first term. Each term is returned ahead of the row, so coven reruns a full
// window only for a write at or before its last row.
sqlive_compose::read!(
    writer ComposerWindowByName(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String,
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
    WHERE (listing.name_sort_key,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
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

sqlive_compose::read!(
    writer ComposerWindowByNameDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String,
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
    WHERE (listing.sort_name_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
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

sqlive_compose::read!(
    writer ComposerWindowByWorks(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String,
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
    WHERE (listing.work_count,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
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

sqlive_compose::read!(
    writer ComposerWindowByWorksDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String,
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
    WHERE (listing.sort_works_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
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

sqlive_compose::read!(
    writer ComposerWindowByReleases(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String,
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
    WHERE (listing.linked_release_count,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
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

sqlive_compose::read!(
    writer ComposerWindowByReleasesDescending(
        field_2: Option<String>,
        field_3: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_4: SortValue,
        key_5: SortValue,
        key_name: SortValue,
        key_id: String,
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
    WHERE (listing.sort_releases_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'works_descending' THEN -listing.work_count WHEN 'releases' THEN listing.linked_release_count WHEN 'releases_descending' THEN -listing.linked_release_count ELSE 0 END,
           CASE :field_3 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_4, :key_5, :key_name, :key_id)
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
