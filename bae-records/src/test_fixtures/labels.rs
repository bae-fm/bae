//! A library of one release, and how a test writes, adds and reads back the
//! labels that release names.

use crate::records::label_resolution::{label_write, write_release_labels_on, LabelRow};
use crate::{DbAlbum, DbRelease, IdentityWriteError, LabelParts, Records};
use bae_store::Store;
use bae_types::pressing::ReleaseLabel;
use rusqlite::params;

pub const RELEASE: &str = "5d1c2c9e-4f0b-4a43-9a51-0e6c1d3b7a10";

/// A database on `schema` holding one album and its release, `RELEASE`.
pub async fn library(schema: bae_store::Schema) -> (bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    let album = DbAlbum::new_test("Album Title");
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

pub async fn add_label(
    db: &Store,
    id: &str,
    name: &str,
    musicbrainz: Option<&str>,
    discogs: Option<&str>,
) {
    let (id, name) = (id.to_string(), name.to_string());
    let (musicbrainz, discogs) = (musicbrainz.map(str::to_string), discogs.map(str::to_string));
    db.call(move |sql| {
        sql.execute(
            "INSERT INTO labels \
                 (id, name, name_key, musicbrainz_label_id, discogs_label_id, created_at) \
             VALUES (?, ?, ?, ?, ?, '2026-01-01T00:00:00Z')",
            params![id, name, bae_types::text_match::label_name_key(&name), musicbrainz, discogs],
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

/// Write `RELEASE`'s labels as an edit would.
pub async fn write(store: &Store, labels: Vec<ReleaseLabel>) -> Result<(), IdentityWriteError> {
    let parts = LabelParts::new(&coven::UuidIds, store.now_utc(), &labels);
    crate::records::artist_resolution::write_resolving_identities(store, move |sql| {
        write_release_labels_on(&sql, RELEASE, &labels, &label_write(&parts))
    })
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
