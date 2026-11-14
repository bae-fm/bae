//! A composer's page: its works under every work they are part of, then its
//! unlinked release and track credits, each part opened by a heading, read in
//! windows through the page's index and kept in order as the rows it reads
//! change.

use crate::album_detail::{ComposerDetailItem, ComposerHeading};
use crate::*;
use bae_catalogs::catalog::Catalog;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::live_query::top_window;
use bae_store::test_fixtures::temp_db;
use bae_types::credit_role::{CreditRoleKind, CreditedAs};

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
/// order weighs after the letters.
async fn write_page(db: &Store) {
    let written_by = (Catalog::Discogs, CreditedAs::of(CreditRoleKind::Writer));
    let composer = (Catalog::MusicBrainz, CreditedAs::of(CreditRoleKind::Composer));
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
        .release_role(CREDIT_ON_SECOND, RELEASE_SECOND, COMPOSER, written_by.clone())
        .release_role(CREDIT_ON_FIRST, RELEASE_FIRST, COMPOSER, written_by.clone())
        .track_role(CREDIT_ON_TRACK_ONE, TRACK_ONE, COMPOSER, composer.clone())
        .track_role(CREDIT_ON_TRACK_ZERO, TRACK_ZERO, COMPOSER, composer.clone())
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
    let views = LibraryViews::new(db.clone());
    let mut live = views.subscribe_composer_detail_window(COMPOSER, top_window());
    let window = crate::composer_detail::resolve_composer_detail_window(
        live.next().await.expect("the query stays open").result.unwrap(),
    );
    let crate::ListRows::ComposerPage(rows) = window.rows else {
        panic!("a composer's page window holds its page's rows");
    };
    let count = views
        .subscribe_composer_detail_count(COMPOSER)
        .next()
        .await
        .expect("the query stays open")
        .unwrap();
    assert_eq!(count, 12);
    let headings = rows
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
    let parts = rows
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
        let written_by = (Catalog::Discogs, CreditedAs::of(CreditRoleKind::Writer));
        let composer = (Catalog::MusicBrainz, CreditedAs::of(CreditRoleKind::Composer));
        let links = RecordsFixture::new()
            .release_role(CREDIT_ON_FIRST, RELEASE_FIRST, COMPOSER, written_by.clone())
            .track_role(CREDIT_ON_TRACK_ONE, TRACK_ONE, COMPOSER, composer.clone())
            .track_role(CREDIT_ON_TRACK_ZERO, TRACK_ZERO, COMPOSER, composer.clone())
            .performs(TRACK_ONE, DELTA_SOLO)
            .rows();
        records.apply_release_links_for_test(RELEASE_FIRST, links).await.unwrap();
        assert_eq!(
            page(db, COMPOSER).await[8..],
            [(true, Credit(CREDIT_ON_SECOND)), (true, Credit(CREDIT_ON_TRACK_ZERO))]
        );
    });
}

#[tokio::test]
async fn the_page_rows_follow_writes_in_their_windows() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    write_page(&db).await;
    let views = LibraryViews::new(db.clone());
    let from = views.composer_detail_key_at(COMPOSER, 8).await.unwrap().expect("a row at 8");
    let mut live = views.subscribe_composer_detail_window(
        COMPOSER,
        ListWindow::Forward { from: Some(from), until: None, limit: 50 },
    );
    let mut count = views.subscribe_composer_detail_count(COMPOSER);
    let first = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(count.next().await.expect("the query stays open").unwrap(), 12);
    assert_eq!(first.rows.len(), 4);

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
    assert_eq!(count.next().await.expect("the query stays open").unwrap(), 11);
    assert_eq!(after.rows.len(), 3);
    assert!(after.rows[0].opens_heading);

    let other = views
        .subscribe_composer_detail_count(OTHER_COMPOSER)
        .next()
        .await
        .expect("the query stays open")
        .unwrap();
    assert_eq!(other, 1);
}
