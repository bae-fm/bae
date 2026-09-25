use super::super::*;
use super::*;
use crate::db::{identity, DbAlbum, DbAlbumArtist, DbArtist, DbRelease};
use crate::import::{Catalog, ReleaseRecord};
use chrono::Utc;

fn record(source: Catalog, release_id: &str) -> ReleaseRecord {
    ReleaseRecord::new(
        &crate::import::MetadataRef::new(source, release_id),
        Some(format!("group-{release_id}")),
        source == Catalog::MusicBrainz,
    )
}

async fn ids(db: &Database, query: &'static str) -> Vec<String> {
    db.read(move |sql| {
        sql.query(query, [], |row| row.get::<_, String>(0))
            .map_err(DbError::from)
    })
    .await
    .unwrap()
}

/// A release's records and an album's credits are facts: each row's id is
/// computed from the fact it states, so another device stating the same fact
/// writes the same row.
#[tokio::test]
async fn fact_rows_take_their_ids_from_their_facts() {
    let (db, _tmp) = temp_db().await;
    let now = Utc::now();

    let artist = DbArtist {
        id: ARTIST_1.to_string(),
        name: "Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist).await.unwrap();

    let mut album = DbAlbum::new_test("Album", &artist.id);
    album.id = ALBUM_1.to_string();
    db.insert_album(&album).await.unwrap();
    db.insert_album_artist(&DbAlbumArtist::new(&album.id, &artist.id, 0, now))
        .await
        .unwrap();

    let release = DbRelease::new_test(&album.id, RELEASE_1);
    db.insert_release(&release).await.unwrap();
    db.insert_release_records(
        &release.id,
        &[record(Catalog::Discogs, "discogs-release-1")],
    )
    .await
    .unwrap();
    assert_eq!(
        ids(&db, "SELECT id FROM release_records").await,
        vec![identity::release_record_id(&release.id, Catalog::Discogs)],
    );

    // Moving the release to a fresh album copies its credits under the new
    // album's ids.
    let target = DbAlbum::new_test("Target Album", &artist.id);
    db.set_records_atomic(
        &release.id,
        &[record(Catalog::MusicBrainz, "mb-release-1")],
        false,
        &album.id,
        &target.id,
        Some(&target),
    )
    .await
    .unwrap();

    let credits = ids(&db, "SELECT id FROM album_artists").await;
    assert!(
        credits.contains(&identity::album_artist_id(&target.id, &artist.id)),
        "the copied credit's id is its (album, artist), got {credits:?}"
    );
    assert_eq!(
        ids(&db, "SELECT id FROM release_records").await,
        vec![identity::release_record_id(
            &release.id,
            Catalog::MusicBrainz
        )],
    );
}
