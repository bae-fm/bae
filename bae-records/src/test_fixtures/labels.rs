//! A library of one release, and how a test writes, adds and reads back the
//! labels that release names.

use crate::records::label_resolution::{pressing_write, write_release_labels_on, LabelRow};
use crate::{DbAlbum, DbRelease, IdentityWriteError, PressingParts, Records};
use bae_store::Store;
use bae_types::pressing::ReleaseLabel;

pub const RELEASE: &str = "5d1c2c9e-4f0b-4a43-9a51-0e6c1d3b7a10";

/// A database on `schema` holding one album and its release, `RELEASE`.
pub async fn library(schema: bae_store::Schema) -> (bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    let album = DbAlbum::new_test("Album Title", &coven::UuidIds);
    let release = DbRelease::new_test(&album.id, RELEASE);
    super::test_records(&db)
        .insert_album_with_release_and_tracks(&album, &[], &release, &[], &[], &[])
        .await
        .unwrap();
    (db, tmp)
}

pub fn label(name: Option<&str>, number: Option<&str>) -> ReleaseLabel {
    ReleaseLabel::new(name.map(str::to_string), number.map(str::to_string)).unwrap()
}

pub fn with_ids(
    label: ReleaseLabel,
    musicbrainz: Option<&str>,
    discogs: Option<&str>,
) -> ReleaseLabel {
    label.with_ids(musicbrainz.map(str::to_string), discogs.map(str::to_string))
}

/// Import a release of its own naming the label `name`, with the catalog ids
/// given, as an import makes a label, and return the library label it names.
pub async fn add_label(
    db: &Store,
    name: &str,
    musicbrainz: Option<&str>,
    discogs: Option<&str>,
) -> String {
    let album = DbAlbum::new_test("Album Title", &coven::UuidIds);
    let mut release =
        DbRelease::new_test(&album.id, &coven::IdSource::new_id(&coven::UuidIds).to_string());
    release.pressing.labels = vec![with_ids(label(Some(name), None), musicbrainz, discogs)];
    super::test_records(db)
        .insert_album_with_release_and_tracks(&album, &[], &release, &[], &[], &[])
        .await
        .unwrap();
    let release_id = release.id;
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT label_id FROM release_labels WHERE release_id = ?",
            [release_id],
            |row| row.get(0),
        )?)
    })
    .await
    .unwrap()
}

/// Write `RELEASE`'s labels as an edit would.
pub async fn write(store: &Store, labels: Vec<ReleaseLabel>) -> Result<(), IdentityWriteError> {
    let pressing = bae_types::pressing::Pressing {
        labels: labels.clone(),
        ..bae_types::pressing::Pressing::blank()
    };
    let parts = PressingParts::new(&coven::UuidIds, store.now_utc(), &pressing);
    store
        .write(move |sql| write_release_labels_on(&sql, RELEASE, &labels, &pressing_write(&parts)))
        .await
}

/// The label each of `RELEASE`'s label rows names, as stored, with its number.
pub async fn placed(db: &Store) -> Vec<(Option<String>, Option<String>)> {
    db.read(|sql| {
        Ok(sql.query(
            "SELECT label_id, catalog_number FROM release_labels \
             WHERE release_id = ? ORDER BY position",
            [RELEASE],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    })
    .await
    .unwrap()
}

pub async fn stored_label(db: &Store, id: &str) -> LabelRow {
    let id = id.to_string();
    db.read(move |sql| {
        Ok(sql.query_row("SELECT * FROM labels WHERE id = ?", [id], LabelRow::from_row)?)
    })
    .await
    .unwrap()
}

/// The labels `RELEASE` reads with, as the library shows them.
pub async fn read_labels(records: &Records) -> Vec<ReleaseLabel> {
    records.find_release_by_id(RELEASE).await.unwrap().unwrap().pressing.labels
}

pub fn ids_of(rows: &[(Option<String>, Option<String>)]) -> Vec<Option<&str>> {
    rows.iter().map(|(id, _)| id.as_deref()).collect()
}
