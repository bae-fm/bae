//! The release detail: its track artists, its files and its index among its
//! album's releases, in order.

use crate::*;
use bae_records::test_fixtures::aggregate_ordering::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::*;
use bae_store::test_fixtures::temp_db;
use bae_types::credit_role::{CreditRoleKind, CreditedAs};

#[tokio::test]
async fn release_detail_orders_track_artists_and_keeps_tracks_without_artists() {
    let (db, _tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let detail = crate::LibraryViews::new(db.clone())
        .find_db_release_detail(RELEASE_A)
        .await
        .unwrap()
        .expect("release detail");

    let track_ids: Vec<&str> = detail.tracks.iter().map(|track| track.track.id.as_str()).collect();
    assert_eq!(track_ids, vec![TRACK_A, TRACK_B]);

    let artist_names: Vec<&str> =
        detail.tracks[0].artists.iter().map(|artist| artist.artist.name.as_str()).collect();
    assert_eq!(artist_names, vec!["Track Artist Name First", "Track Artist Name Second"]);
    assert!(detail.tracks[1].artists.is_empty());

    // Each track shows its own credits, and one with none its album's: the
    // detail and every other reader take that from the one SQL rule.
    let names = |credits: &[bae_records::credits::LibraryCredit]| {
        credits.iter().map(|credit| credit.artist.name.clone()).collect::<Vec<_>>()
    };
    let album_artists = records.get_artists_for_album(&detail.release.album_id).await.unwrap();
    assert!(!album_artists.is_empty(), "the fixture's album credits an artist");
    assert_eq!(names(&detail.tracks[0].shown_artists), artist_names);
    assert_eq!(names(&detail.tracks[1].shown_artists), names(&album_artists));
    for track in &detail.tracks {
        let shown = records.get_shown_artists_for_track(&track.track.id).await.unwrap();
        assert_eq!(names(&shown), names(&track.shown_artists), "{}", track.track.id);
    }
    assert!(records.get_shown_artists_for_track("no-such-track").await.unwrap().is_empty());
}

/// A release's detail lists its files in natural order of their names, case
/// aside, and the read under it walks the index of each release's files in
/// that order, with their pins and uploads.
#[tokio::test]
async fn release_detail_files_are_in_case_insensitive_natural_order() {
    let (db, tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    insert_files_out_of_natural_order(&db, tmp.path()).await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_db_release_detail(RELEASE_A)
        .await
        .unwrap()
        .expect("release detail");
    let names: Vec<&str> = detail.files.iter().map(|f| f.original_filename.as_str()).collect();
    assert_eq!(names, FILES_IN_NATURAL_ORDER, "the release's detail");

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::storage_inspector::INSPECTOR_FILES),
                [RELEASE_A],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("USING INDEX idx_release_files_release_order"))
            && !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the detail's files walk the index with their pins and uploads: {plan:?}"
    );
}

/// A release's index among its album's releases is how many come before it
/// in the album's order, by place and then id, counted on the album's
/// releases index.
#[tokio::test]
async fn a_releases_index_counts_the_releases_before_it() {
    let (db, _tmp) = aggregate_db(crate::TEST_SCHEMA).await;
    // Places 0, 1, 1: the two at place 1 go by id.
    for (release_id, index) in [
        ("8aa66d48-65a0-42e4-8c1d-e7481e8c1861", 0),
        ("0252dedb-ee39-4547-8803-438dbeb57a64", 1),
        ("64e79a1f-404a-4c34-809a-a3cb44bf1942", 2),
    ] {
        let context = crate::LibraryViews::new(db.clone())
            .find_release_detail_context(release_id)
            .await
            .unwrap()
            .expect("the release exists");
        assert_eq!(context.release_index, index, "{release_id}");
    }

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::release::RELEASE_INDEX_IN_ALBUM),
                ["8aa66d48-65a0-42e4-8c1d-e7481e8c1861"],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("idx_releases_album_id")),
        "the count reads the album's releases index: {plan:?}"
    );
}

const CREDIT_ALBUM: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d01";
const CREDIT_RELEASE: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d02";
const CREDITED_WORK: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d03";
const PRODUCER_ONE: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d11";
const PRODUCER_TWO: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d12";
const ARRANGER_OF_TRACKS: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d13";
const ARRANGER_OF_WORK: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d14";
const COMPOSER_OF_WORK: &str = "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d15";
const CREDIT_TRACKS: [&str; 6] = [
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d21",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d22",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d23",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d24",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d25",
    "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d26",
];

/// A release lists the credits of itself, of its tracks and of the works its
/// tracks perform in a role other than a writing one, by role and artist: two
/// producers of the whole release are one row, an arranger of two tracks one
/// row naming them, and a work's composer is not among them.
#[tokio::test]
async fn a_release_lists_its_credits_by_role_and_artist_with_the_tracks_each_covers() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let source = bae_catalogs::catalog::Catalog::MusicBrainz;
    RecordsFixture::new()
        .artist(PRODUCER_ONE, "Producer Name One")
        .artist(PRODUCER_TWO, "Producer Name Two")
        .artist(ARRANGER_OF_TRACKS, "Arranger Name One")
        .artist(ARRANGER_OF_WORK, "Arranger Name Two")
        .artist(COMPOSER_OF_WORK, "Composer Name")
        .album(CREDIT_ALBUM, "Album Title", None, &[PRODUCER_ONE])
        .release(CREDIT_ALBUM, CREDIT_RELEASE)
        .tracks(
            CREDIT_RELEASE,
            &[
                (CREDIT_TRACKS[0], "Track Title 1"),
                (CREDIT_TRACKS[1], "Track Title 2"),
                (CREDIT_TRACKS[2], "Track Title 3"),
                (CREDIT_TRACKS[3], "Track Title 4"),
                (CREDIT_TRACKS[4], "Track Title 5"),
                (CREDIT_TRACKS[5], "Track Title 6"),
            ],
        )
        .release_role(
            "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d31",
            CREDIT_RELEASE,
            PRODUCER_ONE,
            (source, CreditedAs::of(CreditRoleKind::Producer)),
        )
        .release_role(
            "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d32",
            CREDIT_RELEASE,
            PRODUCER_TWO,
            (source, CreditedAs::of(CreditRoleKind::Producer)),
        )
        .track_role(
            "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d33",
            CREDIT_TRACKS[1],
            ARRANGER_OF_TRACKS,
            (source, CreditedAs::of(CreditRoleKind::Arranger)),
        )
        .track_role(
            "a7c1e5d2-3b4f-4c68-9d0e-1f2a3b4c5d34",
            CREDIT_TRACKS[4],
            ARRANGER_OF_TRACKS,
            (source, CreditedAs::of(CreditRoleKind::Arranger)),
        )
        .work(CREDITED_WORK, "Work Title")
        .composer(CREDITED_WORK, COMPOSER_OF_WORK)
        .work_credit(CREDITED_WORK, ARRANGER_OF_WORK, CreditedAs::of(CreditRoleKind::Arranger))
        .performs(CREDIT_TRACKS[2], CREDITED_WORK)
        .write(&bae_records::test_fixtures::test_records(&db))
        .await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_release_detail(CREDIT_RELEASE)
        .await
        .unwrap()
        .unwrap();

    let rows: Vec<(CreditedAs, Vec<&str>, Vec<String>)> = detail
        .credits
        .iter()
        .map(|row| {
            (
                row.role.clone(),
                row.artists.names().iter().map(|credited| credited.name.as_str()).collect(),
                row.tracks.clone(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                CreditedAs::of(CreditRoleKind::Producer),
                vec!["Producer Name One", "Producer Name Two"],
                vec![]
            ),
            (
                CreditedAs::of(CreditRoleKind::Arranger),
                vec!["Arranger Name One"],
                vec!["2".to_string(), "5".to_string()]
            ),
            (
                CreditedAs::of(CreditRoleKind::Arranger),
                vec!["Arranger Name Two"],
                vec!["3".to_string()]
            ),
        ]
    );
}
