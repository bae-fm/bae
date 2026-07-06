use super::*;
use crate::config::{Config, ConfigHandle};
use crate::db::Database;
use crate::keys::KeyService;
use coven::FixedClock;
use coven::SequentialIdProvider;
use tempfile::TempDir;

async fn setup_import_service() -> (ImportService, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
        .await
        .unwrap();
    let library_dir = coven::LibraryDir::new(temp_dir.path());
    let library_id = format!("test-{}", temp_dir.path().display());
    let config = Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    crate::config::install_test_keyring();
    let manager = LibraryManager::new(
        database,
        library_dir,
        Arc::new(ConfigHandle::new(config)),
        KeyService::new(library_id),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let (_commands_tx, commands_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, _) = tokio::sync::broadcast::channel(16);
    (
        ImportService {
            commands_rx,
            event_tx,
            library_manager: manager,
        },
        temp_dir,
    )
}

fn write_test_jpeg(path: &Path) {
    let image = ::image::RgbImage::from_pixel(1, 1, ::image::Rgb([0, 0, 0]));
    image.save(path).unwrap();
}

#[test]
fn affected_roots_maps_changed_paths_to_their_watched_roots() {
    let root_a = PathBuf::from("/music/new rips");
    let root_b = PathBuf::from("/downloads/bandcamp");
    let roots = vec![root_a.clone(), root_b.clone()];

    // A change inside one root flags only that root.
    let changed = [Path::new("/music/new rips/Album/01.flac")];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a.clone()]);

    // Changes under both roots flag both, in roots order, deduped.
    let changed = [
        Path::new("/downloads/bandcamp/X/cover.jpg"),
        Path::new("/music/new rips/Y"),
        Path::new("/music/new rips/Z"),
    ];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a, root_b]);

    // A change outside every watched root flags nothing.
    let changed = [Path::new("/elsewhere/file")];
    assert!(affected_roots(&changed, &roots).is_empty());
}

/// `common_ancestor` derives the local-path root by folding over the
/// files' parent dirs. It must compare path components, not string
/// prefixes, so `/m/Album` and `/m/Album2` collapse to `/m` (a string
/// prefix would wrongly keep `/m/Album`), and an ancestor argument returns
/// itself rather than descending.
#[test]
fn common_ancestor_cases() {
    use std::path::Path;
    // Sibling files share their parent.
    assert_eq!(
        common_ancestor(Path::new("/m/Album/01.flac"), Path::new("/m/Album/02.flac")),
        Path::new("/m/Album")
    );
    // `a` is already an ancestor of `b`: keep `a`.
    assert_eq!(
        common_ancestor(Path::new("/m/Album"), Path::new("/m/Album/Disc1/01.flac")),
        Path::new("/m/Album")
    );
    // Component-wise, not string-prefix: Album vs Album2 don't share /m/Album.
    assert_eq!(
        common_ancestor(Path::new("/m/Album/x"), Path::new("/m/Album2/y")),
        Path::new("/m")
    );
    // Disjoint trees collapse to the root.
    assert_eq!(
        common_ancestor(Path::new("/a/b"), Path::new("/c/d")),
        Path::new("/")
    );
}

/// image_cover_priority decides which folder image wins as the cover when
/// the user makes no explicit pick: a name containing "cover" or "front"
/// (case-insensitive, anywhere in the name) ranks first, everything else
/// second. The fallback sort relies on this ordering.
#[test]
fn image_cover_priority_ranks_front_and_cover_first() {
    assert_eq!(ImportService::image_cover_priority("Cover.jpg"), 0);
    assert_eq!(ImportService::image_cover_priority("front.png"), 0);
    assert_eq!(ImportService::image_cover_priority("FRONT.JPG"), 0);
    assert_eq!(
        ImportService::image_cover_priority("album-front-scan.jpg"),
        0
    );
    assert_eq!(ImportService::image_cover_priority("Back.jpg"), 1);
    assert_eq!(ImportService::image_cover_priority("inlay.png"), 1);
    assert_eq!(ImportService::image_cover_priority("disc1.jpg"), 1);
}

#[tokio::test]
async fn explicit_bmp_cover_is_selected() {
    let (service, tmp) = setup_import_service().await;
    let bmp = tmp.path().join("cover.bmp");
    let jpg = tmp.path().join("front.jpg");
    std::fs::write(&bmp, b"bmp bytes").unwrap();
    std::fs::write(&jpg, b"jpg bytes").unwrap();
    let discovered = vec![
        DiscoveredFile {
            path: bmp.clone(),
            relative_path: "cover.bmp".to_string(),
            size: 9,
        },
        DiscoveredFile {
            path: jpg,
            relative_path: "front.jpg".to_string(),
            size: 9,
        },
    ];

    let (image, bytes) = service
        .build_cover_image_record("release-1", &discovered, Some("cover.bmp"))
        .unwrap()
        .expect("selected cover should build a record");

    assert_eq!(
        image.content_type,
        crate::util::content_type::ContentType::Bmp
    );
    assert_eq!(image.source_url.as_deref(), Some("release://cover.bmp"));
    assert_eq!(bytes, b"bmp bytes");
}

#[tokio::test]
async fn explicit_local_cover_missing_from_discovered_images_is_an_error() {
    let (service, tmp) = setup_import_service().await;
    let fallback = tmp.path().join("front.jpg");
    std::fs::write(&fallback, b"jpg bytes").unwrap();
    let discovered = vec![DiscoveredFile {
        path: fallback,
        relative_path: "front.jpg".to_string(),
        size: 9,
    }];

    let err = service
        .build_cover_image_record("release-1", &discovered, Some("cover.bmp"))
        .unwrap_err();

    assert!(
        err.contains("Selected cover") && err.contains("not found"),
        "got: {err}"
    );
}

#[tokio::test]
async fn explicit_local_cover_with_no_discovered_images_is_an_error() {
    let (service, _tmp) = setup_import_service().await;

    let err = service
        .build_cover_image_record("release-1", &[], Some("cover.bmp"))
        .unwrap_err();

    assert!(
        err.contains("Selected cover") && err.contains("not found"),
        "got: {err}"
    );
}

#[tokio::test]
async fn selected_local_cover_path_must_match_discovered_file() {
    let (service, tmp) = setup_import_service().await;
    let folder = tmp.path().join("release");
    std::fs::create_dir(&folder).unwrap();
    write_test_jpeg(&folder.join("front.jpg"));
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("01.flac"),
    )
    .unwrap();

    let result = service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            folder.to_string_lossy().into_owned(),
            folder,
            Some(CoverSelection::Local("cover.bmp".to_string())),
            StorageMode::Local,
            false,
            crate::import::IdentityChoice::Unknown,
            None,
        )
        .await;

    let err = result.unwrap_err();
    assert!(
        err.contains("Selected cover cover.bmp not found"),
        "got: {err}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_selected_cover_is_an_error() {
    use std::os::unix::fs::PermissionsExt;

    let (service, tmp) = setup_import_service().await;
    let cover = tmp.path().join("cover.jpg");
    std::fs::write(&cover, b"jpg bytes").unwrap();
    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o000)).unwrap();
    let discovered = vec![DiscoveredFile {
        path: cover.clone(),
        relative_path: "cover.jpg".to_string(),
        size: 9,
    }];

    let result = service.build_cover_image_record("release-1", &discovered, Some("cover.jpg"));

    std::fs::set_permissions(&cover, std::fs::Permissions::from_mode(0o600)).unwrap();
    let err = result.unwrap_err();
    assert!(err.contains("Failed to read cover art"), "got: {err}");
}

async fn rescan_seeded_root(
    service: &ImportService,
    root: &Path,
) -> (
    HashMap<PathBuf, HashSet<String>>,
    tokio::sync::broadcast::Receiver<crate::import::handle::ImportEvent>,
) {
    let mut last_keys =
        HashMap::from([(root.to_path_buf(), HashSet::from(["old-key".to_string()]))]);
    let (event_tx, events) = tokio::sync::broadcast::channel(16);
    let folder_registry = Arc::new(Mutex::new(
        crate::import::folder_registry::ImportFolderRegistry::load(
            service.library_manager.library_dir(),
        ),
    ));

    ImportService::rescan_and_reconcile(
        root,
        &mut last_keys,
        &event_tx,
        &service.library_manager,
        &folder_registry,
    )
    .await;

    (last_keys, events)
}

#[tokio::test]
async fn rescan_missing_root_removes_previous_candidates() {
    let (service, tmp) = setup_import_service().await;
    let root = tmp.path().join("missing-root");
    let (last_keys, mut events) = rescan_seeded_root(&service, &root).await;

    match events.recv().await.unwrap() {
        crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }) => {
            assert_eq!(candidate_key, "old-key");
        }
        event => panic!("expected candidate removal, got {event:?}"),
    }
    assert!(matches!(
        events.recv().await.unwrap(),
        crate::import::handle::ImportEvent::Scan(ScanEvent::Finished)
    ));
    assert!(last_keys.get(&root).unwrap().is_empty());
}

#[tokio::test]
async fn rescan_non_directory_root_keeps_previous_candidates() {
    let (service, tmp) = setup_import_service().await;
    let root = tmp.path().join("not-a-directory");
    std::fs::write(&root, b"not a directory").unwrap();
    let (last_keys, mut events) = rescan_seeded_root(&service, &root).await;

    match events.recv().await.unwrap() {
        crate::import::handle::ImportEvent::Scan(ScanEvent::Failed { error }) => {
            assert!(
                error.to_lowercase().contains("not a directory"),
                "got: {error}"
            );
        }
        event => panic!("expected scan failure, got {event:?}"),
    }
    assert_eq!(last_keys.get(&root).unwrap().len(), 1);
    assert!(last_keys.get(&root).unwrap().contains("old-key"));
}

#[test]
fn resolve_file_content_type_uses_probe_for_new_audio_formats() {
    let fixture = |name: &str| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures")
            .join("audio-format")
            .join(name)
    };
    for (name, expected) in [
        (
            "placeholder-pcm.wav",
            crate::util::content_type::ContentType::Pcm,
        ),
        (
            "placeholder-pcm.aiff",
            crate::util::content_type::ContentType::Pcm,
        ),
        (
            "placeholder-opus.opus",
            crate::util::content_type::ContentType::Opus,
        ),
        (
            "placeholder-vorbis.ogg",
            crate::util::content_type::ContentType::Vorbis,
        ),
        (
            "placeholder-wavpack.wv",
            crate::util::content_type::ContentType::WavPack,
        ),
        (
            "placeholder-dsd.dsf",
            crate::util::content_type::ContentType::Dsd,
        ),
        (
            "placeholder-dsd.dff",
            crate::util::content_type::ContentType::Dsd,
        ),
    ] {
        assert_eq!(
            resolve_file_content_type(&fixture(name)).unwrap(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn import_trace_line_escapes_json_strings() {
    let line = import_trace_line(
        "2024-01-01T00:00:00+00:00".to_string(),
        "import-1",
        "Album \\ Title\nA",
        "Artist \"Name\"",
        Duration::from_millis(42),
        &[("resolve_metadata", Duration::from_millis(7))],
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&line).expect("trace line must be valid JSON");
    assert_eq!(parsed["album"], "Album \\ Title\nA");
    assert_eq!(parsed["artist"], "Artist \"Name\"");
    assert_eq!(parsed["steps"]["resolve_metadata"], 7);
}

/// Deterministic clock for the `apply_user_edit_to_seed` tests — the
/// exact instant is immaterial to what they assert (artist-row
/// preservation / rebuild), only that the same one feeds every row.
fn test_clock() -> FixedClock {
    FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    )
}

// ── apply_identity_choice ──────────────────────────────────────────

fn mb_id_exact(group: &str, release: &str) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::MusicBrainz,
        source_group_id: group.to_string(),
        source_release_id: Some(release.to_string()),
    }
}

fn discogs_id_exact(group: &str, release: &str) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::Discogs,
        source_group_id: group.to_string(),
        source_release_id: Some(release.to_string()),
    }
}

fn mb_release_ref() -> crate::import::MetadataRef {
    crate::import::MetadataRef::new("rel-mb", crate::import::MetadataSource::MusicBrainz)
}

#[test]
fn exact_choice_passes_mapper_output_through() {
    let mapper_output = vec![
        mb_id_exact("rg-mb", "rel-mb"),
        discogs_id_exact("master-d", "rel-d"),
    ];
    let result = apply_identity_choice(
        &mapper_output,
        &crate::import::IdentityChoice::Exact {
            release_ref: mb_release_ref(),
        },
    );
    assert_eq!(result, mapper_output);
}

#[test]
fn approximate_choice_nulls_release_ids_on_every_row() {
    // Both the primary identity row AND any cross-source row from
    // url-rels mirror the user's choice — Approximate means a
    // group-level claim across the board.
    let mapper_output = vec![
        mb_id_exact("rg-mb", "rel-mb"),
        discogs_id_exact("master-d", "rel-d"),
    ];
    let result = apply_identity_choice(
        &mapper_output,
        &crate::import::IdentityChoice::Approximate {
            release_ref: mb_release_ref(),
        },
    );
    assert_eq!(result.len(), 2);
    for id in &result {
        assert!(
            id.source_release_id.is_none(),
            "Approximate must NULL source_release_id, got {id:?}"
        );
    }
    // Group IDs survive — the claim is at the group level.
    assert_eq!(result[0].source_group_id, "rg-mb");
    assert_eq!(result[1].source_group_id, "master-d");
}

#[test]
fn unknown_choice_passes_empty_mapper_output_through() {
    // The file-tag mapper emits an empty identity vec; the choice
    // post-process is a no-op. Confirms Unknown writes zero
    // `release_identities` rows even when paired with a mapper
    // that somehow surfaces rows (defensive — file_tag_mapper
    // never does, but the projection is the algebraic identity).
    let result = apply_identity_choice(&[], &crate::import::IdentityChoice::Unknown);
    assert!(result.is_empty());
}

// ── apply_user_edit_to_seed ────────────────────────────────────────

fn make_seed_album_release_track() -> (
    crate::db::DbAlbum,
    crate::db::DbRelease,
    crate::db::DbTrack,
    crate::db::DbArtist,
) {
    let now = chrono::Utc::now();
    let artist = crate::db::DbArtist {
        id: "artist-orig".to_string(),
        name: "Artist Name".to_string(),
        sort_name: Some("Artist Name".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    let album = crate::db::DbAlbum {
        id: "album-1".to_string(),
        title: "Album Title".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2020),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = crate::db::DbRelease {
        id: "release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2020),
            format: Some("CD".to_string()),
            label: Some("Label Name".to_string()),
            catalog_number: Some("CAT-001".to_string()),
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
        metadata_source_release_id: Some("rel-mb".to_string()),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = crate::db::DbTrack {
        id: "track-1".to_string(),
        release_id: release.id.clone(),
        title: "Original Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180000),
        discogs_position: None,
        created_at: now,
    };
    (album, release, track, artist)
}

#[test]
fn user_edit_overrides_seeded_pressing_fields() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_names: vec!["Edited Artist".to_string()],
        pressing: crate::import::PressingEdit {
            year: Some(1995),
            format: Some("Vinyl".to_string()),
            label: Some("Edited Label".to_string()),
            catalog_number: Some("EDIT-1".to_string()),
            country: Some("JP".to_string()),
            barcode: Some("4943674000000".to_string()),
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: "Edited Track".to_string(),
            side: 1,
            track_number: Some(1),
            artist_names: vec![],
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    assert_eq!(album.title, "Edited Title");
    assert_eq!(release.pressing.year, Some(1995));
    assert_eq!(release.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(release.pressing.label.as_deref(), Some("Edited Label"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("EDIT-1"));
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(release.pressing.barcode.as_deref(), Some("4943674000000"));
    assert_eq!(tracks[0].title, "Edited Track");

    // The new album artist gets a placeholder DbArtist row so the
    // import pipeline can canonicalize via find_or_create_artists.
    assert!(artists.iter().any(|a| a.name == "Edited Artist"));
    assert_eq!(
        album.artist_id,
        artists
            .iter()
            .find(|a| a.name == "Edited Artist")
            .unwrap()
            .id
    );
}

#[test]
fn user_edit_can_fill_country_for_approximate_seed() {
    // Approximate seed clears pressing fields; the user can supply
    // them via the editor and the overlay applies the value.
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    // Simulate the Approximate-cleared release row.
    release.pressing = crate::db::Pressing::blank();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec![artists[0].name.clone()],
        pressing: crate::import::PressingEdit {
            country: Some("JP".to_string()),
            ..crate::import::PressingEdit::blank()
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert!(release.pressing.year.is_none());
    assert!(release.pressing.format.is_none());
}

#[test]
fn user_edit_track_count_mismatch_is_an_error() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: "T".to_string(),
        album_artist_names: vec!["A".to_string()],
        pressing: crate::import::PressingEdit::blank(),
        // Two edits but seed has one track.
        tracks: vec![
            crate::import::TrackUserEdit {
                title: "X".to_string(),
                side: 1,
                track_number: Some(1),
                artist_names: vec![],
            },
            crate::import::TrackUserEdit {
                title: "Y".to_string(),
                side: 1,
                track_number: Some(2),
                artist_names: vec![],
            },
        ],
    };

    let err = apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap_err();
    assert!(err.contains("Track count mismatch"), "got: {err}");
}

/// Source-id linkage on artist rows (e.g. `musicbrainz_artist_id`)
/// must survive a user edit that doesn't touch artist names. The
/// editor round-trips an unchanged artist field as the same string
/// it was seeded with, so the apply step must compare and short-
/// circuit rather than rebuild rows from name-only placeholders.
#[test]
fn user_edit_preserves_source_id_artist_rows_when_names_unchanged() {
    let now = chrono::Utc::now();
    // Seeded artist row carrying the MB id the mapper attached.
    let seed_artist = crate::db::DbArtist {
        id: "artist-mb".to_string(),
        name: "Artist Name".to_string(),
        sort_name: Some("Artist Name".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-artist-1".to_string()),
        created_at: now,
    };
    let album = crate::db::DbAlbum {
        id: "album-1".to_string(),
        title: "Album Title".to_string(),
        artist_id: seed_artist.id.clone(),
        year: Some(2020),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = crate::db::DbRelease {
        id: "release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2020),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
        metadata_source_release_id: Some("rel-mb".to_string()),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = crate::db::DbTrack {
        id: "track-1".to_string(),
        release_id: release.id.clone(),
        title: "Track Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: None,
        discogs_position: None,
        created_at: now,
    };
    // Seeded track credit pointing at the MB-id-bearing artist.
    let seed_track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &seed_artist.id,
        0,
        "track-artist-1".to_string(),
        now,
    );

    let mut album = album;
    let mut release = release;
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist.clone()];
    let mut album_artists = Vec::<crate::db::DbAlbumArtist>::new();
    let mut track_artists = vec![seed_track_artist.clone()];

    // The user changes pressing fields but leaves artist names
    // alone. The track's edit ships `artist_names = []` because
    // the editor's "no override" form maps to empty when the
    // track's credit equals the album's.
    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec![seed_artist.name.clone()],
        pressing: crate::import::PressingEdit {
            year: Some(1995),
            ..crate::import::PressingEdit::blank()
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    // The MB-id-bearing artist row must still exist with its
    // source binding intact — no fresh placeholder created.
    assert_eq!(artists.len(), 1, "no extra placeholder rows expected");
    assert_eq!(
        artists[0].musicbrainz_artist_id.as_deref(),
        Some("mb-artist-1"),
        "MB artist id must survive the edit",
    );
    assert_eq!(
        album.artist_id, seed_artist.id,
        "album.artist_id should still reference the seeded row",
    );

    // Track credit must still reference the seeded artist row.
    assert_eq!(track_artists.len(), 1);
    assert_eq!(track_artists[0].artist_id, seed_artist.id);
}

/// User-renaming an artist must rebuild the credit rows. The new
/// name has no source binding, so the inserted `DbArtist` row
/// carries `None` for both source ids.
#[test]
fn user_edit_renaming_album_artist_rebuilds_credits() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist.clone()];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec!["Different Artist".to_string()],
        pressing: crate::import::PressingEdit::blank(),
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    let new_artist = artists
        .iter()
        .find(|a| a.name == "Different Artist")
        .expect("new placeholder should be inserted");
    assert!(new_artist.musicbrainz_artist_id.is_none());
    assert!(new_artist.discogs_artist_id.is_none());
    assert_eq!(album.artist_id, new_artist.id);
}

// ── build_audio_formats: CUE track byte windows ────────────────────

/// Build the `TrackFile::CueBacked` list for a single-file CUE album, reusing
/// the import pipeline's own analysis (`analyze_cue_audio`) so the container is
/// probed exactly as a real import probes it.
fn cue_backed_tracks(dir: &str) -> Vec<TrackFile> {
    let audio_path = PathBuf::from(format!("{dir}/Test Album.ape"));
    let cue_path = PathBuf::from(format!("{dir}/Test Album.cue"));
    let cue_sheet =
        crate::cue_flac::CueFlacProcessor::parse_cue_sheet(&cue_path).expect("parse cue");
    let analysis =
        crate::import::track_to_file_mapper::analyze_cue_audio(&audio_path).expect("analyze ape");
    let cue_pair = Arc::new(crate::import::types::CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![crate::import::types::CueAnalyzedAudioFile {
            file_reference: "Test Album.ape".to_string(),
            path: audio_path.clone(),
            analysis,
        }],
    });
    (0..cue_pair.cue_sheet.tracks.len())
        .map(|index| TrackFile::CueBacked {
            db_track: DbTrack {
                id: format!("track-{index}"),
                release_id: "rel".to_string(),
                title: format!("Track {index}"),
                side: 1,
                track_number: Some(index as i32 + 1),
                duration_ms: None,
                discogs_position: None,
                created_at: test_clock().0,
            },
            file_path: audio_path.clone(),
            cue_pair: Arc::clone(&cue_pair),
            cue_index: index,
        })
        .collect()
}

/// A CUE track's read-ahead ceiling is its `end_byte`; playback fills up to it
/// and stops, so every non-last track must carry a real end byte or the fill
/// streams the whole rest of the shared file. Ends derive from the next track's
/// start byte (`start[N+1]`), computed via `seek_landing_bytes` -- the AVIO
/// landing, defined for every format, including APE whose packets carry no byte
/// position. This drives `build_audio_formats` over the APE CUE fixture and
/// asserts the user-visible outcome: the two non-last tracks get `Some`,
/// ascending, in-file end bytes and the last track runs to EOF (`None`).
#[test]
fn build_audio_formats_gives_ape_cue_tracks_real_end_bytes() {
    crate::audio_codec::init();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_ape");
    let tracks = cue_backed_tracks(dir);
    assert_eq!(
        tracks.len(),
        3,
        "fixture is a 3-track single-file CUE album"
    );

    let file_size = std::fs::metadata(format!("{dir}/Test Album.ape"))
        .unwrap()
        .len() as i64;
    let mut file_ids = HashMap::new();
    file_ids.insert(
        PathBuf::from(format!("{dir}/Test Album.ape")),
        "file-1".to_string(),
    );

    let built = ImportService::build_audio_formats(
        &tracks,
        &file_ids,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .expect("build_audio_formats");

    let main_segments: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == crate::db::DbAudioSegmentRole::Main)
        .collect();
    let ends: Vec<Option<i64>> = main_segments
        .iter()
        .map(|segment| segment.end_byte)
        .collect();
    // Non-last tracks carry a real, ascending, in-file end byte.
    let e0 = ends[0].expect("track 1 (non-last) must have an end byte");
    let e1 = ends[1].expect("track 2 (non-last) must have an end byte");
    assert!(
        e0 > 0 && e0 < file_size,
        "track 1 end within file: {e0} of {file_size}"
    );
    assert!(
        e1 > 0 && e1 < file_size,
        "track 2 end within file: {e1} of {file_size}"
    );
    assert!(e1 > e0, "end bytes ascend track to track: {ends:?}");
    // The last track runs to EOF.
    assert_eq!(ends[2], None, "the last track runs to EOF");

    // Each track's end is the next track's start byte -- one boundary, not two.
    assert_eq!(
        main_segments[1].start_byte,
        Some(e0),
        "track 2 starts where track 1 ends"
    );
    assert_eq!(
        main_segments[2].start_byte,
        Some(e1),
        "track 3 starts where track 2 ends"
    );
}
