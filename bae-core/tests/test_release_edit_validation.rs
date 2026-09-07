#![cfg(feature = "test-utils")]
//! The release-metadata edit rule is enforced on the write, not on one caller.
//!
//! The desktop editor shapes its form through `RawReleaseEdit::shape`, which
//! rejects a blank album title and an artist-less album. MCP's
//! `release_metadata_update` builds a `ReleaseUserEdit`
//! field-for-field and hands it straight to
//! `LibraryManager::apply_release_metadata_user_edit`. These tests drive that
//! write path the way those surfaces do, with no editor in front of it.

use bae_core::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack, Pressing};
use bae_core::import::{
    ArtistAssignment, PressingEdit, ReleaseUserEdit, TrackArtistAssignments, TrackUserEdit,
};
use bae_core::library::LibraryError;
use chrono::Utc;
use uuid::Uuid;

/// One album, one release, one track — enough for an edit to land on.
async fn seed(db: &Database) -> (String, String) {
    let artist = DbArtist {
        id: Uuid::new_v4().to_string(),
        name: "Original Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    let album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Original Album".to_string(),
        artist_id: artist.id.clone(),
        year: None,
        primary_release_id: None,
        is_compilation: false,
        created_at: Utc::now(),
    };
    let release = DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: Pressing::blank(),
        disc_id: None,
        metadata_provenance: Some(bae_core::import::MetadataProvenance::FileTags),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: Utc::now(),
    };
    let track = DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        title: "Original Track".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: None,
        discogs_position: None,
        created_at: Utc::now(),
    };
    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&track).await.unwrap();
    (album.id, release.id)
}

/// A wire edit built field-for-field, exactly as `bae-automation`'s
/// `release_user_edit` builds one from an MCP tool call — no shaping, no trimming.
fn wire_edit(album_title: &str, album_artist_seed_names: &[&str]) -> ReleaseUserEdit {
    ReleaseUserEdit {
        album_title: album_title.to_string(),
        album_artist_assignments: album_artist_seed_names
            .iter()
            .map(|name| ArtistAssignment::new(*name))
            .collect(),
        album_year: None,
        pressing: PressingEdit::blank(),
        tracks: vec![TrackUserEdit {
            title: "Original Track".to_string(),
            side: 1,
            track_number: Some(1),
            artist_assignments: TrackArtistAssignments::AlbumArtists,
            file: None,
        }],
    }
}

/// Every shape the write path refuses: a blank album title — empty or
/// whitespace-only, one keystroke apart — and an album with no named artist,
/// whether none was supplied or the one that was is blank. Each is rejected and
/// leaves the stored row untouched.
#[tokio::test]
async fn invalid_edits_are_rejected_on_the_write_path() {
    for (case, album_title, album_artist_seed_names) in [
        ("an empty album title", "", &["Artist Alpha"][..]),
        (
            "a whitespace-only album title",
            "   ",
            &["Artist Alpha"][..],
        ),
        ("an artist-less album", "Album Alpha", &[][..]),
        ("a blank album artist name", "Album Alpha", &["  "][..]),
    ] {
        let (manager, db, _tmp) = bae_test_support::setup_test_library().await;
        let (album_id, release_id) = seed(&db).await;

        let result = manager
            .apply_release_metadata_user_edit(
                &release_id,
                &wire_edit(album_title, album_artist_seed_names),
            )
            .await;

        assert!(
            matches!(result, Err(LibraryError::Edit(_))),
            "{case} must be rejected, got {result:?}",
        );
        let album = db.find_album_by_id(&album_id).await.unwrap().unwrap();
        assert_eq!(
            album.title, "Original Album",
            "{case}: a rejected edit must not have written",
        );
    }
}

/// The editor trims what the user types before it ever reaches the write. A
/// surface that hands over raw text gets the same treatment, so the two agree on
/// what lands in the row.
#[tokio::test]
async fn an_untrimmed_album_title_is_stored_trimmed() {
    let (manager, db, _tmp) = bae_test_support::setup_test_library().await;
    let (album_id, release_id) = seed(&db).await;

    manager
        .apply_release_metadata_user_edit(
            &release_id,
            &wire_edit("  Album Alpha  ", &["Artist Alpha"]),
        )
        .await
        .unwrap();

    let album = db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Album Alpha");
}

/// The artist names a user edit does supply are trimmed.
#[tokio::test]
async fn new_artist_names_are_stored_trimmed() {
    let (manager, db, _tmp) = bae_test_support::setup_test_library().await;
    let (album_id, release_id) = seed(&db).await;

    manager
        .apply_release_metadata_user_edit(
            &release_id,
            &wire_edit("Album Alpha", &["  Artist Alpha  "]),
        )
        .await
        .unwrap();

    let artists = db.get_artists_for_album(&album_id).await.unwrap();
    assert!(
        artists.iter().any(|a| a.name == "Artist Alpha"),
        "expected a trimmed artist name, got {:?}",
        artists.iter().map(|a| &a.name).collect::<Vec<_>>(),
    );
}
