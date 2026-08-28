use super::super::*;
use super::*;
use crate::db::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease};
use crate::import::{MetadataProvenance, MetadataSource, ReleaseIdentity};
use chrono::Utc;
use coven::SystemClock;
use std::sync::Arc;

/// A deterministic id provider yielding valid RFC4122 v4 UUIDs from a counter,
/// so a `Database` built for a test satisfies coven's `IndependentUuid`
/// row-identity validation (it rejects a non-UUID id in a synced-table write)
/// while staying stable and greppable across runs. The fixed `bae0da7a` prefix
/// marks an id as minted by this test provider.
struct SequentialUuidProvider {
    next: std::sync::atomic::AtomicU64,
}

impl coven::IdProvider for SequentialUuidProvider {
    fn new_id(&self) -> String {
        let n = self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("bae0da7a-0000-4000-8000-{n:012x}")
    }
}

async fn db_with_sequential_ids() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        Arc::new(SequentialUuidProvider {
            next: std::sync::atomic::AtomicU64::new(1),
        }),
    )
    .await
    .unwrap();
    (db, tmp)
}

fn identity(source: MetadataSource, release_id: &str) -> ReleaseIdentity {
    ReleaseIdentity {
        source,
        source_group_id: format!("group-{release_id}"),
        source_release_id: release_id.to_string(),
    }
}

#[tokio::test]
async fn identity_rows_take_their_ids_from_the_injected_provider() {
    let (db, _tmp) = db_with_sequential_ids().await;
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
    db.insert_album_artist(&DbAlbumArtist::new(
        &album.id,
        &artist.id,
        0,
        ALBUM_ARTIST_1.to_string(),
        now,
    ))
    .await
    .unwrap();

    let release = DbRelease::new_test(&album.id, RELEASE_1);
    db.insert_release(&release).await.unwrap();

    db.insert_release_identities(
        &release.id,
        &[identity(MetadataSource::Discogs, "discogs-release-1")],
    )
    .await
    .unwrap();

    let ids: Vec<String> = db
        .read(|sql| {
            sql.query("SELECT id FROM release_identities", [], |row| {
                row.get::<_, String>(0)
            })
            .map_err(DbError::from)
        })
        .await
        .unwrap();

    assert_eq!(ids.len(), 1, "one identity row was written, got {ids:?}",);
    assert!(
        ids[0].starts_with("bae0da7a-"),
        "the identity row's id comes from the injected provider, got {:?}",
        ids[0],
    );

    // Moving the release to a fresh album copies its album_artists rows; those
    // PKs are minted here too.
    let target = DbAlbum::new_test("Target Album", &artist.id);
    db.set_identity_atomic(
        &release.id,
        &[identity(MetadataSource::MusicBrainz, "mb-release-1")],
        Some(MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
            release_id: "mb-release-1".to_string(),
        }),
        &album.id,
        &target.id,
        Some(&target),
    )
    .await
    .unwrap();

    let copied: Vec<String> = db
        .read(move |sql| {
            sql.query("SELECT id FROM album_artists WHERE album_id != '9644b84d-94b2-4b3b-863a-d6583931920c'", [], |row| {
                row.get::<_, String>(0)
            })
            .map_err(DbError::from)
        })
        .await
        .unwrap();

    assert_eq!(
        copied.len(),
        1,
        "the album_artists row was copied, got {copied:?}"
    );
    assert!(
        copied[0].starts_with("bae0da7a-"),
        "the copied album_artists row's id comes from the injected provider, got {:?}",
        copied[0],
    );
}
