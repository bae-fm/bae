/// The MusicBrainz work MBIDs the rollback test seeds. MusicBrainz mints work
/// MBIDs as name-based (version 3) UUIDs, so that is the shape here.
const MB_WORK_SHARED: &str = "8f0a1c2d-3e4b-3a5c-9d6e-7f8091a2b3c4";
const MB_WORK_EXCLUSIVE: &str = "1b2c3d4e-5f60-3718-8293-a4b5c6d7e8f9";
const MB_WORK_PART: &str = "2c3d4e5f-6071-3829-93a4-b5c6d7e8f901";

/// An MbWork with a composer relation and, optionally, one child part.
fn work_with_composer(id: &str, title: &str, composer_mb: &str, part: Option<MbWork>) -> MbWork {
    let mut relations = vec![MbRelation {
        target_type: Some("artist".to_string()),
        relation_type: Some("composer".to_string()),
        artist: Some(MbArtistRef {
            id: Some(composer_mb.to_string()),
            name: Some(format!("Composer {composer_mb}")),
            sort_name: Some(format!("Composer {composer_mb}")),
        }),
        target_credit: Some(format!("Composer {composer_mb}")),
        ..MbRelation::default()
    }];
    if let Some(part) = part {
        relations.push(MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("parts".to_string()),
            direction: Some("forward".to_string()),
            work: Some(part),
            ..MbRelation::default()
        });
    }
    MbWork {
        id: id.to_string(),
        title: title.to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations,
    }
}

/// An MbTrack whose recording performs `work`.
fn mb_track_performing(position: i64, title: &str, work: MbWork) -> MbTrack {
    MbTrack {
        position: Some(position),
        number: Some(position.to_string()),
        title: Some(title.to_string()),
        length: None,
        recording: Some(MbRecording {
            id: Some(format!("rec-{position}-{}", work.id)),
            title: Some(title.to_string()),
            artist_credit: vec![],
            relations: vec![MbRelation {
                target_type: Some("work".to_string()),
                relation_type: Some("performance".to_string()),
                work: Some(work),
                ..MbRelation::default()
            }],
        }),
        artist_credit: vec![],
    }
}

/// Seed a MusicBrainz release (no Discogs cross-link) whose recordings perform
/// the given work graph. Returns the MB release id.
fn seed_mb_release_with_works(
    mb_release_id: &str,
    mb_group_id: &str,
    title: &str,
    tracks: Vec<MbTrack>,
) -> String {
    let response = MbReleaseResponse {
        id: mb_release_id.to_string(),
        title: title.to_string(),
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(MbReleaseGroupRef {
            id: mb_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks,
        }],
        relations: vec![],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, (response, None, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    mb_release_id.to_string()
}

/// Work-graph sibling of `remote_transition_failure_rolls_back_finalized_release`.
/// A classical MusicBrainz import finalizes works, work_parts, work_artists
/// (composers), and track_works; a post-finalize transition failure must roll
/// all of that back too. Otherwise every failed classical remote import leaks
/// orphaned works, and the composer artist rows those surviving work_artists
/// keep alive slip past the artist-rollback guard. A work still performed by a
/// surviving release — and its composer — must be left alone.
#[tokio::test]
async fn remote_transition_failure_rolls_back_finalized_works() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    // A prior LOCAL MusicBrainz import that survives. Its recording performs
    // the shared work, so that work and its composer are referenced by a
    // release the failed remote import below must not touch.
    let prior_mb = seed_mb_release_with_works(
        "mb-rel-prior",
        "mb-group-prior",
        "Prior Symphony",
        vec![mb_track_performing(
            1,
            "Prior Movement",
            work_with_composer(MB_WORK_SHARED, "Shared Work", "composer-shared", None),
        )],
    );
    let prior_dir = f.temp_path().join("prior");
    fs::create_dir_all(&prior_dir).unwrap();
    generate_album_files(&prior_dir, &["01 Prior Movement.flac"]);
    let (prior_release_id, _) = import_folder(
        &f,
        &prior_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
                release_id: prior_mb,
            partners: vec![],
        },
    )
    .await
    .expect("prior local MB import succeeds");

    // The failing REMOTE import: one recording performs the shared work (also
    // performed by the prior release), the other an exclusive work that has its
    // own child part and its own composer.
    let remote_mb = seed_mb_release_with_works(
        "mb-rel-remote",
        "mb-group-remote",
        "Remote Symphony",
        vec![
            mb_track_performing(
                1,
                "Remote Movement One",
                work_with_composer(MB_WORK_SHARED, "Shared Work", "composer-shared", None),
            ),
            mb_track_performing(
                2,
                "Remote Movement Two",
                work_with_composer(
                    MB_WORK_EXCLUSIVE,
                    "Exclusive Work",
                    "composer-exclusive",
                    Some(MbWork {
                        id: MB_WORK_PART.to_string(),
                        title: "Exclusive Part".to_string(),
                        disambiguation: None,
                        work_type: Some("part".to_string()),
                        relations: vec![],
                    }),
                ),
            ),
        ],
    );
    let album_dir = f.temp_path().join("remote");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(
        &album_dir,
        &["01 Remote Movement One.flac", "02 Remote Movement Two.flac"],
    );

    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Remote,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::MusicBrainz,
                release_id: remote_mb,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let error = support::try_wait_for_import_complete(&mut progress_rx)
        .await
        .expect_err("remote transition without a sync provider fails");
    assert!(error.contains("cloud upload"), "unexpected error: {error}");

    let (
        shared_work,
        exclusive_work,
        part_work,
        work_parts_count,
        shared_composer,
        exclusive_composer,
        orphan_work_artists,
    ) =
        f.db.failed_import_work_state_for_test(
            MB_WORK_SHARED,
            MB_WORK_EXCLUSIVE,
            MB_WORK_PART,
            "composer-shared",
            "composer-exclusive",
        )
        .await
        .unwrap();

    assert!(
        shared_work,
        "the shared work stays; the prior release still performs it",
    );
    assert!(shared_composer, "the shared work's composer stays");
    assert!(
        !exclusive_work,
        "the failed import's exclusive work is deleted",
    );
    assert!(!part_work, "the exclusive work's child part is deleted");
    assert_eq!(
        work_parts_count, 0,
        "no work_parts rows survive the rollback"
    );
    assert!(
        !exclusive_composer,
        "the exclusive composer, referenced by nothing else, is deleted",
    );
    assert_eq!(
        orphan_work_artists, 0,
        "no work_artists point at a work with no surviving track link",
    );

    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_some(),
        "the prior release is untouched by the failed remote import",
    );
}

/// A MusicBrainz work MBID is frequently a name-based (version 3) UUID, which
/// coven's synced-row id policy refuses. A `works` row therefore carries a
/// minted id and keeps the MBID in `musicbrainz_work_id`.
const V3_WORK: &str = "e0dc7948-f188-3346-adee-db5cfb6361a9";

/// Two releases performing the same work land one `works` row, keyed on the
/// MBID, with an id the sync layer accepts.
#[tokio::test]
async fn work_mbid_is_stored_beside_a_minted_row_id_and_shared_across_releases() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let first_mb = seed_mb_release_with_works(
        "mb-rel-first",
        "mb-group-first",
        "First Release",
        vec![mb_track_performing(
            1,
            "First Movement",
            work_with_composer(V3_WORK, "Shared Work", "composer-shared", None),
        )],
    );
    let first_dir = f.temp_path().join("first");
    fs::create_dir_all(&first_dir).unwrap();
    generate_album_files(&first_dir, &["01 First Movement.flac"]);
    let (first_release_id, _) = import_folder(
        &f,
        &first_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
                release_id: first_mb,
            partners: vec![],
        },
    )
    .await
    .expect("first local MB import succeeds");

    let second_mb = seed_mb_release_with_works(
        "mb-rel-second",
        "mb-group-second",
        "Second Release",
        vec![mb_track_performing(
            1,
            "Second Movement",
            work_with_composer(V3_WORK, "Shared Work", "composer-shared", None),
        )],
    );
    let second_dir = f.temp_path().join("second");
    fs::create_dir_all(&second_dir).unwrap();
    generate_album_files(&second_dir, &["01 Second Movement.flac"]);
    let (second_release_id, _) = import_folder(
        &f,
        &second_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
                release_id: second_mb,
            partners: vec![],
        },
    )
    .await
    .expect("second local MB import succeeds");

    let (work_ids, linked_release_ids) = f.db.work_links_for_test(V3_WORK).await.unwrap();

    assert_eq!(
        work_ids.len(),
        1,
        "the work the two releases share is one row",
    );
    assert_ne!(
        work_ids[0], V3_WORK,
        "the row id is minted, not the MusicBrainz MBID",
    );
    let mut expected = vec![first_release_id, second_release_id];
    expected.sort();
    assert_eq!(
        linked_release_ids, expected,
        "both releases' track_works point at the one work row",
    );
}

/// coven verifies a blob's plaintext against its row's content hash on every
/// cloud fetch, so a `covers` row must describe the bytes actually stored — the
/// resized thumbnail the import writes, never the image it was made from. A row
/// hashing anything else makes the cover unreadable on every other device.
async fn assert_cover_row_describes_stored_bytes(f: &ImportFixture, release_id: &str) {
    let cover =
        f.db.find_library_image(release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row");
    let bytes = support::read_cover_image_blob(&f.library_manager, release_id)
        .await
        .expect("cover blob readable");
    assert_eq!(
        cover.content_hash.as_str(),
        bae_core::util::fs::hash_bytes(&bytes).as_str(),
        "the covers row's hash must be the hash of the stored blob",
    );
    assert_eq!(
        cover.file_size,
        bytes.len() as i64,
        "the covers row's file_size must be the size of the stored blob",
    );
}

/// 6. Import with local cover art: covers row + the cover blob in coven's local store.
#[tokio::test]
async fn import_with_cover_art() {
    support::tracing_init();

    let release = discogs_release("Cover Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_with_cover(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: Some(CoverSelection::Local("scans/back.jpg".to_string())),
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    // Cover image row in DB
    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap();
    assert!(cover.is_some(), "should have cover image in DB");
    let cover = cover.unwrap();
    assert_eq!(cover.source, "local");

    // Cover bytes readable through the image ref path (coven's local store while Local).
    let cover_bytes = support::read_cover_image_blob(&f.library_manager, &release_id)
        .await
        .expect("cover blob should be readable");
    assert!(!cover_bytes.is_empty(), "cover bytes should not be empty");

    assert_cover_row_describes_stored_bytes(&f, &release_id).await;
}

/// An oversized folder cover is resized to a ≤600 JPEG thumbnail at import: the
/// stored blob decodes to 600×600 JPEG (from a 1000×1000 PNG source) and the
/// `covers` row records JPEG, not the source PNG.
#[tokio::test]
async fn import_resizes_oversized_cover_to_jpeg_thumbnail() {
    support::tracing_init();

    let release = discogs_release("Oversized Cover Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let cover_path = generate_album_with_oversized_cover(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: Some(CoverSelection::Local(cover_path)),
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    // The row records the stored format (JPEG), not the source PNG.
    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover image in DB");
    assert_eq!(
        cover.content_type,
        bae_core::util::content_type::ContentType::Jpeg
    );

    // The stored blob is a ≤600 JPEG downscaled from the 1000×1000 PNG source.
    let cover_bytes = support::read_cover_image_blob(&f.library_manager, &release_id)
        .await
        .expect("cover blob should be readable");
    assert_eq!(
        image::guess_format(&cover_bytes).unwrap(),
        image::ImageFormat::Jpeg
    );
    let decoded = image::load_from_memory(&cover_bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (600, 600));

    assert_cover_row_describes_stored_bytes(&f, &release_id).await;
}

/// On a browsable home the readable cloud_path is computed AT IMPORT — for every
/// release file and for the cover — and stored on its row, so coven keys the blob
/// readably when the gate flips. (An opaque home leaves them NULL; covered by the
/// other imports, which assert nothing here.)
#[tokio::test]
async fn import_on_browsable_home_writes_readable_cloud_paths_at_import() {
    support::tracing_init();

    let release = discogs_release("Browsable Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;
    // Make the home browsable BEFORE importing, so finalize computes readable keys.
    f.library_manager
        .set_home_storage(bae_core::config::HomeStorage::Browsable);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_with_cover(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: Some(CoverSelection::Local("scans/back.jpg".to_string())),
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let album_id =
        f.db.find_release_by_id(&release_id)
            .await
            .unwrap()
            .unwrap()
            .album_id;
    let prefix = format!("{album_id}/{release_id}/");

    // Every release file carries a readable cloud_path under the release prefix.
    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    assert!(!files.is_empty(), "the import recorded release files");
    for file in &files {
        // The readable key is `{album}/{release}/{source_folder}/{original_filename}`,
        // and `original_filename` is the file's path within the release folder — so a
        // `scans/back.jpg` image file keys under a `scans/` level, mirroring the
        // folder the release was imported from.
        let cp = file.cloud_path.as_deref().unwrap_or_else(|| {
            panic!(
                "file {} has no cloud_path on a browsable home",
                file.original_filename
            )
        });
        assert!(
            cp.starts_with(&prefix),
            "audio cloud_path {cp} is under the release prefix {prefix}",
        );
        assert!(
            cp.ends_with(&file.original_filename),
            "audio cloud_path {cp} ends with the file's stored path {}",
            file.original_filename,
        );
    }

    // The cover carries its readable cloud_path too. It names the cover's blob, not
    // just the release, so a later cover change writes a new object rather than
    // overwriting this one.
    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row present");
    assert_eq!(
        cover.cloud_path.as_deref(),
        Some(format!("{prefix}cover-{}.jpg", cover.blob_id).as_str()),
        "the cover's readable cloud_path is computed at import and names its blob",
    );
}

// ── metadata draft + user edit at commit ───────────────────────────────────
