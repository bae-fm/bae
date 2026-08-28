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
async fn conflicting_exact_source_ids_fail_instead_of_choosing_an_artist() {
    let (manager, _tmp) = setup_test_manager().await;
    let discogs_artist = make_artist("Artist One", Some("d123"), None);
    let musicbrainz_artist = make_artist("Artist Two", None, Some("mb-abc"));
    manager.insert_artist(&discogs_artist).await.unwrap();
    manager.insert_artist(&musicbrainz_artist).await.unwrap();

    let incoming = make_artist("Artist Three", Some("d123"), Some("mb-abc"));
    let error = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .expect_err("source IDs belonging to different artists must fail");

    assert!(
        error
            .to_string()
            .contains("source IDs belonging to different library artists"),
        "the conflict names the violated identity invariant: {error}",
    );
    assert!(manager.get_artist_by_id(&incoming.id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_same_name_without_ids_creates_a_distinct_artist() {
    let (manager, _tmp) = setup_test_manager().await;
    let existing = make_artist("Artist One", None, None);
    manager.insert_artist(&existing).await.unwrap();

    let incoming = make_artist("Artist One", None, None);
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], incoming.id);
    assert_ne!(resolved[0], existing.id);
}

#[tokio::test]
#[serial(discogs_rate_limiter)]
async fn fetch_artist_images_warns_and_skips_when_existing_image_check_fails() {
    let (manager, _tmp) = setup_test_manager().await;
    manager
        .set_discogs_key("test-token", crate::config::DiscogsValidation::Valid)
        .unwrap();
    // Renamed rather than dropped: `artist_images` carries a blob, so coven
    // owns a cleanup-guard trigger on it and refuses host SQL that would take
    // the trigger with the table. The rename leaves the lookup with no
    // `artist_images` to read, which is the failure this exercises.
    manager.rename_artist_images_table_for_test().await.unwrap();

    let parsed_artist = make_artist("Artist Name", Some("discogs-artist-1"), None);
    let actual_artist_id = ARTIST_ACTUAL_1.to_string();
    let artist_id_map = HashMap::from([(parsed_artist.id.clone(), actual_artist_id.clone())]);
    let logs = capture_warn_logs_async(|| async {
        manager
            .fetch_discogs_artist_images(std::slice::from_ref(&parsed_artist), &artist_id_map)
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
async fn test_disjoint_source_ids_do_not_merge_by_name() {
    let (manager, _tmp) = setup_test_manager().await;
    // Existing has discogs ID only
    let existing = make_artist("Artist One", Some("d456"), None);
    manager.insert_artist(&existing).await.unwrap();

    // A matching display name is not identity. Only an explicit existing
    // assignment or a shared provider ID may join these rows.
    let incoming = make_artist("Artist One", None, Some("mb-xyz"));
    let resolved = manager
        .find_or_create_artists(std::slice::from_ref(&incoming))
        .await
        .unwrap();

    assert_eq!(resolved[0], incoming.id);
    assert_ne!(resolved[0], existing.id);

    let unchanged = manager
        .get_artist_by_id(&existing.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.discogs_artist_id.as_deref(), Some("d456"));
    assert_eq!(unchanged.musicbrainz_artist_id, None);

    let inserted = manager
        .get_artist_by_id(&incoming.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inserted.discogs_artist_id, None);
    assert_eq!(inserted.musicbrainz_artist_id.as_deref(), Some("mb-xyz"));
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
        id: "e36744a5-1a36-460f-891c-e7e558034edf".to_string(),
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
        artist_id: "e36744a5-1a36-460f-891c-e7e558034edf".to_string(),
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
        metadata_provenance: Some(crate::import::MetadataProvenance::FileTags),
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
        source_release_id: release.to_string(),
    }
}

fn discogs_identity(master: &str, release: &str) -> ReleaseIdentity {
    ReleaseIdentity {
        source: MetadataSource::Discogs,
        source_group_id: master.to_string(),
        source_release_id: release.to_string(),
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
        .insert_album_with_release_and_tracks(album, release, &[track], &[])
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

    // Empty identity vec (File Tags or direct entry) → skip lookup.
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
async fn test_file_tags_import_skips_lookup() {
    // File Tags imports never deduplicate against existing releases —
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
        .insert_release_with_tracks(&release2, &[track2], &[])
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
async fn test_check_album_in_library_other_pressing_of_the_group() {
    // The library holds a different pressing of the same release group. A
    // candidate naming that group is in the library as an album, and not as a
    // pressing.
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let release = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &release,
        &[mb_identity("mb-rg-1", "mb-rel-other")],
    )
    .await;

    let checks = vec![crate::db::LibraryCheck {
        release_id: "mb-rel-1".to_string(),
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
async fn test_check_pressing_match_wins_over_sibling_pressing_row() {
    // Two releases in the group: one at a different pressing, one at the
    // pressing the check names. The pressing match is the row that answers.
    let (manager, _tmp) = setup_test_db_with_artist().await;
    let album = make_album("Album Title");
    let sibling = make_release(&album.id);
    insert_with_identities(
        &manager,
        &album,
        &sibling,
        &[mb_identity("mb-rg-1", "mb-rel-other")],
    )
    .await;
    let named = make_release(&album.id);
    let track = make_track(&named.id, 1);
    manager
        .insert_release_with_tracks(&named, &[track], &[])
        .await
        .unwrap();
    manager
        .insert_release_identities(&named.id, &[mb_identity("mb-rg-1", "mb-rel-1")])
        .await
        .unwrap();

    let checks = vec![crate::db::LibraryCheck {
        release_id: "mb-rel-1".to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("mb-rg-1".to_string()),
    }];
    let statuses = manager.check_releases_in_library(&checks).await.unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(statuses[0].release_in_library);
    assert!(statuses[0].album_in_library);
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
