/// Cut a FLAC's audio in half, keeping every metadata block, so it decodes far
/// fewer samples than its STREAMINFO declares.
fn truncate_flac_body(path: &Path) {
    let bytes = fs::read(path).expect("read flac to truncate");
    assert_eq!(&bytes[..4], b"fLaC", "{} is a FLAC stream", path.display());
    let mut audio_start = 4;
    loop {
        let header = &bytes[audio_start..audio_start + 4];
        let length = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
        audio_start += 4 + length;
        if header[0] & 0x80 != 0 {
            break;
        }
    }
    let keep = audio_start + (bytes.len() - audio_start) / 2;
    fs::write(path, &bytes[..keep]).expect("write truncated flac");
}

/// Import a one-track album whose FLAC is truncated, with
/// `verify_decode_on_import` set to `verify`.
async fn import_truncated_album(verify: bool) -> Result<(String, String), String> {
    let temp = TempDir::new().unwrap();
    let db_dir = temp.path().join("db");
    fs::create_dir_all(&db_dir).unwrap();
    let db = Database::new_test(
        db_dir.join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let library_dir = StoreDir::new(db_dir.clone());
    let config_handle = support::test_config(&library_dir);
    config_handle
        .update_preferences(move |prefs| prefs.verify_decode_on_import = verify)
        .await
        .expect("set verify_decode_on_import");
    let library_manager = LibraryManager::new(
        db.clone(),
        bae_core::config::AppDir::under_home(temp.path()),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(bae_core::util::http::Http::for_test()),
        bae_core::providers::Providers::offline(),
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
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            support::DraftSource::FileTags,
        ))
        .await
        .unwrap();
    let mut progress_rx = handle.subscribe_import(import_id);
    let result = support::try_wait_for_import_complete(&mut progress_rx).await;
    // The files must outlive the import.
    drop(temp);
    result
}

/// The same truncated FLAC imports with `verify_decode_on_import` off and fails
/// decode verification with it on, so the flag decides the outcome.
#[tokio::test]
async fn verify_decode_on_import_gates_a_broken_track() {
    support::tracing_init();

    let off = import_truncated_album(false).await;
    assert!(
        off.is_ok(),
        "with verify_decode_on_import off, a broken album must still import, got: {off:?}",
    );

    // On is the default.
    let on = import_truncated_album(true).await;
    let err = on.expect_err("with verify_decode_on_import on, a broken album must fail the import");
    assert!(
        err.contains("decode verification failed"),
        "the failure must come from decode-verify, got: {err}",
    );
}

// ── album artists survive the confirmation editor ────────────────────────

/// A MusicBrainz release credited to two artists, one CD track.
fn seed_two_credit_mb_release(
    f: &ImportFixture,
    mb_release_id: &str, mb_group_id: &str) -> String {
    let credit = |id: &str, name: &str| MbArtistCredit {
        name: name.to_string(),
        artist: Some(MbArtistRef {
            id: Some(id.to_string()),
            name: Some(name.to_string()),
            sort_name: Some(name.to_string()),
        }),
    };
    let response = MbReleaseResponse {
        date: Some("1999".to_string()),
        artist_credit: vec![
            credit("mb-artist-a", "Artist A"),
            credit("mb-artist-b", "Artist B"),
        ],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: true,
            darkened: false,
        },
        ..support::mb_release(mb_release_id, mb_group_id, "Split Album")
    };
    let mb_release_id = support::seed_mb_release(f.library_manager.providers().musicbrainz(), response, mb_group_id);
    f.images.serve_front(&mb_release_id, support::cover_png());
    mb_release_id
}

/// A two-artist release imported unedited keeps both album artists, the second
/// with its MusicBrainz id: an unedited pick must not read as removing one.
#[tokio::test]
async fn two_credit_mb_release_keeps_both_album_artists() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_two_credit_mb_release(&f, "two-credit-mb-rel", "two-credit-mb-group");

    // Scan the album in, since an import starts from a scanned candidate.
    let collection = f.temp_path().join("two-credit-collection");
    let album_dir = collection.join("two-credit");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);
    let candidate_key = album_dir.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.every_scan_event_for_test();
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

    // Pick the release and change nothing.
    f.handle
        .select_candidate_release(candidate_key.clone(), bae_core::import::PressingLink {
                record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, mb_id.clone()),
                partners: vec![],
            },
        )
        .await
        .unwrap();
    let import_id = f.handle.start_import(&candidate_key).await.unwrap();
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
    f: &ImportFixture,
    mb_release_id: &str,
    mb_group_id: &str,
    track_count: usize,
) -> String {
    let tracks = (1..=track_count)
        .map(|position| {
            let mut track = support::mb_track(position as i64, &format!("Source Track {position}"));
            let recording = track.recording.as_mut().expect("a recorded track");
            recording.id = Some(format!("rec-slots-{position}"));
            track
        })
        .collect();
    seed_mb_release_with_media(f, mb_release_id, mb_group_id, vec![support::mb_medium(tracks)])
}

/// Seed a plain MusicBrainz release on `media`, credited to one artist.
fn seed_mb_release_with_media(
    f: &ImportFixture,
    mb_release_id: &str,
    mb_group_id: &str,
    media: Vec<bae_core::musicbrainz::MbMedium>,
) -> String {
    let response = MbReleaseResponse {
        date: Some("2004".to_string()),
        country: Some("GB".to_string()),
        status: None,
        packaging: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-slots".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        media,
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: true,
            darkened: false,
        },
        ..support::mb_release(mb_release_id, mb_group_id, "Album Title")
    };
    let mb_release_id = support::seed_mb_release(f.library_manager.providers().musicbrainz(), response, mb_group_id);
    f.images.serve_front(&mb_release_id, support::cover_png());
    mb_release_id
}

/// A release's tracks in track order, each with the file its samples come from.
async fn committed_track_files(f: &ImportFixture, release_id: &str) -> Vec<(String, String)> {
    f.db.committed_track_files_for_test(release_id)
        .await
        .expect("read committed track files")
}

/// Scan `album_dir` in and pick `mb_id` for it, returning the candidate key and
/// its pane after the pick.
async fn pick_release_for_folder(
    f: &ImportFixture,
    collection: &Path,
    album_dir: &Path,
    mb_id: &str,
) -> (String, bae_core::import::ImportCandidateDetail) {
    let candidate_key = album_dir.to_string_lossy().into_owned();
    let mut scan_rx = f.handle.every_scan_event_for_test();
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
        .select_candidate_release(candidate_key.clone(), bae_core::import::PressingLink {
                record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, mb_id.to_string()),
                partners: vec![],
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

/// Scan in a folder of `track_count` audio files and return its candidate key.
async fn scanned_folder_of(f: &ImportFixture, track_count: usize) -> String {
    let collection = f.temp_path().join("collection");
    let album_dir = collection.join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let names: Vec<String> = (1..=track_count).map(|n| format!("{n:02} Track.flac")).collect();
    generate_album_files(
        &album_dir,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut scan_rx = f.handle.every_scan_event_for_test();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    let expected = album_dir.clone();
    wait_for_scan_event(&mut scan_rx, "the candidate", move |event| {
        matches!(event, ScanEvent::FolderCandidate { candidate, .. } if candidate.path == expected)
    })
    .await;
    album_dir.to_string_lossy().into_owned()
}

/// A person's pick of MusicBrainz release `mb_id` for the candidate.
async fn pick_release(
    f: &ImportFixture,
    candidate_key: &str,
    mb_id: &str,
) -> Result<u64, bae_core::import::ImportError> {
    f.handle
        .select_candidate_release(candidate_key.to_string(), pressing_link(mb_id))
        .await
}

fn pressing_link(mb_id: &str) -> bae_core::import::PressingLink {
    bae_core::import::PressingLink {
        record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, mb_id.to_string()),
        partners: vec![],
    }
}

async fn candidate_pane(
    f: &ImportFixture,
    candidate_key: &str,
) -> bae_core::import::ImportCandidateDetail {
    f.handle
        .candidate_pane(candidate_key)
        .await
        .unwrap()
        .expect("the candidate reads back")
}

/// A release listing another number of tracks than the folder holds is
/// refused whatever the person picked it from, naming both counts, and
/// nothing changes: the draft, its cover, the release the folder is linked to
/// and the result it stored stay as the pick before left them.
#[tokio::test]
async fn a_release_listing_another_track_count_is_refused_and_changes_nothing() {
    support::tracing_init();
    for release_tracks in [10, 14] {
        let f = ImportFixture::new().await;
        let fitting = seed_mb_release_with_track_count(&f, "mb-rel-fits", "mb-group-fits", 12);
        let other = seed_mb_release_with_track_count(
            &f,
            &format!("mb-rel-count-{release_tracks}"),
            &format!("mb-group-count-{release_tracks}"),
            release_tracks,
        );
        let candidate_key = scanned_folder_of(&f, 12).await;
        pick_release(&f, &candidate_key, &fitting).await.unwrap();
        let before = candidate_pane(&f, &candidate_key).await;

        let error = pick_release(&f, &candidate_key, &other)
            .await
            .expect_err("a release of another track count is refused");

        assert!(
            matches!(
                error,
                bae_core::import::ImportError::MetadataTrackCount {
                    folder_tracks: 12,
                    release_tracks: listed,
                } if listed as usize == release_tracks
            ),
            "{error:?}"
        );
        assert_eq!(
            error.ui_error(),
            bae_core::ui::UiError::Diagnostic {
                category: bae_core::ui::UiErrorCategory::MetadataTrackCount {
                    folder_tracks: 12,
                    release_tracks: release_tracks as u32,
                },
                detail: error.to_string(),
            }
        );
        let after = candidate_pane(&f, &candidate_key).await;
        assert_eq!(
            after.release_link,
            Some(bae_core::import::ReleaseLink::Pressing(pressing_link(&fitting)))
        );
        assert_eq!(after.release_link, before.release_link);
        assert_eq!(after.metadata_draft, before.metadata_draft);
        assert_eq!(after.metadata_provenance, before.metadata_provenance);
        assert_eq!(after.metadata_revision, before.metadata_revision);
        assert_eq!(after.cover, before.cover);
        assert_eq!(after.resumed_identify_state, before.resumed_identify_state);
        let tracks = bae_core::import::mapping_tracks(&after.mapping);
        assert_eq!(tracks.len(), 12);
        assert!(tracks.iter().all(|track| track.file.is_some()));
    }
}

/// A release listing as many tracks as the folder holds is applied: the
/// folder is linked to it and the draft is read from it.
#[tokio::test]
async fn a_release_listing_the_folder_s_track_count_applies() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let fitting = seed_mb_release_with_track_count(&f, "mb-rel-fits", "mb-group-fits", 12);
    let candidate_key = scanned_folder_of(&f, 12).await;

    pick_release(&f, &candidate_key, &fitting).await.unwrap();

    let pane = candidate_pane(&f, &candidate_key).await;
    assert_eq!(
        pane.release_link,
        Some(bae_core::import::ReleaseLink::Pressing(pressing_link(&fitting)))
    );
    assert_eq!(
        pane.metadata_provenance,
        Some(bae_core::import::MetadataProvenance::ExternalRelease {
            record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, fitting.clone()),
        })
    );
    let titles: Vec<_> = pane
        .metadata_draft
        .tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect();
    let listed: Vec<_> = (1..=12).map(|n| format!("Source Track {n}")).collect();
    assert_eq!(titles, listed.iter().map(String::as_str).collect::<Vec<_>>());
}

/// A release that lists no tracks says nothing about the folder's count, so
/// it is not refused: the folder is linked to it, the draft takes its album
/// fields, and each track keeps what it had, on the audio it plays.
#[tokio::test]
async fn a_release_listing_no_tracks_applies() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let lists_nothing =
        seed_mb_release_with_media(&f, "mb-rel-no-tracks", "mb-group-no-tracks", vec![]);
    let candidate_key = scanned_folder_of(&f, 12).await;
    let before = candidate_pane(&f, &candidate_key).await;

    pick_release(&f, &candidate_key, &lists_nothing).await.unwrap();

    let after = candidate_pane(&f, &candidate_key).await;
    assert_eq!(
        after.release_link,
        Some(bae_core::import::ReleaseLink::Pressing(pressing_link(&lists_nothing)))
    );
    assert_eq!(
        after.metadata_provenance,
        Some(bae_core::import::MetadataProvenance::ExternalRelease {
            record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, lists_nothing.clone()),
        })
    );
    assert_eq!(after.metadata_draft.album_title, "Album Title");
    assert_eq!(after.metadata_draft.tracks, before.metadata_draft.tracks);
    let tracks = bae_core::import::mapping_tracks(&after.mapping);
    assert_eq!(tracks.len(), 12);
    assert!(tracks.iter().all(|track| track.file.is_some()));
}

/// A track plays the folder's audio unit at its position: an edit that names
/// another row's audio is refused, and the commit binds each track to the
/// file in its place.
#[tokio::test]
async fn the_commit_binds_each_track_to_the_audio_in_its_place() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_mb_release_with_track_count(&f, "mb-rel-repair", "mb-group-repair", 3);

    let collection = f.temp_path().join("collection-repair");
    let album_dir = collection.join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let names = ["01 Track.flac", "02 Track.flac", "03 Track.flac"];
    generate_album_files(&album_dir, &names);

    let (candidate_key, pane) = pick_release_for_folder(&f, &collection, &album_dir, &mb_id).await;

    let mut tracks = bae_core::import::mapping_tracks(&pane.mapping);
    assert_eq!(tracks.len(), 3);
    tracks[0].file = tracks[1].file.clone();
    assert!(f
        .handle
        .set_candidate_track_edit(&candidate_key, tracks[0].clone())
        .await
        .is_err());
    let import_id = f.handle.start_import(&candidate_key).await.unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut rx).await;

    assert_eq!(
        committed_track_files(&f, &release_id).await,
        vec![
            ("Source Track 1".to_string(), "01 Track.flac".to_string()),
            ("Source Track 2".to_string(), "02 Track.flac".to_string()),
            ("Source Track 3".to_string(), "03 Track.flac".to_string()),
        ],
    );
}
// ── the commit takes the cover from the picked release ─────────────────────

/// Seed a MusicBrainz release whose document says the Cover Art Archive holds
/// its front image.
fn seed_mb_release_with_front_cover(
    f: &ImportFixture,
    mb_release_id: &str, mb_group_id: &str, title: &str) -> String {
    let response = MbReleaseResponse {
        country: None,
        status: None,
        packaging: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: None,
        }],
        media: vec![support::mb_medium(vec![MbTrack {
            title: Some("Track".to_string()),
            recording: None,
            ..support::mb_track(1, "Track")
        }])],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: true,
            darkened: false,
        },
        ..support::mb_release(mb_release_id, mb_group_id, title)
    };
    support::seed_mb_release(f.library_manager.providers().musicbrainz(), response, mb_group_id)
}

/// An import with no cover pick takes the release's own cover: no pick means
/// the user changed nothing, not that they want no cover.
#[tokio::test]
async fn an_import_with_no_cover_pick_takes_the_release_s_own_cover() {
    support::tracing_init();

    let f = ImportFixture::new().await;

    let mb_id = "mb-rel-derived-cover";
    let release_id_key =
        seed_mb_release_with_front_cover(&f, mb_id, "mb-group-derived-cover", "Derived Cover Album");
    f.images.serve_front(mb_id, support::cover_png());

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    // No folder art: the only cover this release can end up with is the one its
    // own document points at.
    generate_album_files(&album_dir, &["01 Track.flac"]);

    let (release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        ImportDestination::Local,
        support::DraftSource::Pick(bae_core::import::PressingLink {
            record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, release_id_key),
            partners: vec![],
        }),
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

/// When the release's own cover will not download, the import fails rather
/// than landing without it.
#[tokio::test]
async fn an_import_fails_when_the_release_s_own_cover_will_not_download() {
    support::tracing_init();

    let f = ImportFixture::new().await;

    let mb_id = "mb-rel-unreachable-cover";
    let release_id_key = seed_mb_release_with_front_cover(&f,
        mb_id,
        "mb-group-unreachable-cover",
        "Unreachable Cover Album",
    );
    f.images.fail_front(mb_id, 503);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track.flac"]);

    let error = import_folder(
        &f,
        &album_dir,
        None,
        ImportDestination::Local,
        support::DraftSource::Pick(bae_core::import::PressingLink {
            record: bae_core::import::MetadataRef::new(Catalog::MusicBrainz, release_id_key),
            partners: vec![],
        }),
    )
    .await
    .expect_err("a cover the source says exists but cannot be fetched fails the import");
    assert!(
        error.contains("503") || error.to_lowercase().contains("cover"),
        "the failure names the cover download, got: {error}"
    );
}
