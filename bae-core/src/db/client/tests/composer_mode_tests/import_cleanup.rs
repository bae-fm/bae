use super::super::*;
use super::*;
use coven::SystemClock;

/// A replacement with no blob/outbox cleanup — the released-in-place local
/// files these album-cleanup tests use carry no cloud state to tear down.
fn empty_cleanup_plan() -> DeleteCleanupPlan {
    DeleteCleanupPlan::default()
}

fn scanned_flac() -> crate::import::folder_scanner::ScannedAudio {
    crate::import::folder_scanner::ScannedAudio {
        content_type: ContentType::Flac,
        duration_ms: 1_000,
        format: crate::album_detail::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: Some(16),
            bitrate_kbps: None,
            channels: 2,
        },
    }
}

/// Shared arrange for the two reimport-replacement tests. Seeds `album-old` with
/// `existing_release_ids` and its `primary_release_id` at `replaced_release_id`,
/// then finalizes a reimport whose new release `rel-new` lands in the fresh
/// `album-new`, carrying an `ImportReplacementDelete` for `replaced_release_id`.
/// Returns the finalize outcomes, so each test asserts the album's fate itself.
async fn finalize_reimport_replacing_release(
    db: &Database,
    tmp: &tempfile::TempDir,
    now: chrono::DateTime<chrono::Utc>,
    existing_release_ids: &[&str],
    replaced_release_id: &str,
) -> Vec<ImportReplacementOutcome> {
    let artist = DbArtist {
        id: ARTIST_A.to_string(),
        name: "Artist Name A".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist).await.unwrap();

    let album_old = DbAlbum {
        id: ALBUM_OLD.to_string(),
        title: "Album Title Old".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2026),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    db.insert_album(&album_old).await.unwrap();
    for id in existing_release_ids {
        db.insert_release(&DbRelease::new_test(&album_old.id, id))
            .await
            .unwrap();
    }
    db.set_album_primary_release(&album_old.id, replaced_release_id)
        .await
        .unwrap();

    let album_new = DbAlbum {
        id: ALBUM_NEW.to_string(),
        title: "Album Title New".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2026),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release_new = DbRelease::new_test(&album_new.id, REL_NEW);
    let track = DbTrack {
        id: TRACK_NEW.to_string(),
        release_id: release_new.id.clone(),
        title: "Track Title New".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(1000),
        discogs_position: None,
        created_at: now,
    };
    let file = DbFile::new(
        &release_new.id,
        "Track Title New.flac",
        1024,
        ContentType::Flac,
        FILE_NEW.to_string(),
        now,
        crate::util::fs::hash_bytes(b"fixture"),
    );
    let track_files = vec![crate::import::TrackFile::Standalone {
        db_track: track,
        file_path: tmp.path().join("Track Title New.flac"),
        source_audio: scanned_flac(),
    }];

    let replacement = ImportReplacementDelete {
        release_id: replaced_release_id.to_string(),
        album_id: album_old.id.clone(),
        cleanup: empty_cleanup_plan(),
    };
    db.finalize_import_atomic(
        Some(&album_new),
        &release_new,
        &track_files,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[file],
        &[],
        &[],
        None,
        &[],
        None,
        &[],
        tmp.path().to_str().unwrap(),
        crate::config::HomeStorage::Opaque,
        &[replacement],
    )
    .await
    .unwrap()
}

async fn seeded_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
            VALUES
                ('85f70840-aba5-4eb9-8e1a-0d319e53b798', 'Album Artist A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('5412b7ad-bdc1-4561-8985-b6d6ef8a2880', 'Displayed Composer A', 'Hidden Composer Sort A', NULL, 'mb-artist-composer-a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '85f70840-aba5-4eb9-8e1a-0d319e53b798', 2026, '0252dedb-ee39-4547-8803-438dbeb57a64', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, release_name, year, disc_id, metadata_source, metadata_source_release_id, format, label, catalog_number, country, barcode, remote, source_folder_name, content_hash, album_loudness_lufs, album_peak_linear, _updated_at, created_at)
            VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', NULL, 2026, NULL, 'musicbrainz', 'mb-release-a', 'CD', NULL, NULL, NULL, NULL, 1, NULL, NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, discogs_position, _updated_at, created_at)
            VALUES ('0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO works (id, title, disambiguation, work_type, musicbrainz_work_id, _updated_at, created_at)
            VALUES
                ('6b05af7a-ee0c-4f12-8938-1d5536697271', 'Parent Work A', NULL, 'work', 'mb-work-parent-a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('f63d8e66-6a81-4a67-8005-1fbe870f27eb', 'Displayed Work A', NULL, 'part', 'mb-work-child-a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
            VALUES ('ec41a8cd-a9a4-473e-8b70-d78168aefd8e', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', '5412b7ad-bdc1-4561-8985-b6d6ef8a2880', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO work_parts (id, parent_work_id, child_work_id, position, source, _updated_at, created_at)
            VALUES ('fa383452-6671-4335-87bc-751a52bbdde5', '6b05af7a-ee0c-4f12-8938-1d5536697271', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO track_works (id, track_id, work_id, position, source, _updated_at, created_at)
            VALUES ('d410a973-6a19-4ad3-87d8-b0c8c13d6015', '0482872e-d4bf-4080-8426-441a0a3e71fc', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', 0, 'musicbrainz', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, tmp)
}

#[tokio::test]
async fn finalize_import_persists_composer_work_and_role_rows() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let album_artist = DbArtist {
        id: ARTIST_ALBUM.to_string(),
        name: "Album Artist A".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    let composer = DbArtist {
        id: ARTIST_COMPOSER.to_string(),
        name: "Composer Artist A".to_string(),
        sort_name: Some("Composer Artist A".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-artist-composer-a".to_string()),
        created_at: now,
    };
    db.insert_artist(&album_artist).await.unwrap();
    db.insert_artist(&composer).await.unwrap();

    let album = DbAlbum {
        id: ALBUM_A.to_string(),
        title: "Album Title A".to_string(),
        artist_id: album_artist.id.clone(),
        year: Some(2026),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: RELEASE_A.to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: Pressing {
            year: Some(2026),
            format: Some("CD".to_string()),
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-release-a".to_string(),
        }),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = DbTrack {
        id: TRACK_A.to_string(),
        release_id: release.id.clone(),
        title: "Track Title A".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(1000),
        discogs_position: None,
        created_at: now,
    };
    let track_files = vec![crate::import::TrackFile::Standalone {
        db_track: track,
        file_path: tmp.path().join("Track.flac"),
        source_audio: scanned_flac(),
    }];
    let works = vec![DbWork {
        id: WORK_A.to_string(),
        title: "Work Title A".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        musicbrainz_work_id: "mb-work-a".to_string(),
        created_at: now,
    }];
    let work_artists = vec![DbWorkArtist::new(
        WORK_A,
        &composer.id,
        0,
        crate::import::MetadataSource::MusicBrainz,
        WORK_ARTIST_A.to_string(),
        now,
    )];
    let track_works = vec![DbTrackWork::new(
        TRACK_A,
        WORK_A,
        0,
        crate::import::MetadataSource::MusicBrainz,
        TRACK_WORK_A.to_string(),
        now,
    )];
    let release_roles = vec![DbReleaseArtistRole::new(
        &release.id,
        &composer.id,
        0,
        crate::import::MetadataSource::Discogs,
        Some("Conducted By".to_string()),
        RELEASE_ROLE_A.to_string(),
        now,
    )];
    let track_roles = vec![DbTrackArtistRole::new(
        TRACK_A,
        &composer.id,
        0,
        crate::import::MetadataSource::MusicBrainz,
        Some("arranger".to_string()),
        TRACK_ROLE_A.to_string(),
        now,
    )];

    db.finalize_import_atomic(
        Some(&album),
        &release,
        &track_files,
        &[],
        &[],
        &works,
        &work_artists,
        &[],
        &track_works,
        &release_roles,
        &track_roles,
        &[],
        &[],
        &[],
        &[],
        &[],
        None,
        &[],
        Some((&album.id, &release.id)),
        &[],
        tmp.path().to_str().unwrap(),
        crate::config::HomeStorage::Opaque,
        &[],
    )
    .await
    .unwrap();

    let composer_detail = db
        .find_composer_detail(&composer.id)
        .await
        .unwrap()
        .expect("composer detail");
    assert_eq!(composer_detail.work_groups.len(), 1);
    assert_eq!(composer_detail.work_groups[0].works[0].work.id, WORK_A);
    let release_role_count = db
        .read(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM release_artist_roles WHERE id = '9b72bbbf-621e-41ca-8930-1623b643a20d'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert_eq!(release_role_count, 1);

    let work_detail = db
        .find_work_detail(WORK_A)
        .await
        .unwrap()
        .expect("work detail");
    assert_eq!(work_detail.tracks.len(), 1);
    let track_role_count = db
        .read(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM track_artist_roles WHERE id = 'fa0c8483-f09a-4b69-8903-b1ebcdc31322'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(DbError::from)
        })
        .await
    .unwrap();
    assert_eq!(track_role_count, 1);
}

#[tokio::test]
async fn fail_import_and_delete_release_removes_finalized_import_state_atomically() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let artist = DbArtist {
        id: ARTIST_A.to_string(),
        name: "Artist Name A".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist).await.unwrap();

    let album = DbAlbum {
        id: ALBUM_A.to_string(),
        title: "Album Title A".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2026),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: RELEASE_A.to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: Pressing {
            year: Some(2026),
            format: Some("CD".to_string()),
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_provenance: Some(crate::import::MetadataProvenance::FileTags),
        remote: false,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = DbTrack {
        id: TRACK_A.to_string(),
        release_id: release.id.clone(),
        title: "Track Title A".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(1000),
        discogs_position: None,
        created_at: now,
    };
    let file = DbFile::new(
        &release.id,
        "Track Title A.flac",
        1024,
        ContentType::Flac,
        FILE_A.to_string(),
        now,
        crate::util::fs::hash_bytes(b"fixture"),
    );
    let track_files = vec![crate::import::TrackFile::Standalone {
        db_track: track,
        file_path: tmp.path().join("Track Title A.flac"),
        source_audio: scanned_flac(),
    }];

    db.finalize_import_atomic(
        Some(&album),
        &release,
        &track_files,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[file],
        &[],
        &[],
        None,
        &[],
        Some((&album.id, &release.id)),
        &[],
        tmp.path().to_str().unwrap(),
        crate::config::HomeStorage::Opaque,
        &[],
    )
    .await
    .unwrap();
    assert!(db.external_blob(FILE_A).await.unwrap().is_some());

    db.fail_import_and_delete_release(RELEASE_A).await.unwrap();

    assert!(db.find_release_by_id(RELEASE_A).await.unwrap().is_none());
    assert!(db.find_album_by_id(ALBUM_A).await.unwrap().is_none());
    // The registration was keyed by the `release_files` row, so the row
    // going takes it with it — there is no "is the ref still there?" left to
    // ask once the row is gone.
    assert!(db
        .get_files_for_release(RELEASE_A)
        .await
        .unwrap()
        .is_empty());
}

/// Reimport replacing one of several releases in an album: the prior release
/// leaves, the album survives, and a `primary_release_id` pointing at the
/// departed release goes NULL — read paths fall back to the first release left.
#[tokio::test]
async fn finalize_replacement_in_surviving_album_clears_dangling_primary() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    // album-old holds rel-one (primary) and rel-two; the reimport replaces
    // rel-one, so the album survives on rel-two.
    let outcomes =
        finalize_reimport_replacing_release(&db, &tmp, now, &[REL_ONE, REL_TWO], REL_ONE).await;

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].album_id, ALBUM_OLD);
    assert_eq!(outcomes[0].release_id, REL_ONE);
    assert!(!outcomes[0].album_deleted);

    let surviving = db
        .find_album_by_id(ALBUM_OLD)
        .await
        .unwrap()
        .expect("album survives while rel-two remains");
    assert_eq!(surviving.primary_release_id, None);
    assert!(db.find_release_by_id(REL_ONE).await.unwrap().is_none());
    assert!(db.find_release_by_id(REL_TWO).await.unwrap().is_some());
}

/// Reimport replacing an album's sole release, landing in a new album: the prior
/// album empties and is deleted, and the outcome reports that.
#[tokio::test]
async fn finalize_replacement_of_last_release_deletes_prior_album() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    // album-old holds only rel-old; replacing it empties and deletes it.
    let outcomes = finalize_reimport_replacing_release(&db, &tmp, now, &[REL_OLD], REL_OLD).await;

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].album_id, ALBUM_OLD);
    assert!(outcomes[0].album_deleted);

    assert!(db.find_album_by_id(ALBUM_OLD).await.unwrap().is_none());
    assert!(db.find_release_by_id(REL_OLD).await.unwrap().is_none());
    assert!(db.find_album_by_id(ALBUM_NEW).await.unwrap().is_some());
    assert!(db.find_release_by_id(REL_NEW).await.unwrap().is_some());
}

/// Failed-import rollback of one of several releases in an album: the album
/// survives, a `primary_release_id` pointing at the failed release goes NULL,
/// and the sibling release is untouched.
#[tokio::test]
async fn fail_import_and_delete_release_in_surviving_album_clears_dangling_primary() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let artist = DbArtist {
        id: ARTIST_A.to_string(),
        name: "Artist Name A".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist).await.unwrap();

    let album = DbAlbum {
        id: ALBUM_A.to_string(),
        title: "Album Title A".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2026),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease::new_test(&album.id, REL_A);
    let track = DbTrack {
        id: TRACK_A.to_string(),
        release_id: release.id.clone(),
        title: "Track Title A".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(1000),
        discogs_position: None,
        created_at: now,
    };
    let file = DbFile::new(
        &release.id,
        "Track Title A.flac",
        1024,
        ContentType::Flac,
        FILE_A.to_string(),
        now,
        crate::util::fs::hash_bytes(b"fixture"),
    );
    let track_files = vec![crate::import::TrackFile::Standalone {
        db_track: track,
        file_path: tmp.path().join("Track Title A.flac"),
        source_audio: scanned_flac(),
    }];

    // Finalize the import, pointing the album's primary at the release
    // this import created.
    db.finalize_import_atomic(
        Some(&album),
        &release,
        &track_files,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[file],
        &[],
        &[],
        None,
        &[],
        Some((&album.id, &release.id)),
        &[],
        tmp.path().to_str().unwrap(),
        crate::config::HomeStorage::Opaque,
        &[],
    )
    .await
    .unwrap();

    // A sibling release in the same album keeps it alive through the
    // rollback.
    let sibling = DbRelease::new_test(&album.id, REL_B);
    db.insert_release(&sibling).await.unwrap();

    db.fail_import_and_delete_release(REL_A).await.unwrap();

    let surviving = db
        .find_album_by_id(ALBUM_A)
        .await
        .unwrap()
        .expect("album survives while sibling remains");
    assert_eq!(surviving.primary_release_id, None);
    assert!(db.find_release_by_id(REL_A).await.unwrap().is_none());
    assert!(db.find_release_by_id(REL_B).await.unwrap().is_some());
}

#[tokio::test]
async fn failed_import_rollback_refuses_a_deletion_plan_that_changed_before_write() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let artist = DbArtist {
        id: ARTIST_A.to_string(),
        name: "Artist Name A".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist).await.unwrap();
    let album = DbAlbum {
        id: ALBUM_A.to_string(),
        title: "Album Title A".to_string(),
        artist_id: artist.id,
        year: Some(2026),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease::new_test(&album.id, REL_A);
    db.insert_album_with_release_and_tracks(&album, &release, &[], &[])
        .await
        .unwrap();

    let concurrent_db = db.clone();
    let sibling = DbRelease::new_test(&album.id, REL_B);
    let error = db
        .fail_import_and_delete_release_after_planning_for_test(REL_A, move || async move {
            concurrent_db.insert_release(&sibling).await.unwrap();
        })
        .await
        .expect_err("a changed rollback plan must abort");

    assert!(error.to_string().contains("changed after planning"));
    assert!(db.find_album_by_id(ALBUM_A).await.unwrap().is_some());
    assert!(db.find_release_by_id(REL_A).await.unwrap().is_some());
    assert!(db.find_release_by_id(REL_B).await.unwrap().is_some());
}

/// A failed remote import's cover and artist-image blobs live only in coven's
/// on-device store, since the release never went remote. The rollback declares
/// each blob its row deletions orphan in the same coven write: the release cover
/// and each deleted artist's image are reclaimed, while an image whose artist is
/// still referenced survives.
#[tokio::test]
async fn fail_import_and_delete_release_reclaims_orphaned_image_blobs_atomically() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let artist = |id: &str| DbArtist {
        id: id.to_string(),
        name: id.to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    db.insert_artist(&artist(ARTIST_EXCLUSIVE)).await.unwrap();
    db.insert_artist(&artist(ARTIST_SHARED)).await.unwrap();

    let pressing = || Pressing {
        year: Some(2026),
        format: Some("CD".to_string()),
        label: None,
        catalog_number: None,
        country: None,
        barcode: None,
    };
    let album = |id: &str, artist_id: &str| DbAlbum {
        id: id.to_string(),
        title: id.to_string(),
        artist_id: artist_id.to_string(),
        year: Some(2026),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = |id: &str, album_id: &str| DbRelease {
        id: id.to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: pressing(),
        disc_id: None,
        metadata_provenance: Some(crate::import::MetadataProvenance::FileTags),
        remote: false,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = |id: &str, release_id: &str| DbTrack {
        id: id.to_string(),
        release_id: release_id.to_string(),
        title: id.to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(1000),
        discogs_position: None,
        created_at: now,
    };

    // A prior surviving album references artist-shared, so the failed
    // import below must keep artist-shared and its image.
    db.insert_album_with_release_and_tracks(
        &album(ALBUM_PRIOR, ARTIST_SHARED),
        &release(RELEASE_PRIOR, ALBUM_PRIOR),
        &[track(TRACK_PRIOR, RELEASE_PRIOR)],
        &[],
    )
    .await
    .unwrap();

    let album_a = album(ALBUM_A, ARTIST_EXCLUSIVE);
    let release_a = release(RELEASE_A, ALBUM_A);
    let file_a = DbFile::new(
        RELEASE_A,
        "Track A.flac",
        1024,
        ContentType::Flac,
        FILE_A.to_string(),
        now,
        crate::util::fs::hash_bytes(b"fixture"),
    );
    let track_files = vec![crate::import::TrackFile::Standalone {
        db_track: track(TRACK_A, RELEASE_A),
        file_path: tmp.path().join("Track A.flac"),
        source_audio: scanned_flac(),
    }];
    // The failed release also credits artist-shared, so both artists are
    // rollback candidates; only artist-exclusive should be deleted.
    let album_artists = vec![DbAlbumArtist {
        id: AA_SHARED.to_string(),
        album_id: ALBUM_A.to_string(),
        artist_id: ARTIST_SHARED.to_string(),
        position: 1,
        created_at: now,
    }];
    let image = |id: &str, image_type: LibraryImageType| DbLibraryImage {
        id: id.to_string(),
        blob_id: format!("{id}-blob"),
        image_type,
        content_type: ContentType::Jpeg,
        file_size: 3,
        width: None,
        height: None,
        source: "local".to_string(),
        source_url: None,
        cloud_path: None,
        content_hash: crate::util::fs::hash_bytes(&[1u8, 2, 3]),
        created_at: now,
    };
    let cover = image(RELEASE_A, LibraryImageType::Cover);
    let img_exclusive = image(ARTIST_EXCLUSIVE, LibraryImageType::Artist);
    let img_shared = image(ARTIST_SHARED, LibraryImageType::Artist);
    let bytes = [1u8, 2, 3];

    db.finalize_import_atomic(
        Some(&album_a),
        &release_a,
        &track_files,
        &[],
        &album_artists,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[file_a],
        &[],
        &[],
        Some((&cover, &bytes)),
        &[(&img_exclusive, &bytes), (&img_shared, &bytes)],
        Some((&album_a.id, &release_a.id)),
        &[],
        tmp.path().to_str().unwrap(),
        crate::config::HomeStorage::Opaque,
        &[],
    )
    .await
    .unwrap();

    // The bytes coven holds for each host-provided image, before the rollback.
    let store_dir = coven::StoreDir::new(tmp.path());
    let blob_path = |namespace: &str, blob_id: &str| {
        store_dir
            .local_blob_path(namespace, blob_id)
            .expect("a valid blob path")
    };
    // The fixture's `image` helper derives each blob id from its subject id,
    // so the stored paths are named the same way.
    let cover_blob = blob_path(crate::sync::COVERS_NAMESPACE, &format!("{RELEASE_A}-blob"));
    let exclusive_blob = blob_path(
        crate::sync::ARTIST_IMAGES_NAMESPACE,
        &format!("{ARTIST_EXCLUSIVE}-blob"),
    );
    let shared_blob = blob_path(
        crate::sync::ARTIST_IMAGES_NAMESPACE,
        &format!("{ARTIST_SHARED}-blob"),
    );
    for path in [&cover_blob, &exclusive_blob, &shared_blob] {
        assert!(path.exists(), "finalize stored {}", path.display());
    }

    db.fail_import_and_delete_release(RELEASE_A).await.unwrap();

    // The rollback declares the blobs its row deletions orphan, so coven
    // reclaims their bytes in the same write. A bare row DELETE would leave
    // these files behind forever — coven's local-blob cleanup is intent-driven
    // and only ever acts on a declared deletion.
    assert!(
        !cover_blob.exists(),
        "the failed release's cover blob is reclaimed"
    );
    assert!(
        !exclusive_blob.exists(),
        "the swept artist's image blob is reclaimed"
    );
    assert!(
        shared_blob.exists(),
        "the surviving artist still has its image blob"
    );

    // The shared artist and its image row survive; the exclusive one is gone.
    assert!(db.find_artist_by_id(ARTIST_SHARED).await.unwrap().is_some());
    assert!(db
        .find_artist_by_id(ARTIST_EXCLUSIVE)
        .await
        .unwrap()
        .is_none());
}
