//! Which name read off the object tied a release's files to its record: what
//! a commit keeps, and what pointing the release at another record does to it.

use super::*;
use crate::db::{Database, DbAlbum, DbArtist, DbRelease};
use crate::import::{Catalog, MarkKind, ReleaseRecord};

const IDENTIFIED_ARTIST: &str = "a3a3a3a3-0000-4000-8000-000000000001";
const IDENTIFIED_ALBUM: &str = "b3b3b3b3-0000-4000-8000-000000000001";
const IDENTIFIED_RELEASE: &str = "c3c3c3c3-0000-4000-8000-000000000001";

/// Seed the artist and album a release hangs off, and return the release row
/// the commit will write, already naming what tied its files to its record.
async fn seeded(db: &Database, identified_by: Option<MarkKind>) -> (DbAlbum, DbRelease) {
    let now = fixed_now();
    let artist = DbArtist {
        id: IDENTIFIED_ARTIST.to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist).await.unwrap();
    let album = DbAlbum {
        id: IDENTIFIED_ALBUM.to_string(),
        title: "Album Title".to_string(),
        artist_id: artist.id.clone(),
        year: None,
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let mut release = DbRelease::new_test(&album.id, IDENTIFIED_RELEASE);
    release.identified_by = identified_by;
    (album, release)
}

async fn commit(db: &Database, album: &DbAlbum, release: &DbRelease) {
    db.finalize_import_atomic(
        crate::db::ImportCommitGuard::UncheckedTestSetup,
        Some(album),
        release,
        &[],
        crate::db::ImportRows::default(),
        Vec::new(),
        None,
        &[],
        None,
        crate::config::HomeStorage::Opaque,
        &[],
    )
    .await
    .unwrap();
}

async fn stored(db: &Database) -> Option<MarkKind> {
    db.find_release_detail(IDENTIFIED_RELEASE)
        .await
        .unwrap()
        .expect("the committed release reads back")
        .release
        .identified_by
}

/// The name the candidate's lookup answered to survives the commit, and a
/// release nothing tied to its record says so rather than naming a name.
#[tokio::test]
async fn a_commit_keeps_what_tied_the_files_to_the_record() {
    for named in [
        Some(MarkKind::DiscId),
        Some(MarkKind::Barcode),
        Some(MarkKind::CatalogNumber),
        None,
    ] {
        let (db, _tmp) = empty_db().await;
        let (album, release) = seeded(&db, named).await;
        commit(&db, &album, &release).await;
        assert_eq!(stored(&db).await, named);
    }
}

/// Pointing a release at another record replaces what tied its files to the
/// old one. A person picking a record is not a name read off the object, so
/// the release stops naming one — the alternative is a stale answer about a
/// record this release no longer reads.
#[tokio::test]
async fn re_pointing_the_records_rewrites_what_tied_them() {
    let (db, _tmp) = empty_db().await;
    let (album, release) = seeded(&db, Some(MarkKind::DiscId)).await;
    commit(&db, &album, &release).await;

    db.set_records_atomic(
        &release.id,
        &[ReleaseRecord::new(
            &crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-rel-2"),
            Some("mb-group-2".to_string()),
            true,
        )],
        false,
        None,
        &album.id,
        &album.id,
        None,
    )
    .await
    .unwrap();

    assert_eq!(stored(&db).await, None);
}
