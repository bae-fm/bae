//! An import's commit: the release's rows, written in the transaction that
//! checks the candidate still is what the import was prepared from. The row
//! reads as importing until that write.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::identification::tests::*;
use bae_records::test_fixtures::{
    ALBUM_A, ARTIST_ALBUM, ARTIST_COMPOSER, RELEASE_A, RELEASE_ROLE_A, TRACK_A, TRACK_ROLE_A,
    TRACK_WORK_A, WORK_A,
};
use bae_store::test_fixtures::temp_db;
use bae_util::content_type::ContentType;
use bae_util::test_channels::HANG_GUARD;

fn scanned_flac() -> crate::folder_scanner::ScannedAudio {
    crate::folder_scanner::ScannedAudio {
        content_type: ContentType::Flac,
        container: bae_util::audio_container::AudioContainer::Flac,
        duration_ms: 1_000,
        format: bae_types::audio_format::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: Some(16),
            bitrate_kbps: None,
            channels: 2,
        },
    }
}

/// A named artist with no external ids and no sort name.
fn test_artist(id: &str, name: &str, now: chrono::DateTime<chrono::Utc>) -> DbArtist {
    DbArtist {
        id: id.to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    }
}

/// A 2026 album; its credits are the import's rows.
fn test_album(id: &str, title: &str, now: chrono::DateTime<chrono::Utc>) -> DbAlbum {
    DbAlbum {
        id: id.to_string(),
        title: title.to_string(),
        year: Some(2026),
        is_compilation: false,
        created_at: now,
    }
}

/// A local release read from file tags, pressed on CD in 2026.
fn test_release(id: &str, album_id: &str, now: chrono::DateTime<chrono::Utc>) -> DbRelease {
    DbRelease {
        id: id.to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: bae_types::pressing::Pressing {
            year: Some(2026),
            labels: Vec::new(),
            barcodes: Vec::new(),
            facts: bae_types::pressing::made_of(bae_types::pressing::Medium::Cd, 1),
        },

        source_folder_name: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    }
}

/// Track one, side one, one second long.
fn test_track(
    id: &str,
    release_id: &str,
    title: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> DbTrack {
    DbTrack {
        id: id.to_string(),
        release_id: release_id.to_string(),
        title: title.to_string(),
        medium_id: bae_records::identity::release_medium_id(release_id, 1),
        side: std::num::NonZeroU8::MIN,
        track_number: std::num::NonZeroU32::MIN,
        duration_ms: 1000,
        created_at: now,
    }
}

/// What a finalize varies between these tests. Everything else it takes — the
/// guard over a stored candidate and opaque home storage — is the same in all
/// of them.
#[derive(Default)]
struct Commit<'a> {
    album: Option<&'a DbAlbum>,
    track_files: &'a [crate::TrackFile],
    rows: crate::tables::ImportRows<'a>,
    files: Vec<crate::service::PreparedImportFile>,
    library_image: Option<(&'a DbLibraryImage, &'a [u8])>,
    artist_images: crate::tables::NewArtistImages<'a>,
}

/// The real guard over a scanned candidate the import has claimed: stored by
/// the scan, prepared (one edit, so it has a state row), and owned by
/// `import-1`, as the import queue leaves it when it commits.
async fn claimed_candidate_guard(db: &Store) -> crate::tables::ImportCommitGuard {
    use crate::tables::import_state::tests::{
        pane_candidate_key, pane_candidate_path, stored_pane_candidate, PaneControls,
    };
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, content_hash) = stored_pane_candidate(db).await;
    PaneControls::of_pane_candidate(db)
        .type_field(&content_hash, crate::CandidateEditField::PressingYear, "2026")
        .await
        .unwrap();
    let state = tables
        .load_import_candidate_state(&content_hash)
        .await
        .unwrap()
        .expect("the edit stored the candidate's state");
    let candidate_key = pane_candidate_key();
    assert_eq!(
        tables.claim_candidate_import(&candidate_key, "import-1", None).await.unwrap(),
        crate::tables::ImportClaim::Claimed { identification: None }
    );
    crate::tables::ImportCommitGuard::Candidate {
        candidate_key,
        source: crate::release_candidate::CandidateSource {
            path: pane_candidate_path(),
            scope: crate::folder_scanner::ReleaseFileScope::Direct,
            parts: Vec::new(),
        },
        expectation: crate::service::ImportExpectation {
            candidate: crate::CandidateAsRead {
                content_hash,
                file_edit_revision: state.file_edits.revision,
                metadata_revision: state.metadata_revision,
            },
            file_tag_snapshot: None,
        },
        import_id: "import-1".to_string(),
    }
}

/// Finalize `release` through the real guard over a claimed candidate. Its
/// tracks sit on its one CD ([`test_track`]).
async fn commit_import(db: &Store, release: &DbRelease, commit: Commit<'_>) {
    let guard = claimed_candidate_guard(db).await;
    let mediums = [bae_records::DbReleaseMedium::new_test(&release.id)];
    ImportTables::new(db.clone())
        .finalize_import_atomic(
            &coven::UuidIds,
            guard,
            commit.album,
            release,
            commit.track_files,
            crate::tables::ImportRows { mediums: &mediums, ..commit.rows },
            commit.files,
            commit.library_image,
            commit.artist_images,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn finalize_refuses_metadata_that_changed_after_queue_admission() {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = tmp.path().join("watch").to_path_buf();
    let candidate_path = std::path::PathBuf::from(&root).join("Album");
    let files = crate::folder_scanner::CategorizedFiles {
        files: vec![crate::folder_scanner::CandidateFile {
            file: crate::folder_scanner::ScannedFile::new(
                candidate_path.join("01.flac"),
                "01.flac".to_string(),
                123,
                1,
            )
            .with_test_flac_audio(),
            role: crate::folder_scanner::FileRole::Audio,
        }],
        parts: Vec::new(),
    };
    let candidate = crate::tables::tests::candidate_with(
        std::path::Path::new(&root),
        "Album",
        files,
        crate::folder_scanner::ReleaseFileScope::Direct,
    );
    let content_hash = candidate.files.content_hash();
    let pane = crate::tables::import_state::tests::PaneControls::of(&db, &candidate);
    tables.add_watched_import_folder(std::path::Path::new(&root)).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(&root), crate::VolumeKind::Local)
        .await
        .unwrap();
    tables
        .save_folder_scan_item(
            std::path::Path::new(&root),
            generation,
            &crate::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");

    pane.type_field(&content_hash, crate::CandidateEditField::PressingYear, "2026").await.unwrap();
    let release = DbRelease::new_test("album-never-written", "release-never-written");
    let error = tables
        .finalize_import_atomic(
            &coven::UuidIds,
            crate::tables::ImportCommitGuard::Candidate {
                candidate_key: crate::CandidateKey::folder(&candidate_path),
                source: crate::release_candidate::CandidateSource {
                    path: candidate_path,
                    scope: crate::folder_scanner::ReleaseFileScope::Direct,
                    parts: Vec::new(),
                },
                expectation: crate::service::ImportExpectation {
                    candidate: crate::CandidateAsRead {
                        content_hash,
                        file_edit_revision: 0,
                        metadata_revision: 0,
                    },
                    file_tag_snapshot: None,
                },
                import_id: "import-1".to_string(),
            },
            None,
            &release,
            &[],
            crate::tables::ImportRows::default(),
            Vec::new(),
            None,
            crate::tables::NewArtistImages::default(),
        )
        .await
        .expect_err("the final transaction must re-check the queued metadata revision");

    assert!(
        matches!(
            error,
            bae_store::WriteError::Refused(crate::tables::ImportCommitRefusal::CandidateMoved(
                crate::service::CandidateMoved::MetadataChanged(_)
            ))
        ),
        "{error:?}"
    );
    assert!(records.find_release_by_id("release-never-written").await.unwrap().is_none());
}

#[tokio::test]
async fn finalize_import_persists_composer_work_and_role_rows() {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let album_artist = test_artist(ARTIST_ALBUM, "Album Artist A", now);
    let composer = DbArtist {
        sort_name: Some("Composer Artist A".to_string()),
        musicbrainz_artist_id: Some("mb-artist-composer-a".to_string()),
        ..test_artist(ARTIST_COMPOSER, "Composer Artist A", now)
    };
    records.insert_artist(&album_artist).await.unwrap();
    records.insert_artist(&composer).await.unwrap();

    let album = test_album(ALBUM_A, "Album Title A", now);
    let album_artists = [bae_records::DbAlbumArtist::new(&album.id, &album_artist.id, 0, now)];
    let release = DbRelease { ..test_release(RELEASE_A, &album.id, now) };
    let track_files = vec![crate::TrackFile {
        db_track: test_track(TRACK_A, &release.id, "Track Title A", now),
        audio: crate::TrackAudio::Standalone {
            file_path: tmp.path().join("Track.flac"),
            source_audio: scanned_flac(),
        },
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
        bae_catalogs::catalog::Catalog::MusicBrainz,
        None,
        now,
    )];
    let track_works = vec![DbTrackWork::new(
        TRACK_A,
        WORK_A,
        0,
        bae_catalogs::catalog::Catalog::MusicBrainz,
        TRACK_WORK_A.to_string(),
        now,
    )];
    let release_roles = vec![DbReleaseArtistRole::new(
        &release.id,
        &composer.id,
        0,
        bae_catalogs::catalog::Catalog::Discogs,
        Some("Conducted By".to_string()),
        None,
        RELEASE_ROLE_A.to_string(),
        now,
    )];
    let track_roles = vec![DbTrackArtistRole::new(
        TRACK_A,
        &composer.id,
        0,
        bae_catalogs::catalog::Catalog::MusicBrainz,
        Some("arranger".to_string()),
        None,
        TRACK_ROLE_A.to_string(),
        now,
    )];

    let credits = [album_artist.clone(), composer.clone()];
    let picked = [album_artist.id.clone(), composer.id.clone()];
    commit_import(
        &db,
        &release,
        Commit {
            album: Some(&album),
            track_files: &track_files,
            rows: crate::tables::ImportRows {
                artists: bae_records::ArtistCredits { credits: &credits, picked: &picked },
                works: &works,
                work_artists: &work_artists,
                track_works: &track_works,
                release_artist_roles: &release_roles,
                track_artist_roles: &track_roles,
                album_artists: &album_artists,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .await;

    assert_eq!(
        db.query_texts_for_test(&format!(
            "SELECT work_id FROM work_artists WHERE artist_id = '{}'",
            composer.id
        ))
        .await
        .unwrap(),
        [WORK_A],
        "the work is the composer's"
    );
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

    assert_eq!(
        db.query_texts_for_test(&format!(
            "SELECT track_id FROM track_works WHERE work_id = '{WORK_A}'"
        ))
        .await
        .unwrap(),
        [TRACK_A],
        "the track performs the work"
    );
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

/// An import reads as importing in every delivery until the one in which its
/// release takes the row to Done: no delivery shows the row on Found with its
/// import ended, gone from Found with no release, or on Done still importing.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_reads_as_importing_until_its_release_is_written() {
    let fixture = Fixture::importing("standing-import-to-done").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    fixture.identify_ready(&dir, "mb-standing-import", "rg-standing-import").await;

    let list = list_subscription(&fixture, page_window(0, 50)).await;
    let initial = next_page(&list).await;
    assert!(!page_rows(&initial)[&key].1.facts.importing());
    let tables = crate::tables::ImportTables::new(fixture.db.clone());
    assert_eq!(tables.load_tab_counts().await.unwrap().done, 0);
    // The pane reads where the candidate stands and whether this device
    // imported it in one read; it is followed beside the list so no read of
    // it is passed over.
    let mut pane = fixture.import.subscribe_candidate(Some(key.clone()));
    pane.next().await.expect("the query stays open").result.unwrap();
    let pane_saw_both = tokio::spawn(async move {
        loop {
            let detail = pane
                .next()
                .await
                .expect("the query stays open")
                .result
                .unwrap()
                .expect("the candidate stays stored")
                .resolve();
            if detail.is_added() {
                return detail.live.facts.importing();
            }
        }
    });

    fixture.import.start_import(&key).await.unwrap();
    let mut importing = 0;
    loop {
        let snapshot = next_page(&list).await;
        match page_rows(&snapshot).remove(&key) {
            Some((_, live)) if live.facts.importing() => importing += 1,
            Some((_, live)) => assert_eq!(
                importing, 0,
                "a delivery shows the row on Found with its import ended: {live:?}"
            ),
            None => {
                assert!(importing > 0, "the import read as importing");
                assert_eq!(
                    tables.load_tab_counts().await.unwrap().done,
                    1,
                    "the row left Found without its release"
                );
                break;
            }
        }
    }
    assert!(
        !tokio::time::timeout(HANG_GUARD, pane_saw_both)
            .await
            .expect("the pane reads the release")
            .expect("the pane is followed to the end"),
        "a read shows the release beside the import still going"
    );
}

/// An import whose credit's MusicBrainz and Discogs ids name two library
/// artists, and whose label's two ids name two library labels, imports: the
/// MusicBrainz id decides each, and the Discogs ids that disagree are dropped,
/// so every library artist and label stays as it was.
#[tokio::test]
async fn an_import_whose_ids_name_two_library_entries_goes_by_musicbrainz() {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let by_discogs = DbArtist {
        discogs_artist_id: Some("discogs-1".to_string()),
        ..test_artist("artist-by-discogs", "Artist Name", now)
    };
    let by_musicbrainz = DbArtist {
        musicbrainz_artist_id: Some("mb-1".to_string()),
        ..test_artist("artist-by-musicbrainz", "Artist Name", now)
    };
    for artist in [&by_discogs, &by_musicbrainz] {
        records.insert_artist(artist).await.unwrap();
    }
    use bae_records::test_fixtures::labels::{add_label, label, with_ids};
    add_label(&db, "Label Name", None, Some("7190")).await;
    let label_by_musicbrainz = add_label(&db, "Other Label Name", Some("mb-label"), None).await;

    let incoming = DbArtist {
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: Some("mb-1".to_string()),
        ..test_artist("artist-incoming", "Artist Name", now)
    };
    let album = test_album(ALBUM_A, "Album Title A", now);
    let mut release = test_release(RELEASE_A, &album.id, now);
    release.pressing.labels =
        vec![with_ids(label(Some("Label Name"), Some("LN 1")), Some("mb-label"), Some("7190"))];
    let track = test_track(TRACK_A, &release.id, "Track Title A", now);
    let track_files = vec![crate::TrackFile {
        db_track: track.clone(),
        audio: crate::TrackAudio::Standalone {
            file_path: tmp.path().join("Track.flac"),
            source_audio: scanned_flac(),
        },
    }];
    let credits = [incoming.clone()];
    let album_artists = [bae_records::DbAlbumArtist::new(&album.id, &incoming.id, 0, now)];
    let track_artists = [bae_records::DbTrackArtist::new(&track.id, &incoming.id, 0, now)];

    commit_import(
        &db,
        &release,
        Commit {
            album: Some(&album),
            track_files: &track_files,
            rows: crate::tables::ImportRows {
                artists: bae_records::ArtistCredits { credits: &credits, picked: &[] },
                album_artists: &album_artists,
                track_artists: &track_artists,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .await;

    let credited: Vec<String> = records
        .get_artists_for_album(&album.id)
        .await
        .unwrap()
        .into_iter()
        .map(|artist| artist.artist.id)
        .collect();
    assert_eq!(credited, std::slice::from_ref(&by_musicbrainz.id));
    for artist in [&by_discogs, &by_musicbrainz] {
        let stored = records.find_artist_by_id(&artist.id).await.unwrap().unwrap();
        assert_eq!(
            (stored.discogs_artist_id, stored.musicbrainz_artist_id),
            (artist.discogs_artist_id.clone(), artist.musicbrainz_artist_id.clone())
        );
    }
    let placed: Vec<(Option<String>, Option<String>)> = db
        .read(|sql| {
            Ok(sql.query(
                "SELECT l.discogs_label_id, r.label_id FROM release_labels r \
                 JOIN labels l ON l.id = r.label_id WHERE r.release_id = ? ORDER BY r.position",
                [RELEASE_A],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(placed, [(None, Some(label_by_musicbrainz))]);
}
