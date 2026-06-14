#![cfg(feature = "test-utils")]
//! Reset metadata to source. Verifies `LibraryManager::reset_metadata_to_source`
//! re-runs the seeding projection from the cached source data and returns the
//! projected `ReleaseUserEdit` shape — without writing the DB or touching
//! identity / metadata-source columns.
mod support;

use crate::support::{test_config_and_keys, tracing_init};
use bae_core::db::{
    Database, DbAlbum, DbArtist, DbFile, DbRelease, DbReleaseMetadata, DbTrack, Pressing,
    ReleaseMetadataSource,
};
use bae_core::import::{MetadataSource, ReleaseIdentity};
use bae_core::library::LibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use std::path::PathBuf;
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
    .unwrap();
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

/// Release that doesn't claim an identity yet — caller wires
/// `metadata_source` / `metadata_source_release_id` and inserts identity rows
/// to set up the scenario under test.
fn make_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: Pressing::blank(),
        disc_id: None,
        metadata_source: ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        managed: true,
        source_folder_name: None,
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

// ── MusicBrainz ─────────────────────────────────────────────────────────

/// Build a minimal-but-valid MB release JSON with a release group, an
/// artist credit, and one track per supplied title. Mirrors the shape
/// `commit_mb_release` archives in `release_metadata`.
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
                        title: Some((*t).to_string()),
                    }),
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
    };
    serde_json::to_string(&response).unwrap()
}

#[tokio::test]
async fn reset_mb_exact_returns_full_pressing_data_from_cache() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let mut release = make_release(&album.id);
    release.metadata_source = ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-release-1".to_string());
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
            source_release_id: Some("mb-release-1".to_string()),
        }],
    )
    .await
    .unwrap();

    db.insert_release_metadata(&DbReleaseMetadata::new(
        &release.id,
        "musicbrainz",
        mb_release_json(
            "mb-release-1",
            "mb-rg-1",
            "Cached Album",
            "Cached Artist",
            &["Cached Track 1", "Cached Track 2"],
        ),
        Uuid::new_v4().to_string(),
        Utc::now(),
    ))
    .await
    .unwrap();

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Cached Album");
    assert_eq!(edit.album_artist_names, vec!["Cached Artist".to_string()]);
    assert_eq!(edit.pressing.year, Some(1999));
    assert_eq!(edit.pressing.format.as_deref(), Some("CD"));
    assert_eq!(edit.pressing.label.as_deref(), Some("Test Label"));
    assert_eq!(edit.pressing.catalog_number.as_deref(), Some("CAT-001"));
    assert_eq!(edit.pressing.country.as_deref(), Some("US"));
    assert_eq!(edit.pressing.barcode.as_deref(), Some("0123456789"));
    assert_eq!(edit.tracks.len(), 2);
    assert_eq!(edit.tracks[0].title, "Cached Track 1");
    assert_eq!(edit.tracks[1].title, "Cached Track 2");

    // Reset is read-only: identity rows and metadata-source columns
    // stay exactly as they were.
    let identities = db.get_release_identities(&release.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some("mb-release-1")
    );
    let saved_release = db.find_release_by_id(&release.id).await.unwrap().unwrap();
    assert_eq!(
        saved_release.metadata_source,
        ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        saved_release.metadata_source_release_id.as_deref(),
        Some("mb-release-1")
    );
    // And it doesn't touch the persisted album / release / tracks either —
    // the projected values are returned to the caller, who decides whether
    // to save them via apply_release_metadata_user_edit.
    let saved_album = db.find_album_by_id(&album.id).await.unwrap().unwrap();
    assert_eq!(saved_album.title, "Original Album");
    let saved_tracks = db.get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(saved_tracks[0].title, "Original Track 1");
}

#[tokio::test]
async fn reset_mb_approximate_clears_pressing_fields() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let mut release = make_release(&album.id);
    release.metadata_source = ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-release-2".to_string());
    let t1 = make_track(&release.id, 1, "Original Track");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&t1).await.unwrap();

    // Approximate: identity row carries the group but NULL release_id.
    db.insert_release_identities(
        &release.id,
        &[ReleaseIdentity {
            source: MetadataSource::MusicBrainz,
            source_group_id: "mb-rg-2".to_string(),
            source_release_id: None,
        }],
    )
    .await
    .unwrap();

    db.insert_release_metadata(&DbReleaseMetadata::new(
        &release.id,
        "musicbrainz",
        mb_release_json(
            "mb-release-2",
            "mb-rg-2",
            "Cached Album",
            "Cached Artist",
            &["Cached Track"],
        ),
        Uuid::new_v4().to_string(),
        Utc::now(),
    ))
    .await
    .unwrap();

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    // Album-group-stable fields project from cache.
    assert_eq!(edit.album_title, "Cached Album");
    assert_eq!(edit.album_artist_names, vec!["Cached Artist".to_string()]);
    assert_eq!(edit.tracks.len(), 1);
    assert_eq!(edit.tracks[0].title, "Cached Track");
    // Pressing-level fields are cleared because the identity row's
    // `source_release_id` is NULL — Approximate doesn't claim a pressing.
    assert_eq!(edit.pressing.year, None);
    assert_eq!(edit.pressing.format, None);
    assert_eq!(edit.pressing.label, None);
    assert_eq!(edit.pressing.catalog_number, None);
    assert_eq!(edit.pressing.country, None);
    assert_eq!(edit.pressing.barcode, None);
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
async fn reset_discogs_exact_returns_full_pressing_data_from_cache() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let mut release = make_release(&album.id);
    release.metadata_source = ReleaseMetadataSource::Discogs;
    release.metadata_source_release_id = Some("12345".to_string());
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
            source_release_id: Some("12345".to_string()),
        }],
    )
    .await
    .unwrap();

    db.insert_release_metadata(&DbReleaseMetadata::new(
        &release.id,
        "discogs",
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
        Uuid::new_v4().to_string(),
        Utc::now(),
    ))
    .await
    .unwrap();

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Cached Discogs Album");
    assert_eq!(
        edit.album_artist_names,
        vec!["Cached Discogs Artist".to_string()]
    );
    assert_eq!(edit.pressing.year, Some(1985));
    assert_eq!(edit.pressing.format.as_deref(), Some("CD"));
    assert_eq!(edit.pressing.label.as_deref(), Some("Cached Label"));
    assert_eq!(edit.pressing.catalog_number.as_deref(), Some("CACHE-1"));
    assert_eq!(edit.pressing.country.as_deref(), Some("JP"));
    assert_eq!(edit.tracks.len(), 1);
    assert_eq!(edit.tracks[0].title, "Cached Discogs Track");
}

#[tokio::test]
async fn reset_discogs_approximate_clears_pressing_fields() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let mut release = make_release(&album.id);
    release.metadata_source = ReleaseMetadataSource::Discogs;
    release.metadata_source_release_id = Some("22345".to_string());
    let t1 = make_track(&release.id, 1, "Original Track");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&t1).await.unwrap();

    db.insert_release_identities(
        &release.id,
        &[ReleaseIdentity {
            source: MetadataSource::Discogs,
            source_group_id: "67891".to_string(),
            source_release_id: None,
        }],
    )
    .await
    .unwrap();

    db.insert_release_metadata(&DbReleaseMetadata::new(
        &release.id,
        "discogs",
        discogs_release_json(
            22345,
            "Cached Discogs Album",
            998,
            "Cached Discogs Artist",
            1985,
            "Cached Label",
            "CACHE-2",
            "JP",
            &["Cached Discogs Track"],
        ),
        Uuid::new_v4().to_string(),
        Utc::now(),
    ))
    .await
    .unwrap();

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    // Album-group-stable fields seed.
    assert_eq!(edit.album_title, "Cached Discogs Album");
    assert_eq!(
        edit.album_artist_names,
        vec!["Cached Discogs Artist".to_string()]
    );
    assert_eq!(edit.tracks.len(), 1);
    assert_eq!(edit.tracks[0].title, "Cached Discogs Track");
    // Pressing fields cleared.
    assert_eq!(edit.pressing.year, None);
    assert_eq!(edit.pressing.format, None);
    assert_eq!(edit.pressing.label, None);
    assert_eq!(edit.pressing.catalog_number, None);
    assert_eq!(edit.pressing.country, None);
    assert_eq!(edit.pressing.barcode, None);
}

// ── File tags (Unknown) ─────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Copy a FLAC fixture into `dest_dir/{name}` and stamp it with the
/// given Vorbis tags. Mirrors the helper inside `file_tag_mapper`'s own
/// tests; we duplicate it here because that helper isn't exported.
fn copy_and_tag(
    source: &std::path::Path,
    dest_dir: &std::path::Path,
    name: &str,
    title: &str,
    artist: &str,
    album_title: &str,
    album_artist: &str,
    year: u16,
    track: u32,
) -> PathBuf {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::items::Timestamp;
    use lofty::tag::ItemKey;
    use lofty::tag::{Accessor, Tag, TagType};

    let dest = dest_dir.join(name);
    std::fs::copy(source, &dest).expect("copy fixture");

    let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album_title.to_string());
    tag.insert_text(ItemKey::AlbumArtist, album_artist.to_string());
    tag.set_date(Timestamp {
        year,
        month: None,
        day: None,
        hour: None,
        minute: None,
        second: None,
    });
    tag.set_track(track);
    tagged.insert_tag(tag);
    tagged
        .save_to_path(&dest, WriteOptions::default())
        .expect("save tags");
    dest
}

#[tokio::test]
async fn reset_file_tags_unknown_returns_tags_from_disk() {
    let (lm, db, tmp) = setup().await;

    // Create real audio files inside an unmanaged folder so the release
    // can resolve them via `local_file_path`.
    let audio_dir = tmp.path().join("source-folder");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let src = fixtures_dir().join("flac").join("01 Test Track 1.flac");
    let src2 = fixtures_dir().join("flac").join("02 Test Track 2.flac");
    let f1 = copy_and_tag(
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
    let f2 = copy_and_tag(
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
    release.metadata_source = ReleaseMetadataSource::FileTags;
    release.metadata_source_release_id = None;
    release.managed = false;
    let t1 = make_track(&release.id, 1, "Original Track 1");
    let t2 = make_track(&release.id, 2, "Original Track 2");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.upsert_release_local_copy(&bae_core::db::DbReleaseLocalCopy {
        release_id: release.id.clone(),
        unmanaged_path: Some(audio_dir.to_str().unwrap().to_string()),
        pinned_locally: false,
    })
    .await
    .unwrap();
    db.insert_track(&t1).await.unwrap();
    db.insert_track(&t2).await.unwrap();

    let now = Utc::now();
    let file1 = DbFile {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        original_filename: f1.file_name().unwrap().to_string_lossy().into_owned(),
        file_size: std::fs::metadata(&f1).unwrap().len() as i64,
        content_type: ContentType::Flac,
        created_at: now,
    };
    let file2 = DbFile {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        original_filename: f2.file_name().unwrap().to_string_lossy().into_owned(),
        file_size: std::fs::metadata(&f2).unwrap().len() as i64,
        content_type: ContentType::Flac,
        created_at: now,
    };
    db.insert_file(&file1).await.unwrap();
    db.insert_file(&file2).await.unwrap();

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Tag Album");
    assert_eq!(edit.album_artist_names, vec!["Tag Artist".to_string()]);
    assert_eq!(edit.pressing.year, Some(2010));
    assert_eq!(edit.pressing.format.as_deref(), Some("FLAC"));
    assert_eq!(edit.tracks.len(), 2);
    assert_eq!(edit.tracks[0].title, "Tag Track 1");
    assert_eq!(edit.tracks[1].title, "Tag Track 2");

    // Identity remained empty (Unknown), `metadata_source` stayed FileTags.
    let identities = db.get_release_identities(&release.id).await.unwrap();
    assert!(identities.is_empty());
    let saved = db.find_release_by_id(&release.id).await.unwrap().unwrap();
    assert_eq!(saved.metadata_source, ReleaseMetadataSource::FileTags);
    assert!(saved.metadata_source_release_id.is_none());
}

#[tokio::test]
async fn reset_mb_missing_cached_payload_errors() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Artist");
    let album = make_album(&artist.id, "Album");
    let mut release = make_release(&album.id);
    release.metadata_source = ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-release-missing".to_string());

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();

    let err = lm
        .reset_metadata_to_source(&release.id)
        .await
        .expect_err("missing cached payload should error");
    let msg = err.to_string();
    assert!(
        msg.contains("no cached MusicBrainz payload"),
        "unexpected error: {msg}"
    );
}

/// `set_identity` can redirect `metadata_source_release_id` to a different
/// pressing without re-fetching, leaving the `release_metadata` cache
/// pointing at the *previous* pressing. Reset must refuse to project the
/// stale payload — silently using it would surface the wrong pressing's
/// fields without telling the user.
#[tokio::test]
async fn reset_mb_cache_pointer_divergence_errors() {
    let (lm, db, _tmp) = setup().await;

    let artist = make_artist("Artist");
    let album = make_album(&artist.id, "Album");
    let mut release = make_release(&album.id);
    release.metadata_source = ReleaseMetadataSource::MusicBrainz;
    // Pointer says we want pressing Y…
    release.metadata_source_release_id = Some("mb-release-Y".to_string());

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();

    db.insert_release_identities(
        &release.id,
        &[ReleaseIdentity {
            source: MetadataSource::MusicBrainz,
            source_group_id: "mb-rg-1".to_string(),
            source_release_id: Some("mb-release-Y".to_string()),
        }],
    )
    .await
    .unwrap();

    // …but the cache still holds pressing X (a previous pressing in the same
    // group, redirected via `set_identity` without a fresh fetch).
    db.insert_release_metadata(&DbReleaseMetadata::new(
        &release.id,
        "musicbrainz",
        mb_release_json(
            "mb-release-X",
            "mb-rg-1",
            "Stale Pressing",
            "Artist",
            &["Stale Track"],
        ),
        Uuid::new_v4().to_string(),
        Utc::now(),
    ))
    .await
    .unwrap();

    let err = lm
        .reset_metadata_to_source(&release.id)
        .await
        .expect_err("stale cached payload should error");
    let msg = err.to_string();
    assert!(
        msg.contains("doesn't match current pointer"),
        "unexpected error: {msg}"
    );
    assert!(msg.contains("mb-release-X"), "missing cached id in: {msg}");
    assert!(msg.contains("mb-release-Y"), "missing pointer id in: {msg}");
}
