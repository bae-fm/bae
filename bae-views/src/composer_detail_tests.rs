//! A composer's page: its works under every work they are part of, then its
//! unlinked release and track credits, each part opened by a heading, read in
//! windows through the page's index and kept in order as the rows it reads
//! change.

use crate::album_detail::{ComposerDetailItem, ComposerHeading};
use crate::*;
use bae_db::test_fixtures::live_query::first_page;
use bae_db::test_fixtures::{exec_batch, temp_db};

const COMPOSER: &str = "1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01";
const OTHER_COMPOSER: &str = "1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a02";
/// Two works the composer isn't credited on, each heading parts it is.
const ALPHA_PARENT: &str = "2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b01";
const BETA_PARENT: &str = "2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b02";
/// A part of Beta Parent's.
const MOVEMENT_TWO: &str = "2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b03";
/// A part of both parents'.
const MOVEMENT_ONE: &str = "2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b04";
/// A credited work part of none that also heads a credited part.
const GAMMA_WORK: &str = "2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b05";
const PART_OF_GAMMA: &str = "2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b06";
/// A credited work part of none, with no parts.
const DELTA_SOLO: &str = "2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b07";
const CREDIT_ON_FIRST: &str = "6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f01";
const CREDIT_ON_SECOND: &str = "6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f02";
const CREDIT_ON_TRACK_ZERO: &str = "6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f03";
const CREDIT_ON_TRACK_ONE: &str = "6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f04";

/// A composer credited on parts of two works it isn't credited on, one part
/// of both, a work part of none heading a part of its own, and a lone work;
/// and holding release credits on two albums and track credits on two tracks
/// of the first, none linked by a work. Titles mix case, which the page's
/// order ignores.
const PAGE: &str = "
    INSERT INTO artists (id, name, name_key, created_at) VALUES
        ('1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 'Composer Name', 'composer name', '2026-01-01T00:00:00Z'),
        ('1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a02', 'Composer Name Other', 'composer name other', '2026-01-01T00:00:00Z');
    INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
        ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b01', 'Alpha Parent', 'work', 'mb-alpha', '2026-01-01T00:00:00Z'),
        ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b02', 'beta Parent', 'work', 'mb-beta', '2026-01-01T00:00:00Z'),
        ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b03', 'Movement Two', 'part', 'mb-two', '2026-01-01T00:00:00Z'),
        ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b04', 'movement One', 'part', 'mb-one', '2026-01-01T00:00:00Z'),
        ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b05', 'Gamma Work', 'work', 'mb-gamma', '2026-01-01T00:00:00Z'),
        ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b06', 'Part Of Gamma', 'part', 'mb-gamma-part', '2026-01-01T00:00:00Z'),
        ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b07', 'Delta Solo', 'work', 'mb-delta', '2026-01-01T00:00:00Z');
    INSERT INTO work_parts (id, parent_work_id, child_work_id, position, source, created_at) VALUES
        ('7c8d3a0e-6b7d-4c9e-8f0a-2b3c4d5e6a01', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b02', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b03', 1, 'musicbrainz', '2026-01-01T00:00:00Z'),
        ('7c8d3a0e-6b7d-4c9e-8f0a-2b3c4d5e6a02', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b02', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b04', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
        ('7c8d3a0e-6b7d-4c9e-8f0a-2b3c4d5e6a03', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b01', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b04', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
        ('7c8d3a0e-6b7d-4c9e-8f0a-2b3c4d5e6a04', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b05', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b06', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
    INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
        ('8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b01', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b03', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
        ('8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b02', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b04', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
        ('8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b03', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b05', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
        ('8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b04', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b06', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
        ('8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b05', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b07', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'musicbrainz', '2026-01-01T00:00:00Z'),
        ('8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b06', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b07', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a02', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
    INSERT INTO albums (id, title, is_compilation, created_at) VALUES
        ('3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c01', 'Album First', 0, '2026-01-01T00:00:00Z'),
        ('3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c02', 'album Second', 0, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d01', '3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c01', '2026-01-01T00:00:00Z', 0),
        ('4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d02', '3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c02', '2026-01-01T00:00:00Z', 0);
    INSERT INTO release_mediums (id, release_id, position, side_count, created_at) VALUES
        ('9e0f5c2a-8d9f-4e1a-8b2c-4d5e6f7a8c01', '4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d01', 1, 1, '2026-01-01T00:00:00Z');
    INSERT INTO tracks (id, release_id, position, title, medium_id, side, track_number, duration_ms, created_at) VALUES
        ('5a6b1e8c-4f5b-4a7c-8d9e-0f1a2b3c4e01', '4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d01', 0, 'Track Title Zero', '9e0f5c2a-8d9f-4e1a-8b2c-4d5e6f7a8c01', 1, 1, 1000, '2026-01-01T00:00:00Z'),
        ('5a6b1e8c-4f5b-4a7c-8d9e-0f1a2b3c4e02', '4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d01', 1, 'Track Title One', '9e0f5c2a-8d9f-4e1a-8b2c-4d5e6f7a8c01', 1, 2, 1000, '2026-01-01T00:00:00Z');
    INSERT INTO release_artist_roles (id, release_id, artist_id, position, source, source_credit, created_at) VALUES
        ('6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f02', '4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d02', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'discogs', 'Written-By', '2026-01-01T00:00:00Z'),
        ('6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f01', '4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d01', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'discogs', 'Written-By', '2026-01-01T00:00:00Z');
    INSERT INTO track_artist_roles (id, track_id, artist_id, position, source, source_credit, created_at) VALUES
        ('6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f04', '5a6b1e8c-4f5b-4a7c-8d9e-0f1a2b3c4e02', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'musicbrainz', 'composer', '2026-01-01T00:00:00Z'),
        ('6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f03', '5a6b1e8c-4f5b-4a7c-8d9e-0f1a2b3c4e01', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'musicbrainz', 'composer', '2026-01-01T00:00:00Z');
";

/// What a row of a page shows, as the tests read it.
#[derive(Debug, PartialEq)]
enum Shown {
    /// A work, its id, and whether it is listed as a part.
    Work(&'static str, bool),
    /// A credit, by its id.
    Credit(&'static str),
}

use bae_util::test_channels::HANG_GUARD;
use Shown::{Credit, Work};

/// The rows of `artist`'s page, each with whether it opens a heading, by
/// what they show: the work's or the credit's id, matched to the fixture's.
async fn page(db: &Database, artist: &str) -> Vec<(bool, Shown)> {
    let ids = [
        ALPHA_PARENT,
        BETA_PARENT,
        MOVEMENT_TWO,
        MOVEMENT_ONE,
        GAMMA_WORK,
        PART_OF_GAMMA,
        DELTA_SOLO,
        CREDIT_ON_FIRST,
        CREDIT_ON_SECOND,
        CREDIT_ON_TRACK_ZERO,
        CREDIT_ON_TRACK_ONE,
    ];
    let named = |id: &str| *ids.iter().find(|known| **known == id).expect("a fixture id");
    LibraryViews::new(db.clone())
        .get_composer_detail_rows(artist, 0, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            let shown = match row.item {
                DbComposerDetailItem::Work { work, part } => Work(named(&work.work.id), part),
                DbComposerDetailItem::ReleaseCredit(credit) => Credit(named(&credit.role.id)),
                DbComposerDetailItem::TrackCredit(credit) => Credit(named(&credit.role.id)),
            };
            (row.opens_heading, shown)
        })
        .collect()
}

/// The page [`PAGE`] makes for its composer.
fn whole_page() -> Vec<(bool, Shown)> {
    vec![
        (true, Work(ALPHA_PARENT, false)),
        (false, Work(MOVEMENT_ONE, true)),
        (false, Work(BETA_PARENT, false)),
        (false, Work(MOVEMENT_ONE, true)),
        (false, Work(MOVEMENT_TWO, true)),
        (false, Work(DELTA_SOLO, false)),
        (false, Work(GAMMA_WORK, false)),
        (false, Work(PART_OF_GAMMA, true)),
        (true, Credit(CREDIT_ON_FIRST)),
        (false, Credit(CREDIT_ON_SECOND)),
        (true, Credit(CREDIT_ON_TRACK_ZERO)),
        (false, Credit(CREDIT_ON_TRACK_ONE)),
    ]
}

#[tokio::test]
async fn a_composer_page_lists_each_work_under_every_work_it_is_part_of_then_its_credits() {
    let (db, _tmp) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    exec_batch(&db, PAGE).await;

    // Movement One is part of both parents and shows under each; Gamma Work,
    // credited and heading a credited part, shows once.
    assert_eq!(page(&db, COMPOSER).await, whole_page());
    assert_eq!(page(&db, OTHER_COMPOSER).await, vec![(true, Work(DELTA_SOLO, false))]);

    let detail = LibraryViews::new(db.clone())
        .find_composer_detail(COMPOSER)
        .await
        .unwrap()
        .expect("the composer list shows the composer");
    assert_eq!(detail.default_work_id.as_deref(), Some(ALPHA_PARENT));
}

#[tokio::test]
async fn each_part_of_the_page_opens_its_heading() {
    let (db, _tmp) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    exec_batch(&db, PAGE).await;
    let mut live = LibraryViews::new(db.clone()).subscribe_composer_detail_rows(
        COMPOSER,
        [bae_db::LibraryPageWindow { offset: 0, limit: 100 }].into_iter().collect(),
    );
    let event = live.next().await.expect("the query stays open");
    let snapshot = crate::composer_detail::resolve_composer_detail_rows(
        event.result.unwrap(),
        event.revision.0,
        event.cause,
    );
    assert_eq!(snapshot.total_count, 12);
    let headings = snapshot.windows[0]
        .rows
        .iter()
        .enumerate()
        .filter_map(|(at, row)| row.heading.map(|heading| (at, heading)))
        .collect::<Vec<_>>();
    assert_eq!(
        headings,
        vec![
            (0, ComposerHeading::Works),
            (8, ComposerHeading::Credits),
            (10, ComposerHeading::Recordings)
        ]
    );
    let parts = snapshot.windows[0]
        .rows
        .iter()
        .filter(|row| matches!(row.item, ComposerDetailItem::Work { part: true, .. }))
        .count();
    assert_eq!(parts, 4);
}

#[tokio::test]
async fn the_page_keeps_its_order_as_titles_parts_and_credits_change() {
    let (db, _tmp) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    exec_batch(&db, PAGE).await;

    // A retitled heading work moves its group; the rows it heads move with it.
    exec_batch(
        &db,
        "UPDATE works SET title = 'Zeta Parent' WHERE id = '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b01';",
    )
    .await;
    let mut expected = whole_page();
    let alpha = expected.drain(0..2).collect::<Vec<_>>();
    expected.splice(6..6, alpha.into_iter().map(|(_, shown)| (false, shown)));
    expected[0].0 = true;
    assert_eq!(page(&db, COMPOSER).await, expected);

    // A part leaving the only work it is part of heads a group of its own.
    exec_batch(&db, "DELETE FROM work_parts WHERE id = '7c8d3a0e-6b7d-4c9e-8f0a-2b3c4d5e6a01';")
        .await;
    assert_eq!(
        page(&db, COMPOSER).await[..7],
        [
            (true, Work(BETA_PARENT, false)),
            (false, Work(MOVEMENT_ONE, true)),
            (false, Work(DELTA_SOLO, false)),
            (false, Work(GAMMA_WORK, false)),
            (false, Work(PART_OF_GAMMA, true)),
            (false, Work(MOVEMENT_TWO, false)),
            (false, Work(ALPHA_PARENT, false)),
        ]
    );

    // A retitled album and a track moving move the credits; credits a work
    // comes to link leave the page.
    exec_batch(
        &db,
        "UPDATE albums SET title = 'album Third' WHERE id = '3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c01';
         UPDATE tracks SET position = 7 WHERE id = '5a6b1e8c-4f5b-4a7c-8d9e-0f1a2b3c4e01';",
    )
    .await;
    assert_eq!(
        page(&db, COMPOSER).await[8..],
        [
            (true, Credit(CREDIT_ON_SECOND)),
            (false, Credit(CREDIT_ON_FIRST)),
            (true, Credit(CREDIT_ON_TRACK_ONE)),
            (false, Credit(CREDIT_ON_TRACK_ZERO)),
        ]
    );
    exec_batch(
        &db,
        "INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES
            ('0f1a6d3b-9e0a-4f2b-9c3d-5e6f7a8b9d01', '5a6b1e8c-4f5b-4a7c-8d9e-0f1a2b3c4e02', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b07', 0, 'musicbrainz', '2026-01-01T00:00:00Z');",
    )
    .await;
    assert_eq!(
        page(&db, COMPOSER).await[8..],
        [(true, Credit(CREDIT_ON_SECOND)), (true, Credit(CREDIT_ON_TRACK_ZERO))]
    );

    // A composer no longer credited on a part leaves it, and a parent heading
    // only that part goes with it.
    exec_batch(
        &db,
        "DELETE FROM work_artists WHERE id IN ('8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b02', '8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b04');",
    )
    .await;
    assert_eq!(
        page(&db, COMPOSER).await,
        vec![
            (true, Work(DELTA_SOLO, false)),
            (false, Work(GAMMA_WORK, false)),
            (false, Work(MOVEMENT_TWO, false)),
            (true, Credit(CREDIT_ON_SECOND)),
            (true, Credit(CREDIT_ON_TRACK_ZERO)),
        ]
    );
}

/// The rows `conn` holds of the composer's page, in the page's order: each
/// one's part, group, place and what it shows.
fn held_rows(conn: &rusqlite::Connection) -> Vec<(i64, String, Option<i64>, String)> {
    let order = crate::composer_detail::composer_detail_order("listing").clause();
    conn.prepare(&format!(
        "SELECT section, group_id, place, item_id FROM composer_detail_row listing
         WHERE artist_id = ?1 ORDER BY {order}"
    ))
    .unwrap()
    .query_map([COMPOSER], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

#[test]
fn rows_whose_titles_arrive_late_wait_for_them() {
    // Rows from another device can arrive before the rows they name: here a
    // part's parent, and a credit's release and album.
    let conn = bae_db::schema::bare_library_connection(bae_db::test_fixtures::TEST_SCHEMA);
    conn.execute_batch(
        "PRAGMA foreign_keys = OFF;
         INSERT INTO artists (id, name, name_key, created_at) VALUES
            ('1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 'Composer Name', 'composer name', '2026-01-01T00:00:00Z');
         INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
            ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b03', 'Movement Two', 'part', 'mb-two', '2026-01-01T00:00:00Z');
         INSERT INTO work_parts (id, parent_work_id, child_work_id, position, source, created_at) VALUES
            ('7c8d3a0e-6b7d-4c9e-8f0a-2b3c4d5e6a01', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b02', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b03', 1, 'musicbrainz', '2026-01-01T00:00:00Z');
         INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
            ('8d9e4b1f-7c8e-4d0f-9a1b-3c4d5e6f7b01', '2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b03', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
         INSERT INTO release_artist_roles (id, release_id, artist_id, position, source, source_credit, created_at) VALUES
            ('6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f02', '4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d02', '1c2f7a4e-0b1d-4c3e-8f5a-6b7c8d9e0a01', 0, 'discogs', 'Written-By', '2026-01-01T00:00:00Z');",
    )
    .unwrap();
    // The part heads its own group while its parent is missing; the credit
    // waits for its release and album.
    assert_eq!(
        held_rows(&conn),
        vec![(0, MOVEMENT_TWO.to_string(), Some(0), MOVEMENT_TWO.to_string())]
    );

    conn.execute_batch(
        "INSERT INTO releases (id, album_id, created_at, album_order) VALUES
            ('4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d02', '3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c02', '2026-01-01T00:00:00Z', 0);
         INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
            ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b02', 'beta Parent', 'work', 'mb-beta', '2026-01-01T00:00:00Z');",
    )
    .unwrap();
    assert_eq!(
        held_rows(&conn),
        vec![
            (0, BETA_PARENT.to_string(), Some(0), BETA_PARENT.to_string()),
            (0, BETA_PARENT.to_string(), Some(1), MOVEMENT_TWO.to_string()),
        ]
    );

    conn.execute_batch(
        "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
            ('3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c02', 'album Second', 0, '2026-01-01T00:00:00Z');",
    )
    .unwrap();
    assert_eq!(
        held_rows(&conn)[2..],
        [(
            1,
            "4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d02".to_string(),
            None,
            CREDIT_ON_SECOND.to_string()
        )]
    );
}

#[tokio::test]
async fn the_page_rows_follow_writes_in_their_windows() {
    let (db, _tmp) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    exec_batch(&db, PAGE).await;
    let mut live = LibraryViews::new(db.clone()).subscribe_composer_detail_rows(
        COMPOSER,
        [bae_db::LibraryPageWindow { offset: 8, limit: 4 }].into_iter().collect(),
    );
    let first = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(first.total_count, 12);
    assert_eq!(first.windows[0].rows.len(), 4);

    exec_batch(
        &db,
        "DELETE FROM release_artist_roles WHERE id = '6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f02';",
    )
    .await;
    let after = tokio::time::timeout(HANG_GUARD, live.next())
        .await
        .expect("a credit leaving the page wakes its rows")
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(after.total_count, 11);
    assert_eq!(after.windows[0].rows.len(), 3);
    assert!(after.windows[0].rows[0].opens_heading);

    let other = LibraryViews::new(db.clone())
        .subscribe_composer_detail_rows(OTHER_COMPOSER, first_page())
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert_eq!(other.total_count, 1);
}

/// A window beside a known row of a page is read through the page's index:
/// no step scans the page's rows or sorts them. What a row's work shows
/// (`work_summary_columns`) sorts that work's own tracks.
#[tokio::test]
async fn every_seek_of_the_page_reads_its_index() {
    let (db, _tmp) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    exec_batch(&db, PAGE).await;
    let order = crate::composer_detail::composer_detail_order("listing");
    for at in 0..12 {
        let key = crate::tests::key_at(
            &db,
            &order,
            |sql, order, seek, limit, offset| {
                crate::composer_detail::composer_detail_rows_on(
                    sql, COMPOSER, order, seek, limit, offset,
                )
            },
            at,
        )
        .await;
        for seek in order.seeks_beside(&key) {
            let statement = crate::composer_detail::composer_detail_rows_query(&order, &seek);
            let steps = db
                .read(move |sql| {
                    seek.bound(50, 0, |params| {
                        let mut params = params.to_vec();
                        params.push((":artist_id", &COMPOSER as &dyn rusqlite::ToSql));
                        Ok(sql.query(
                            &format!("EXPLAIN QUERY PLAN {statement}"),
                            params.as_slice(),
                            |row| Ok((row.get::<_, i64>(1)?, row.get::<_, String>(3)?)),
                        )?)
                    })
                })
                .await
                .unwrap();
            assert!(
                steps.iter().any(|(_, step)| step.starts_with(
                    "SEARCH listing USING COVERING INDEX idx_composer_detail_row_order"
                )),
                "row {at}: {steps:?}"
            );
            assert!(
                !steps.iter().any(|(parent, step)| step.starts_with("SCAN listing")
                    || (*parent == 0 && step.contains("TEMP B-TREE"))),
                "row {at}: {steps:?}"
            );
        }
    }
}

/// A refresh reads the works or credit rows of one artist's work or one
/// credit, never every row the views hold: no step scans a table, and each
/// read of the artist's credited works names the work.
#[tokio::test]
async fn the_page_refreshes_read_one_work_or_one_credit() {
    let (db, _tmp) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    exec_batch(&db, PAGE).await;
    for statement in [
        "SELECT * FROM composer_detail_works WHERE artist_id = 'a' AND item_id = 'w'",
        "SELECT * FROM composer_detail_credits WHERE item_id = 'c'",
    ] {
        let steps = db
            .read(move |sql| {
                Ok(sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), [], |row| {
                    row.get::<_, String>(3)
                })?)
            })
            .await
            .unwrap();
        assert!(
            !steps.iter().any(|step| (step.starts_with("SCAN ")
                && step != "SCAN composer_detail_works"
                && step != "SCAN composer_detail_credits")
                || (step.starts_with("SEARCH credited") && !step.contains("work_id=?"))),
            "{statement}: {steps:?}"
        );
    }
}
