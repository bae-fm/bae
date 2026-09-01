/// Truncate a FLAC's body on disk while keeping its header and tags: a valid
/// STREAMINFO (declaring the full sample count) over a short audio body. Kept
/// size stays above the scanner's gross-size floor (10% of the raw PCM size, ~44
/// KB for the 5s mono fixture) so the file passes scan and reaches the loudness
/// loop, but far too little audio to decode the declared samples -- the
/// decode-verify shortfall signature.
fn truncate_flac_body(path: &Path) {
    let bytes = fs::read(path).expect("read flac to truncate");
    // The 5s/44.1k/mono/16-bit fixture declares 220_500 samples => 441_000 raw
    // bytes; the scan rejects below 44_100. 46_000 clears that while cutting the
    // bulk of the ~68 KB audio body.
    let keep = 46_000usize.min(bytes.len());
    assert!(
        bytes.len() > keep,
        "fixture is smaller than the truncation target ({} bytes)",
        bytes.len(),
    );
    fs::write(path, &bytes[..keep]).expect("write truncated flac");
}

/// Import a one-track album whose FLAC is truncated (valid header, short body),
/// with `verify_decode_on_import` set to `verify`. Returns the import outcome.
async fn import_truncated_album(verify: bool) -> Result<(String, String), String> {
    let temp = TempDir::new().unwrap();
    let db_dir = temp.path().join("db");
    fs::create_dir_all(&db_dir).unwrap();
    let db = Database::new_test(
        db_dir.join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let library_dir = StoreDir::new(db_dir.clone());
    let config_handle = support::test_config(&library_dir);
    config_handle
        .update(|c| c.verify_decode_on_import = verify)
        .expect("set verify_decode_on_import");
    let library_manager = LibraryManager::new(
        db.clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    let handle = library_manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .expect("import service starts");

    let album_dir = temp.path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Broken Album",
        "Broken Artist",
        None,
        &[TaggedTrack {
            filename: "01.flac",
            title: "Broken Track",
            track_number: 1,
        }],
    );
    truncate_flac_body(&album_dir.join("01.flac"));

    let import_id = uuid::Uuid::new_v4().to_string();
    handle
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
    let mut progress_rx = handle.subscribe_import(import_id);
    let result = support::try_wait_for_import_complete(&mut progress_rx).await;
    // Keep the temp dir (and its files) alive until the import has finished.
    drop(temp);
    result
}

/// `verify_decode_on_import` gates a broken track end to end: a truncated FLAC
/// (valid header, short body) that decodes short imports fine with the flag off,
/// and fails at the decode-verify gate -- before finalize commits anything -- with
/// the flag on. Proves the flag drives the outcome, not the fixture.
#[tokio::test]
async fn verify_decode_on_import_gates_a_broken_track() {
    support::tracing_init();

    // Flag off: the import does not assert on decode integrity, so the broken
    // album still imports.
    let off = import_truncated_album(false).await;
    assert!(
        off.is_ok(),
        "with verify_decode_on_import off, a broken album must still import, got: {off:?}",
    );

    // Flag on (the default): the same album fails at the decode-verify gate.
    let on = import_truncated_album(true).await;
    let err = on.expect_err("with verify_decode_on_import on, a broken album must fail the import");
    assert!(
        err.contains("decode verification failed"),
        "the failure must come from decode-verify, got: {err}",
    );
}

// ── album artists survive the confirmation editor ────────────────────────

/// A MusicBrainz release credited to two artists, one CD track.
fn seed_two_credit_mb_release(mb_release_id: &str, mb_group_id: &str) -> String {
    let credit = |id: &str, name: &str| MbArtistCredit {
        name: name.to_string(),
        artist: Some(MbArtistRef {
            id: Some(id.to_string()),
            name: Some(name.to_string()),
            sort_name: Some(name.to_string()),
        }),
    };
    let response = MbReleaseResponse {
        id: mb_release_id.to_string(),
        title: "Split Album".to_string(),
        date: Some("1999".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![
            credit("mb-artist-a", "Artist A"),
            credit("mb-artist-b", "Artist B"),
        ],
        release_group: Some(MbReleaseGroupRef {
            id: mb_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![MbTrack {
                position: Some(1),
                number: Some("1".to_string()),
                title: None,
                length: None,
                recording: Some(MbRecording {
                    id: None,
                    title: Some("Track One".to_string()),
                    artist_credit: vec![],
                    relations: vec![],
                }),
                artist_credit: vec![],
            }],
        }],
        relations: vec![],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: true,
            darkened: false,
        },
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, (response, None, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    support::cover_art_archive().serve_front(mb_release_id, support::cover_png());
    mb_release_id.to_string()
}

/// A release credited to two artists keeps both through the confirmation editor:
/// the primary on `albums.artist_id`, the second as an `album_artists` junction
/// row still carrying its MusicBrainz id.
///
/// The editor seeds from the same projection the commit worker maps, so a user
/// who changes nothing sends back the artist list the mapper produced, and the
/// commit's artist comparison sees no edit. Seeding it from the picker's display
/// shape — which collapses the credits to one name — read as "the user deleted
/// artist B" and destroyed her junction row on every such import.
#[tokio::test]
async fn two_credit_mb_release_keeps_both_album_artists() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_two_credit_mb_release("two-credit-mb-rel", "two-credit-mb-group");

    // Scan the album in so the prefetch runs against a candidate key the
    // service actually knows — the key is what core reads the identify evidence
    // behind the claim from, so a made-up one would exercise a path no surface
    // takes.
    let collection = f.temp_path().join("two-credit-collection");
    let album_dir = collection.join("two-credit");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);
    let candidate_key = album_dir.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    wait_for_scan_event(
        &mut scan_rx,
        "the two-credit candidate",
        |event| matches!(event, ScanEvent::FolderCandidate { candidate: c, .. } if c.path == album_dir),
    )
    .await;

    // The confirmation pane's form, unedited: what the commit reads back off
    // the pick when the user touches nothing.
    f.handle
        .select_candidate_metadata_provenance(
            candidate_key.clone(),
            bae_core::import::MetadataProvenance::ExternalRelease {
                source: MetadataSource::MusicBrainz,
                release_id: mb_id.clone(),
            },
        )
        .await
        .unwrap();
    let import_id = f
        .handle
        .start_import(&candidate_key, StorageMode::Local, false)
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (_release_id, album_id) = support::wait_for_import_complete(&mut rx).await;

    let artists = f.db.get_artists_for_album(&album_id).await.unwrap();
    let names: Vec<&str> = artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Artist A", "Artist B"]);
    assert_eq!(
        artists[1].musicbrainz_artist_id.as_deref(),
        Some("mb-artist-b")
    );
}

/// Seed a plain MusicBrainz release of `track_count` tracks on one CD, credited
/// to one artist. The tracklist a folder's audio gets mapped against.
fn seed_mb_release_with_track_count(
    mb_release_id: &str,
    mb_group_id: &str,
    track_count: usize,
) -> String {
    let response = MbReleaseResponse {
        id: mb_release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("2004".to_string()),
        country: Some("GB".to_string()),
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-slots".to_string()),
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
            tracks: (1..=track_count)
                .map(|position| MbTrack {
                    position: Some(position as i64),
                    number: Some(position.to_string()),
                    title: None,
                    length: None,
                    recording: Some(MbRecording {
                        id: Some(format!("rec-slots-{position}")),
                        title: Some(format!("Source Track {position}")),
                        artist_credit: vec![],
                        relations: vec![],
                    }),
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: true,
            darkened: false,
        },
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, (response, None, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    support::cover_art_archive().serve_front(mb_release_id, support::cover_png());
    mb_release_id.to_string()
}

/// A release's tracks in track order, each with the file its samples come from.
async fn committed_track_files(f: &ImportFixture, release_id: &str) -> Vec<(String, String)> {
    f.db.committed_track_files_for_test(release_id)
        .await
        .expect("read committed track files")
}

/// Scan `album_dir` in and pick `mb_id` for it, returning the candidate key and
/// the mapping the pick produced. The path both desktop surfaces take.
async fn pick_release_for_folder(
    f: &ImportFixture,
    collection: &Path,
    album_dir: &Path,
    mb_id: &str,
) -> (String, bae_core::import::ImportCandidateDetail) {
    let candidate_key = album_dir.to_string_lossy().into_owned();
    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    let expected = album_dir.to_path_buf();
    wait_for_scan_event(
        &mut scan_rx,
        "the slot candidate",
        move |event| matches!(event, ScanEvent::FolderCandidate { candidate: c, .. } if c.path == expected),
    )
    .await;

    f.handle
        .select_candidate_metadata_provenance(
            candidate_key.clone(),
            bae_core::import::MetadataProvenance::ExternalRelease {
                source: MetadataSource::MusicBrainz,
                release_id: mb_id.to_string(),
            },
        )
        .await
        .unwrap();
    let pane = f
        .handle
        .candidate_pane(&candidate_key)
        .await
        .unwrap()
        .expect("the picked candidate reads back");
    (candidate_key, pane)
}

/// The tracks the mapping commits, with each row's rendered position and
/// whether the source's tracklist names it — the facts the assertions below
/// read off a row.
fn mapping_rows(
    pane: &bae_core::import::ImportCandidateDetail,
) -> Vec<(bae_core::import::RawTrackEdit, Option<String>, bool)> {
    pane.mapping
        .track_groups
        .iter()
        .flat_map(bae_core::import::MappingTrackGroup::units)
        .filter_map(|unit| match &unit.becomes {
            bae_core::import::MappingBecomes::Track {
                track,
                position,
                named_by_source,
            } => Some((track.clone(), position.clone(), *named_by_source)),
            _ => None,
        })
        .collect()
}

/// Thirteen files against a twelve-track source. The pick produces twelve
/// paired slots and one `FileOnly`, and committing writes thirteen tracks — the
/// thirteenth named after its file. This folder used to fail the commit
/// outright.
#[tokio::test]
async fn thirteen_files_against_a_twelve_track_source_commits_thirteen_tracks() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_mb_release_with_track_count("mb-rel-13v12", "mb-group-13v12", 12);

    let collection = f.temp_path().join("collection-13v12");
    let album_dir = collection.join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let names: Vec<String> = (1..=13).map(|n| format!("{n:02} Track.flac")).collect();
    generate_album_files(
        &album_dir,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let (candidate_key, pane) =
        pick_release_for_folder(&f, &collection, &album_dir, &mb_id).await;

    let rows = mapping_rows(&pane);
    assert_eq!(rows.len(), 13);
    // Twelve rows the source names, and one the folder offers that it does
    // not — which still numbers itself by continuing the tracklist.
    assert_eq!(rows.iter().filter(|(_, _, named)| *named).count(), 12);
    assert!(!rows[12].2);
    assert_eq!(rows[12].1.as_deref(), Some("13"));

    let import_id = f
        .handle
        .start_import(&candidate_key, StorageMode::Local, false)
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut rx).await;

    let committed = committed_track_files(&f, &release_id).await;
    assert_eq!(committed.len(), 13);
    assert_eq!(
        committed[0],
        ("Source Track 1".to_string(), names[0].clone())
    );
    assert_eq!(
        committed[11],
        ("Source Track 12".to_string(), names[11].clone())
    );
    // Nobody named the thirteenth slot, so it commits under its file's name.
    assert_eq!(committed[12], ("13 Track".to_string(), names[12].clone()));
}

/// Fourteen source tracks against thirteen files. The pick produces one
/// `TrackOnly` slot; leaving it unanswered commits the thirteen tracks that
/// have audio and nothing else.
#[tokio::test]
async fn a_track_with_no_audio_commits_as_the_user_left_it() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_mb_release_with_track_count("mb-rel-14v13", "mb-group-14v13", 14);

    let collection = f.temp_path().join("collection-14v13");
    let album_dir = collection.join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let names: Vec<String> = (1..=13).map(|n| format!("{n:02} Track.flac")).collect();
    generate_album_files(
        &album_dir,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let (candidate_key, pane) =
        pick_release_for_folder(&f, &collection, &album_dir, &mb_id).await;

    let rows = mapping_rows(&pane);
    assert_eq!(rows.len(), 14);
    // The fourteenth track the source names has no audio behind it.
    assert!(rows[13].2);
    assert_eq!(rows[13].1.as_deref(), Some("14"));
    assert_eq!(rows[13].0.file, None);

    let import_id = f
        .handle
        .start_import(&candidate_key, StorageMode::Local, false)
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut rx).await;

    let committed = committed_track_files(&f, &release_id).await;
    assert_eq!(committed.len(), 13);
    let titles: Vec<&str> = committed.iter().map(|(title, _)| title.as_str()).collect();
    assert!(
        !titles.contains(&"Source Track 14"),
        "the slot nobody gave audio to has nothing to write: {titles:?}",
    );
}

/// A rip whose files are named in the wrong order. The user re-pairs two slots
/// in the mapping, and the commit binds the files they chose — not the ones
/// their positions would have given them.
#[tokio::test]
async fn a_corrected_pairing_survives_the_commit() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_mb_release_with_track_count("mb-rel-repair", "mb-group-repair", 3);

    let collection = f.temp_path().join("collection-repair");
    let album_dir = collection.join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let names = ["01 Track.flac", "02 Track.flac", "03 Track.flac"];
    generate_album_files(&album_dir, &names);

    let (candidate_key, pane) =
        pick_release_for_folder(&f, &collection, &album_dir, &mb_id).await;

    let mut tracks = bae_core::import::mapping_tracks(&pane.mapping);
    assert_eq!(tracks.len(), 3);
    // Re-pairing moves the bindings, not the tracks: the first two source
    // tracks keep their titles and numbers and swap the audio behind them.
    let first = tracks[0].file.clone();
    tracks[0].file = tracks[1].file.clone();
    tracks[1].file = first;

    f.handle
        .set_candidate_track_edit(&candidate_key, tracks[0].clone())
        .await
        .unwrap();
    f.handle
        .set_candidate_track_edit(&candidate_key, tracks[1].clone())
        .await
        .unwrap();
    let import_id = f
        .handle
        .start_import(&candidate_key, StorageMode::Local, false)
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut rx).await;

    assert_eq!(
        committed_track_files(&f, &release_id).await,
        vec![
            ("Source Track 1".to_string(), "02 Track.flac".to_string()),
            ("Source Track 2".to_string(), "01 Track.flac".to_string()),
            ("Source Track 3".to_string(), "03 Track.flac".to_string()),
        ],
    );
}
// ── Task 2: the commit derives the cover from the picked release ────────────

/// Seed a MusicBrainz release whose document says the Cover Art Archive holds a
/// front image for it, so the commit has an address to fetch and a statement
/// that there is something at it.
fn seed_mb_release_with_front_cover(mb_release_id: &str, mb_group_id: &str, title: &str) -> String {
    let response = MbReleaseResponse {
        id: mb_release_id.to_string(),
        title: title.to_string(),
        date: Some("1996".to_string()),
        country: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: None,
        }],
        release_group: Some(MbReleaseGroupRef {
            id: mb_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![MbTrack {
                position: Some(1),
                number: Some("1".to_string()),
                title: Some("Track".to_string()),
                length: None,
                recording: None,
                artist_credit: vec![],
            }],
        }],
        relations: vec![],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: true,
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

/// A commit that carries no cover pick lands the cover the confirmation pane
/// offered. The pane seeds its selection from the release's own cover options,
/// so "the command names no cover" means the user changed nothing — not that
/// they want none. Reading it as the latter is what imported releases bare
/// whenever the pane's cover options came up empty.
#[tokio::test]
async fn an_import_with_no_cover_pick_takes_the_release_s_own_cover() {
    support::tracing_init();

    let mb_id = "mb-rel-derived-cover";
    let release_id_key =
        seed_mb_release_with_front_cover(mb_id, "mb-group-derived-cover", "Derived Cover Album");
    support::cover_art_archive().serve_front(mb_id, support::cover_png());

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    // No folder art: the only cover this release can end up with is the one its
    // own document points at.
    generate_album_files(&album_dir, &["01 Track.flac"]);

    let (release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
                release_id: release_id_key,
        },
    )
    .await
    .expect("the import succeeds");

    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect(
                "the release document says the archive holds a front image, so the \
                 import must land one",
            );
    assert_eq!(cover.source, "musicbrainz");
    assert!(cover
        .source_url
        .as_deref()
        .expect("a downloaded cover records where it came from")
        .ends_with(&format!("/release/{mb_id}/front")));
}

/// And when that cover will not download, the import fails instead of quietly
/// landing without one. A transient archive failure is exactly the case that
/// used to produce a coverless release with no error anywhere.
#[tokio::test]
async fn an_import_fails_when_the_release_s_own_cover_will_not_download() {
    support::tracing_init();

    let mb_id = "mb-rel-unreachable-cover";
    let release_id_key = seed_mb_release_with_front_cover(
        mb_id,
        "mb-group-unreachable-cover",
        "Unreachable Cover Album",
    );
    support::cover_art_archive().fail_front(mb_id, 503);

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track.flac"]);

    let error = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
                release_id: release_id_key,
        },
    )
    .await
    .expect_err("a cover the source says exists but cannot be fetched fails the import");
    assert!(
        error.contains("503") || error.to_lowercase().contains("cover"),
        "the failure names the cover download, got: {error}"
    );
}
