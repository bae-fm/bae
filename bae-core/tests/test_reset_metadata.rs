#![cfg(feature = "test-utils")]
//! Reset metadata to source. Verifies `LibraryManager::reset_metadata_to_source`
//! re-runs the seeding projection from the archived provider documents and
//! returns the projected `ReleaseUserEdit` shape — without writing the DB or
//! touching identity / metadata-source columns.
use bae_test_support as support;

use bae_core::db::{
    Database, DbAlbum, DbArtist, DbFile, DbRelease, DbSourceReleasePayload, DbTrack, Pressing,
};
use bae_core::import::{
    ArtistAssignment, MetadataProvenance, MetadataSource, NewArtistSeed, PayloadSource,
    ReleaseIdentity,
};
use bae_core::library::LibraryManager;
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use coven::StoreDir;
use std::path::PathBuf;
use support::{test_config, tracing_init};
use tempfile::TempDir;
use uuid::Uuid;

/// Archive one provider document under the source entity it describes, as a
/// fetch would.
async fn seed_payload(db: &Database, source: PayloadSource, source_release_id: &str, json: String) {
    db.save_source_release_payloads(&[DbSourceReleasePayload {
        source,
        source_release_id: source_release_id.to_string(),
        json,
        fetched_at: Utc::now(),
    }])
    .await
    .unwrap();
}

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
    let config_handle = test_config(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    (library_manager, database, temp_dir)
}

fn make_artist(name: &str) -> DbArtist {
    DbArtist {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    }
}

fn make_album(artist_id: &str, title: &str) -> DbAlbum {
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        artist_id: artist_id.to_string(),
        year: None,
        primary_release_id: None,
        is_compilation: false,
        created_at: Utc::now(),
    }
}

/// Release that doesn't claim an identity or provenance yet — caller wires the
/// scenario under test.
fn make_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: Pressing::blank(),
        disc_id: None,
        metadata_provenance: None,
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: Utc::now(),
    }
}

fn make_track(release_id: &str, n: i32, title: &str) -> DbTrack {
    DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release_id.to_string(),
        title: title.to_string(),
        side: 1,
        track_number: Some(n),
        duration_ms: None,
        discogs_position: None,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn edit_seed_exposes_reset_eligibility_from_provenance() {
    let (lm, db, _tmp) = setup().await;
    let artist = make_artist("Artist Name");
    db.insert_artist(&artist).await.unwrap();

    for (index, (provenance, expected)) in [
        (
            Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::MusicBrainz,
                release_id: "mb-release".to_string(),
            }),
            true,
        ),
        (
            Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: "discogs-release".to_string(),
            }),
            true,
        ),
        (Some(MetadataProvenance::FileTags), true),
        (None, false),
    ]
    .into_iter()
    .enumerate()
    {
        let album = make_album(&artist.id, &format!("Album {index}"));
        let mut release = make_release(&album.id);
        release.metadata_provenance = provenance.clone();
        db.insert_album(&album).await.unwrap();
        db.insert_release(&release).await.unwrap();

        let seed = lm.release_edit_seed(&release.id).await.unwrap();
        assert_eq!(
            seed.can_reset_to_source, expected,
            "provenance {provenance:?}"
        );
    }
}

#[tokio::test]
async fn resetting_a_source_less_release_reports_that_it_has_no_provenance() {
    let (lm, db, _tmp) = setup().await;
    let artist = make_artist("Artist Name");
    let album = make_album(&artist.id, "Album Title");
    let release = make_release(&album.id);
    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();

    let error = lm
        .reset_metadata_to_source(&release.id)
        .await
        .expect_err("source-less metadata cannot be reset to a source");
    assert!(
        error.to_string().contains("has no metadata provenance"),
        "unexpected error: {error}"
    );
}

// ── MusicBrainz ─────────────────────────────────────────────────────────

/// Build a minimal-but-valid MB release JSON with a release group, an
/// artist credit, and one track per supplied title. Mirrors the shape
/// identification archives in `source_release_payloads`.
fn mb_release_json(
    release_id: &str,
    release_group_id: &str,
    title: &str,
    artist: &str,
    track_titles: &[&str],
) -> String {
    use bae_core::musicbrainz::{
        MbArtistCredit, MbArtistRef, MbLabel, MbLabelInfo, MbMedium, MbRecording,
        MbReleaseGroupRef, MbReleaseResponse, MbTrack,
    };
    let response = MbReleaseResponse {
        id: release_id.to_string(),
        title: title.to_string(),
        date: Some("1999-01-01".to_string()),
        country: Some("US".to_string()),
        barcode: Some("0123456789".to_string()),
        artist_credit: vec![MbArtistCredit {
            name: artist.to_string(),
            artist: Some(MbArtistRef {
                id: Some(format!("mb-art-{artist}")),
                name: Some(artist.to_string()),
                sort_name: Some(artist.to_string()),
            }),
        }],
        release_group: Some(MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: Some("1999".to_string()),
            relations: None,
        }),
        label_info: vec![MbLabelInfo {
            label: Some(MbLabel {
                name: Some("Test Label".to_string()),
            }),
            catalog_number: Some("CAT-001".to_string()),
        }],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: track_titles
                .iter()
                .enumerate()
                .map(|(i, t)| MbTrack {
                    position: Some((i as i64) + 1),
                    number: Some(format!("{}", i + 1)),
                    title: Some((*t).to_string()),
                    length: None,
                    recording: Some(MbRecording {
                        id: None,
                        title: Some((*t).to_string()),
                        artist_credit: vec![],
                        relations: vec![],
                    }),
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    };
    serde_json::to_string(&response).unwrap()
}

#[tokio::test]
async fn reset_mb_returns_full_pressing_data_from_cache() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let mut release = make_release(&album.id);
    release.metadata_provenance = Some(MetadataProvenance::ExternalRelease {
        source: MetadataSource::MusicBrainz,
        release_id: "mb-release-1".to_string(),
    });
    let t1 = make_track(&release.id, 1, "Original Track 1");
    let t2 = make_track(&release.id, 2, "Original Track 2");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&t1).await.unwrap();
    db.insert_track(&t2).await.unwrap();

    db.insert_release_identities(
        &release.id,
        &[ReleaseIdentity {
            source: MetadataSource::MusicBrainz,
            source_group_id: "mb-rg-1".to_string(),
            source_release_id: "mb-release-1".to_string(),
        }],
    )
    .await
    .unwrap();

    seed_payload(
        &db,
        PayloadSource::MusicBrainz,
        "mb-release-1",
        mb_release_json(
            "mb-release-1",
            "mb-rg-1",
            "Cached Album",
            "Cached Artist",
            &["Cached Track 1", "Cached Track 2"],
        ),
    )
    .await;

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Cached Album");
    assert_eq!(
        edit.album_artist_assignments,
        vec![ArtistAssignment::New {
            seed: NewArtistSeed {
                name: "Cached Artist".to_string(),
                sort_name: Some("Cached Artist".to_string()),
                musicbrainz_artist_id: Some("mb-art-Cached Artist".to_string()),
                discogs_artist_id: None,
            },
        }]
    );
    assert_eq!(edit.pressing.year, Some(1999));
    assert_eq!(edit.pressing.format.as_deref(), Some("CD"));
    assert_eq!(edit.pressing.label.as_deref(), Some("Test Label"));
    assert_eq!(edit.pressing.catalog_number.as_deref(), Some("CAT-001"));
    assert_eq!(edit.pressing.country.as_deref(), Some("US"));
    assert_eq!(edit.pressing.barcode.as_deref(), Some("0123456789"));
    assert_eq!(edit.tracks.len(), 2);
    assert_eq!(edit.tracks[0].title, "Cached Track 1");
    assert_eq!(edit.tracks[1].title, "Cached Track 2");

    // Reset is read-only: identity rows and metadata provenance
    // stay exactly as they were.
    let identities = db.get_release_identities(&release.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(identities[0].source_release_id, "mb-release-1");
    let saved_release = db.find_release_by_id(&release.id).await.unwrap().unwrap();
    assert_eq!(
        saved_release.metadata_provenance,
        Some(MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
            release_id: "mb-release-1".to_string(),
        })
    );
    // And it doesn't touch the persisted album / release / tracks either —
    // the projected values are returned to the caller, who decides whether
    // to save them via apply_release_metadata_user_edit.
    let saved_album = db.find_album_by_id(&album.id).await.unwrap().unwrap();
    assert_eq!(saved_album.title, "Original Album");
    let saved_tracks = db.get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(saved_tracks[0].title, "Original Track 1");
}

// ── Discogs ─────────────────────────────────────────────────────────────

/// Discogs API response shape, in the subset `parse_discogs_release_json`
/// reads. Lets tests hand-roll cached payloads without going through the
/// HTTP client.
fn discogs_release_json(
    release_id: u64,
    title: &str,
    artist_id: u64,
    artist: &str,
    year: u32,
    label: &str,
    catno: &str,
    country: &str,
    track_titles: &[&str],
) -> String {
    let tracks: Vec<serde_json::Value> = track_titles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            serde_json::json!({
                "position": format!("{}", i + 1),
                "title": *t,
                "type_": "track",
                "artists": [],
            })
        })
        .collect();
    serde_json::json!({
        "id": release_id,
        "title": title,
        "year": year,
        "artists": [
            { "id": artist_id, "name": artist }
        ],
        "labels": [
            { "name": label, "catno": catno }
        ],
        "country": country,
        "formats": [{ "name": "CD" }],
        "tracklist": tracks,
    })
    .to_string()
}

#[tokio::test]
async fn reset_discogs_returns_full_pressing_data_from_cache() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let mut release = make_release(&album.id);
    release.metadata_provenance = Some(MetadataProvenance::ExternalRelease {
        source: MetadataSource::Discogs,
        release_id: "12345".to_string(),
    });
    let t1 = make_track(&release.id, 1, "Original Track");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&t1).await.unwrap();

    db.insert_release_identities(
        &release.id,
        &[ReleaseIdentity {
            source: MetadataSource::Discogs,
            source_group_id: "67890".to_string(),
            source_release_id: "12345".to_string(),
        }],
    )
    .await
    .unwrap();

    seed_payload(
        &db,
        PayloadSource::Discogs,
        "12345",
        discogs_release_json(
            12345,
            "Cached Discogs Album",
            999,
            "Cached Discogs Artist",
            1985,
            "Cached Label",
            "CACHE-1",
            "JP",
            &["Cached Discogs Track"],
        ),
    )
    .await;

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Cached Discogs Album");
    assert_eq!(
        edit.album_artist_assignments,
        vec![ArtistAssignment::New {
            seed: NewArtistSeed {
                name: "Cached Discogs Artist".to_string(),
                sort_name: Some("Cached Discogs Artist".to_string()),
                musicbrainz_artist_id: None,
                discogs_artist_id: Some("999".to_string()),
            },
        }]
    );
    assert_eq!(edit.pressing.year, Some(1985));
    assert_eq!(edit.pressing.format.as_deref(), Some("CD"));
    assert_eq!(edit.pressing.label.as_deref(), Some("Cached Label"));
    assert_eq!(edit.pressing.catalog_number.as_deref(), Some("CACHE-1"));
    assert_eq!(edit.pressing.country.as_deref(), Some("JP"));
    assert_eq!(edit.tracks.len(), 1);
    assert_eq!(edit.tracks[0].title, "Cached Discogs Track");
}

// ── File tags (Unknown) ─────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

#[tokio::test]
async fn reset_file_tags_unknown_returns_tags_from_disk() {
    let (lm, db, tmp) = setup().await;

    // Create real audio files inside a local folder so the release
    // can resolve them via `local_file_path`.
    let audio_dir = tmp.path().join("source-folder");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");
    let f1 = support::copy_and_tag(
        &src,
        &audio_dir,
        "01.flac",
        "Tag Track 1",
        "Tag Artist",
        "Tag Album",
        "Tag Artist",
        2010,
        1,
    );
    let f2 = support::copy_and_tag(
        &src2,
        &audio_dir,
        "02.flac",
        "Tag Track 2",
        "Tag Artist",
        "Tag Album",
        "Tag Artist",
        2010,
        2,
    );

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let mut release = make_release(&album.id);
    release.metadata_provenance = Some(MetadataProvenance::FileTags);
    release.remote = false;
    let t1 = make_track(&release.id, 1, "Original Track 1");
    let t2 = make_track(&release.id, 2, "Original Track 2");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&t1).await.unwrap();
    db.insert_track(&t2).await.unwrap();

    let now = Utc::now();
    let file1 = DbFile {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        original_filename: f1.file_name().unwrap().to_string_lossy().into_owned(),
        file_size: std::fs::metadata(&f1).unwrap().len() as i64,
        content_type: ContentType::Flac,
        cloud_path: None,
        content_hash: bae_core::util::fs::hash_bytes(b"fixture"),
        created_at: now,
    };
    let file2 = DbFile {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        original_filename: f2.file_name().unwrap().to_string_lossy().into_owned(),
        file_size: std::fs::metadata(&f2).unwrap().len() as i64,
        content_type: ContentType::Flac,
        cloud_path: None,
        content_hash: bae_core::util::fs::hash_bytes(b"fixture"),
        created_at: now,
    };
    db.insert_file(&file1).await.unwrap();
    db.insert_file(&file2).await.unwrap();
    // A Local release's files are coven external refs at their in-place location;
    // register them now that the file rows exist so reset can read the tags.
    db.register_release_external_refs_for_test(&release.id, audio_dir.to_str().unwrap())
        .await
        .unwrap();

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Tag Album");
    assert_eq!(
        edit.album_artist_assignments,
        vec![ArtistAssignment::new("Tag Artist")]
    );
    assert_eq!(edit.pressing.year, Some(2010));
    assert_eq!(edit.pressing.format.as_deref(), Some("FLAC"));
    assert_eq!(edit.tracks.len(), 2);
    assert_eq!(edit.tracks[0].title, "Tag Track 1");
    assert_eq!(edit.tracks[1].title, "Tag Track 2");

    // Identity remained empty and provenance stayed File Tags.
    let identities = db.get_release_identities(&release.id).await.unwrap();
    assert!(identities.is_empty());
    let saved = db.find_release_by_id(&release.id).await.unwrap().unwrap();
    assert_eq!(
        saved.metadata_provenance,
        Some(MetadataProvenance::FileTags)
    );
}

#[tokio::test]
async fn reset_mb_missing_archived_payload_errors() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Artist");
    let album = make_album(&artist.id, "Album");
    let mut release = make_release(&album.id);
    release.metadata_provenance = Some(MetadataProvenance::ExternalRelease {
        source: MetadataSource::MusicBrainz,
        release_id: "mb-release-missing".to_string(),
    });

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();

    let err = lm
        .reset_metadata_to_source(&release.id)
        .await
        .expect_err("a pointer with nothing archived under it should error");
    let msg = err.to_string();
    assert!(
        msg.contains("no archived musicbrainz payload"),
        "unexpected error: {msg}"
    );
    assert!(
        msg.contains("mb-release-missing"),
        "the error names the source release nothing was archived for: {msg}"
    );
}

/// `set_identity` can redirect provenance to a different
/// pressing without fetching it. The documents are keyed by the source
/// release, so the previous pressing's cannot be read in the new one's place:
/// reset finds nothing under the new pointer and says so, rather than
/// surfacing the wrong pressing's fields without telling the user.
#[tokio::test]
async fn reset_mb_reads_only_the_pressing_the_pointer_names() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Artist");
    let album = make_album(&artist.id, "Album");
    let mut release = make_release(&album.id);
    // Provenance says we want pressing Y…
    release.metadata_provenance = Some(MetadataProvenance::ExternalRelease {
        source: MetadataSource::MusicBrainz,
        release_id: "mb-release-Y".to_string(),
    });

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();

    db.insert_release_identities(
        &release.id,
        &[ReleaseIdentity {
            source: MetadataSource::MusicBrainz,
            source_group_id: "mb-rg-1".to_string(),
            source_release_id: "mb-release-Y".to_string(),
        }],
    )
    .await
    .unwrap();

    // …and the only archived document is pressing X's, the one it was pointed
    // away from. Documents are keyed by the source release, so nothing under Y
    // was ever written and X's cannot be read in its place.
    seed_payload(
        &db,
        PayloadSource::MusicBrainz,
        "mb-release-X",
        mb_release_json(
            "mb-release-X",
            "mb-rg-1",
            "Other Pressing",
            "Artist",
            &["Other Track"],
        ),
    )
    .await;

    let err = lm
        .reset_metadata_to_source(&release.id)
        .await
        .expect_err("a pointer with no archived payload should error");
    let msg = err.to_string();
    assert!(
        msg.contains("no archived musicbrainz payload"),
        "unexpected error: {msg}"
    );
    assert!(msg.contains("mb-release-Y"), "missing pointer id in: {msg}");
}
