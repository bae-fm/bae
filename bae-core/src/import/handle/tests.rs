use super::*;
use crate::db::{Database, DbArtist};
use crate::test_logs::capture_warn_logs_async;
use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

fn test_config_and_keys(
    library_dir: &coven::StoreDir,
) -> (
    std::sync::Arc<crate::config::ConfigHandle>,
    crate::keys::StoreKeys,
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
        crate::keys::StoreKeys::new(library_id),
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
    let library_dir = coven::StoreDir::new(temp_dir.path());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let manager = LibraryManager::new(
        database,
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
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
            sql.tx().execute("DROP TABLE artist_images", [])?;
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
        matches!(&err, crate::import::ImportError::AlreadyInLibrary { album_title } if album_title == "Album Title"),
        "Expected duplicate error naming the album, got: {err}",
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

#[test]
fn remap_artist_links_rewrites_id_and_preserves_the_rest() {
    let ta = track_artist("parsed-1");
    let map = std::collections::HashMap::from([("parsed-1".to_string(), "db-1".to_string())]);

    let remapped = remap_artist_links(
        std::slice::from_ref(&ta),
        &map,
        "track artist",
        |ta| &ta.artist_id,
        |ta, artist_id| ta.artist_id = artist_id,
    )
    .unwrap();
    assert_eq!(remapped.len(), 1);
    assert_eq!(remapped[0].artist_id, "db-1");
    // Everything other than the remapped artist id carries through.
    assert_eq!(remapped[0].id, ta.id);
    assert_eq!(remapped[0].track_id, "track-1");
    assert_eq!(remapped[0].position, 3);
}

#[test]
fn remap_artist_links_errors_on_unmapped_id() {
    let ta = track_artist("orphan-track-artist");
    let err = remap_artist_links(
        std::slice::from_ref(&ta),
        &std::collections::HashMap::new(),
        "track artist",
        |ta| &ta.artist_id,
        |ta, artist_id| ta.artist_id = artist_id,
    )
    .unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::Internal { detail } if detail.contains("orphan-track-artist")),
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

// ── ImportCandidateState reducer ────────────────────────────────────

use crate::import::folder_scanner::{AudioContent, CategorizedFiles, InvalidReason};
use crate::import::types::{ImportPhase, ImportProgress};
use std::path::{Path, PathBuf};

/// An empty file set — the reducer only reads a candidate's identity and
/// grouping fields, never its files, so the audio content can be blank.
fn empty_categorized() -> CategorizedFiles {
    CategorizedFiles {
        audio: AudioContent::TrackFiles {
            tracks: Vec::new(),
            format_label: "FLAC".to_string(),
        },
        artwork: Vec::new(),
        documents: Vec::new(),
        unpaired_cue_sheets: Vec::new(),
    }
}

fn folder_candidate(path: &str, watched: &str) -> FolderCandidate {
    FolderCandidate {
        path: PathBuf::from(path),
        name: format!("Candidate {path}"),
        files: empty_categorized(),
        watched_folder_path: watched.to_string(),
        skipped: false,
        is_added: false,
    }
}

fn invalid_candidate(path: &str, watched: &str) -> InvalidCandidate {
    InvalidCandidate {
        path: PathBuf::from(path),
        name: format!("Invalid {path}"),
        watched_folder_path: watched.to_string(),
        reason: InvalidReason::NoValidAudio,
    }
}

fn watched(path: &str) -> WatchedFolder {
    WatchedFolder {
        path: path.to_string(),
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
    }
}

#[test]
fn replace_root_populates_snapshot_with_folder_and_invalid_candidates() {
    let mut state = ImportCandidateState::default();
    state.replace_root(
        Path::new("/watch/a"),
        vec![folder_candidate("/watch/a/rel1", "/watch/a")],
        vec![invalid_candidate("/watch/a/bad", "/watch/a")],
    );

    let snapshot = state.snapshot(vec![watched("/watch/a")]);
    assert_eq!(snapshot.folder_candidates.len(), 1);
    assert_eq!(
        snapshot.folder_candidates[0].candidate.path,
        PathBuf::from("/watch/a/rel1")
    );
    assert!(!snapshot.folder_candidates[0].candidate.skipped);
    assert_eq!(snapshot.invalid_candidates.len(), 1);
    assert_eq!(
        snapshot.invalid_candidates[0].path,
        PathBuf::from("/watch/a/bad")
    );
}

#[test]
fn replace_root_supersedes_and_remove_root_clears() {
    let mut state = ImportCandidateState::default();
    let root = Path::new("/watch/a");
    state.replace_root(
        root,
        vec![folder_candidate("/watch/a/old", "/watch/a")],
        Vec::new(),
    );
    // A second replace_root for the same root drops the prior candidates.
    state.replace_root(
        root,
        vec![folder_candidate("/watch/a/new", "/watch/a")],
        Vec::new(),
    );
    let snapshot = state.snapshot(vec![watched("/watch/a")]);
    assert_eq!(snapshot.folder_candidates.len(), 1);
    assert_eq!(
        snapshot.folder_candidates[0].candidate.path,
        PathBuf::from("/watch/a/new")
    );

    state.remove_root(root);
    let snapshot = state.snapshot(vec![watched("/watch/a")]);
    assert!(snapshot.folder_candidates.is_empty());
    assert!(snapshot.invalid_candidates.is_empty());
}

#[test]
fn set_skipped_round_trips_on_folder_candidate() {
    let mut state = ImportCandidateState::default();
    state.replace_root(
        Path::new("/watch/a"),
        vec![folder_candidate("/watch/a/rel1", "/watch/a")],
        Vec::new(),
    );

    state.set_skipped("/watch/a/rel1", true);
    assert!(
        state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
            .candidate
            .skipped
    );

    state.set_skipped("/watch/a/rel1", false);
    assert!(
        !state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
            .candidate
            .skipped
    );
}

#[test]
fn record_event_overlays_import_progress_onto_folder_runtime() {
    let mut state = ImportCandidateState::default();
    state.replace_root(
        Path::new("/watch/a"),
        vec![folder_candidate("/watch/a/rel1", "/watch/a")],
        Vec::new(),
    );

    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "/watch/a/rel1".to_string(),
        progress: ImportProgress::Progress {
            id: "rel1".to_string(),
            percent: 42,
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    });

    // The snapshot joins the runtime map at read time, so it reflects the
    // recorded progress.
    let running = state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
        .runtime
        .import_status
        .clone()
        .expect("import status overlaid");
    match running {
        CandidateImportStatusSnapshot::Importing {
            progress_percent,
            step,
        } => {
            assert_eq!(progress_percent, 42);
            assert_eq!(
                step,
                Some(ImportStep::Running(ImportPhase::MeasuringLoudness))
            );
        }
        other => panic!("expected Importing, got {other:?}"),
    }

    // A later Complete event replaces the overlay with the terminal snapshot.
    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "/watch/a/rel1".to_string(),
        progress: ImportProgress::Complete {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb-1".to_string(),
        },
    });
    let complete = state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
        .runtime
        .import_status
        .clone()
        .expect("import status overlaid");
    match complete {
        CandidateImportStatusSnapshot::Complete {
            release_id,
            album_id,
        } => {
            assert_eq!(release_id, "rel1");
            assert_eq!(album_id, "alb-1");
        }
        other => panic!("expected Complete, got {other:?}"),
    }
}

#[test]
fn record_event_overlays_identify_state() {
    let mut state = ImportCandidateState::default();
    state.replace_root(
        Path::new("/watch/a"),
        vec![folder_candidate("/watch/a/rel1", "/watch/a")],
        Vec::new(),
    );

    state.record_event(&ImportEvent::IdentifyStateChanged {
        candidate_key: "/watch/a/rel1".to_string(),
        state: crate::identify::IdentifyState::Idle,
        toolbar: Vec::new(),
    });

    let runtime = &state.snapshot(vec![watched("/watch/a")]).folder_candidates[0].runtime;
    assert!(matches!(
        runtime.identify_state,
        crate::identify::IdentifyState::Idle
    ));
}

#[test]
fn snapshot_orders_by_watched_folder_then_key() {
    let mut state = ImportCandidateState::default();
    state.replace_root(
        Path::new("/watch/b"),
        vec![
            folder_candidate("/watch/b/z", "/watch/b"),
            folder_candidate("/watch/b/a", "/watch/b"),
        ],
        Vec::new(),
    );
    state.replace_root(
        Path::new("/watch/a"),
        vec![folder_candidate("/watch/a/m", "/watch/a")],
        Vec::new(),
    );

    // Watched-folder order (a before b) is the primary sort; the candidate key
    // breaks ties within a folder (a before z).
    let snapshot = state.snapshot(vec![watched("/watch/a"), watched("/watch/b")]);
    let paths: Vec<String> = snapshot
        .folder_candidates
        .iter()
        .map(|c| c.candidate.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(paths, vec!["/watch/a/m", "/watch/b/a", "/watch/b/z"]);
}

#[test]
fn get_resolves_reidentify_runtime_and_scanned_candidates() {
    let mut state = ImportCandidateState::default();

    // A `reidentify:` key has no scanned candidate — only runtime, created by
    // recording an event against it.
    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "reidentify:rel-1".to_string(),
        progress: ImportProgress::Started {
            id: "rel-1".to_string(),
            import_id: "imp-1".to_string(),
        },
    });
    match state.get("reidentify:rel-1") {
        Some(ImportCandidateSnapshot::Runtime { key, runtime }) => {
            assert_eq!(key, "reidentify:rel-1");
            assert!(matches!(
                runtime.import_status,
                Some(CandidateImportStatusSnapshot::Importing { .. })
            ));
        }
        other => panic!("expected runtime snapshot, got {other:?}"),
    }

    // A scanned folder key resolves to its folder candidate; an unknown key is
    // None.
    state.replace_root(
        Path::new("/watch/a"),
        vec![folder_candidate("/watch/a/rel1", "/watch/a")],
        Vec::new(),
    );
    assert!(matches!(
        state.get("/watch/a/rel1"),
        Some(ImportCandidateSnapshot::Folder { .. })
    ));
    assert!(state.get("/watch/a/missing").is_none());
}

#[test]
fn runtime_recorded_before_scan_survives_replace_root() {
    let mut state = ImportCandidateState::default();

    // An event can arrive for a candidate key before the folder scan reports
    // the candidate. The runtime entry sits in the runtime map with no scanned
    // candidate yet.
    state.record_event(&ImportEvent::ImportProgress {
        candidate_key: "/watch/a/rel1".to_string(),
        progress: ImportProgress::Progress {
            id: "rel1".to_string(),
            percent: 42,
            phase: ImportPhase::MeasuringLoudness,
            import_id: "imp-1".to_string(),
        },
    });

    // The scan then reports the candidate. replace_root does not touch the
    // pre-existing runtime entry (remove_root only clears runtime for keys that
    // were already scanned candidates under the root).
    state.replace_root(
        Path::new("/watch/a"),
        vec![folder_candidate("/watch/a/rel1", "/watch/a")],
        Vec::new(),
    );

    // The read-time join surfaces the recorded runtime on the scanned candidate.
    let status = state.snapshot(vec![watched("/watch/a")]).folder_candidates[0]
        .runtime
        .import_status
        .clone()
        .expect("recorded import status joined onto the candidate");
    match status {
        CandidateImportStatusSnapshot::Importing {
            progress_percent,
            step,
        } => {
            assert_eq!(progress_percent, 42);
            assert_eq!(
                step,
                Some(ImportStep::Running(ImportPhase::MeasuringLoudness))
            );
        }
        other => panic!("expected Importing, got {other:?}"),
    }
}

#[test]
fn remove_root_returns_the_removed_candidate_keys() {
    let mut state = ImportCandidateState::default();
    state.replace_root(
        Path::new("/watch/a"),
        vec![folder_candidate("/watch/a/rel1", "/watch/a")],
        vec![invalid_candidate("/watch/a/bad", "/watch/a")],
    );
    state.replace_root(
        Path::new("/watch/b"),
        vec![folder_candidate("/watch/b/rel1", "/watch/b")],
        Vec::new(),
    );

    let mut removed = state.remove_root(Path::new("/watch/a"));
    removed.sort();
    assert_eq!(
        removed,
        vec!["/watch/a/bad".to_string(), "/watch/a/rel1".to_string()]
    );

    // A second removal of the same root finds nothing left.
    assert!(state.remove_root(Path::new("/watch/a")).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_watched_folder_cancels_in_flight_extraction() {
    use crate::signals::{
        ArtworkAnalysis, ArtworkAnalyzer, ExtractionService, ExtractionSource, TextSignal,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    // Local delayed OCR stub: counts `analyze` calls so the test can assert the
    // pass stopped early, and sleeps each call so a cancel lands mid-pass.
    struct DelayedAnalyzer {
        calls: AtomicUsize,
        delay: Duration,
    }
    impl ArtworkAnalyzer for DelayedAnalyzer {
        fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            ArtworkAnalysis {
                barcodes: Vec::new(),
                text_lines: vec!["Line".to_string()],
            }
        }
    }

    // Minimal on-disk release: one MP3 (satisfies the audio gate) and three
    // JPEGs for the OCR pass to iterate.
    fn minimal_mp3() -> Vec<u8> {
        let mut v = Vec::with_capacity(32);
        v.extend_from_slice(b"ID3");
        v.resize(32, 0);
        v
    }
    fn minimal_jpeg() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
    }

    let (manager, tmp) = setup_test_manager().await;
    let root = tmp.path().join("watch-root");
    std::fs::create_dir_all(&root).unwrap();
    let release_folder = root.join("Artist Name - Album Title");
    std::fs::create_dir_all(&release_folder).unwrap();
    std::fs::write(release_folder.join("01 - Track.mp3"), minimal_mp3()).unwrap();
    for img in ["p1.jpg", "p2.jpg", "p3.jpg"] {
        std::fs::write(release_folder.join(img), minimal_jpeg()).unwrap();
    }

    let import_handle = crate::import::ImportService::start(
        tokio::runtime::Handle::current(),
        manager.clone(),
        crate::import::cover_art::CoverArtArchiveClient::hermetic(),
    );
    let extraction = ExtractionService::start(
        tokio::runtime::Handle::current(),
        import_handle.event_sender_for_test(),
        manager.clone(),
    );
    let analyzer = std::sync::Arc::new(DelayedAnalyzer {
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(200),
    });
    extraction.register_analyzer(analyzer.clone());

    let mut events = import_handle.subscribe_events();
    import_handle
        .add_watched_folder(root.to_string_lossy().to_string())
        .unwrap();

    // Wait for the scan to surface the release as a candidate. Take the key
    // from the emitted candidate's path (robust to path canonicalization).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let candidate_path = loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let event = tokio::time::timeout(remaining, events.recv())
            .await
            .expect("timed out waiting for the folder candidate")
            .expect("event channel closed");
        if let ImportEvent::Scan(ScanEvent::FolderCandidate(candidate)) = event {
            break candidate.path;
        }
    };
    let key = candidate_path.to_string_lossy().to_string();

    // Start extraction the way the bridge does, then remove the folder mid-OCR.
    extraction.start(
        key.clone(),
        ExtractionSource::Folder(candidate_path.clone()),
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    import_handle
        .remove_watched_folder(root.to_string_lossy().to_string())
        .unwrap();

    // Drain this key's SignalsUpdated until quiet: the cancelled run never settles.
    loop {
        match tokio::time::timeout(Duration::from_millis(500), events.recv()).await {
            Ok(Ok(ImportEvent::SignalsUpdated {
                candidate_key,
                signals,
            })) if candidate_key == key => {
                assert!(
                    !matches!(signals.text, TextSignal::Settled { .. }),
                    "extraction for a removed folder must not settle, got {:?}",
                    signals.text,
                );
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }
    assert!(
        analyzer.calls.load(Ordering::SeqCst) < 3,
        "removing the folder must stop the OCR pass early",
    );
}
