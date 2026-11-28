//! Reads and a write of the import list that this domain's tests and the
//! import's tests check it with.

// How many placement rows each tab, item and filter entry holds, by a count
// over the placement.
#[cfg(test)]
coven::read!(
    writer PlacedCounts { held: i64 },
    r#"
    SELECT placed.tab, placed.item, placed.pending_entry, COUNT(*) AS held
    FROM import_list_placement AS placed
    GROUP BY placed.tab, placed.item, placed.pending_entry
    ORDER BY placed.tab, placed.item, placed.pending_entry
"#
);

// The counts kept beside the placement, in order.
#[cfg(test)]
coven::read!(
    writer StoredCounts,
    r#"
    SELECT kept.tab, kept.item, kept.pending_entry, kept.count
    FROM import_list_count AS kept
    ORDER BY kept.tab, kept.item, kept.pending_entry
"#
);

// Each group's members in each filter entry, by a count over the placement,
// in the order of `StoredGroupCounts`.
#[cfg(test)]
coven::read!(
    writer PlacedGroupCounts {
        watched: String,
        group_text: String,
        held: i64
    },
    r#"
    SELECT coven_path_text(placed.watched_folder_path) AS watched,
        coven_path_text(placed.group_folder) AS group_text, placed.pending_entry,
        COUNT(*) AS held
    FROM import_list_placement AS placed
    WHERE placed.item <> 'group' AND placed.group_folder IS NOT NULL
    GROUP BY placed.watched_folder_path, placed.group_folder, placed.pending_entry
    ORDER BY 1, 2, 3
"#
);

// The group counts kept beside the placement, in order.
#[cfg(test)]
coven::read!(
    writer StoredGroupCounts { watched: String, group_text: String },
    r#"
    SELECT coven_path_text(kept.watched_folder_path) AS watched,
        coven_path_text(kept.group_folder) AS group_text, kept.pending_entry, kept.count
    FROM import_list_group_count AS kept
    ORDER BY 1, 2, 3
"#
);

// Every row of the search index, by its placement row's number.
#[cfg(test)]
coven::read!(
    writer IndexedText { title: Option<String>, artists: Option<String> },
    r#"
    SELECT indexed.rowid AS entry, indexed.title, indexed.artists
    FROM import_list_search AS indexed
    ORDER BY indexed.rowid
"#
);

// The folded shown text of every placement row that is not a group's header,
// by its number.
#[cfg(test)]
coven::read!(
    writer PlacedText,
    r#"
    SELECT placed.entry, placed.filter_title, placed.filter_artists
    FROM import_list_placement AS placed
    WHERE placed.item <> 'group'
    ORDER BY placed.entry
"#
);

// The folder of each group header that says nothing of whether its folder
// reads as one release.
#[cfg(test)]
coven::read!(
    writer UnjudgedHeaders { folder: String },
    r#"
    SELECT coven_path_text(header.key) AS folder
    FROM import_list_placement AS header
    WHERE header.item = 'group' AND header.combinable IS NULL
"#
);

// The key of each placement row that does not sort under its group's header.
#[cfg(test)]
coven::read!(
    writer MisplacedRows { folder: String },
    r#"
    SELECT coven_path_text(placed.key) AS folder
    FROM import_list_placement AS placed
    WHERE placed.unit_entry IS NOT COALESCE((
        SELECT header.entry FROM import_list_placement AS header
        WHERE header.watched_folder_path = placed.watched_folder_path
          AND header.item = 'group' AND header.key = placed.group_folder), placed.entry)
"#
);

// Every placement row as it stands.
#[cfg(test)]
coven::read!(
    writer EveryPlacement {
        watched: String,
        key_text: String,
        unit_date: Option<i64>,
        display_path: Option<String>,
    },
    r#"
    SELECT coven_path_text(placed.watched_folder_path) AS watched, placed.item,
        coven_path_text(placed.key) AS key_text, placed.root_position, placed.tab,
        placed.pending_state, placed.needs_you_reason, placed.group_name, placed.own_date,
        CASE placed.item WHEN 'group' THEN placed.group_date ELSE placed.own_date END
            AS unit_date,
        iif(placed.folder IS NULL, NULL, placed.path_text) AS display_path,
        placed.shown_title, placed.shown_artists
    FROM import_list_placement AS placed
"#
);

// The tab, title and artists of every candidate's placement row.
#[cfg(test)]
coven::read!(
    writer CandidatePlacements,
    r#"
    SELECT placed.tab, placed.shown_title, placed.shown_artists
    FROM import_list_placement AS placed
    WHERE placed.item = 'candidate'
"#
);

// Check the search index against its own records, failing when they differ.
#[cfg(test)]
coven::write!(
    CheckSearchIndex,
    r#"INSERT INTO import_list_search (import_list_search, rank) VALUES ('integrity-check', 1)"#
);

// Where every row of the list stands and what its commands are decided from,
// by its number.
#[cfg(test)]
coven::read!(
    writer EveryPlacementStanding { key: Vec<u8> },
    r#"
    SELECT placed.entry, placed.item, placed.key, placed.tab, placed.pending_state,
        placed.pending_entry, placed.needs_you_reason, placed.error_detail, placed.own_date,
        placed.group_date, placed.upload_position, placed.shown_title, placed.shown_artists,
        placed.actionable, placed.import_failed, placed.draft_valid, placed.lookup,
        placed.separable
    FROM import_list_placement AS placed
    ORDER BY placed.entry
"#
);

/// Where every row of the list in `db` stands, by its number.
#[cfg(test)]
pub(crate) async fn every_placement_standing(
    db: &bae_store::Store,
) -> Vec<EveryPlacementStandingRow> {
    db.read_rows(EveryPlacementStandingParams {}).await.unwrap()
}

/// The counts kept beside the placement in `db`, in order.
#[cfg(test)]
pub(crate) async fn stored_counts(db: &bae_store::Store) -> Vec<StoredCountsRow> {
    db.read_rows(StoredCountsParams {}).await.unwrap()
}
