// ── File Tags metadata source ────────────────────────────────────────────────

fn file_tag_artist_assignment(name: &str) -> ArtistAssignment {
    ArtistAssignment::New {
        seed: bae_core::import::NewArtistSeed {
            name: name.to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: None,
        },
    }
}

/// File Tags commit reads embedded tags, writes zero `release_identities`
/// rows, stores File Tags provenance, and seeds the album / tracks
/// from what's on disk. No external source consulted.
#[tokio::test]
async fn file_tags_import_seeds_from_file_tags_and_writes_no_identity() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album From Tags",
        "Artist From Tags",
        Some(2003),
        &[
            TaggedTrack {
                filename: "01.flac",
                title: "Track One",
                track_number: 1,
            },
            TaggedTrack {
                filename: "02.flac",
                title: "Track Two",
                track_number: 2,
            },
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(
        release.metadata_provenance,
        Some(MetadataProvenance::FileTags),
    );
    assert_eq!(release.pressing.year, Some(2003));
    assert_eq!(release.pressing.format, None);

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert!(
        identities.is_empty(),
        "File Tags imports must write zero external identity rows, got {identities:?}",
    );

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Album From Tags");

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
}

#[tokio::test]
async fn file_tags_preview_for_cue_matches_commit_layout() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("cue-album");
    fs::create_dir_all(&album_dir).unwrap();
    copy_cue_flac_fixture(&album_dir);

    let candidate_key = album_dir.to_string_lossy().into_owned();
    f.handle
        .add_watched_folder(candidate_key.clone())
        .await
        .unwrap();
    f.handle
        .refresh_watched_folder(candidate_key.clone())
        .await
        .unwrap();
    let preview = f
        .handle
        .preview_file_tags_for_folder(candidate_key)
        .await
        .unwrap();

    assert_eq!(preview.album_title, "Test Album");
    assert_eq!(
        preview.album_artist_assignments,
        vec![file_tag_artist_assignment("Test Artist")]
    );
    assert_eq!(preview.pressing.year, None);
    assert_eq!(preview.pressing.format, None);

    let preview_tracks: Vec<(String, TrackArtistAssignments)> = preview
        .tracks
        .iter()
        .map(|t| (t.title.clone(), t.artist_assignments.clone()))
        .collect();
    assert_eq!(
        preview_tracks,
        vec![
            (
                "Track One (Silence)".to_string(),
                TrackArtistAssignments::Explicit(vec![file_tag_artist_assignment("Test Artist")]),
            ),
            (
                "Track Two (White Noise)".to_string(),
                TrackArtistAssignments::Explicit(vec![file_tag_artist_assignment("Test Artist")]),
            ),
            (
                "Track Three (Brown Noise)".to_string(),
                TrackArtistAssignments::Explicit(vec![file_tag_artist_assignment("Test Artist")]),
            ),
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "cue".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;
    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(preview.album_title, album.title);

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(preview.pressing.year, release.pressing.year);
    assert_eq!(preview.pressing.format, release.pressing.format);

    let album_detail =
        f.db.find_album_detail(&album_id)
            .await
            .unwrap()
            .expect("album detail");
    let committed_album_artist_assignments: Vec<ArtistAssignment> = album_detail
        .artists
        .iter()
        .map(|artist| file_tag_artist_assignment(&artist.name))
        .collect();
    assert!(album_detail
        .artists
        .iter()
        .all(|artist| artist.sort_name.is_none()));
    assert_eq!(
        preview.album_artist_assignments,
        committed_album_artist_assignments
    );

    let release_detail =
        f.db.find_release_detail(&release_id)
            .await
            .unwrap()
            .expect("release detail");
    let committed_tracks: Vec<(String, TrackArtistAssignments)> = release_detail
        .tracks
        .iter()
        .map(|track| {
            (
                track.track.title.clone(),
                TrackArtistAssignments::Explicit(
                    track
                        .artists
                        .iter()
                        .map(|artist| file_tag_artist_assignment(&artist.name))
                        .collect(),
                ),
            )
        })
        .collect();
    assert_eq!(preview_tracks, committed_tracks);
}

/// A tagged rip whose only artwork is embedded in the audio (no folder
/// image, no remote selection) gets that embedded picture as its cover.
#[tokio::test]
async fn file_tags_import_seeds_embedded_cover_when_no_folder_image() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files_with_embedded_cover(
        &album_dir,
        "Embedded Cover Album",
        "Artist",
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("embedded cover should be written when no folder/remote image exists");
    assert_eq!(
        cover.source, "embedded",
        "cover must be sourced from the embedded picture"
    );

    // The embedded picture (≤600) keeps its dimensions but the store path
    // re-encodes it to JPEG, so assert on the decoded image, not raw bytes.
    let bytes = support::read_cover_image_blob(&f.library_manager, &release_id)
        .await
        .expect("cover blob readable");
    assert_eq!(
        image::guess_format(&bytes).unwrap(),
        image::ImageFormat::Jpeg
    );
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), EMBEDDED_COVER_DIMS);

    assert_cover_row_describes_stored_bytes(&f, &release_id).await;
}

/// Embedded artwork leads the folder's images when File Tags supplies both.
#[tokio::test]
async fn file_tags_import_embedded_cover_wins_over_folder_image() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files_with_embedded_cover(
        &album_dir,
        "Both Covers Album",
        "Artist",
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );
    // A folder image alongside the embedded-cover audio. No explicit
    // selection — File Tags' embedded artwork still leads.
    let scans = album_dir.join("scans");
    fs::create_dir_all(&scans).unwrap();
    fs::write(scans.join("cover.jpg"), embedded_cover_jpeg()).unwrap();

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("a cover should be written");
    assert_eq!(
        cover.source, "embedded",
        "the embedded picture must win over the folder image, got source {:?}",
        cover.source
    );
}

/// File Tags imports never deduplicate against existing releases — even
/// when an identified release with the same album title is already in
/// the library, a File Tags import lands on a fresh album.
#[tokio::test]
async fn file_tags_import_always_creates_a_fresh_album() {
    support::tracing_init();

    // First import: identified, lands on its own album.
    let release = discogs_release_rich("Album Title", "master-existing", &["Track One"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let identified_dir = f.temp_path().join("identified");
    fs::create_dir_all(&identified_dir).unwrap();
    generate_album_files(&identified_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "identified".to_string(),
            folder: identified_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
            }),
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (_, identified_album_id) = support::wait_for_import_complete(&mut rx).await;

    // Second import: File Tags, same album title in tags. Must NOT
    // attach to the identified album.
    let file_tags_dir = f.temp_path().join("file-tags");
    fs::create_dir_all(&file_tags_dir).unwrap();
    generate_tagged_album_files(
        &file_tags_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );

    let import_id2 = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id2.clone(),
            candidate_key: "file-tags".to_string(),
            folder: file_tags_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx2 = f.handle.subscribe_import(import_id2);
    let (_, file_tags_album_id) = support::wait_for_import_complete(&mut rx2).await;

    assert_ne!(
        identified_album_id, file_tags_album_id,
        "File Tags import must land on a fresh album",
    );
}

/// User-edit overlay applies on top of the file-tag seed: the user
/// can override album title, artist, year, pressing fields, and track
/// titles via the editor before commit. Persisted metadata reflects
/// the edits.
#[tokio::test]
async fn file_tags_import_with_user_edit_overlay() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Wrong Album Title",
        "Wrong Artist",
        Some(1999),
        &[TaggedTrack {
            filename: "01.flac",
            title: "Wrong Track Title",
            track_number: 1,
        }],
    );

    let edit = ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_assignments: vec![ArtistAssignment::new("Artist Edited")],
        album_year: Some(1998),
        pressing: PressingEdit {
            year: Some(2010),
            format: Some("CD".to_string()),
            label: Some("Edited Label".to_string()),
            catalog_number: Some("EDIT-1".to_string()),
            country: Some("JP".to_string()),
            barcode: Some("4943674000000".to_string()),
        },
        tracks: vec![TrackUserEdit {
            title: "Edited Track Title".to_string(),
            side: 1,
            track_number: Some(1),
            artist_assignments: TrackArtistAssignments::AlbumArtists,
            file: None,
        }],
    };

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: Some(edit),
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.pressing.year, Some(2010));
    assert_eq!(release.pressing.format.as_deref(), Some("CD"));
    assert_eq!(release.pressing.label.as_deref(), Some("Edited Label"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("EDIT-1"));
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(release.pressing.barcode.as_deref(), Some("4943674000000"));
    assert_eq!(
        release.metadata_provenance,
        Some(MetadataProvenance::FileTags),
    );

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert!(
        identities.is_empty(),
        "user_edit must not introduce external identity rows for File Tags",
    );

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Edited Title");

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Edited Track Title");
}

/// File Tags commit of a rip with no usable album-level tags seeds the
/// album title from the containing folder name rather than failing —
/// the permissive file-tag projection never hard-fails on a missing
/// ALBUM tag (the editable confirmation form gates a blank title before
/// save). The artist falls back to empty for the user to fill.
#[tokio::test]
async fn file_tags_import_with_no_tags_seeds_title_from_folder_name() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("Mystery Rip");
    fs::create_dir_all(&album_dir).unwrap();
    // The fixture FLAC carries no Vorbis comments — no ALBUM/ARTIST tag
    // for the projection to read, so the folder name is the album title.
    generate_album_files(&album_dir, &["01.flac", "02.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(
        album.title, "Mystery Rip",
        "untagged rip takes the folder name as its album title",
    );
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(
        release.metadata_provenance,
        Some(MetadataProvenance::FileTags),
    );
    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2, "both untagged files import as tracks");
}
