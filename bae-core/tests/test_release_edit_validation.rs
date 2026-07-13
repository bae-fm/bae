#![cfg(feature = "test-utils")]
//! The release-metadata edit rule is enforced on the write, not on one caller.
//!
//! The desktop editor shapes its form through `RawReleaseEdit::shape`, which
//! rejects a blank album title and an artist-less album. Every other surface —
//! MCP's `release_metadata_update`, the CLI — builds a `ReleaseUserEdit`
//! field-for-field and hands it straight to
//! `LibraryManager::apply_release_metadata_user_edit`. These tests drive that
//! write path the way those surfaces do, with no editor in front of it.

use bae_core::db::{
    Database, DbAlbum, DbArtist, DbRelease, DbTrack, Pressing, ReleaseMetadataSource,
};
use bae_core::import::{PressingEdit, ReleaseUserEdit, TrackUserEdit};
use bae_core::library::{LibraryError, LibraryManager};
use bae_test_support::{test_config_and_keys, tracing_init};
use chrono::Utc;
use coven::StoreDir;
use tempfile::TempDir;
use uuid::Uuid;

async fn setup() -> (LibraryManager, Database, TempDir) {
    tracing_init();
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let manager = LibraryManager::new(
        database.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    (manager, database, temp_dir)
}

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
        metadata_source: ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
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
fn wire_edit(album_title: &str, album_artist_names: &[&str]) -> ReleaseUserEdit {
    ReleaseUserEdit {
        album_title: album_title.to_string(),
        album_artist_names: album_artist_names.iter().map(|s| s.to_string()).collect(),
        pressing: PressingEdit::blank(),
        tracks: vec![TrackUserEdit {
            title: "Original Track".to_string(),
            side: 1,
            track_number: Some(1),
            artist_names: Vec::new(),
        }],
    }
}

#[tokio::test]
async fn empty_album_title_is_rejected_on_the_write_path() {
    let (manager, db, _tmp) = setup().await;
    let (album_id, release_id) = seed(&db).await;

    let result = manager
        .apply_release_metadata_user_edit(&release_id, &wire_edit("", &["The Beatles"]))
        .await;

    assert!(
        matches!(result, Err(LibraryError::Edit(_))),
        "an empty album title must be rejected, got {result:?}",
    );
    let album = db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(
        album.title, "Original Album",
        "a rejected edit must not have written",
    );
}

/// Whitespace-only is the same blank, one keystroke away.
#[tokio::test]
async fn whitespace_only_album_title_is_rejected_on_the_write_path() {
    let (manager, db, _tmp) = setup().await;
    let (album_id, release_id) = seed(&db).await;

    let result = manager
        .apply_release_metadata_user_edit(&release_id, &wire_edit("   ", &["The Beatles"]))
        .await;

    assert!(matches!(result, Err(LibraryError::Edit(_))));
    let album = db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Original Album");
}

/// The editor trims what the user types before it ever reaches the write. A
/// surface that hands over raw text gets the same treatment, so the two agree on
/// what lands in the row.
#[tokio::test]
async fn an_untrimmed_album_title_is_stored_trimmed() {
    let (manager, db, _tmp) = setup().await;
    let (album_id, release_id) = seed(&db).await;

    manager
        .apply_release_metadata_user_edit(
            &release_id,
            &wire_edit("  Abbey Road  ", &["The Beatles"]),
        )
        .await
        .unwrap();

    let album = db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Abbey Road");
}

#[tokio::test]
async fn an_artist_less_edit_is_rejected_on_the_write_path() {
    let (manager, db, _tmp) = setup().await;
    let (album_id, release_id) = seed(&db).await;

    let result = manager
        .apply_release_metadata_user_edit(&release_id, &wire_edit("Abbey Road", &[]))
        .await;

    assert!(matches!(result, Err(LibraryError::Edit(_))));
    let album = db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Original Album");
}

/// A blank artist name is the artist-less case wearing a string: the edit names
/// one artist, and the name is empty.
#[tokio::test]
async fn a_blank_artist_name_is_rejected_on_the_write_path() {
    let (manager, db, _tmp) = setup().await;
    let (album_id, release_id) = seed(&db).await;

    let result = manager
        .apply_release_metadata_user_edit(&release_id, &wire_edit("Abbey Road", &["  "]))
        .await;

    assert!(matches!(result, Err(LibraryError::Edit(_))));
    let album = db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Original Album");
}

/// The artist names a user edit does supply are trimmed, so "The Beatles" and
/// " The Beatles " don't resolve to two library artists.
#[tokio::test]
async fn artist_names_are_stored_trimmed() {
    let (manager, db, _tmp) = setup().await;
    let (album_id, release_id) = seed(&db).await;

    manager
        .apply_release_metadata_user_edit(
            &release_id,
            &wire_edit("Abbey Road", &["  The Beatles  "]),
        )
        .await
        .unwrap();

    let artists = db.get_artists_for_album(&album_id).await.unwrap();
    assert!(
        artists.iter().any(|a| a.name == "The Beatles"),
        "expected a trimmed artist name, got {:?}",
        artists.iter().map(|a| &a.name).collect::<Vec<_>>(),
    );
}
