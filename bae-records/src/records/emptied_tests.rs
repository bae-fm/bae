//! An album whose last release leaves, a work nothing performs and an artist
//! nothing credits are deleted by the write that empties them, as one device
//! writing alone would delete them.

use crate::test_fixtures::*;
use bae_store::test_fixtures::temp_db;

const ALBUM_B: &str = "5d0b1a0e-2b44-4a37-9d51-0a9e7a2b6c01";
const WORK_PART: &str = "b1e4c7d2-9a3f-4c5e-8d6b-3f2a1e0d9c88";

async fn album_held(db: &bae_store::Store, id: &str) -> bool {
    db.read_row(super::AlbumIdHeldParams { id: id.to_string() }).await.unwrap().held
}

async fn work_held(db: &bae_store::Store, id: &str) -> bool {
    db.read_row(super::WorkIdHeldParams { id: id.to_string() }).await.unwrap().held
}

async fn artist_held(db: &bae_store::Store, id: &str) -> bool {
    db.read_row(super::ArtistIdHeldParams { id: id.to_string() }).await.unwrap().held
}

/// One album by `ARTIST_PRIMARY` with one release of one track performing
/// `WORK_A`, composed by `ARTIST_COMPOSER`.
fn performed_album() -> library::RecordsFixture {
    artist_album_release()
        .artist(ARTIST_COMPOSER, "Composer Name")
        .tracks(RELEASE_A, &[(TRACK_A, "Track A")])
        .work(WORK_A, "Work A")
        .composer(WORK_A, ARTIST_COMPOSER)
        .performs(TRACK_A, WORK_A)
}

#[tokio::test]
async fn deleting_the_last_release_deletes_its_album_work_and_artists() {
    let (db, _tmp) = temp_db(TEST_SCHEMA).await;
    let records = test_records(&db);
    performed_album().write(&records).await;

    delete_release(&records, RELEASE_A).await;

    assert!(!album_held(&db, ALBUM_A).await, "the album");
    assert!(!work_held(&db, WORK_A).await, "the work");
    assert!(!artist_held(&db, ARTIST_PRIMARY).await, "the album's artist");
    assert!(!artist_held(&db, ARTIST_COMPOSER).await, "the composer");
}

#[tokio::test]
async fn a_release_left_in_the_album_keeps_it_and_what_it_holds() {
    let (db, _tmp) = temp_db(TEST_SCHEMA).await;
    let records = test_records(&db);
    performed_album().release(ALBUM_A, RELEASE_B).write(&records).await;

    delete_release(&records, RELEASE_A).await;

    assert!(album_held(&db, ALBUM_A).await, "the album");
    assert!(artist_held(&db, ARTIST_PRIMARY).await, "the album's artist");
}

#[tokio::test]
async fn what_another_album_still_uses_stays() {
    let (db, _tmp) = temp_db(TEST_SCHEMA).await;
    let records = test_records(&db);
    performed_album()
        .album(ALBUM_B, "Album B", None, &[ARTIST_PRIMARY])
        .release(ALBUM_B, RELEASE_B)
        .tracks(RELEASE_B, &[(TRACK_B, "Track B")])
        .performs(TRACK_B, WORK_A)
        .write(&records)
        .await;

    delete_release(&records, RELEASE_A).await;

    assert!(!album_held(&db, ALBUM_A).await, "the emptied album");
    assert!(album_held(&db, ALBUM_B).await, "the other album");
    assert!(work_held(&db, WORK_A).await, "the work its track performs");
    assert!(artist_held(&db, ARTIST_PRIMARY).await, "credited on the other album");
    assert!(artist_held(&db, ARTIST_COMPOSER).await, "composer of the kept work");
}

/// A symphony no track performs itself is performed through its movement.
#[tokio::test]
async fn a_work_stays_while_a_part_of_it_is_performed_and_goes_with_it() {
    let (db, _tmp) = temp_db(TEST_SCHEMA).await;
    let records = test_records(&db);
    artist_album_release()
        .tracks(RELEASE_A, &[(TRACK_A, "Track A")])
        .work(WORK_A, "Symphony")
        .work(WORK_PART, "Movement")
        .part(WORK_A, WORK_PART)
        .performs(TRACK_A, WORK_PART)
        .write(&records)
        .await;
    assert!(work_held(&db, WORK_A).await, "the symphony is written");

    delete_release(&records, RELEASE_A).await;

    assert!(!work_held(&db, WORK_PART).await, "the movement");
    assert!(!work_held(&db, WORK_A).await, "the symphony");
}

#[tokio::test]
async fn deleting_every_release_of_an_album_deletes_it() {
    let (db, _tmp) = temp_db(TEST_SCHEMA).await;
    let records = test_records(&db);
    performed_album().release(ALBUM_A, RELEASE_B).write(&records).await;
    let mut plans = Vec::new();
    for release in [RELEASE_A, RELEASE_B] {
        plans.push(records.plan_release_deletion(release).await.unwrap().unwrap());
    }

    records.delete_album_with_cleanup(ALBUM_A, plans).await.unwrap();

    assert!(!album_held(&db, ALBUM_A).await, "the album");
    assert!(!artist_held(&db, ARTIST_PRIMARY).await, "its artist");
}

/// A release moving to another album empties the album it left.
#[tokio::test]
async fn a_release_moving_out_deletes_the_album_it_left() {
    let (db, _tmp) = temp_db(TEST_SCHEMA).await;
    let records = test_records(&db);
    artist_album_release().write(&records).await;
    let target = crate::DbAlbum::new_test("Target Album", &coven::UuidIds);

    records
        .set_records_atomic(crate::RecordsChange {
            release_id: RELEASE_A.to_string(),
            records: Vec::new(),
            current_album_id: ALBUM_A.to_string(),
            target_album_id: target.id.clone(),
            new_album: Some(target.clone()),
        })
        .await
        .unwrap();

    assert!(!album_held(&db, ALBUM_A).await, "the album it left");
    assert!(album_held(&db, &target.id).await, "the album it joined");
    assert!(artist_held(&db, ARTIST_PRIMARY).await, "the copy of the credits keeps the artist");
}

/// An edit that credits someone else leaves the old artist credited nowhere.
#[tokio::test]
async fn an_edit_crediting_another_artist_deletes_the_one_nothing_credits() {
    let (db, _tmp) = temp_db(TEST_SCHEMA).await;
    let records = test_records(&db);
    artist_album_release().artist(ARTIST_B, "Other Artist").write(&records).await;

    records.credit_album_for_test(ALBUM_A, &[ARTIST_B]).await.unwrap();

    assert!(!artist_held(&db, ARTIST_PRIMARY).await, "no longer credited");
    assert!(artist_held(&db, ARTIST_B).await, "credited now");
}
