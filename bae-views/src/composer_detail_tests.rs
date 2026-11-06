//! A composer's page: its works under every work they are part of, then its
//! unlinked release and track credits, each part opened by a heading, read in
//! windows through the page's index and kept in order as the rows it reads
//! change.

use crate::album_detail::{ComposerDetailItem, ComposerHeading};
use crate::*;
use bae_catalogs::catalog::Catalog;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::live_query::first_page;
use bae_store::test_fixtures::temp_db;

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

const RELEASE_FIRST: &str = "4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d01";
const RELEASE_SECOND: &str = "4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d02";
const ALBUM_FIRST: &str = "3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c01";
const ALBUM_SECOND: &str = "3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c02";
const TRACK_ZERO: &str = "5a6b1e8c-4f5b-4a7c-8d9e-0f1a2b3c4e01";
const TRACK_ONE: &str = "5a6b1e8c-4f5b-4a7c-8d9e-0f1a2b3c4e02";

/// A composer credited on parts of two works it isn't credited on, one part
/// of both, a work part of none heading a part of its own, and a lone work;
/// and holding release credits on two albums and track credits on two tracks
/// of the first, none linked by a work. Titles mix case, which the page's
/// order ignores.
async fn write_page(db: &Store) {
    let written_by = (Catalog::Discogs, "Written-By");
    let composer = (Catalog::MusicBrainz, "composer");
    RecordsFixture::new()
        .artist(COMPOSER, "Composer Name")
        .artist(OTHER_COMPOSER, "Composer Name Other")
        .work(ALPHA_PARENT, "Alpha Parent")
        .work(BETA_PARENT, "beta Parent")
        .work_of_type(MOVEMENT_TWO, "Movement Two", "part")
        .work_of_type(MOVEMENT_ONE, "movement One", "part")
        .work(GAMMA_WORK, "Gamma Work")
        .work_of_type(PART_OF_GAMMA, "Part Of Gamma", "part")
        .work(DELTA_SOLO, "Delta Solo")
        .part(BETA_PARENT, MOVEMENT_ONE)
        .part(BETA_PARENT, MOVEMENT_TWO)
        .part(ALPHA_PARENT, MOVEMENT_ONE)
        .part(GAMMA_WORK, PART_OF_GAMMA)
        .composer(MOVEMENT_TWO, COMPOSER)
        .composer(MOVEMENT_ONE, COMPOSER)
        .composer(GAMMA_WORK, COMPOSER)
        .composer(PART_OF_GAMMA, COMPOSER)
        .composer(DELTA_SOLO, COMPOSER)
        .composer(DELTA_SOLO, OTHER_COMPOSER)
        .album(ALBUM_FIRST, "Album First", None, &[])
        .album(ALBUM_SECOND, "album Second", None, &[])
        .release(ALBUM_FIRST, RELEASE_FIRST)
        .release(ALBUM_SECOND, RELEASE_SECOND)
        .tracks(RELEASE_FIRST, &[(TRACK_ZERO, "Track Title Zero"), (TRACK_ONE, "Track Title One")])
        .release_role(CREDIT_ON_SECOND, RELEASE_SECOND, COMPOSER, written_by)
        .release_role(CREDIT_ON_FIRST, RELEASE_FIRST, COMPOSER, written_by)
        .track_role(CREDIT_ON_TRACK_ONE, TRACK_ONE, COMPOSER, composer)
        .track_role(CREDIT_ON_TRACK_ZERO, TRACK_ZERO, COMPOSER, composer)
        .write(&test_records(db))
        .await;
}

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
async fn page(db: &Store, artist: &str) -> Vec<(bool, Shown)> {
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

/// The page [`write_page`] makes for its composer.
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
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_page(&db).await;

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
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_page(&db).await;
    let mut live = LibraryViews::new(db.clone()).subscribe_composer_detail_rows(
        COMPOSER,
        [bae_store::LibraryPageWindow { offset: 0, limit: 100 }].into_iter().collect(),
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

/// A heading work another device names otherwise moves its group, the rows
/// it heads with it; a retitled album moves its release's credits; credits a
/// work an edit comes to link leave the page.
#[test]
fn the_page_keeps_its_order_as_titles_and_credits_change() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let db = devices.a();
        let records = test_records(db);
        devices.pause().await;
        write_page(db).await;
        RecordsFixture::new()
            .work(ALPHA_PARENT, "Zeta Parent")
            .write(&test_records(devices.b()))
            .await;
        devices.resume().await;
        devices
            .settle("the later title", async |_, db| {
                db.read(|sql| {
                    Ok(sql.query_row(
                        "SELECT title FROM works WHERE id = ?1",
                        [ALPHA_PARENT],
                        |row| row.get::<_, String>(0),
                    )?)
                })
                .await
                .unwrap()
                    == "Zeta Parent"
            })
            .await;
        let mut expected = whole_page();
        let alpha = expected.drain(0..2).collect::<Vec<_>>();
        expected.splice(6..6, alpha.into_iter().map(|(_, shown)| (false, shown)));
        expected[0].0 = true;
        assert_eq!(page(db, COMPOSER).await, expected);

        records.rename_album_for_test(ALBUM_FIRST, "album Third").await.unwrap();
        assert_eq!(
            page(db, COMPOSER).await[8..],
            [
                (true, Credit(CREDIT_ON_SECOND)),
                (false, Credit(CREDIT_ON_FIRST)),
                (true, Credit(CREDIT_ON_TRACK_ZERO)),
                (false, Credit(CREDIT_ON_TRACK_ONE)),
            ]
        );

        // An edit applying a catalog that names the same credits and links
        // track one to the composer's lone work.
        let written_by = (Catalog::Discogs, "Written-By");
        let composer = (Catalog::MusicBrainz, "composer");
        let links = RecordsFixture::new()
            .release_role(CREDIT_ON_FIRST, RELEASE_FIRST, COMPOSER, written_by)
            .track_role(CREDIT_ON_TRACK_ONE, TRACK_ONE, COMPOSER, composer)
            .track_role(CREDIT_ON_TRACK_ZERO, TRACK_ZERO, COMPOSER, composer)
            .performs(TRACK_ONE, DELTA_SOLO)
            .rows();
        records.apply_release_links_for_test(RELEASE_FIRST, links).await.unwrap();
        assert_eq!(
            page(db, COMPOSER).await[8..],
            [(true, Credit(CREDIT_ON_SECOND)), (true, Credit(CREDIT_ON_TRACK_ZERO))]
        );
    });
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
    // Within one apply of another device's rows, coven defers the foreign
    // keys to the commit and may put a row before the rows it names when a
    // unique claim orders it so: here a part before its parent, and a credit
    // before its release and album. The keys hold once the write commits.
    let mut conn = crate::tests::bare_library();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let apply = conn.transaction().unwrap();
    apply.execute_batch(
        "PRAGMA defer_foreign_keys = ON;
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
        held_rows(&apply),
        vec![(0, MOVEMENT_TWO.to_string(), Some(0), MOVEMENT_TWO.to_string())]
    );

    apply.execute_batch(
        "INSERT INTO releases (id, album_id, created_at, album_order) VALUES
            ('4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d02', '3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c02', '2026-01-01T00:00:00Z', 0);
         INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
            ('2d3e8b5f-1c2e-4d4f-9a6b-7c8d9e0f1b02', 'beta Parent', 'work', 'mb-beta', '2026-01-01T00:00:00Z');",
    )
    .unwrap();
    assert_eq!(
        held_rows(&apply),
        vec![
            (0, BETA_PARENT.to_string(), Some(0), BETA_PARENT.to_string()),
            (0, BETA_PARENT.to_string(), Some(1), MOVEMENT_TWO.to_string()),
        ]
    );

    apply
        .execute_batch(
            "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
            ('3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c02', 'album Second', 0, '2026-01-01T00:00:00Z');",
        )
        .unwrap();
    assert_eq!(
        held_rows(&apply)[2..],
        [(
            1,
            "4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d02".to_string(),
            None,
            CREDIT_ON_SECOND.to_string()
        )]
    );
    apply.commit().unwrap();
}

#[tokio::test]
async fn the_page_rows_follow_writes_in_their_windows() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_page(&db).await;
    let mut live = LibraryViews::new(db.clone()).subscribe_composer_detail_rows(
        COMPOSER,
        [bae_store::LibraryPageWindow { offset: 8, limit: 4 }].into_iter().collect(),
    );
    let first = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(first.total_count, 12);
    assert_eq!(first.windows[0].rows.len(), 4);

    // An edit applying a catalog that credits nobody on the second release.
    test_records(&db)
        .apply_release_links_for_test(RELEASE_SECOND, RecordsFixture::new().rows())
        .await
        .unwrap();
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
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_page(&db).await;
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
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_page(&db).await;
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

/// Two albums sharing a title keep their releases' credits apart: releases go
/// by album title, then by album, then by their place among the album's, so a
/// later release of the first album comes before the other album's first.
#[tokio::test]
async fn credits_on_albums_sharing_a_title_stay_by_album() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_page(&db).await;
    let written_by = (Catalog::Discogs, "Written-By");
    RecordsFixture::new()
        .album("3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c03", "album first", None, &[])
        .release("3e4f9c6a-2d3f-4e5a-8b7c-8d9e0f1a2c03", "4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d03")
        .release(ALBUM_FIRST, "4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d04")
        .release_role(
            "6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f05",
            "4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d03",
            COMPOSER,
            written_by,
        )
        .release_role(
            "6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f06",
            "4f5a0d7b-3e4a-4f6b-9c8d-9e0f1a2b3d04",
            COMPOSER,
            written_by,
        )
        .write(&test_records(&db))
        .await;

    let credits = LibraryViews::new(db.clone())
        .get_composer_detail_rows(COMPOSER, 0, 100)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|row| match row.item {
            DbComposerDetailItem::ReleaseCredit(credit) => Some(credit.role.id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        credits,
        [
            CREDIT_ON_FIRST,
            "6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f06",
            "6b7c2f9d-5a6c-4b8d-9e0f-1a2b3c4d5f05",
            CREDIT_ON_SECOND,
        ]
    );
}
