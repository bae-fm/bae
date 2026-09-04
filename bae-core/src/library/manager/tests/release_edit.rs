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
        pregap_ms: None,
        generated_pregap_ms: None,
        pregap_samples: None,
        generated_pregap_samples: None,
        sample_rate: 44_100,
        bits_per_sample: Some(16),
        channels: 2,
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
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Track Title", Some(1));
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        &manager,
        &track,
        &[("track.flac", crate::album_detail::SourceAudioLayout::File)],
    )
    .await;

    let seed = manager.release_edit_seed(&release.id).await.unwrap();

    assert_eq!(seed.edit.tracks[0].id, track.id);
}

#[tokio::test]
async fn release_edit_reset_preserves_persisted_track_ids() {
    use crate::import::{MetadataRef, MetadataSource, ReleaseReseed};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    insert_n_tracks(&manager.database, &release.id, 2).await;
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
    seed_release_cache(source_release_id, (response, None, raw_json));
    seed_release_group_json_cache(
        source_group_id,
        r#"{"id":"reset-editor-group"}"#.to_string(),
    );
    manager
        .re_identify_release(
            &release.id,
            ReleaseReseed::ExternalRelease {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: source_release_id.to_string(),
                },
                partners: vec![],
            },
        )
        .await
        .unwrap();

    let reset = manager
        .reset_release_edit_to_source(&release.id)
        .await
        .unwrap();
    assert_eq!(
        reset
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
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Track Title", Some(1));
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();

    let error = manager
        .release_edit_seed(&release.id)
        .await
        .expect_err("an editor seed needs the track's stored audio relationship");

    assert!(error.to_string().contains(&track.id), "{error}");
}

#[tokio::test]
async fn release_edit_seed_projects_track_sources_in_segment_order() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.format = Some("2xCD".to_string());
    let first = crate::db::DbTrack::new_test(&release.id, TRACK_1, "First Track", Some(1));
    let mut second = crate::db::DbTrack::new_test(&release.id, TRACK_2, "Second Track", Some(1));
    second.side = 2;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
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
    add_cover_row(&manager, &release.id).await;

    let seed = manager.release_edit_seed(&release.id).await.unwrap();

    assert_eq!(
        seed.cover.as_ref().map(|cover| cover.id.as_str()),
        Some(release.id.as_str())
    );
    assert_eq!(seed.display.tracks[0].track_id, first.id);
    assert_eq!(
        seed.display.tracks[0].sources[0].layout,
        crate::album_detail::SourceAudioLayout::File
    );
    assert_eq!(
        seed.display.tracks[0].side,
        crate::album_detail::TrackSide::Disc { disc: 1 }
    );
    assert_eq!(seed.display.tracks[1].track_id, second.id);
    assert_eq!(
        seed.display.tracks[1]
            .sources
            .iter()
            .map(|source| source.name.as_str())
            .collect::<Vec<_>>(),
        vec!["second-a.flac", "second-b.flac"]
    );
    assert_eq!(
        seed.display.tracks[1].side,
        crate::album_detail::TrackSide::Disc { disc: 2 }
    );
    assert!(matches!(
        seed.display.source_audio,
        Some(crate::album_detail::SourceAudioSummary::Mixed { .. })
    ));
}

#[tokio::test]
async fn release_metadata_edit_preserves_identity_provenance_and_audio() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_provenance = Some(crate::import::MetadataProvenance::FileTags);
    let track = crate::db::DbTrack::new_test(&release.id, TRACK_1, "Track Title", Some(1));
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    add_track_audio_sources(
        &manager,
        &track,
        &[("track.flac", crate::album_detail::SourceAudioLayout::File)],
    )
    .await;
    let identity = crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::MusicBrainz,
        source_release_id: "source-release".to_string(),
        source_group_id: "source-group".to_string(),
    };
    manager
        .database
        .insert_release_identities(&release.id, std::slice::from_ref(&identity))
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
                album_artist_assignments: vec![crate::import::ArtistAssignment::new(
                    "Edited Album Artist",
                )],
                album_year: Some(1984),
                pressing: crate::import::PressingEdit {
                    year: Some(1991),
                    format: Some("CD".to_string()),
                    label: Some("Edited Label".to_string()),
                    catalog_number: Some("CAT-1".to_string()),
                    country: Some("US".to_string()),
                    barcode: Some("123456789".to_string()),
                },
                tracks: vec![crate::import::TrackUserEdit {
                    title: "Edited Track".to_string(),
                    side: 2,
                    track_number: Some(3),
                    artist_assignments: crate::import::TrackArtistAssignments::Explicit(vec![
                        crate::import::ArtistAssignment::new("Edited Track Artist"),
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
            .get_release_identities(&release.id)
            .await
            .unwrap(),
        vec![identity]
    );
    assert_eq!(
        stored_after.release.metadata_provenance,
        release.metadata_provenance
    );
    assert_eq!(stored_after.release.id, release.id);
    assert_eq!(stored_after.release.album_id, album.id);
    assert_eq!(stored_after.release.pressing.year, Some(1991));
    assert_eq!(stored_after.tracks[0].track.title, "Edited Track");
    assert_eq!(stored_after.tracks[0].track.side, 2);
    assert_eq!(stored_after.tracks[0].track.track_number, Some(3));
    assert_eq!(
        manager
            .get_artists_for_track(&track.id)
            .await
            .unwrap()
            .into_iter()
            .map(|artist| artist.name)
            .collect::<Vec<_>>(),
        vec!["Edited Track Artist"]
    );
}
