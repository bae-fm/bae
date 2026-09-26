#![cfg(feature = "test-utils")]
//! Reset metadata to source. Verifies `LibraryManager::reset_metadata_to_source`
//! re-runs the seeding projection from the stored release — fetching it first
//! on a device that never did — and returns the projected `ReleaseUserEdit`
//! shape, without writing the release's rows or touching its records.
use bae_test_support as support;

use bae_core::db::{Database, DbAlbum, DbArtist, DbFile, DbRelease, DbTrack};
use bae_core::import::payloads::ReleasePayloads;
use bae_core::import::{ArtistAssignment, ArtistCredit, Catalog, MetadataRef, ReleaseRecord};
use bae_core::util::content_type::ContentType;
use chrono::Utc;
use std::path::PathBuf;
use uuid::Uuid;

/// Store the release a lone provider document describes, as a fetch would.
async fn seed_release(db: &Database, catalog: Catalog, release_id: &str, json: String) {
    db.save_source_release(
        &ReleasePayloads::for_test(MetadataRef::new(catalog, release_id), json, Vec::new())
            .extract()
            .unwrap(),
    )
    .await
    .unwrap();
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

/// Release no catalog describes and whose draft came from nowhere — caller
/// wires the scenario under test.
fn make_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: bae_core::pressing::Pressing::blank(),
        draft_from_tags: false,
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
        side: Some(1),
        track_number: Some(n),
        duration_ms: Some(180_000),
        discogs_position: None,
        created_at: Utc::now(),
    }
}

/// The one record a release was read from, as a commit writes it.
fn record_read_from(catalog: Catalog, key: &str, group: &str) -> ReleaseRecord {
    ReleaseRecord::new(
        &MetadataRef::new(catalog, key),
        Some(group.to_string()),
        true,
    )
}

/// What a release says it was read from: a catalog's release, the files' own
/// tags, or nothing.
enum DraftSource {
    Record(ReleaseRecord),
    FileMetadata,
    Nothing,
}

/// Resetting is offered exactly when it can happen: the draft came off the
/// files' tags, or off a release this device holds or can fetch. MusicBrainz
/// can always be asked; Discogs only with a key, and this library holds none.
#[tokio::test]
async fn edit_seed_exposes_reset_eligibility_from_where_the_draft_was_read() {
    let (lm, db, _tmp) = support::setup_test_library().await;
    let artist = make_artist("Artist Name");
    db.insert_artist(&artist).await.unwrap();
    seed_release(
        &db,
        Catalog::Discogs,
        "4242",
        discogs_release_json(
            4242,
            "Album Title",
            999,
            "Artist Name",
            1985,
            "Label Name",
            "CAT-1",
            "Japan",
            &["Track Title"],
        ),
    )
    .await;

    for (index, (source, expected)) in [
        (
            DraftSource::Record(record_read_from(
                Catalog::MusicBrainz,
                "mb-release",
                "mb-group",
            )),
            true,
        ),
        (
            DraftSource::Record(record_read_from(Catalog::Discogs, "4242", "909")),
            true,
        ),
        (
            DraftSource::Record(record_read_from(Catalog::Discogs, "4343", "909")),
            false,
        ),
        (DraftSource::FileMetadata, true),
        (DraftSource::Nothing, false),
    ]
    .into_iter()
    .enumerate()
    {
        let album = make_album(&artist.id, &format!("Album {index}"));
        let mut release = make_release(&album.id);
        release.draft_from_tags = matches!(source, DraftSource::FileMetadata);
        db.insert_album(&album).await.unwrap();
        db.insert_release(&release).await.unwrap();
        if let DraftSource::Record(record) = &source {
            db.insert_release_records(&release.id, std::slice::from_ref(record))
                .await
                .unwrap();
        }

        let seed = lm.release_edit_seed(&release.id).await.unwrap();
        assert_eq!(seed.can_reset_to_source, expected, "case {index}");
    }
}

#[tokio::test]
async fn resetting_a_source_less_release_reports_that_it_has_no_provenance() {
    let (lm, db, _tmp) = support::setup_test_library().await;
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
        error
            .to_string()
            .contains("was read from nothing to reset to"),
        "unexpected error: {error}"
    );
}

// ── MusicBrainz ─────────────────────────────────────────────────────────

/// Build a minimal-but-valid MB release JSON with a release group, an
/// artist credit, and one track per supplied title — the release endpoint's
/// own shape.
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
        status: None,
        packaging: None,
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
            discs: vec![],
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
async fn reset_mb_returns_full_pressing_data_from_the_stored_release() {
    let (lm, db, _tmp) = support::setup_test_library().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let release = make_release(&album.id);
    let t1 = make_track(&release.id, 1, "Original Track 1");
    let t2 = make_track(&release.id, 2, "Original Track 2");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&t1).await.unwrap();
    db.insert_track(&t2).await.unwrap();

    db.insert_release_records(
        &release.id,
        &[ReleaseRecord::new(
            &MetadataRef::new(Catalog::MusicBrainz, "mb-release-1".to_string()),
            Some("mb-rg-1".to_string()),
            true,
        )],
    )
    .await
    .unwrap();

    seed_release(
        &db,
        Catalog::MusicBrainz,
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
        vec![ArtistAssignment::Credit {
            credit: ArtistCredit {
                name: "Cached Artist".to_string(),
                sort_name: Some("Cached Artist".to_string()),
                musicbrainz_artist_id: Some("mb-art-Cached Artist".to_string()),
                discogs_artist_id: None,
            },
        }]
    );
    assert_eq!(edit.pressing.year, Some(1999));
    assert_eq!(
        edit.pressing.facts.media,
        vec![bae_core::pressing::MediaCount {
            medium: bae_core::pressing::Medium::Cd,
            count: 1,
        }]
    );
    assert_eq!(edit.pressing.label.as_deref(), Some("Test Label"));
    assert_eq!(edit.pressing.catalog_number.as_deref(), Some("CAT-001"));
    assert_eq!(
        edit.pressing.facts.area,
        Some(bae_core::pressing::ReleaseArea::Country(
            bae_core::pressing::Country::from_code("US").unwrap()
        ))
    );
    assert_eq!(edit.pressing.barcode.as_deref(), Some("0123456789"));
    assert_eq!(edit.tracks.len(), 2);
    assert_eq!(edit.tracks[0].title, "Cached Track 1");
    assert_eq!(edit.tracks[1].title, "Cached Track 2");

    // Reset is read-only: the records stay exactly as they were.
    let records = db.get_release_records(&release.id).await.unwrap();
    assert_eq!(
        records,
        vec![record_read_from(
            Catalog::MusicBrainz,
            "mb-release-1",
            "mb-rg-1"
        )]
    );
    let saved_release = db.find_release_by_id(&release.id).await.unwrap().unwrap();
    assert!(!saved_release.draft_from_tags);
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
/// reads. Lets tests hand-roll a release without going through the HTTP
/// client.
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
async fn reset_discogs_returns_full_pressing_data_from_the_stored_release() {
    let (lm, db, _tmp) = support::setup_test_library().await;

    let artist = make_artist("Original Artist");
    let album = make_album(&artist.id, "Original Album");
    let release = make_release(&album.id);
    let t1 = make_track(&release.id, 1, "Original Track");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&t1).await.unwrap();

    db.insert_release_records(
        &release.id,
        &[ReleaseRecord::new(
            &MetadataRef::new(Catalog::Discogs, "12345".to_string()),
            Some("67890".to_string()),
            true,
        )],
    )
    .await
    .unwrap();

    seed_release(
        &db,
        Catalog::Discogs,
        "12345",
        discogs_release_json(
            12345,
            "Cached Discogs Album",
            999,
            "Cached Discogs Artist",
            1985,
            "Cached Label",
            "CACHE-1",
            "Japan",
            &["Cached Discogs Track"],
        ),
    )
    .await;

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Cached Discogs Album");
    assert_eq!(
        edit.album_artist_assignments,
        vec![ArtistAssignment::Credit {
            credit: ArtistCredit {
                name: "Cached Discogs Artist".to_string(),
                sort_name: Some("Cached Discogs Artist".to_string()),
                musicbrainz_artist_id: None,
                discogs_artist_id: Some("999".to_string()),
            },
        }]
    );
    assert_eq!(edit.pressing.year, Some(1985));
    assert_eq!(
        edit.pressing.facts.media,
        vec![bae_core::pressing::MediaCount {
            medium: bae_core::pressing::Medium::Cd,
            count: 1,
        }]
    );
    assert_eq!(edit.pressing.label.as_deref(), Some("Cached Label"));
    assert_eq!(edit.pressing.catalog_number.as_deref(), Some("CACHE-1"));
    assert_eq!(
        edit.pressing.facts.area,
        Some(bae_core::pressing::ReleaseArea::Country(
            bae_core::pressing::Country::from_code("JP").unwrap()
        ))
    );
    assert_eq!(edit.tracks.len(), 1);
    assert_eq!(edit.tracks[0].title, "Cached Discogs Track");
}

// ── File tags (Unknown) ─────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    bae_test_support::fixture_dir!()
}

#[tokio::test]
async fn reset_file_metadata_unknown_returns_tags_from_disk() {
    let (lm, db, tmp) = support::setup_test_library().await;

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
    release.draft_from_tags = true;
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
        source_audio: None,
        cloud_path: None,
        created_at: now,
    };
    let file2 = DbFile {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        original_filename: f2.file_name().unwrap().to_string_lossy().into_owned(),
        file_size: std::fs::metadata(&f2).unwrap().len() as i64,
        content_type: ContentType::Flac,
        source_audio: None,
        cloud_path: None,
        created_at: now,
    };
    db.insert_external_file_for_test(&file1, &f1).await.unwrap();
    db.insert_external_file_for_test(&file2, &f2).await.unwrap();

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Tag Album");
    assert_eq!(
        edit.album_artist_assignments,
        vec![ArtistAssignment::named("Tag Artist")]
    );
    assert_eq!(edit.pressing.year, Some(2010));
    assert!(edit.pressing.facts.media.is_empty());
    assert_eq!(edit.tracks.len(), 2);
    assert_eq!(edit.tracks[0].title, "Tag Track 1");
    assert_eq!(edit.tracks[1].title, "Tag Track 2");

    // No catalog describes it, and the draft still reads the files' own tags.
    let records = db.get_release_records(&release.id).await.unwrap();
    assert!(records.is_empty());
    let saved = db.find_release_by_id(&release.id).await.unwrap().unwrap();
    assert!(saved.draft_from_tags);
}

/// A device that never fetched the release the draft was read from — the
/// release synced here from the device that imported it — fetches it when
/// asked to reset, stores it, and resets from it.
#[tokio::test]
async fn reset_fetches_a_release_this_device_never_stored() {
    let (lm, db, _tmp) = support::setup_test_library().await;

    let artist = make_artist("Artist");
    let album = make_album(&artist.id, "Album");
    let release = make_release(&album.id);
    let track = make_track(&release.id, 1, "Original Track");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&track).await.unwrap();
    db.insert_release_records(
        &release.id,
        &[record_read_from(
            Catalog::MusicBrainz,
            "mb-release-remote",
            "mb-rg-remote",
        )],
    )
    .await
    .unwrap();
    lm.providers().musicbrainz().seed_release_cache(
        "mb-release-remote",
        mb_release_json(
            "mb-release-remote",
            "mb-rg-remote",
            "Fetched Album",
            "Fetched Artist",
            &["Fetched Track"],
        ),
    );

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();

    assert_eq!(edit.album_title, "Fetched Album");
    assert_eq!(edit.tracks[0].title, "Fetched Track");
    let stored = db
        .load_source_release(&MetadataRef::new(Catalog::MusicBrainz, "mb-release-remote"))
        .await
        .unwrap();
    assert!(
        stored.is_some(),
        "the fetched release is stored for the next read"
    );
}

/// A release this device neither holds nor can fetch — a Discogs release, and
/// no Discogs key — cannot be reset to, and says why rather than resetting to
/// anything else.
#[tokio::test]
async fn reset_to_a_release_that_cannot_be_fetched_fails_loud() {
    let (lm, db, _tmp) = support::setup_test_library().await;

    let artist = make_artist("Artist");
    let album = make_album(&artist.id, "Album");
    let release = make_release(&album.id);

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_release_records(
        &release.id,
        &[record_read_from(Catalog::Discogs, "4343", "909")],
    )
    .await
    .unwrap();

    let err = lm
        .reset_metadata_to_source(&release.id)
        .await
        .expect_err("a release that cannot be fetched cannot be reset to");
    assert!(
        err.to_string().contains("Discogs API key not configured"),
        "unexpected error: {err}"
    );
}

/// The records can be pointed at a different pressing without fetching it.
/// Stored releases are keyed by the source release, so the previous
/// pressing's cannot be read in the new one's place: reset reads the pressing
/// the record names, fetching it, rather than surfacing the wrong pressing's
/// fields.
#[tokio::test]
async fn reset_mb_reads_only_the_pressing_the_pointer_names() {
    let (lm, db, _tmp) = support::setup_test_library().await;

    let artist = make_artist("Artist");
    let album = make_album(&artist.id, "Album");
    // The record says we want pressing Y…
    let release = make_release(&album.id);
    let track = make_track(&release.id, 1, "Original Track");

    db.insert_artist(&artist).await.unwrap();
    db.insert_album(&album).await.unwrap();
    db.insert_release(&release).await.unwrap();
    db.insert_track(&track).await.unwrap();

    db.insert_release_records(
        &release.id,
        &[ReleaseRecord::new(
            &MetadataRef::new(Catalog::MusicBrainz, "mb-release-Y".to_string()),
            Some("mb-rg-1".to_string()),
            true,
        )],
    )
    .await
    .unwrap();

    // …and the only stored release is pressing X, the one it was pointed away
    // from.
    seed_release(
        &db,
        Catalog::MusicBrainz,
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
    lm.providers().musicbrainz().seed_release_cache(
        "mb-release-Y",
        mb_release_json(
            "mb-release-Y",
            "mb-rg-1",
            "Pointed Pressing",
            "Artist",
            &["Pointed Track"],
        ),
    );

    let edit = lm.reset_metadata_to_source(&release.id).await.unwrap();
    assert_eq!(edit.album_title, "Pointed Pressing");
    assert_eq!(edit.tracks[0].title, "Pointed Track");
}
