#![cfg(feature = "test-utils")]
mod support;

use crate::support::{test_config_and_keys, tracing_init};
use bae_core::db::{
    Database, DbAlbum, DbAlbumArtist, DbRelease, DbTrack, Pressing, ReleaseMetadataSource,
};
use bae_core::library::{LibraryEvent, LibraryManager};
use bae_core::library_dir::LibraryDir;
use chrono::Utc;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

async fn setup() -> (LibraryManager, Database, TempDir) {
    tracing_init();
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .expect("Failed to create database");
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        library_dir,
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
        None,
    );
    (library_manager, database, temp_dir)
}

fn make_artist(name: &str) -> bae_core::db::DbArtist {
    bae_core::db::DbArtist {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    }
}

fn make_album(artist_id: &str) -> DbAlbum {
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Test Album".to_string(),
        artist_id: artist_id.to_string(),
        year: Some(2024),
        primary_release_id: None,
        is_compilation: false,
        created_at: Utc::now(),
    }
}

fn make_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: Pressing {
            year: Some(2024),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        managed: true,
        source_folder_name: None,
        created_at: Utc::now(),
    }
}

fn make_track(release_id: &str, track_number: i32) -> DbTrack {
    DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release_id.to_string(),
        title: format!("Track {}", track_number),
        side: 1,
        track_number: Some(track_number),
        duration_ms: Some(180000),
        discogs_position: None,
        created_at: Utc::now(),
    }
}

/// Collect all events from a receiver with a timeout.
async fn collect_events(
    rx: &mut tokio::sync::broadcast::Receiver<LibraryEvent>,
    timeout: Duration,
) -> Vec<LibraryEvent> {
    let mut events = Vec::new();
    while let Ok(Ok(event)) = tokio::time::timeout(timeout, rx.recv()).await {
        events.push(event);
    }
    events
}

/// Insert an album with release and tracks, then emit events.
async fn insert_album_and_emit(
    lm: &LibraryManager,
    db: &Database,
    artist: &bae_core::db::DbArtist,
    album: &DbAlbum,
    release: &DbRelease,
    tracks: &[DbTrack],
) {
    db.insert_artist(artist).await.unwrap();
    db.insert_album(album).await.unwrap();
    db.insert_release(release).await.unwrap();
    for track in tracks {
        db.insert_track(track).await.unwrap();
    }
    // Insert album_artists junction
    let aa = DbAlbumArtist {
        id: Uuid::new_v4().to_string(),
        album_id: album.id.clone(),
        artist_id: artist.id.clone(),
        position: 0,
        created_at: Utc::now(),
    };
    db.insert_album_artist(&aa).await.unwrap();

    lm.emit_album_added(&album.id).await;
}

#[tokio::test]
async fn test_insert_album_emits_album_added() {
    let (lm, db, _tmp) = setup().await;
    let mut rx = lm.subscribe_events();

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release = make_release(&album.id);
    let track = make_track(&release.id, 1);

    insert_album_and_emit(&lm, &db, &artist, &album, &release, &[track]).await;

    let events = collect_events(&mut rx, Duration::from_millis(100)).await;

    let album_added_count = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::AlbumAdded { .. }))
        .count();

    assert_eq!(
        album_added_count, 1,
        "expected exactly one AlbumAdded event"
    );

    // Verify the AlbumAdded payload contains the release
    if let Some(LibraryEvent::AlbumAdded { album: detail }) = events
        .iter()
        .find(|e| matches!(e, LibraryEvent::AlbumAdded { .. }))
    {
        assert_eq!(detail.album.id, album.id);
        assert_eq!(detail.releases.len(), 1);
    }

    // No track-level events
    let track_events = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::TracksDeleted { .. }))
        .count();
    assert_eq!(track_events, 0);
}

#[tokio::test]
async fn test_insert_second_release_emits_release_added() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release1 = make_release(&album.id);
    let track1 = make_track(&release1.id, 1);

    // First: insert album with first release (don't subscribe yet)
    insert_album_and_emit(&lm, &db, &artist, &album, &release1, &[track1]).await;

    // Now subscribe and add a second release
    let mut rx = lm.subscribe_events();

    let release2 = make_release(&album.id);
    let track2 = make_track(&release2.id, 1);
    db.insert_release(&release2).await.unwrap();
    db.insert_track(&track2).await.unwrap();

    lm.emit_release_added(&album.id, &release2.id).await;

    let events = collect_events(&mut rx, Duration::from_millis(100)).await;

    let release_added = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::ReleaseAdded { .. }))
        .count();
    let album_added = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::AlbumAdded { .. }))
        .count();

    assert_eq!(release_added, 1, "expected ReleaseAdded");
    assert_eq!(album_added, 0, "no AlbumAdded for second release");
}

#[tokio::test]
async fn test_delete_release_multi_release_emits_release_removed() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release1 = make_release(&album.id);
    let release2 = make_release(&album.id);
    let track1 = make_track(&release1.id, 1);
    let track2 = make_track(&release2.id, 1);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release1).await.unwrap();
    db.insert_release(&release2).await.unwrap();
    db.insert_track(&track1).await.unwrap();
    db.insert_track(&track2).await.unwrap();

    let mut rx = lm.subscribe_events();
    lm.delete_release(&release1.id).await.unwrap();

    let events = collect_events(&mut rx, Duration::from_millis(100)).await;

    let release_removed = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::ReleaseRemoved { .. }))
        .count();
    let album_removed = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::AlbumRemoved { .. }))
        .count();

    assert_eq!(release_removed, 1, "expected ReleaseRemoved");
    assert_eq!(album_removed, 0, "album still has releases");
}

#[tokio::test]
async fn test_delete_primary_release_emits_album_updated_with_new_primary() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release1 = make_release(&album.id);
    let release2 = make_release(&album.id);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release1).await.unwrap();
    db.insert_release(&release2).await.unwrap();

    lm.set_album_primary_release(&album.id, &release1.id)
        .await
        .unwrap();

    let mut rx = lm.subscribe_events();
    lm.delete_release(&release1.id).await.unwrap();

    let events = collect_events(&mut rx, Duration::from_millis(100)).await;

    let target_album_id = album.id.clone();
    let updated_primaries: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            LibraryEvent::AlbumUpdated { album } if album.album.id == target_album_id => {
                Some(album.primary_release_id.clone())
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        updated_primaries.len(),
        1,
        "expected one AlbumUpdated event after deleting primary release",
    );
    assert_eq!(
        updated_primaries[0], release2.id,
        "AlbumUpdated payload must carry the surviving release as primary",
    );
}

#[tokio::test]
async fn test_delete_last_release_emits_album_removed() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release = make_release(&album.id);
    let track = make_track(&release.id, 1);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&track).await.unwrap();

    let mut rx = lm.subscribe_events();
    lm.delete_release(&release.id).await.unwrap();

    let events = collect_events(&mut rx, Duration::from_millis(100)).await;

    let removed: Vec<&LibraryEvent> = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::AlbumRemoved { .. }))
        .collect();
    let release_removed = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::ReleaseRemoved { .. }))
        .count();

    assert_eq!(removed.len(), 1, "expected AlbumRemoved");
    assert_eq!(release_removed, 0, "no ReleaseRemoved when album is gone");
    match removed[0] {
        LibraryEvent::AlbumRemoved { release_ids, .. } => {
            assert_eq!(
                release_ids,
                std::slice::from_ref(&release.id),
                "AlbumRemoved must carry the album's last release"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn test_delete_album_emits_album_removed() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release = make_release(&album.id);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();

    let mut rx = lm.subscribe_events();
    lm.delete_album(&album.id).await.unwrap();

    let events = collect_events(&mut rx, Duration::from_millis(100)).await;

    let removed: Vec<&LibraryEvent> = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::AlbumRemoved { .. }))
        .collect();

    assert_eq!(removed.len(), 1, "expected AlbumRemoved");
    match removed[0] {
        LibraryEvent::AlbumRemoved { release_ids, .. } => {
            assert_eq!(
                release_ids,
                std::slice::from_ref(&release.id),
                "AlbumRemoved must carry the deleted album's releases"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn test_set_cover_release_emits_album_updated() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release = make_release(&album.id);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();

    let mut rx = lm.subscribe_events();
    lm.set_album_primary_release(&album.id, &release.id)
        .await
        .unwrap();

    let events = collect_events(&mut rx, Duration::from_millis(100)).await;

    let album_updated = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::AlbumUpdated { .. }))
        .count();
    assert_eq!(album_updated, 1, "expected AlbumUpdated");
}

#[tokio::test]
async fn test_user_edit_overwrites_album_release_track_metadata() {
    use bae_core::import::{PressingEdit, ReleaseUserEdit, TrackUserEdit};

    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id);
    let release = make_release(&album.id);
    let track1 = make_track(&release.id, 1);
    let track2 = make_track(&release.id, 2);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&track1).await.unwrap();
    db.insert_track(&track2).await.unwrap();

    let mut rx = lm.subscribe_events();

    let edit = ReleaseUserEdit {
        album_title: "Edited Album".to_string(),
        album_artist_names: vec!["Edited Artist".to_string()],
        pressing: PressingEdit {
            year: Some(1999),
            format: Some("Vinyl".to_string()),
            label: Some("Edited Label".to_string()),
            catalog_number: Some("CAT-001".to_string()),
            country: Some("US".to_string()),
            barcode: Some("01234567".to_string()),
        },
        tracks: vec![
            TrackUserEdit {
                title: "Edited Track 1".to_string(),
                side: 1,
                track_number: Some(1),
                artist_names: vec![],
            },
            TrackUserEdit {
                title: "Edited Track 2".to_string(),
                side: 2,
                track_number: Some(2),
                artist_names: vec!["Guest Artist".to_string()],
            },
        ],
    };

    lm.apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .unwrap();

    let events = collect_events(&mut rx, Duration::from_millis(100)).await;
    let album_updated = events
        .iter()
        .filter(|e| matches!(e, LibraryEvent::AlbumUpdated { .. }))
        .count();
    assert!(album_updated >= 1, "expected at least one AlbumUpdated");

    let saved_album = db.find_album_by_id(&album.id).await.unwrap().unwrap();
    assert_eq!(saved_album.title, "Edited Album");

    let saved_release = db.find_release_by_id(&release.id).await.unwrap().unwrap();
    assert_eq!(saved_release.pressing.year, Some(1999));
    assert_eq!(saved_release.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(
        saved_release.pressing.label.as_deref(),
        Some("Edited Label")
    );
    assert_eq!(
        saved_release.pressing.catalog_number.as_deref(),
        Some("CAT-001")
    );
    assert_eq!(saved_release.pressing.country.as_deref(), Some("US"));
    assert_eq!(saved_release.pressing.barcode.as_deref(), Some("01234567"));
    // Identity-source columns must NOT be touched.
    assert_eq!(
        saved_release.metadata_source,
        ReleaseMetadataSource::FileTags,
    );
    assert!(saved_release.metadata_source_release_id.is_none());

    let saved_tracks = db.get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(saved_tracks.len(), 2);
    assert_eq!(saved_tracks[0].title, "Edited Track 1");
    assert_eq!(saved_tracks[0].side, 1);
    assert_eq!(saved_tracks[1].title, "Edited Track 2");
    assert_eq!(saved_tracks[1].side, 2);

    // The original artist row stays in place; a new "Edited Artist" row was
    // inserted, and the album points at it.
    let edited_artist = db
        .get_artist_by_name("Edited Artist")
        .await
        .unwrap()
        .expect("Edited Artist row missing");
    assert_eq!(saved_album.artist_id, edited_artist.id);

    // Per-track artist landed for the second track only.
    let guest = db
        .get_artist_by_name("Guest Artist")
        .await
        .unwrap()
        .expect("Guest Artist row missing");
    let track2_artists = db.get_artists_for_track(&saved_tracks[1].id).await.unwrap();
    assert_eq!(track2_artists.len(), 1);
    assert_eq!(track2_artists[0].id, guest.id);
    let track1_artists = db.get_artists_for_track(&saved_tracks[0].id).await.unwrap();
    assert!(track1_artists.is_empty());
}

#[tokio::test]
async fn test_user_edit_multi_artist_round_trip() {
    use bae_core::import::{PressingEdit, ReleaseUserEdit, TrackUserEdit};

    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id);
    let release = make_release(&album.id);
    let track1 = make_track(&release.id, 1);
    let track2 = make_track(&release.id, 2);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&track1).await.unwrap();
    db.insert_track(&track2).await.unwrap();

    let edit = ReleaseUserEdit {
        album_title: "Multi Album".to_string(),
        album_artist_names: vec![
            "Artist A".to_string(),
            "Artist B".to_string(),
            "Artist C".to_string(),
        ],
        pressing: PressingEdit::blank(),
        tracks: vec![
            TrackUserEdit {
                title: "T1".to_string(),
                side: 1,
                track_number: Some(1),
                artist_names: vec![],
            },
            TrackUserEdit {
                title: "T2".to_string(),
                side: 1,
                track_number: Some(2),
                artist_names: vec!["Guest X".to_string(), "Guest Y".to_string()],
            },
        ],
    };

    lm.apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .unwrap();

    // Album-level: three distinct artists in the supplied positional order.
    // Each name became its own artists row; none collapsed to a single
    // "Artist A, Artist B, Artist C" row.
    let album_artists = db.get_artists_for_album(&album.id).await.unwrap();
    assert_eq!(album_artists.len(), 3, "expected 3 album artists");
    assert_eq!(album_artists[0].name, "Artist A");
    assert_eq!(album_artists[1].name, "Artist B");
    assert_eq!(album_artists[2].name, "Artist C");

    // Track 1: no override, no per-track artist rows.
    let saved_tracks = db.get_tracks_for_release(&release.id).await.unwrap();
    let t1_artists = db.get_artists_for_track(&saved_tracks[0].id).await.unwrap();
    assert!(t1_artists.is_empty(), "track 1 should have no override");

    // Track 2: two distinct artists, in supplied order. The literal joined
    // string "Guest X, Guest Y" must NOT have become a single artist row.
    let t2_artists = db.get_artists_for_track(&saved_tracks[1].id).await.unwrap();
    assert_eq!(t2_artists.len(), 2, "expected 2 track artists");
    assert_eq!(t2_artists[0].name, "Guest X");
    assert_eq!(t2_artists[1].name, "Guest Y");

    // Each unique name should have produced exactly one artists row (no
    // duplicates from the resolver). Including the original "Original Artist"
    // pre-existing row, that's 6 total: Original + A + B + C + Guest X + Guest Y.
    let combined_name = "Artist A, Artist B, Artist C";
    let leaked = db.get_artist_by_name(combined_name).await.unwrap();
    assert!(
        leaked.is_none(),
        "literal joined string should not exist as an artist row"
    );
}

#[tokio::test]
async fn test_user_edit_empty_album_artists_errors() {
    use bae_core::import::{PressingEdit, ReleaseUserEdit};

    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release = make_release(&album.id);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();

    let edit = ReleaseUserEdit {
        album_title: "x".to_string(),
        album_artist_names: vec![],
        pressing: PressingEdit::blank(),
        tracks: vec![],
    };

    let err = lm
        .apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .expect_err("expected validation error");
    assert!(err.to_string().contains("at least one artist"));
}

#[tokio::test]
async fn test_user_edit_track_count_mismatch_errors() {
    use bae_core::import::{PressingEdit, ReleaseUserEdit};

    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Test Artist");
    let album = make_album(&artist.id);
    let release = make_release(&album.id);
    let track = make_track(&release.id, 1);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&track).await.unwrap();

    let edit = ReleaseUserEdit {
        album_title: "x".to_string(),
        album_artist_names: vec!["y".to_string()],
        pressing: PressingEdit::blank(),
        tracks: vec![],
    };

    let err = lm
        .apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .expect_err("expected mismatch error");
    let msg = err.to_string();
    assert!(
        msg.contains("Track count mismatch"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn test_sync_changeset_walker_dedupes_per_album() {
    use bae_core::library::sync_events::changes_from_row_changes;
    use coven::changeset::{ChangeOp, RowChange};

    // Column layouts match the synced schema: albums col0=id; releases col0=id,
    // col1=album_id; tracks col0=id, col1=release_id.
    fn insert(table: &str, cols: &[&str]) -> RowChange {
        RowChange {
            table: table.to_string(),
            op: ChangeOp::Insert,
            columns: cols.iter().map(|c| Some(c.to_string())).collect(),
        }
    }

    // An album plus its release and two tracks, inserted together. The album
    // event subsumes the release and track changes: one album event, no release
    // events.
    let changes = changes_from_row_changes(&[
        insert("albums", &["alb-1"]),
        insert("releases", &["rel-1", "alb-1"]),
        insert("tracks", &["trk-1", "rel-1"]),
        insert("tracks", &["trk-2", "rel-1"]),
    ]);

    assert_eq!(
        changes.album_events.len(),
        1,
        "expected one deduped album event, got {}",
        changes.album_events.len()
    );
    assert_eq!(changes.release_events.len(), 0);
}

#[test]
fn test_sync_changeset_album_delete_carries_child_release_ids() {
    use bae_core::library::sync_events::{changes_from_row_changes, AlbumChangeEvent};
    use coven::changeset::{ChangeOp, RowChange};

    fn delete(table: &str, cols: &[&str]) -> RowChange {
        RowChange {
            table: table.to_string(),
            op: ChangeOp::Delete,
            columns: cols.iter().map(|c| Some(c.to_string())).collect(),
        }
    }

    // Deleting an album cascades to its releases. The per-release deletes are
    // suppressed (album-level trumps release-level), but their ids must ride
    // along on the album removal so consumers can drop the children without
    // re-deriving the set from their own state.
    let changes = changes_from_row_changes(&[
        delete("albums", &["alb-1"]),
        delete("releases", &["rel-1", "alb-1"]),
        delete("releases", &["rel-2", "alb-1"]),
    ]);

    assert_eq!(changes.album_events.len(), 1);
    assert_eq!(changes.release_events.len(), 0);
    match &changes.album_events[0] {
        AlbumChangeEvent::Removed {
            album_id,
            release_ids,
        } => {
            assert_eq!(album_id, "alb-1");
            let mut got = release_ids.clone();
            got.sort();
            assert_eq!(got, vec!["rel-1".to_string(), "rel-2".to_string()]);
        }
        other => panic!("expected AlbumChangeEvent::Removed, got {other:?}"),
    }
}
