//! A release's media, barcodes and labels are rows, one per place in their
//! list: a device's edit of them reaches every other device, and two devices
//! editing different places at once keep both edits.

use super::*;
use bae_types::pressing::{MediaCount, Medium};

const RELEASE: &str = "0252dedb-ee39-4547-8803-438dbeb57a64";

fn count(medium: Medium, count: u32) -> MediaCount {
    MediaCount { medium, count }
}

/// Make `RELEASE`'s media `media` on `db`.
async fn set_media(db: &crate::db::Database, media: Vec<MediaCount>) {
    db.call(move |conn| super::super::write::replace_release_media(conn, RELEASE, &media))
        .await
        .unwrap();
}

/// `RELEASE`'s media as `db` reads the release, and as its Storage Manager
/// listing sorts it.
async fn media_on(db: &crate::db::Database) -> (Vec<MediaCount>, Option<String>) {
    let release = db.find_release_by_id(RELEASE).await.unwrap().unwrap();
    let key = db
        .read(|sql| {
            Ok(sql.query_row(
                "SELECT media_key FROM release_listing WHERE release_id = ?1",
                [RELEASE],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    (release.pressing.facts.media, key)
}

/// The rows both devices must agree on.
const MEDIA_ROWS: &str = "SELECT id || ':' || position || ':' || medium || ':' || count \
     FROM release_media ORDER BY id";

/// One device's edit of a release's media reaches the other, which reads the
/// release and sorts its listing by them.
#[test]
fn a_media_edit_reaches_the_other_device() {
    crate::sync::test_devices::run_two_device_test(check_media_edit_reaches);
}

async fn check_media_edit_reaches(devices: &mut crate::sync::test_devices::TwoDevices) {
    devices.a().database().call(seed_artist_album_release).await.unwrap();
    set_media(devices.a().database(), vec![count(Medium::Cd, 1)]).await;
    devices.converge(MEDIA_ROWS).await;

    set_media(devices.a().database(), vec![count(Medium::Vinyl, 2), count(Medium::Cd, 1)]).await;
    devices.converge(MEDIA_ROWS).await;

    assert_eq!(
        media_on(devices.b().database()).await,
        (
            vec![count(Medium::Vinyl, 2), count(Medium::Cd, 1)],
            Some("vinyl:00002,cd:00001".to_string())
        )
    );
}

/// Two devices, apart, each change a different place in one release's media.
/// Once each has the other's write, both hold both changes.
#[test]
fn concurrent_edits_of_different_places_both_stay() {
    crate::sync::test_devices::run_two_device_test(check_concurrent_places);
}

async fn check_concurrent_places(devices: &mut crate::sync::test_devices::TwoDevices) {
    devices.a().database().call(seed_artist_album_release).await.unwrap();
    set_media(devices.a().database(), vec![count(Medium::Cd, 1), count(Medium::Dvd, 1)]).await;
    devices.converge(MEDIA_ROWS).await;

    devices.pause().await;
    set_media(devices.a().database(), vec![count(Medium::Cd, 2), count(Medium::Dvd, 1)]).await;
    set_media(devices.b().database(), vec![count(Medium::Cd, 1), count(Medium::BluRay, 1)]).await;
    devices.resume().await;
    devices.converge(MEDIA_ROWS).await;

    let both = vec![count(Medium::Cd, 2), count(Medium::BluRay, 1)];
    assert_eq!(media_on(devices.a().database()).await.0, both);
    assert_eq!(media_on(devices.b().database()).await.0, both);
}

/// Make `RELEASE`'s barcodes `barcodes` on `db`.
async fn set_barcodes(db: &crate::db::Database, barcodes: &[&str]) {
    let barcodes: Vec<String> = barcodes.iter().map(|code| code.to_string()).collect();
    db.call(move |conn| super::super::write::replace_release_barcodes(conn, RELEASE, &barcodes))
        .await
        .unwrap();
}

/// `RELEASE`'s barcodes as `db` reads the release.
async fn barcodes_on(db: &crate::db::Database) -> Vec<String> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.barcodes
}

const BARCODE_ROWS: &str = "SELECT id || ':' || position || ':' || barcode \
     FROM release_barcodes ORDER BY id";

/// Two devices, apart, each change a different place in one release's
/// barcodes; once each has the other's write, both hold both changes. A
/// later edit that drops one leaves only the other on both.
#[test]
fn barcode_edits_merge_by_place_and_reach_the_other_device() {
    crate::sync::test_devices::run_two_device_test(check_barcodes);
}

async fn check_barcodes(devices: &mut crate::sync::test_devices::TwoDevices) {
    devices.a().database().call(seed_artist_album_release).await.unwrap();
    set_barcodes(devices.a().database(), &["0123456789012", "5012345678900"]).await;
    devices.converge(BARCODE_ROWS).await;

    devices.pause().await;
    set_barcodes(devices.a().database(), &["0123456789029", "5012345678900"]).await;
    set_barcodes(devices.b().database(), &["0123456789012", "5012345678917"]).await;
    devices.resume().await;
    devices.converge(BARCODE_ROWS).await;
    let both = ["0123456789029", "5012345678917"];
    assert_eq!(barcodes_on(devices.a().database()).await, both);
    assert_eq!(barcodes_on(devices.b().database()).await, both);

    set_barcodes(devices.b().database(), &["0123456789029"]).await;
    devices.converge(BARCODE_ROWS).await;
    assert_eq!(barcodes_on(devices.a().database()).await, ["0123456789029"]);
}

/// Make `RELEASE`'s labels `labels` on `db`, as an edit writes them.
async fn set_labels(db: &crate::db::Database, labels: Vec<bae_types::pressing::ReleaseLabel>) {
    let parts = db.label_parts(&labels);
    db.call(move |conn| {
        super::super::label_resolution::write_release_labels_on(
            conn,
            RELEASE,
            &labels,
            &super::super::label_write(&parts),
        )
        .map_err(super::super::IdentityWriteError::into_db_error)
    })
    .await
    .unwrap();
}

/// `RELEASE`'s labels as `db` reads the release.
async fn labels_on(db: &crate::db::Database) -> Vec<bae_types::pressing::ReleaseLabel> {
    db.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.labels
}

const LABEL_ROWS: &str = "SELECT id || ':' || name || ':' || ifnull(discogs_label_id, '') \
     FROM labels UNION ALL \
     SELECT id || ':' || position || ':' || ifnull(label_id, '') || ':' \
         || ifnull(catalog_number, '') FROM release_labels ORDER BY 1";

/// Two devices, apart, each write a label Discogs names onto one release:
/// both make the one label row its id names, and once each has the other's
/// write, both read the release with the label at each place either wrote.
#[test]
fn label_rows_converge_on_both_devices() {
    crate::sync::test_devices::run_two_device_test(check_labels);
}

async fn check_labels(devices: &mut crate::sync::test_devices::TwoDevices) {
    use bae_types::pressing::ReleaseLabel;
    let rhino = |number: &str| {
        ReleaseLabel::new(Some("Rhino".to_string()), Some(number.to_string()))
            .unwrap()
            .with_ids(None, Some("7190".to_string()))
    };
    devices.a().database().call(seed_artist_album_release).await.unwrap();
    devices.converge(LABEL_ROWS).await;

    devices.pause().await;
    set_labels(devices.a().database(), vec![rhino("R2 1")]).await;
    set_labels(
        devices.b().database(),
        vec![rhino("R2 1"), ReleaseLabel::new(None, Some("XY 3".to_string())).unwrap()],
    )
    .await;
    devices.resume().await;
    devices.converge(LABEL_ROWS).await;

    let both = vec![
        ReleaseLabel::new(Some("Rhino".to_string()), Some("R2 1".to_string())).unwrap(),
        ReleaseLabel::new(None, Some("XY 3".to_string())).unwrap(),
    ];
    assert_eq!(labels_on(devices.a().database()).await, both);
    assert_eq!(labels_on(devices.b().database()).await, both);
    let labels: i64 = devices
        .a()
        .database()
        .read(|sql| Ok(sql.query_row("SELECT count(*) FROM labels", [], |row| row.get(0))?))
        .await
        .unwrap();
    assert_eq!(labels, 1);
}
