use super::*;

/// A typed field replaces that one field of the form and leaves the rest to
/// the pick. Committing it empty is the person clearing the field, not undoing
/// their edit: the blank is stored and the form comes back blank.
#[tokio::test(flavor = "multi_thread")]
async fn a_typed_field_lands_in_the_next_form_empty_included() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let seeded = pane(&handle, &key).await;
    let seeded_artists = seeded
        .metadata_draft
        .album_artist_assignments;

    handle
        .set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            "Typed Title".to_string(),
        )
        .await
        .unwrap();

    let edited = pane(&handle, &key).await.metadata_draft;
    assert_eq!(edited.album_title, "Typed Title");
    assert_eq!(
        edited.album_artist_assignments, seeded_artists,
        "nothing else moved with it"
    );

    handle
        .set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            String::new(),
        )
        .await
        .unwrap();

    assert_eq!(pane(&handle, &key).await.metadata_draft.album_title, "");
    let stored = handle
        .library_manager
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap();
    assert_eq!(
        stored.metadata_draft.album_title,
        String::new(),
        "a cleared field is a value the person set, not an absent edit"
    );

    shut_down(handle).await;
}

/// A source-less candidate owns a draft immediately, so direct entry needs no
/// source selection before it can persist edits.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_with_no_metadata_source_updates_the_draft() {
    let (manager, tmp) = setup_test_manager().await;
    let (_candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();

    handle
        .set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::PressingYear,
            "1991".into(),
        )
        .await
        .unwrap();
    assert_eq!(pane(&handle, &key).await.metadata_draft.pressing.year, "1991");

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn album_artist_assignments_preserve_existing_and_new_artist_choices() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let existing = make_artist("Existing Artist", Some("discogs-existing"), None);
    handle
        .library_manager
        .insert_artist(&existing)
        .await
        .unwrap();
    let assignments = vec![
        crate::import::ArtistAssignment::existing(existing.clone().into()),
        crate::import::ArtistAssignment::new("New Artist"),
    ];

    handle
        .set_candidate_album_artists(&key, assignments.clone())
        .await
        .unwrap();

    assert_eq!(
        pane(&handle, &key)
            .await
            .metadata_draft
            .album_artist_assignments,
        assignments
    );
    shut_down(handle).await;
}

/// Pointing a row at audio another row holds swaps the two rows' bindings in
/// one write: the displaced row takes the chosen row's previous audio, so two
/// rows never hold one file and the displaced file never silently unbinds.
#[tokio::test(flavor = "multi_thread")]
async fn choosing_audio_another_row_holds_swaps_the_two_rows() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.mapping);
    let first_file = before[0].file.clone().expect("row 0 is paired");
    let second_file = before[1].file.clone().expect("row 1 is paired");

    handle
        .set_candidate_track_edit(
            &key,
            crate::import::RawTrackEdit {
                file: Some(second_file.clone()),
                ..before[0].clone()
            },
        )
        .await
        .unwrap();

    let after = track_rows(&pane(&handle, &key).await.mapping);
    assert_eq!(after[0].file, Some(second_file));
    assert_eq!(after[1].file, Some(first_file));
    assert_eq!(after[1].title, before[1].title);

    shut_down(handle).await;
}

/// An edited row comes back edited and its neighbours come back untouched.
#[tokio::test(flavor = "multi_thread")]
async fn an_edited_track_row_redraws_alone() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.mapping);
    let first = before.first().expect("the folder names tracks").clone();
    let untouched = before[1].title.clone();

    handle
        .set_candidate_track_edit(
            &key,
            crate::import::RawTrackEdit {
                title: "Renamed".to_string(),
                artist_assignments: crate::import::TrackArtistAssignments::Explicit(vec![
                    crate::import::ArtistAssignment::new("Someone"),
                ]),
                ..first.clone()
            },
        )
        .await
        .unwrap();

    let after = track_rows(&pane(&handle, &key).await.mapping);
    assert_eq!(after.len(), before.len());
    assert_eq!(after[0].title, "Renamed");
    assert_eq!(
        after[0].artist_assignments,
        crate::import::TrackArtistAssignments::Explicit(vec![
            crate::import::ArtistAssignment::new("Someone")
        ])
    );
    assert_eq!(
        after[0].file, first.file,
        "the audio the row was bound to rides through the edit"
    );
    assert_eq!(after[1].title, untouched);

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_metadata_only_track_edit_does_not_freeze_automatic_file_alignment() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let mut tracks = track_rows(&pane(&handle, &key).await.mapping);
    tracks[1].title = "Renamed".to_string();
    handle
        .set_candidate_track_edit(&key, tracks[1].clone())
        .await
        .unwrap();

    handle
        .set_file_role(
            key,
            "01 Track.flac".to_string(),
            crate::import::folder_scanner::FileRoleChoice::NotATrack,
        )
        .await
        .unwrap();

    let preparation = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the reshaped candidate remains prepared");
    assert_eq!(
        preparation.track_mappings[0].file.audio(),
        Some(&crate::import::AudioFile::Standalone {
            file_id: "02 Track.flac".to_string(),
        })
    );
    assert_eq!(preparation.track_mappings[1].file.audio(), None);
    assert!(preparation.track_mappings.iter().all(|mapping| matches!(
        mapping.file,
        crate::import::CandidateTrackFileBinding::Automatic(_)
    )));

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_track_edit_that_keeps_artist_ids_keeps_the_prepared_artist_image() {
    let (handle, tmp, key, hash) = pane_fixture().await;
    let preparation = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the picked candidate has prepared metadata");
    let mut edit = preparation.metadata_draft;
    edit.album_artist_assignments = vec![crate::import::ArtistAssignment::New {
        seed: crate::import::NewArtistSeed {
            name: "Artist Name".to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: Some("discogs-artist".to_string()),
        },
    }];
    let image = crate::import::PreparedArtistImage::Image {
        discogs_artist_id: "discogs-artist".to_string(),
        source_url: "https://images.example/artist.jpg".to_string(),
        image: crate::import::cover_art::RemoteImage {
            content_type: crate::util::content_type::ContentType::Jpeg,
            bytes: vec![1, 2, 3, 4],
        },
    };
    handle
        .library_manager
        .replace_candidate_metadata_prepared(
            &tmp.path().join("watched").to_string_lossy(),
            &hash,
            &key,
            preparation.file_edit_revision,
            preparation.metadata_revision,
            &crate::import::CandidateMetadataDraft {
                edit,
                track_mappings: preparation.track_mappings,
                source_discogs_artist_ids: Default::default(),
                provenance: preparation.metadata_provenance,
                cover: preparation.cover,
                assets: crate::import::CandidatePreparedAssets {
                    remote_cover: preparation.assets.remote_cover,
                    artist_images: vec![image.clone()],
                },
            },
        )
        .await
        .unwrap();
    let mut track = track_rows(&pane(&handle, &key).await.mapping)
        .into_iter()
        .next()
        .expect("the candidate has a track");
    track.title = "Renamed Track".to_string();

    handle.set_candidate_track_edit(&key, track).await.unwrap();

    assert_eq!(
        handle
            .library_manager
            .load_import_candidate_prepared_assets(&hash)
            .await
            .unwrap()
            .artist_images,
        vec![image],
        "an edit that retains the same provider identities retains their exact prepared bytes"
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn discogs_artist_image_is_prepared_with_the_candidate_and_materialized_by_import() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();

    let numeric_id = || (uuid::Uuid::new_v4().as_u128() % u128::from(u64::MAX)) as u64;
    let album_artist_id = numeric_id();
    let track_artist_id = numeric_id();
    let source_release_id = numeric_id().to_string();
    let raw_release = serde_json::json!({
        "id": source_release_id.parse::<u64>().unwrap(),
        "title": "Album Title",
        "year": 2020,
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": album_artist_id, "name": "Album Artist" }],
        "tracklist": [
            {
                "position": "1",
                "title": "Track One",
                "duration": "0:01",
                "artists": [{ "id": track_artist_id, "name": "Track Artist" }],
            },
            { "position": "2", "title": "Track Two", "duration": "0:01" },
        ],
    })
    .to_string();
    let parsed_release = crate::discogs::client::parse_discogs_release_json(&raw_release).unwrap();
    crate::discogs::client::seed_release_cache(
        &source_release_id,
        (parsed_release, raw_release),
    );
    crate::musicbrainz::seed_discogs_url_lookup(&source_release_id, None);
    crate::discogs::client::seed_artist_image_response(&album_artist_id.to_string(), None);
    let prepared_artist_id = track_artist_id.to_string();
    let expected_bytes = bae_test_support::cover_png();
    let source_url = bae_test_support::cover_art_archive().serve_image(
        &format!("/discogs-artist-{}.png", uuid::Uuid::new_v4()),
        expected_bytes.clone(),
    );
    crate::discogs::client::seed_artist_image_response(
        &prepared_artist_id,
        Some(source_url.clone()),
    );

    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::Discogs,
                release_id: source_release_id,
            },
        )
        .await
        .unwrap();

    let prepared = handle
        .library_manager
        .load_import_candidate_prepared_assets(&hash)
        .await
        .unwrap();
    assert!(prepared.artist_images.contains(
        &crate::import::PreparedArtistImage::Image {
            discogs_artist_id: prepared_artist_id.clone(),
            source_url: source_url.clone(),
            image: crate::import::cover_art::RemoteImage {
                content_type: crate::util::content_type::ContentType::Png,
                bytes: expected_bytes.clone(),
            },
        }
    ));

    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let (release_id, _album_id) = loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the import reports its result")
            .expect("the import event stream remains open");
        match event {
            crate::import::handle::ImportEvent::ImportProgress {
                progress:
                    crate::import::ImportProgress::Complete {
                        import_id: completed_import_id,
                        id,
                        album_id,
                    },
                ..
            } if completed_import_id == import_id => break (id, album_id),
            crate::import::handle::ImportEvent::ImportProgress {
                progress:
                    crate::import::ImportProgress::Failed {
                        import_id: failed_import_id,
                        error,
                    },
                ..
            } if failed_import_id == import_id => panic!("import failed: {error}"),
            _ => {}
        }
    };
    let first_track = handle
        .library_manager
        .get_tracks_for_release(&release_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("the imported release has its first track");
    let artist = handle
        .library_manager
        .get_artists_for_track(&first_track.id)
        .await
        .unwrap()
        .into_iter()
        .find(|artist| artist.discogs_artist_id.as_deref() == Some(&prepared_artist_id))
        .expect("the imported track retains the prepared Discogs artist");
    let artist_search = handle
        .library_manager
        .search_artists(
            &crate::library::LibrarySearchQuery::parse("Track Artist")
                .expect("the artist query is not blank"),
        )
        .await
        .unwrap()
        .into_iter()
        .find(|result| result.artist.id == artist.id)
        .expect("the imported track artist is searchable");
    let image = artist_search
        .image
        .expect("the imported artist has a materialized image");
    assert_eq!(
        handle
            .library_manager
            .read_image_blob(&image)
            .await
            .unwrap()
            .expect("the materialized artist image is readable"),
        expected_bytes
    );

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn import_admission_refuses_an_incomplete_candidate_revision() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    handle
        .library_manager
        .replace_import_candidate_album_artists(
            &hash,
            &[crate::import::ArtistAssignment::new("Changed Artist")],
        )
        .await
        .unwrap();

    let error = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .expect_err("the incomplete revision must not enter the import queue");

    assert!(error.to_string().contains("complete prepared asset set"));
    assert!(handle.candidate_runtime(&key).is_none());
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_claimed_candidate_refuses_every_preparation_mutation() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let before = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate is prepared");
    let pane_before = pane(&handle, &key).await;
    let first_track = track_rows(&pane_before.mapping)[0].clone();
    let cover = pane_before
        .cover
        .expect("the fixture has a selected cover")
        .selection;
    handle.claim_candidate_for_import_for_test(&key).await;

    fn assert_refused<T>(result: Result<T, crate::import::ImportError>) {
        assert!(matches!(
            result,
            Err(crate::import::ImportError::CandidateImportInProgress)
        ));
    }

    assert_refused(
        handle
            .set_candidate_edit_field(
                &key,
                crate::import::CandidateEditField::AlbumTitle,
                "Blocked title".to_string(),
            )
            .await,
    );
    assert_refused(
        handle
            .set_candidate_album_artists(
                &key,
                vec![crate::import::ArtistAssignment::new("Blocked artist")],
            )
            .await,
    );
    assert_refused(
        handle
            .set_candidate_track_edit(&key, first_track.clone())
            .await,
    );
    assert_refused(
        handle
            .set_candidate_track_artists(
                &key,
                vec![first_track.id.clone()],
                crate::import::TrackArtistAssignments::AlbumArtists,
            )
            .await,
    );
    assert_refused(
        handle
            .drop_candidate_track(&key, first_track.id)
            .await,
    );
    assert_refused(handle.set_candidate_cover(&key, cover).await);
    assert_refused(
        handle
            .select_candidate_metadata_provenance(
                key.clone(),
                crate::import::MetadataProvenance::FileTags,
            )
            .await,
    );
    assert_refused(handle.clear_candidate_metadata(key.clone()).await);
    assert_refused(
        handle
            .set_file_role(
                key.clone(),
                "02 Track.flac".to_string(),
                crate::import::folder_scanner::FileRoleChoice::NotATrack,
            )
            .await,
    );

    let after = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate remains prepared");
    assert_eq!(after, before);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_imported_candidate_refuses_metadata_edits() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .set_candidate_album_artists(
            &key,
            vec![crate::import::ArtistAssignment::new("Fixture Artist")],
        )
        .await
        .unwrap();
    let pane_before = pane(&handle, &key).await;
    let first_track = track_rows(&pane_before.mapping)[0].clone();
    let cover = pane_before
        .cover
        .expect("the fixture has a selected cover")
        .selection;
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .expect("the prepared candidate enters the import queue");
    let release_id = loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the import reports its result")
            .expect("the import event stream remains open");
        match event {
            crate::import::handle::ImportEvent::ImportProgress {
                progress:
                    crate::import::ImportProgress::Complete {
                        import_id: completed_import_id,
                        id,
                        ..
                    },
                ..
            } if completed_import_id == import_id => break id,
            crate::import::handle::ImportEvent::ImportProgress {
                progress:
                    crate::import::ImportProgress::Failed {
                        import_id: failed_import_id,
                        error,
                    },
                ..
            } if failed_import_id == import_id => panic!("import failed: {error}"),
            _ => {}
        }
    };
    let candidate_before = handle
        .library_manager
        .load_import_candidate_preparation(&_hash)
        .await
        .unwrap()
        .expect("the imported candidate preparation remains stored");
    let release_before = handle
        .library_manager
        .release_edit_seed(&release_id)
        .await
        .unwrap();

    fn assert_refused<T>(result: Result<T, crate::import::ImportError>) {
        assert!(matches!(
            result,
            Err(crate::import::ImportError::CandidateAlreadyImported)
        ));
    }
    assert_refused(
        handle
            .set_candidate_edit_field(
                &key,
                crate::import::CandidateEditField::AlbumTitle,
                "Edited after import".to_string(),
            )
            .await,
    );
    assert_refused(
        handle
            .set_candidate_album_artists(
                &key,
                vec![crate::import::ArtistAssignment::new("Blocked artist")],
            )
            .await,
    );
    assert_refused(
        handle
            .set_candidate_track_edit(&key, first_track.clone())
            .await,
    );
    assert_refused(
        handle
            .set_candidate_track_artists(
                &key,
                vec![first_track.id.clone()],
                crate::import::TrackArtistAssignments::AlbumArtists,
            )
            .await,
    );
    assert_refused(
        handle
            .drop_candidate_track(&key, first_track.id)
            .await,
    );
    assert_refused(handle.set_candidate_cover(&key, cover).await);
    assert_refused(
        handle
            .select_candidate_metadata_provenance(
                key.clone(),
                crate::import::MetadataProvenance::FileTags,
            )
            .await,
    );
    assert_refused(handle.clear_candidate_metadata(key.clone()).await);
    assert_refused(
        handle
            .set_file_role(
                key,
                "02 Track.flac".to_string(),
                crate::import::folder_scanner::FileRoleChoice::NotATrack,
            )
            .await,
    );

    assert_eq!(
        handle
            .library_manager
            .load_import_candidate_preparation(&_hash)
            .await
            .unwrap()
            .expect("the imported candidate preparation remains stored"),
        candidate_before
    );
    assert_eq!(
        handle
            .library_manager
            .release_edit_seed(&release_id)
            .await
            .unwrap(),
        release_before
    );

    let mut persisted_edit = release_before.edit;
    persisted_edit.album_title = "Edited imported release".to_string();
    handle
        .library_manager
        .apply_release_metadata_user_edit(&release_id, &persisted_edit.shape().unwrap())
        .await
        .unwrap();
    assert_eq!(
        handle
            .library_manager
            .load_import_candidate_preparation(&_hash)
            .await
            .unwrap()
            .expect("the persisted release edit leaves candidate preparation alone"),
        candidate_before
    );
    assert_eq!(
        handle
            .library_manager
            .release_edit_seed(&release_id)
            .await
            .unwrap()
            .edit
            .album_title,
        "Edited imported release"
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn import_worker_refuses_a_prepared_but_invalid_metadata_draft() {
    let (manager, tmp) = setup_test_manager().await;
    let (_candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();

    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .expect("the complete candidate enters source validation");
    let error = loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the import reports its result")
            .expect("the import event stream remains open");
        match event {
            crate::import::handle::ImportEvent::ImportProgress {
                progress:
                    crate::import::ImportProgress::Failed {
                        error,
                        import_id: failed_import_id,
                    },
                ..
            } if failed_import_id == import_id => break error,
            _ => {}
        }
    };

    assert!(
        error.contains("Album title is required"),
        "unexpected worker error: {error}"
    );
    shut_down(handle).await;
}

/// One spreadsheet fill writes the same artist choice onto every named row
/// while preserving each row's title and audio mapping.
#[tokio::test(flavor = "multi_thread")]
async fn track_artist_assignments_fill_across_named_rows() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.mapping);
    let target_ids = before.iter().map(|track| track.id.clone()).collect();
    let assignments = crate::import::TrackArtistAssignments::Explicit(vec![
        crate::import::ArtistAssignment::new("Filled Artist"),
    ]);

    handle
        .set_candidate_track_artists(&key, target_ids, assignments.clone())
        .await
        .unwrap();

    let after = track_rows(&pane(&handle, &key).await.mapping);
    assert_eq!(after.len(), before.len());
    for (before, after) in before.iter().zip(after.iter()) {
        assert_eq!(after.artist_assignments, assignments);
        assert_eq!(after.title, before.title);
        assert_eq!(after.file, before.file);
    }

    shut_down(handle).await;
}

/// A dropped row leaves the table: the release commits without that track.
#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_track_leaves_the_table() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.mapping);
    let dropped = before[0].id.clone();
    let kept = before[1].id.clone();

    handle
        .drop_candidate_track(&key, dropped.clone())
        .await
        .unwrap();

    let after = track_rows(&pane(&handle, &key).await.mapping);
    assert!(
        !after.iter().any(|track| track.id == dropped),
        "the dropped row is gone"
    );
    assert!(after.iter().any(|track| track.id == kept));

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn metadata_source_changes_preserve_explicit_track_file_mappings() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let mut tracks = track_rows(&pane(&handle, &key).await.mapping);
    let first_file = tracks[0].file.clone();
    let second_file = tracks[1].file.clone();
    tracks[0].file.clone_from(&second_file);
    tracks[1].file.clone_from(&first_file);
    for track in &tracks {
        handle
            .set_candidate_track_edit(&key, track.clone())
            .await
            .unwrap();
    }

    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileTags,
        )
        .await
        .unwrap();
    let reapplied = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the reapplied source remains prepared");
    assert_eq!(
        reapplied.track_mappings[0].file.audio().cloned(),
        second_file
    );
    assert_eq!(
        reapplied.track_mappings[1].file.audio().cloned(),
        first_file
    );

    handle.clear_candidate_metadata(key).await.unwrap();
    let cleared = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the cleared source remains prepared");
    assert_eq!(cleared.track_mappings[0].file.audio().cloned(), second_file);
    assert_eq!(cleared.track_mappings[1].file.audio().cloned(), first_file);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_decision_after_a_drop_preserves_every_mapping_identity() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let tracks = track_rows(&pane(&handle, &key).await.mapping);
    let dropped_id = tracks[0].id.clone();
    let kept_id = tracks[1].id.clone();
    handle
        .drop_candidate_track(&key, dropped_id.clone())
        .await
        .unwrap();

    handle
        .set_file_role(
            key,
            "02 Track.flac".to_string(),
            crate::import::folder_scanner::FileRoleChoice::NotATrack,
        )
        .await
        .unwrap();

    let preparation = handle
        .library_manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the reshaped candidate remains prepared");
    let active = crate::import::edits::apply_track_mappings_to_draft(
        preparation.metadata_draft,
        &preparation.track_mappings,
    )
    .expect("the draft and physical mappings retain the same identities");
    assert_eq!(active.tracks.len(), 1);
    assert_eq!(active.tracks[0].id, kept_id);
    assert!(preparation
        .track_mappings
        .iter()
        .any(|mapping| mapping.track_id == dropped_id && mapping.dropped));
    shut_down(handle).await;
}

/// Applying File Tags persists the default cover, so the pane and queue keep
/// drawing it without relying on selection state.
#[tokio::test(flavor = "multi_thread")]
async fn file_tags_persists_the_conventional_folder_cover() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let cover = pane(&handle, &key)
        .await
        .cover
        .expect("File Tags applies its deterministic default cover");
    assert_eq!(
        cover.selection,
        crate::import::CoverSelection::Local("cover.jpg".to_string())
    );
    let crate::import::cover_art::CoverImageSource::Local { path } = cover.preview else {
        panic!("a folder image is drawn from disk, not fetched");
    };
    assert!(path.ends_with("cover.jpg"));

    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn file_tags_persists_embedded_artwork_ahead_of_the_folder_cover() {
    let (manager, tmp) = setup_test_manager().await;
    let (_candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let bytes = vec![1, 2, 3, 4];
    handle
        .file_tag_snapshot_with_reader(
            &key,
            std::sync::Arc::new(CountingFileTagReader::with_embedded_cover(bytes.clone())),
        )
        .await
        .unwrap();
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileTags,
        )
        .await
        .unwrap();

    let cover = pane(&handle, &key)
        .await
        .cover
        .expect("the embedded default is projected");
    assert_eq!(
        cover.selection,
        crate::import::CoverSelection::Embedded("01 Track.flac".to_string())
    );
    assert_eq!(
        cover.preview,
        crate::import::cover_art::CoverImageSource::Bytes {
            data: bytes.clone()
        }
    );
    shut_down(handle).await;

    let reopened = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    assert_eq!(
        pane(&reopened, &key)
            .await
            .cover
            .expect("the persisted embedded default survives a relaunch")
            .selection,
        crate::import::CoverSelection::Embedded("01 Track.flac".to_string())
    );
    shut_down(reopened).await;
}
