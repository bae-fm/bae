use super::*;
use crate::db::{Database, DbArtist};
use crate::test_logs::capture_warn_logs_async;
use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

fn test_config_and_keys(
    library_dir: &coven::LibraryDir,
) -> (
    std::sync::Arc<crate::config::ConfigHandle>,
    crate::keys::KeyService,
) {
    // Unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    let library_id = format!("test-{}", uuid::Uuid::new_v4());
    let config = crate::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    crate::config::install_test_keyring();
    (
        std::sync::Arc::new(crate::config::ConfigHandle::new(config)),
        crate::keys::KeyService::new(library_id),
    )
}

async fn setup_test_manager() -> (LibraryManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let library_dir = coven::LibraryDir::new(temp_dir.path());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let manager = LibraryManager::new(
        database,
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    (manager, temp_dir)
}

fn make_artist(name: &str, discogs_id: Option<&str>, mb_id: Option<&str>) -> DbArtist {
    let now = Utc::now();
    DbArtist {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: discogs_id.map(|s| s.to_string()),
        musicbrainz_artist_id: mb_id.map(|s| s.to_string()),
        created_at: now,
    }
}

#[tokio::test]
async fn test_same_discogs_id_reuses_existing() {
    let (manager, _tmp) = setup_test_manager().await;
    let existing = make_artist("Artist One", Some("d123"), None);
    manager.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", Some("d123"), None);
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_mb_id_reuses_existing() {
    let (manager, _tmp) = setup_test_manager().await;
    let existing = make_artist("Artist One", None, Some("mb-abc"));
    manager.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", None, Some("mb-abc"));
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_no_ids_reuses_existing() {
    let (manager, _tmp) = setup_test_manager().await;
    let existing = make_artist("Artist One", None, None);
    manager.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", None, None);
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn fetch_artist_images_warns_and_skips_when_existing_image_check_fails() {
    let (manager, _tmp) = setup_test_manager().await;
    let database = manager.database_for_test();
    database
        .handle()
        .sql(|sql| {
            sql.connection().execute("DROP TABLE artist_images", [])?;
            Ok::<(), coven::CovenError>(())
        })
        .await
        .unwrap();

    let parsed_artist = make_artist("Artist Name", Some("discogs-artist-1"), None);
    let actual_artist_id = "artist-actual-1".to_string();
    let artist_id_map = HashMap::from([(parsed_artist.id.clone(), actual_artist_id.clone())]);
    let discogs_client = DiscogsClient::new("token".to_string());

    let logs = capture_warn_logs_async(|| async {
        fetch_artist_images(
            &manager,
            &discogs_client,
            std::slice::from_ref(&parsed_artist),
            &artist_id_map,
        )
        .await;
    })
    .await;

    assert!(
        logs.contains("failed to check existing artist image")
            && logs.contains(&actual_artist_id)
            && logs.contains("artist_images"),
        "expected existing artist image check warning, got {logs:?}",
    );
}

#[tokio::test]
async fn test_same_name_same_mb_id_reuses() {
    let (manager, _tmp) = setup_test_manager().await;
    let existing = make_artist("Artist Two", None, Some("mb-artist-two"));
    manager.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", None, Some("mb-artist-two"));
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_different_mb_id_creates_new() {
    let (manager, _tmp) = setup_test_manager().await;
    let existing = make_artist("Artist Two", None, Some("mb-artist-two-uk"));
    manager.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", None, Some("mb-artist-two-ca"));
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    // Should create a new artist, not reuse existing
    assert_eq!(resolved[0], incoming.id);
    assert_ne!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_same_name_different_discogs_id_creates_new() {
    let (manager, _tmp) = setup_test_manager().await;
    let existing = make_artist("Artist Two", Some("d100"), None);
    manager.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist Two", Some("d200"), None);
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], incoming.id);
    assert_ne!(resolved[0], existing.id);
}

#[tokio::test]
async fn test_name_match_accumulates_ids() {
    let (manager, _tmp) = setup_test_manager().await;
    // Existing has discogs ID only
    let existing = make_artist("Artist One", Some("d456"), None);
    manager.insert_artist(&existing).await.unwrap();

    // Incoming has MB ID only — no conflict, should merge
    let incoming = make_artist("Artist One", None, Some("mb-xyz"));
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], existing.id);

    // Verify the existing artist now has both IDs
    let updated = manager
        .get_artist_by_id(&existing.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.discogs_artist_id.as_deref(), Some("d456"));
    assert_eq!(updated.musicbrainz_artist_id.as_deref(), Some("mb-xyz"));
}

#[tokio::test]
async fn test_new_artist_inserts() {
    let (manager, _tmp) = setup_test_manager().await;

    let incoming = make_artist("New Band", Some("d999"), Some("mb-999"));
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], incoming.id);

    // Verify it's in the DB
    let saved = manager
        .get_artist_by_id(&incoming.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.name, "New Band");
    assert_eq!(saved.discogs_artist_id.as_deref(), Some("d999"));
    assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-999"));
}

// ── find_existing_album_for_import (identity-based dedup) ────────

use crate::db::{DbAlbum, DbRelease, DbTrack};
use crate::import::ReleaseIdentity;

/// Set up a manager with a single test artist that the helpers below
/// reference for inserted albums.
async fn setup_test_db_with_artist() -> (LibraryManager, TempDir) {
    let (manager, tmp) = setup_test_manager().await;
    let artist = DbArtist {
        id: "test-artist-id".to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    manager.insert_artist(&artist).await.unwrap();
    (manager, tmp)
}

fn make_album(title: &str) -> DbAlbum {
    let now = Utc::now();
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        artist_id: "test-artist-id".to_string(),
        year: Some(2024),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    }
}

fn make_release(album_id: &str) -> DbRelease {
    let now = Utc::now();
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2024),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    }
}

fn make_track(release_id: &str, number: i32) -> DbTrack {
    let now = Utc::now();
    DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release_id.to_string(),
        title: format!("Track {}", number),
        side: 1,
        track_number: Some(number),
        duration_ms: Some(180000),
        discogs_position: None,
        created_at: now,
    }
}

fn mb_identity(group: &str, release: &str) -> ReleaseIdentity {
    ReleaseIdentity {
        source: MetadataSource::MusicBrainz,
        source_group_id: group.to_string(),
        source_release_id: Some(release.to_string()),
    }
}

fn discogs_identity(master: &str, release: &str) -> ReleaseIdentity {
    ReleaseIdentity {
        source: MetadataSource::Discogs,
        source_group_id: master.to_string(),
        source_release_id: Some(release.to_string()),
    }
}

/// Insert an album + release with the supplied identity rows. Mirrors
/// what the import commit path does, minus tracks/files; tests only
/// need the identity rows reachable from the album.
async fn insert_with_identities(
    manager: &LibraryManager,
    album: &DbAlbum,
    release: &DbRelease,
    identities: &[ReleaseIdentity],
) {
    let track = make_track(&release.id, 1);
    manager
        .insert_album_with_release_and_tracks(album, release, &[track], &[], &[])
        .await
        .unwrap();
    manager
        .insert_release_identities(&release.id, identities)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_exact_release_duplicate_rejected() {
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    let identities = vec![mb_identity("mb-rg-456", "mb-rel-123")];
    insert_with_identities(&manager, &album, &release, &identities).await;

    // Same MB release ID → rejected as duplicate.
    let incoming = vec![mb_identity("mb-rg-789", "mb-rel-123")];
    let result = manager.find_existing_album_for_import(&incoming).await;

    let err = result.expect_err("duplicate import should be rejected");
    assert!(
        err.contains("already in your library"),
        "Expected duplicate error, got: {err}",
    );
    assert!(
        err.contains("Album Title"),
        "Expected album title in error, got: {err}",
    );

    // Discogs side mirrors the same logic.
    let album2 = make_album("Other Album");
    let release2 = make_release(&album2.id);
    let identities2 = vec![discogs_identity("d-master-456", "d-rel-123")];
    insert_with_identities(&manager, &album2, &release2, &identities2).await;

    let incoming2 = vec![discogs_identity("d-master-456", "d-rel-123")];
    let result2 = manager.find_existing_album_for_import(&incoming2).await;
    assert!(result2.is_err());
}

#[tokio::test]
async fn test_same_release_group_finds_existing_album() {
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[mb_identity("mb-rg-456", "mb-rel-123")],
    )
    .await;

    // Different release within the same group → merge into existing album.
    let incoming = vec![mb_identity("mb-rg-456", "mb-rel-999")];
    let merged = manager
        .find_existing_album_for_import(&incoming)
        .await
        .unwrap();
    assert_eq!(merged, Some(album.id));
}

#[tokio::test]
async fn test_same_discogs_master_finds_existing_album() {
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[discogs_identity("d-master-456", "d-rel-123")],
    )
    .await;

    let incoming = vec![discogs_identity("d-master-456", "d-rel-999")];
    let merged = manager
        .find_existing_album_for_import(&incoming)
        .await
        .unwrap();
    assert_eq!(merged, Some(album.id));
}

#[tokio::test]
async fn test_no_match_returns_none() {
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Existing Album");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[mb_identity("mb-rg-456", "mb-rel-123")],
    )
    .await;

    // Different group + different release → no match.
    let incoming = vec![mb_identity("mb-rg-999", "mb-rel-999")];
    let result = manager
        .find_existing_album_for_import(&incoming)
        .await
        .unwrap();
    assert_eq!(result, None);

    // Empty identity vec (Unknown) → skip lookup.
    let result_unknown = manager.find_existing_album_for_import(&[]).await.unwrap();
    assert_eq!(result_unknown, None);
}

#[tokio::test]
async fn test_cross_source_no_false_merge() {
    let (manager, _tmp) = setup_test_db_with_artist().await;
    // Existing album holds only a Discogs identity row.
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[discogs_identity("d-master-200", "d-rel-100")],
    )
    .await;

    // Unrelated MB import → should not merge.
    let incoming = vec![mb_identity("mb-rg-600", "mb-rel-500")];
    let result = manager
        .find_existing_album_for_import(&incoming)
        .await
        .unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn test_cross_source_merge_via_path_2() {
    // An MB-rooted release that carries both an MB and a Discogs row
    // (because MB url-rels resolved to a Discogs release at commit
    // time) is reachable from a later Discogs-only import of the same
    // master.
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    let identities = vec![
        mb_identity("mb-rg-100", "mb-rel-50"),
        discogs_identity("d-master-200", "d-rel-75"),
    ];
    insert_with_identities(&manager, &album, &release, &identities).await;

    // Later Discogs import of the same master, different pressing.
    let incoming = vec![discogs_identity("d-master-200", "d-rel-300")];
    let merged = manager
        .find_existing_album_for_import(&incoming)
        .await
        .unwrap();
    assert_eq!(merged, Some(album.id));
}

#[tokio::test]
async fn test_cross_source_merge_via_path_2_inverse() {
    // Inverse of `test_cross_source_merge_via_path_2`: existing album
    // holds only a Discogs row. A later MB-rooted import resolves a
    // cross-link Discogs row pointing to the same master and thus
    // attaches via the Discogs match — even though the existing album
    // has no MB row to compare against.
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[discogs_identity("d-master-200", "d-rel-75")],
    )
    .await;

    // MB-rooted import: an MB row plus a Discogs cross-link to the
    // same Discogs master the existing album sits on.
    let incoming = vec![
        mb_identity("mb-rg-100", "mb-rel-50"),
        discogs_identity("d-master-200", "d-rel-300"),
    ];
    let merged = manager
        .find_existing_album_for_import(&incoming)
        .await
        .unwrap();
    assert_eq!(merged, Some(album.id));
}

#[tokio::test]
async fn test_unknown_import_skips_lookup() {
    // Unknown imports never deduplicate against existing releases —
    // they always create a fresh album.
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Existing Album");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[mb_identity("mb-rg-1", "mb-rel-1")],
    )
    .await;

    // Empty identity vec — no match should be returned.
    let result = manager.find_existing_album_for_import(&[]).await.unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn test_merge_release_into_existing_album() {
    // A second release with the same MB group attaches to the
    // existing album via `find_existing_album_for_import` returning
    // `Some(album_id)`. The caller redirects the new release's
    // album_id and inserts it as a sibling.
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release1 = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release1,
        &[mb_identity("mb-rg-500", "mb-rel-100")],
    )
    .await;

    // Lookup returns the existing album for a release in the same
    // MB group.
    let incoming = vec![mb_identity("mb-rg-500", "mb-rel-200")];
    let existing_album_id = manager
        .find_existing_album_for_import(&incoming)
        .await
        .unwrap();
    assert_eq!(existing_album_id, Some(album.id.clone()));

    // Insert a sibling release pointing at the existing album.
    let mut release2 = make_release(&album.id);
    release2.album_id = existing_album_id.unwrap();
    let track2 = make_track(&release2.id, 1);
    manager
        .insert_release_with_tracks(&release2, &[track2], &[], &[])
        .await
        .unwrap();
    manager
        .insert_release_identities(&release2.id, &incoming)
        .await
        .unwrap();

    let releases = manager.get_releases_for_album(&album.id).await.unwrap();
    assert_eq!(releases.len(), 2);
    let release_ids: Vec<&str> = releases.iter().map(|r| r.id.as_str()).collect();
    assert!(release_ids.contains(&release1.id.as_str()));
    assert!(release_ids.contains(&release2.id.as_str()));
}

// ── check_releases_in_library (identity-based status badges) ─────

#[tokio::test]
async fn test_check_release_in_library_exact_match() {
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[mb_identity("mb-rg-1", "mb-rel-1")],
    )
    .await;

    let checks = vec![crate::db::LibraryCheck {
        release_id: "mb-rel-1".to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = manager.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(statuses[0].release_in_library);
    assert!(statuses[0].album_in_library);
    assert_eq!(statuses[0].album_id.as_deref(), Some(album.id.as_str()));
    assert_eq!(statuses[0].album_title.as_deref(), Some("Album Title"));
}

#[tokio::test]
async fn test_check_album_in_library_group_only() {
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[mb_identity("mb-rg-1", "mb-rel-1")],
    )
    .await;

    // Different release ID, same group → album_in_library only.
    let checks = vec![crate::db::LibraryCheck {
        release_id: "mb-rel-OTHER".to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = manager.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(!statuses[0].release_in_library);
    assert!(statuses[0].album_in_library);
    assert_eq!(statuses[0].album_id.as_deref(), Some(album.id.as_str()));
}

#[tokio::test]
async fn test_check_release_not_in_library() {
    let (manager, _tmp) = setup_test_db_with_artist().await;

    let checks = vec![crate::db::LibraryCheck {
        release_id: "mb-rel-NONE".to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("mb-rg-NONE".to_string()),
    }];
    let statuses = manager.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(!statuses[0].release_in_library);
    assert!(!statuses[0].album_in_library);
    assert!(statuses[0].album_id.is_none());
}

#[tokio::test]
async fn test_check_cross_source_doesnt_leak() {
    // An MB candidate against a Discogs-only library entry should
    // not match — different sources.
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[discogs_identity("d-master-1", "d-rel-1")],
    )
    .await;

    let checks = vec![crate::db::LibraryCheck {
        release_id: "mb-rel-1".to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = manager.check_releases_in_library(&checks).await.unwrap();
    assert!(!statuses[0].release_in_library);
    assert!(!statuses[0].album_in_library);
}

// -- Pure helpers: validation folding and artist-id remapping --

#[test]
fn validation_folds_validate_token_outcomes() {
    use crate::config::DiscogsValidation;
    use crate::discogs::client::DiscogsError;

    assert_eq!(
        validation_from_validate_result(Ok(())),
        DiscogsValidation::Valid
    );
    // A 401 is the one outcome that rejects the stored key.
    assert_eq!(
        validation_from_validate_result(Err(DiscogsError::InvalidApiKey)),
        DiscogsValidation::Rejected
    );
    // Anything that merely fails to confirm the key leaves it unvalidated
    // to retry — never rejected.
    for couldnt_confirm in [
        DiscogsError::RateLimit,
        DiscogsError::NotFound,
        DiscogsError::Serialization(serde_json::from_str::<i32>("nope").unwrap_err()),
    ] {
        assert_eq!(
            validation_from_validate_result(Err(couldnt_confirm)),
            DiscogsValidation::Unvalidated
        );
    }
}

fn track_artist(artist_id: &str) -> crate::db::DbTrackArtist {
    crate::db::DbTrackArtist {
        id: Uuid::new_v4().to_string(),
        track_id: "track-1".to_string(),
        artist_id: artist_id.to_string(),
        position: 3,
        created_at: Utc::now(),
    }
}

fn album_artist(artist_id: &str) -> crate::db::DbAlbumArtist {
    crate::db::DbAlbumArtist {
        id: Uuid::new_v4().to_string(),
        album_id: "album-1".to_string(),
        artist_id: artist_id.to_string(),
        position: 2,
        created_at: Utc::now(),
    }
}

#[test]
fn remap_track_artists_rewrites_id_and_preserves_the_rest() {
    let ta = track_artist("parsed-1");
    let map = std::collections::HashMap::from([("parsed-1".to_string(), "db-1".to_string())]);

    let remapped = remap_track_artists(std::slice::from_ref(&ta), &map).unwrap();
    assert_eq!(remapped.len(), 1);
    assert_eq!(remapped[0].artist_id, "db-1");
    // Everything other than the remapped artist id carries through.
    assert_eq!(remapped[0].id, ta.id);
    assert_eq!(remapped[0].track_id, "track-1");
    assert_eq!(remapped[0].position, 3);
}

#[test]
fn remap_track_artists_errors_on_unmapped_id() {
    let ta = track_artist("orphan-track-artist");
    let err = remap_track_artists(std::slice::from_ref(&ta), &std::collections::HashMap::new())
        .unwrap_err();
    assert!(
        err.contains("orphan-track-artist"),
        "error should name the unmapped id: {err}"
    );
}

#[test]
fn remap_album_artists_rewrites_id_and_preserves_the_rest() {
    let aa = album_artist("parsed-2");
    let map = std::collections::HashMap::from([("parsed-2".to_string(), "db-2".to_string())]);

    let remapped = remap_album_artists(std::slice::from_ref(&aa), &map).unwrap();
    assert_eq!(remapped.len(), 1);
    assert_eq!(remapped[0].artist_id, "db-2");
    assert_eq!(remapped[0].id, aa.id);
    assert_eq!(remapped[0].album_id, "album-1");
    assert_eq!(remapped[0].position, 2);
}

#[test]
fn remap_album_artists_errors_on_unmapped_id() {
    let aa = album_artist("orphan-album-artist");
    let err = remap_album_artists(std::slice::from_ref(&aa), &std::collections::HashMap::new())
        .unwrap_err();
    assert!(
        err.contains("orphan-album-artist"),
        "error should name the unmapped id: {err}"
    );
}

fn detail_track(
    title: &str,
    side: u32,
    position: &str,
    artist: Option<&str>,
) -> crate::import::search::ReleaseTrack {
    crate::import::search::ReleaseTrack {
        title: title.to_string(),
        artist: artist.map(str::to_string),
        duration_ms: None,
        position: position.to_string(),
        side,
    }
}

/// A 2-side vinyl detail: A1, A2 on side 1; B1 on side 2. Album artist is
/// "Artist Name".
fn vinyl_detail() -> crate::import::search::ImportSearchReleaseDetail {
    crate::import::search::ImportSearchReleaseDetail {
        release_id: "rel-1".to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: None,
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(1969),
        format: Some("Vinyl".to_string()),
        label: Some("Label".to_string()),
        catalog_number: Some("CAT-1".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        track_count: 3,
        tracks: vec![
            detail_track("A1 title", 1, "A1", None),
            detail_track("A2 title", 1, "A2", None),
            detail_track("B1 title", 2, "B1", None),
        ],
        cover_art: vec![],
    }
}

fn exact_choice() -> crate::import::IdentityChoice {
    crate::import::IdentityChoice::Exact {
        release_ref: crate::import::MetadataRef {
            id: "rel-1".to_string(),
            source: MetadataSource::MusicBrainz,
        },
    }
}

/// The editor seed must carry the same per-side track numbering the
/// commit-side mappers assign (A1,A2 -> 1,2 ; B1 -> 1), not a release-global
/// 1..N index. `apply_user_edit_to_seed` writes `track_number` verbatim onto
/// the seed, so a flat index would overwrite the mapper's correct per-side
/// numbers — corrupting any multi-side vinyl/cassette/multi-disc release
/// edited via the Exact/Approximate confirmation pane.
#[test]
fn shape_user_edit_numbers_tracks_per_side() {
    let edit = shape_user_edit_from_search_detail(&vinyl_detail(), &exact_choice());
    let numbers: Vec<Option<i32>> = edit.tracks.iter().map(|t| t.track_number).collect();
    assert_eq!(numbers, vec![Some(1), Some(2), Some(1)]);
    let sides: Vec<i32> = edit.tracks.iter().map(|t| t.side).collect();
    assert_eq!(sides, vec![1, 1, 2]);
}

/// Exact seeds the pressing fields from the picked release; Approximate and
/// Unknown blank them — the user didn't claim a specific pressing.
#[test]
fn shape_user_edit_pressing_follows_identity_choice() {
    let exact = shape_user_edit_from_search_detail(&vinyl_detail(), &exact_choice());
    assert_eq!(exact.pressing.year, Some(1969));
    assert_eq!(exact.pressing.label.as_deref(), Some("Label"));
    assert_eq!(exact.pressing.country.as_deref(), Some("US"));

    let blank = crate::import::PressingEdit::blank();
    let approx = shape_user_edit_from_search_detail(
        &vinyl_detail(),
        &crate::import::IdentityChoice::Approximate {
            release_ref: crate::import::MetadataRef {
                id: "rel-1".to_string(),
                source: MetadataSource::MusicBrainz,
            },
        },
    );
    assert_eq!(approx.pressing.year, blank.year);
    assert_eq!(approx.pressing.label, blank.label);
    assert_eq!(approx.pressing.country, blank.country);

    let unknown = shape_user_edit_from_search_detail(
        &vinyl_detail(),
        &crate::import::IdentityChoice::Unknown,
    );
    assert_eq!(unknown.pressing.year, blank.year);
    assert_eq!(unknown.pressing.label, blank.label);
}

/// A track whose source artist matches the album artist (or is missing)
/// seeds an empty per-track override (the editor's "share the album artist"
/// convention); a differing artist seeds that name verbatim.
#[test]
fn shape_user_edit_per_track_artist_override() {
    let mut detail = vinyl_detail();
    detail.tracks = vec![
        detail_track("same", 1, "A1", Some("Artist Name")),
        detail_track("none", 1, "A2", None),
        detail_track("diff", 2, "B1", Some("Guest Artist")),
    ];
    let edit = shape_user_edit_from_search_detail(&detail, &exact_choice());
    assert_eq!(edit.tracks[0].artist_names, Vec::<String>::new());
    assert_eq!(edit.tracks[1].artist_names, Vec::<String>::new());
    assert_eq!(
        edit.tracks[2].artist_names,
        vec!["Guest Artist".to_string()]
    );
}
