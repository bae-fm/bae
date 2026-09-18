//! What the rip databases said about a release's audio: what a commit keeps,
//! and what the read hands back.

use super::*;
use crate::db::{Database, DbAlbum, DbArtist, DbRelease};
use crate::import::{TrackVerification, Verification, VerificationSource};

const VERIFIED_ARTIST: &str = "a2a2a2a2-0000-4000-8000-000000000001";
const VERIFIED_ALBUM: &str = "b2b2b2b2-0000-4000-8000-000000000001";
const VERIFIED_RELEASE: &str = "c2c2c2c2-0000-4000-8000-000000000001";

/// Seed the artist and album a release hangs off, and return the release row
/// the commit will write.
async fn seeded_release(db: &Database) -> (DbAlbum, DbRelease) {
    let now = fixed_now();
    let artist = DbArtist {
        id: VERIFIED_ARTIST.to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist).await.unwrap();
    let album = DbAlbum {
        id: VERIFIED_ALBUM.to_string(),
        title: "Album Title".to_string(),
        artist_id: artist.id.clone(),
        year: None,
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease::new_test(&album.id, VERIFIED_RELEASE);
    (album, release)
}

async fn commit(
    db: &Database,
    album: &DbAlbum,
    release: &DbRelease,
    rows: crate::db::ImportRows<'_>,
) {
    db.finalize_import_atomic(
        crate::db::ImportCommitGuard::UncheckedTestSetup,
        Some(album),
        release,
        &[],
        rows,
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

/// The commit writes one row per track and the read hands every one of them
/// back: the counts each database answered with, the CRC of the bits they are
/// about, and the track that no database confirmed — which is what leaves the
/// release unverified as a whole.
#[tokio::test]
async fn a_commit_keeps_every_track_s_counts() {
    let (db, _tmp) = empty_db().await;
    let (album, release) = seeded_release(&db).await;
    let verification = Verification {
        source: VerificationSource::Log,
        tracks: vec![
            TrackVerification {
                number: 1,
                accuraterip_confidence: Some(37),
                ctdb_confidence: Some(12),
                crc: Some(0xE94F_69D5),
            },
            TrackVerification {
                number: 2,
                accuraterip_confidence: Some(299),
                ctdb_confidence: None,
                crc: Some(0xBF12_B7A9),
            },
            TrackVerification {
                number: 3,
                accuraterip_confidence: None,
                ctdb_confidence: None,
                crc: None,
            },
        ],
    };

    commit(
        &db,
        &album,
        &release,
        crate::db::ImportRows {
            verification: Some(&verification),
            ..Default::default()
        },
    )
    .await;

    let stored = db
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("the committed release reads back")
        .verification;
    assert_eq!(stored.as_ref(), Some(&verification));
    assert_eq!(
        stored.and_then(|stored| stored.matched_copies()),
        None,
        "a track no database confirmed leaves the release unverified"
    );
}

/// A release no source verified carries no rows, and the read says so rather
/// than handing back an empty verification.
#[tokio::test]
async fn a_release_no_source_verified_carries_none() {
    let (db, _tmp) = empty_db().await;
    let (album, release) = seeded_release(&db).await;
    commit(&db, &album, &release, crate::db::ImportRows::default()).await;

    assert!(db
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("the committed release reads back")
        .verification
        .is_none());
}
