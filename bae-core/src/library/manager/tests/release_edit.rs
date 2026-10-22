async fn add_track_audio_sources(
    manager: &LibraryManager,
    track: &crate::db::DbTrack,
    sources: &[(&str, crate::album_detail::SourceAudioLayout)],
) -> Vec<DbFile> {
    let now = Utc::now();
    let mut files = Vec::with_capacity(sources.len());
    for (name, layout) in sources {
        let mut file = DbFile::new(
            &track.release_id,
            name,
            1_000,
            ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
        );
        file.source_audio = Some(crate::album_detail::SourceAudioFile {
            layout: Some(*layout),
            format: crate::album_detail::AudioFormat {
                codec: "FLAC".to_string(),
                sample_rate_hz: 44_100,
                bits_per_sample: Some(16),
                bitrate_kbps: None,
                channels: 2,
            },
            content_type: ContentType::Flac,
            duration_ms: 1_000,
        });
        manager.add_file(&file).await.unwrap();
        files.push(file);
    }
    let audio_format = crate::db::DbAudioFormat {
        id: Uuid::new_v4().to_string(),
        track_id: track.id.clone(),
        content_type: ContentType::Flac,
        pregap_samples: None,
        generated_pregap_samples: None,
        sample_rate: std::num::NonZeroU32::new(44_100).unwrap(),
        bits_per_sample: Some(std::num::NonZeroU32::new(16).unwrap()),
        channels: std::num::NonZeroU32::new(2).unwrap(),
        track_loudness_lufs: None,
        track_peak_linear: None,
        created_at: now,
    };
    let segments = files
        .iter()
        .enumerate()
        .map(|(index, file)| crate::db::DbAudioSegment {
            id: Uuid::new_v4().to_string(),
            audio_format_id: audio_format.id.clone(),
            segment_index: i64::try_from(index).unwrap(),
            role: crate::db::DbAudioSegmentRole::Main,
            file_id: file.id.clone(),
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
            created_at: now,
        })
        .collect::<Vec<_>>();
    manager
        .insert_audio_format_with_segments_for_test(&audio_format, &segments)
        .await
        .unwrap();
    files
}

#[tokio::test]
async fn release_edit_seed_uses_persisted_track_ids() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.database.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        &manager,
        &track,
        &[("track.flac", crate::album_detail::SourceAudioLayout::File)],
    )
    .await;

    let seed = manager.open_release_edit(&release.id).await.unwrap()
        .opened()
        .clone();

    assert_eq!(seed.edit.tracks[0].id, track.id);
}

#[tokio::test]
async fn an_applied_catalog_s_form_keeps_the_persisted_track_ids() {
    use crate::import::{Catalog, MetadataRef};
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_editable_tracks(&manager, &release.id, 2).await;
    let persisted_ids = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap()
        .into_iter()
        .map(|track| track.id)
        .collect::<Vec<_>>();

    let source_release_id = "reset-editor-release";
    let source_group_id = "reset-editor-group";
    let response = make_mb_release_for_re_identify(source_release_id, source_group_id, 2);
    let raw_json = serde_json::to_string(&response).unwrap();
    manager.providers().musicbrainz().seed_release_cache(source_release_id, raw_json);
    manager
        .providers()
        .musicbrainz()
        .seed_release_group_json_cache(
        source_group_id,
        r#"{"id":"reset-editor-group"}"#.to_string(),
    );
    let session = manager.open_release_edit(&release.id).await.unwrap();
    let applied = manager
        .apply_release_edit_catalog(
            &session,
            &crate::import::PressingLink {
                record: MetadataRef::new(Catalog::MusicBrainz, source_release_id),
                partners: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        applied
            .edit
            .tracks
            .into_iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        persisted_ids
    );
}

#[tokio::test]
async fn release_edit_seed_refuses_a_track_without_stored_audio() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.database.insert_track(&track).await.unwrap();

    let error = manager
        .open_release_edit(&release.id)
        .await
        .expect_err("an editor seed needs the track's stored audio relationship");

    assert!(error.to_string().contains(&track.id), "{error}");
}

#[tokio::test]
async fn release_edit_seed_projects_track_sources_in_segment_order() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.facts = crate::pressing::made_of(crate::pressing::Medium::Cd, 2);
    let first = crate::db::DbTrack::new_test(&release.id, TRACK_1, "First Track", 1);
    let mut second = crate::db::DbTrack::new_test(&release.id, TRACK_2, "Second Track", 1);
    let second_disc = crate::db::DbReleaseMedium::new(
        &release.id,
        std::num::NonZeroU16::new(2).expect("disc 2"),
        crate::mediums::ReleaseMedium::one_sided(Some(crate::pressing::Medium::Cd)),
        chrono::Utc::now(),
    );
    second.medium_id.clone_from(&second_disc.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager
        .database
        .insert_release_medium(&second_disc)
        .await
        .unwrap();
    manager.database.insert_track(&first).await.unwrap();
    manager.database.insert_track(&second).await.unwrap();
    add_track_audio_sources(
        &manager,
        &first,
        &[("first.flac", crate::album_detail::SourceAudioLayout::File)],
    )
    .await;
    add_track_audio_sources(
        &manager,
        &second,
        &[
            ("second-a.flac", crate::album_detail::SourceAudioLayout::Cue),
            ("second-b.flac", crate::album_detail::SourceAudioLayout::Cue),
        ],
    )
    .await;
    store_test_cover(&manager, &release.id, b"cover").await;

    let seed = manager.open_release_edit(&release.id).await.unwrap()
        .opened()
        .clone();

    assert_eq!(
        seed.cover.as_ref().map(|cover| cover.id.as_str()),
        Some(release.id.as_str())
    );
    let sides = &seed.display.sides;
    assert_eq!(
        sides.iter().map(|side| &side.side).collect::<Vec<_>>(),
        [
            &crate::album_detail::TrackSide::Disc { disc: 1 },
            &crate::album_detail::TrackSide::Disc { disc: 2 },
        ]
    );
    let crate::album_detail::ReleaseEditTrackRun::Track(first_context) = &sides[0].runs[0] else {
        panic!("a track on its own file is a run of its own");
    };
    assert_eq!(first_context.track_id, first.id);
    assert_eq!(
        first_context.sources[0].layout,
        crate::album_detail::SourceAudioLayout::File
    );
    // A track playing two files is no sheet's run, though a sheet carves both.
    let crate::album_detail::ReleaseEditTrackRun::Track(second_context) = &sides[1].runs[0] else {
        panic!("a track over two files is a run of its own");
    };
    assert_eq!(second_context.track_id, second.id);
    assert_eq!(
        second_context
            .sources
            .iter()
            .map(|source| source.name.as_str())
            .collect::<Vec<_>>(),
        vec!["second-a.flac", "second-b.flac"]
    );
    assert!(matches!(
        seed.display.source_audio,
        Some(crate::album_detail::SourceAudioSummary::Mixed { .. })
    ));
}

#[tokio::test]
async fn release_metadata_edit_preserves_records_and_audio() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Track Title", 1);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    manager.database.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        &manager,
        &track,
        &[("track.flac", crate::album_detail::SourceAudioLayout::File)],
    )
    .await;
    let identity = crate::import::ReleaseRecord::new(
        &crate::import::MetadataRef::new(crate::import::Catalog::MusicBrainz, "source-release"),
        Some("source-group".to_string()));
    manager
        .database
        .insert_release_records(&release.id, std::slice::from_ref(&identity))
        .await
        .unwrap();
    let stored_before = manager
        .database
        .find_release_detail_context(&release.id)
        .await
        .unwrap()
        .expect("the release detail is stored")
        .detail;

    manager
        .apply_release_metadata_user_edit(
            &release.id,
            &crate::import::ReleaseUserEdit {
                album_title: "Edited Album".to_string(),
                album_artist_assignments: vec![crate::import::Credited::plain(crate::import::ArtistAssignment::named(
                    "Edited Album Artist",
                ))],
                album_year: Some(1984),
                pressing: crate::pressing::Pressing {
                    year: Some(1991),
                    labels: vec![crate::pressing::ReleaseLabel::of(Some("Edited Label"), Some("CAT-1"))],
                    facts: crate::pressing::PressingFacts { area: Some(crate::pressing::area("US")), ..Default::default() },
                    barcodes: vec!["123456789".to_string()],
                },
                tracks: vec![crate::import::TrackUserEdit {
                    title: "Edited Track".to_string(),
                    artist_assignments: crate::import::TrackArtistAssignments::Explicit(vec![
                        crate::import::Credited::plain(crate::import::ArtistAssignment::named("Edited Track Artist")),
                    ]),
                    file: None,
                }],
            },
        )
        .await
        .unwrap();

    let stored_after = manager
        .database
        .find_release_detail_context(&release.id)
        .await
        .unwrap()
        .expect("the edited release remains stored")
        .detail;
    assert_eq!(stored_after.audio_formats, stored_before.audio_formats);
    assert_eq!(stored_after.audio_segments, stored_before.audio_segments);
    assert_eq!(stored_after.files, stored_before.files);
    assert_eq!(
        manager
            .database
            .get_release_records(&release.id)
            .await
            .unwrap(),
        vec![identity]
    );
    assert_eq!(stored_after.release.id, release.id);
    assert_eq!(stored_after.release.album_id, album.id);
    assert_eq!(stored_after.release.pressing.year, Some(1991));
    assert_eq!(stored_after.tracks[0].track.title, "Edited Track");
    // The track's place is not in the edit: it stays where the import put it.
    assert_eq!(stored_after.tracks[0].track.side, track.side);
    assert_eq!(stored_after.tracks[0].track.track_number, track.track_number);
    assert_eq!(
        manager
            .get_artists_for_track(&track.id)
            .await
            .unwrap()
            .into_iter()
            .map(|artist| artist.artist.name)
            .collect::<Vec<_>>(),
        vec!["Edited Track Artist"]
    );
}

/// Saving an edit whose album artist is named, not picked, links the library
/// artist of that name when the library already holds one — as saving the
/// same edit a second time does.
#[tokio::test]
async fn a_named_album_artist_already_in_the_library_is_linked() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    let edit = crate::import::ReleaseUserEdit {
        album_title: "Album Title".to_string(),
        album_artist_assignments: vec![
            crate::import::Credited::plain(crate::import::ArtistAssignment::named("Album Artist One")),
            crate::import::Credited::plain(crate::import::ArtistAssignment::named("Album Artist Two")),
        ],
        album_year: None,
        pressing: crate::pressing::Pressing::blank(),
        tracks: Vec::new(),
    };
    manager
        .apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .unwrap();
    let artists = manager.get_artists_for_album(&album.id).await.unwrap();

    manager
        .apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .unwrap();

    assert_eq!(
        manager.get_artists_for_album(&album.id).await.unwrap(),
        artists
    );
    assert_eq!(
        artists
            .into_iter()
            .map(|artist| artist.artist.name)
            .collect::<Vec<_>>(),
        vec!["Album Artist One", "Album Artist Two"]
    );
}

/// A credit list naming one artist twice keeps both credits: each place in
/// the list is its own row.
#[tokio::test]
async fn an_album_credit_list_naming_one_artist_twice_keeps_both_credits() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    let edit = crate::import::ReleaseUserEdit {
        album_title: "Album Title".to_string(),
        album_artist_assignments: vec![
            crate::import::Credited::plain(crate::import::ArtistAssignment::named("Album Artist One")),
            crate::import::Credited::plain(crate::import::ArtistAssignment::named("Album Artist Two")),
            crate::import::Credited::plain(crate::import::ArtistAssignment::named("Album Artist One")),
        ],
        album_year: None,
        pressing: crate::pressing::Pressing::blank(),
        tracks: Vec::new(),
    };

    manager
        .apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .unwrap();

    let artists = manager.get_artists_for_album(&album.id).await.unwrap();
    assert_eq!(
        artists
            .iter()
            .map(|artist| artist.artist.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Album Artist One", "Album Artist Two", "Album Artist One"]
    );
    assert_eq!(artists[0].artist.id, artists[2].artist.id, "one library artist, credited twice");
}

/// An edit of a two-track release crediting two album artists and, per track,
/// the artists `track_artists` names, in order.
fn credited_release_edit(track_artists: [&[&str]; 2]) -> crate::import::ReleaseUserEdit {
    crate::import::ReleaseUserEdit {
        album_title: "Album Title".to_string(),
        album_artist_assignments: vec![
            crate::import::Credited::plain(crate::import::ArtistAssignment::named("Album Artist One")),
            crate::import::Credited::plain(crate::import::ArtistAssignment::named("Album Artist Two")),
        ],
        album_year: Some(2024),
        pressing: crate::pressing::Pressing::blank(),
        tracks: track_artists
            .iter()
            .enumerate()
            .map(|(index, artists)| crate::import::TrackUserEdit {
                title: format!("Track {}", index + 1),
                artist_assignments: crate::import::TrackArtistAssignments::Explicit(
                    artists
                        .iter()
                        .map(|name| {
                            crate::import::Credited::plain(crate::import::ArtistAssignment::named(
                                *name,
                            ))
                        })
                        .collect(),
                ),
                file: None,
            })
            .collect(),
    }
}

async fn queued_write_rows(manager: &LibraryManager) -> Vec<Vec<String>> {
    manager.database.queued_write_rows_for_test().await.unwrap()
}

/// Saving a release's metadata again without changing it is no write: nothing
/// waits to sync.
#[tokio::test]
async fn saving_unchanged_metadata_writes_nothing() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_n_tracks(&manager.database, &release.id, 2).await;
    let edit = credited_release_edit([
        &["Track Artist One", "Track Artist Two"],
        &["Track Artist One", "Track Artist Three"],
    ]);
    manager
        .apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .unwrap();
    let writes = queued_write_rows(&manager).await;

    manager
        .apply_release_metadata_user_edit(&release.id, &edit)
        .await
        .unwrap();

    assert_eq!(queued_write_rows(&manager).await, writes);
}

/// Changing one track's credit rewrites the row of that place in its list
/// and nothing else: every credit the edit leaves alone is left alone in the
/// store.
#[tokio::test]
async fn changing_one_track_credit_writes_only_that_credit() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    insert_n_tracks(&manager.database, &release.id, 2).await;
    manager
        .apply_release_metadata_user_edit(
            &release.id,
            &credited_release_edit([
                &["Track Artist One", "Track Artist Two"],
                &["Track Artist One", "Track Artist Three"],
            ]),
        )
        .await
        .unwrap();
    let writes = queued_write_rows(&manager).await.len();

    manager
        .apply_release_metadata_user_edit(
            &release.id,
            &credited_release_edit([
                &["Track Artist One", "Track Artist Four"],
                &["Track Artist One", "Track Artist Three"],
            ]),
        )
        .await
        .unwrap();

    let rows = queued_write_rows(&manager).await;
    assert_eq!(rows.len(), writes + 1);
    let credit_rows = rows[writes]
        .iter()
        .filter(|row| row.starts_with("album_artists") || row.starts_with("track_artists"))
        .collect::<Vec<_>>();
    assert_eq!(
        credit_rows,
        ["track_artists"],
        "the one place whose artist changed"
    );
    assert_eq!(
        rows[writes].iter().filter(|row| *row == "artists").count(),
        1,
        "the new artist"
    );
    let first_track = &manager.get_tracks_for_release(&release.id).await.unwrap()[0];
    assert_eq!(
        manager
            .get_artists_for_track(&first_track.id)
            .await
            .unwrap()
            .into_iter()
            .map(|artist| artist.artist.name)
            .collect::<Vec<_>>(),
        vec!["Track Artist One", "Track Artist Four"]
    );
}
