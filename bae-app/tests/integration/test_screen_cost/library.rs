//! The library screens' plain reads: the album grid flat and grouped by
//! artist with its sections and "with ..." line, the artist list and the
//! composer list. Each is the plain query over the records' own tables, with
//! no listing table, no copied sort key and no index made for it, timed at
//! each seed size; `perf.py` reads the numbers (`screen-library`).

use super::seed::{sizes, SEEDS};
use bae_store::test_fixtures::screen_cost::{list_costs, plain_cost, ListShape};
use bae_store::Store;

/// The language the device orders names in, as the listing triggers read it.
const LANGUAGE: &str = "(SELECT language FROM name_sort)";

/// An album the grid shows: one with a release.
const SHOWN: &str = "EXISTS (SELECT 1 FROM releases r WHERE r.album_id = a.id)";

/// An album's credited names, in credit order, joined by the separator the
/// sort keys use, as the album listing trigger builds them.
const CREDIT_NAMES: &str = "(SELECT GROUP_CONCAT(COALESCE(credited.sort_name, credited.name), char(31) ORDER BY credit.position) \
    FROM album_artists credit \
    JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id \
    JOIN artists credited ON credited.id = shown.shown_artist_id \
    WHERE credit.album_id = a.id)";

/// The album grid, flat: each sort's window by jump and by cursor, and the
/// count.
async fn album_grid_flat(db: &Store, size: u64) {
    let source = format!("FROM albums a WHERE {SHOWN}");
    let title_key = format!("bae_name_key(a.title, {LANGUAGE})");
    let artist_key = format!("bae_name_list_key({CREDIT_NAMES}, {LANGUAGE})");
    for (sort, key, descending) in [
        ("title", title_key.as_str(), false),
        ("title-desc", title_key.as_str(), true),
        ("year", "COALESCE(a.year, 100000)", false),
        ("added", "a.created_at", false),
        ("artist", artist_key.as_str(), false),
    ] {
        list_costs(
            db,
            size,
            &ListShape {
                metric: &format!("album-grid/flat/{sort}"),
                select: "a.id, a.title, a.year, a.created_at, a.is_compilation",
                source: &source,
                key,
                tie: "a.id",
                descending,
            },
        )
        .await;
    }
}

/// The albums under each artist their credits show, once per artist, ordered
/// by the artist's sort name and then the album's title: the grouped grid.
const GROUPED_ALBUMS: &str = "\
    FROM (SELECT DISTINCT shown.shown_artist_id AS artist_id, credit.album_id \
          FROM album_artists credit \
          JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id) link \
    JOIN artists g ON g.id = link.artist_id \
    JOIN albums a ON a.id = link.album_id \
    WHERE EXISTS (SELECT 1 FROM releases r WHERE r.album_id = a.id)";

/// The grouped grid's order: the artist's key, then the album's title key.
fn grouped_key() -> String {
    format!(
        "hex(bae_name_key(COALESCE(g.sort_name, g.name), {LANGUAGE})) \
         || hex(bae_name_key(a.title, {LANGUAGE}))"
    )
}

/// The album grid grouped by artist: its window, the "with ..." line each
/// row shows (the album's other credited artists), and its artist sections.
async fn album_grid_grouped(db: &Store, size: u64) {
    let key = grouped_key();
    list_costs(
        db,
        size,
        &ListShape {
            metric: "album-grid/grouped/title",
            select: "g.id, a.id, a.title, a.year",
            source: GROUPED_ALBUMS,
            key: &key,
            tie: "g.id || a.id",
            descending: false,
        },
    )
    .await;
    let with_line = "(SELECT GROUP_CONCAT(COALESCE(other.name, ''), ', ') \
        FROM album_artists credit \
        JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id \
        JOIN artists other ON other.id = shown.shown_artist_id \
        WHERE credit.album_id = a.id AND other.id <> g.id)";
    list_costs(
        db,
        size,
        &ListShape {
            metric: "album-grid/grouped-with-line/title",
            select: &format!("g.id, a.id, a.title, a.year, {with_line}"),
            source: GROUPED_ALBUMS,
            key: &key,
            tie: "g.id || a.id",
            descending: false,
        },
    )
    .await;
    // The artist sections: each artist with the albums under it and where its
    // first album sits in the grid.
    let sections = format!(
        "SELECT g.id, g.name, COUNT(*) AS albums, \
                SUM(COUNT(*)) OVER (ORDER BY hex(bae_name_key(COALESCE(g.sort_name, g.name), {LANGUAGE})), g.id) - COUNT(*) AS first_row \
         {GROUPED_ALBUMS} \
         GROUP BY g.id \
         ORDER BY hex(bae_name_key(COALESCE(g.sort_name, g.name), {LANGUAGE})), g.id"
    );
    let sections_at = bae_store::test_fixtures::screen_cost::size_name(size);
    let rows = plain_cost(
        db,
        &format!("album-grid/sections@{sections_at}"),
        Some(50.0),
        &sections,
        vec![],
    )
    .await;
    assert!(rows > 0, "the grouped grid has sections");
}

/// The artist list: the artists their albums show, by name and by how many
/// albums they hold.
async fn artists(db: &Store, size: u64) {
    let albums = "(SELECT COUNT(DISTINCT credit.album_id) FROM album_artists credit \
        JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id \
        WHERE shown.shown_artist_id = g.id \
          AND EXISTS (SELECT 1 FROM releases r WHERE r.album_id = credit.album_id))";
    let source = "FROM artists g WHERE EXISTS (SELECT 1 FROM album_artists credit \
        JOIN artist_shown_as shown ON shown.artist_id = credit.artist_id \
        WHERE shown.shown_artist_id = g.id)";
    let name_key = format!("bae_name_key(COALESCE(g.sort_name, g.name), {LANGUAGE})");
    for (sort, key, descending) in [("name", name_key.as_str(), false), ("albums", albums, true)] {
        list_costs(
            db,
            size,
            &ListShape {
                metric: &format!("artists/{sort}"),
                select: &format!("g.id, g.name, {albums} AS albums"),
                source,
                key,
                tie: "g.id",
                descending,
            },
        )
        .await;
    }
}

/// What each composer holds: works, releases performing works they wrote, and
/// credits no work they wrote links, counted from the views over the
/// records' tables.
const COMPOSER_COUNTS: &str = "\
    FROM (SELECT artist_id, \
                 COUNT(*) FILTER (WHERE kind = 'work') AS work_count, \
                 COUNT(*) FILTER (WHERE kind = 'release') AS release_count, \
                 COUNT(*) FILTER (WHERE kind = 'unlinked' AND writing) AS unlinked_count \
          FROM (SELECT artist_id, 'work' AS kind, NULL AS writing FROM work_composers \
                UNION ALL SELECT artist_id, 'release', NULL FROM release_work_composers \
                UNION ALL SELECT artist_id, 'unlinked', writing FROM unlinked_composer_credits) \
          GROUP BY artist_id) counts \
    JOIN artists g ON g.id = counts.artist_id \
    WHERE counts.work_count + counts.release_count + counts.unlinked_count > 0";

/// The composer list in each of its sorts.
async fn composers(db: &Store, size: u64) {
    let name_key = format!("bae_name_key(g.name, {LANGUAGE})");
    for (sort, key, descending) in [
        ("name", name_key.as_str(), false),
        ("works", "counts.work_count", true),
        ("releases", "counts.release_count", true),
    ] {
        list_costs(
            db,
            size,
            &ListShape {
                metric: &format!("composers/{sort}"),
                select:
                    "g.id, g.name, counts.work_count, counts.release_count, counts.unlinked_count",
                source: COMPOSER_COUNTS,
                key,
                tie: "g.id",
                descending,
            },
        )
        .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn library_plain_reads() {
    for size in sizes() {
        let (db, _copy) = SEEDS.seeded_copy(size).await;
        album_grid_flat(&db, size).await;
        album_grid_grouped(&db, size).await;
        artists(&db, size).await;
        composers(&db, size).await;
    }
}
