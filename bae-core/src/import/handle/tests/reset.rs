use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_restores_removed_audio_and_initial_metadata() {
    let StoredCandidate {
        handle,
        key,
        tmp: _tmp,
        ..
    } = stored_candidate().await;
    let initial = pane(&handle, &key).await;
    handle
        .drop_candidate_track(&key, initial.metadata_draft.tracks[0].id.clone())
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            "Edited Album".into(),
        )
        .await
        .unwrap();
    handle.reset_candidate_setup(&key).await.unwrap();
    let restored = pane(&handle, &key).await;
    assert_eq!(
        restored.metadata_draft.tracks.len(),
        2,
        "Reset must restore every source track"
    );
    assert_eq!(restored.metadata_draft.album_title, "");
    assert_eq!(restored.metadata_provenance, None);
    shut_down(handle).await;
}

use super::track_restoration::{cue_candidate, preparation};
use crate::import::folder_scanner::{FileRoleChoice, SheetDisc};
use crate::import::{
    AudioFile, CandidateAsRead, CandidateMetadataDraft, CandidatePreparedAssets, MetadataProvenance,
};

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_restores_cue_choices_and_saves_complete_tags() {
    for prefill in [false, true] {
        for choice in ["removed", "ignored", "binding", "role", "disc"] {
            let StoredCandidate {
                mut handle,
                manager,
                candidate,
                key,
                tmp: _tmp,
            } = cue_candidate().await;
            let original_files = candidate.files.clone();
            match choice {
                "removed" => {
                    for row in pane(&handle, &key).await.metadata_draft.tracks {
                        handle.drop_candidate_track(&key, row.id).await.unwrap();
                    }
                }
                "ignored" => handle
                    .set_sheet_disc(key.clone(), "Disc.cue".into(), SheetDisc::Ignored)
                    .await
                    .unwrap(),
                "binding" => handle
                    .set_sheet_binding(key.clone(), "Disc.cue".into(), "01 Track.flac".into(), None)
                    .await
                    .unwrap(),
                "role" => handle
                    .set_file_role(
                        key.clone(),
                        "01 Track.flac".into(),
                        FileRoleChoice::NotATrack,
                    )
                    .await
                    .unwrap(),
                "disc" => handle
                    .set_sheet_disc(
                        key.clone(),
                        "Disc.cue".into(),
                        SheetDisc::Disc { number: 4 },
                    )
                    .await
                    .unwrap(),
                _ => unreachable!(),
            }
            manager.set_prefill_with_tags(prefill).unwrap();
            handle.file_tags = Arc::new(CountingFileTagReader::with_embedded_cover(cover_jpeg()));
            handle
                .set_candidate_cover(
                    &key,
                    crate::import::CoverSelection::Local("cover.jpg".into()),
                )
                .await
                .unwrap();
            let before = preparation(&handle, &candidate.files.content_hash()).await;
            handle.reset_candidate_setup(&key).await.unwrap();
            let after = preparation(&handle, &candidate.files.content_hash()).await;
            let reset = handle.get_release_candidate(&key).await.unwrap().unwrap();
            assert_eq!(
                reset.files(),
                &original_files,
                "{choice}, prefill={prefill}"
            );
            assert_eq!(after.file_edit_revision, before.file_edit_revision + 1);
            assert_eq!(after.metadata_revision, before.metadata_revision + 1);
            assert_eq!(after.draft.tracks.len(), 3);
            assert!(manager
                .load_candidate_file_edits(&candidate.files.content_hash())
                .await
                .unwrap()
                .is_empty());
            assert!(after
                .draft
                .tracks
                .iter()
                .all(|row| matches!(row.edit.file, AudioFile::SheetSlice { .. })));
            if prefill {
                assert_eq!(after.draft.album_title, "Cue Album");
                assert_eq!(after.draft.tracks[0].edit.title, "Cue First");
                assert_eq!(
                    after.metadata_provenance,
                    Some(MetadataProvenance::FileTags)
                );
                assert_eq!(
                    after.cover,
                    Some(crate::import::CoverSelection::Embedded(
                        "01 Track.flac".into()
                    ))
                );
                let stored = manager
                    .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
                    .await
                    .unwrap()
                    .unwrap();
                let snapshot = stored.snapshot.unwrap();
                assert_eq!(snapshot.files.len(), 2);
                assert_eq!(snapshot.file_edit_revision, reset.file_edit_revision());
                assert_eq!(snapshot.scan_generation, stored.scan_generation);
                handle.file_tags = Arc::new(CountingFileTagReader::failing(0));
                handle
                    .select_candidate_metadata_provenance(key.clone(), MetadataProvenance::FileTags)
                    .await
                    .unwrap();
                assert_eq!(
                    preparation(&handle, &candidate.files.content_hash())
                        .await
                        .draft,
                    after.draft
                );
            } else {
                assert!(after.draft.album_title.is_empty());
                assert!(after
                    .draft
                    .tracks
                    .iter()
                    .all(|row| row.edit.title.is_empty()));
                assert_eq!(after.cover, None);
                assert_eq!(after.metadata_provenance, None);
            }
            shut_down(handle).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_preserves_combination_members_and_disc_layout() {
    for prefill in [false, true] {
        let (manager, _library) = setup_test_manager().await;
        let first_root = TempDir::new().unwrap();
        let second_root = TempDir::new().unwrap();
        let (first, first_key, _) = picked_candidate(&manager, &first_root, "Volume A").await;
        let (second, second_key, _) = picked_candidate(&manager, &second_root, "Volume B").await;
        let handle = manager
            .start_import_service(tokio::runtime::Handle::current())
            .await
            .unwrap();
        let member_before = [
            preparation(&handle, &first.files.content_hash()).await,
            preparation(&handle, &second.files.content_hash()).await,
        ];
        let key = handle
            .combine_candidates(vec![first_key, second_key])
            .await
            .unwrap();
        let source = handle.get_release_candidate(&key).await.unwrap().unwrap();
        let initial = pane(&handle, &key).await;
        let rows = initial.metadata_draft.tracks;
        handle
            .drop_candidate_track(&key, rows[2].id.clone())
            .await
            .unwrap();
        handle
            .set_candidate_edit_field(
                &key,
                crate::import::CandidateEditField::AlbumTitle,
                "Edited collection".into(),
            )
            .await
            .unwrap();
        manager.set_prefill_with_tags(prefill).unwrap();
        handle.reset_candidate_setup(&key).await.unwrap();
        let reset = pane(&handle, &key).await;
        assert_eq!(reset.candidate.files(), source.files());
        let crate::import::release_candidate::ReleaseCandidate::Combined(before) = &source else {
            panic!("combined source")
        };
        let crate::import::release_candidate::ReleaseCandidate::Combined(after) = &reset.candidate
        else {
            panic!("combined source")
        };
        assert_eq!(before.combination, after.combination);
        assert_eq!(
            reset
                .metadata_draft
                .tracks
                .iter()
                .map(|row| (row.side, row.track_number))
                .collect::<Vec<_>>(),
            [
                (Some(1), Some(1)),
                (Some(1), Some(2)),
                (Some(2), Some(1)),
                (Some(2), Some(2))
            ]
        );
        if !prefill {
            assert_eq!(reset.metadata_draft.album_title, source.name());
        }
        assert_eq!(
            preparation(&handle, &first.files.content_hash()).await,
            member_before[0]
        );
        assert_eq!(
            preparation(&handle, &second.files.content_hash()).await,
            member_before[1]
        );
        let once = preparation(&handle, &source.files().content_hash()).await;
        handle.reset_candidate_setup(&key).await.unwrap();
        let twice = preparation(&handle, &source.files().content_hash()).await;
        assert_eq!(once.draft, twice.draft);
        assert_eq!(once.cover, twice.cover);
        assert_eq!(once.metadata_provenance, twice.metadata_provenance);
        shut_down(handle).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_tag_failure_keeps_source_preparation_and_snapshot() {
    let StoredCandidate {
        mut handle,
        manager,
        candidate,
        key,
        tmp: _tmp,
    } = stored_candidate().await;
    handle
        .select_candidate_metadata_provenance(key.clone(), MetadataProvenance::FileTags)
        .await
        .unwrap();
    let initial = pane(&handle, &key).await;
    handle
        .drop_candidate_track(&key, initial.metadata_draft.tracks[0].id.clone())
        .await
        .unwrap();
    let before = preparation(&handle, &candidate.files.content_hash()).await;
    let snapshot = manager
        .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
        .await
        .unwrap()
        .unwrap();
    manager.set_prefill_with_tags(true).unwrap();
    handle.file_tags = Arc::new(CountingFileTagReader::failing(1));
    assert!(handle
        .reset_candidate_setup(&key)
        .await
        .unwrap_err()
        .to_string()
        .contains("fixture tag read 1 failed"));
    assert_eq!(
        preparation(&handle, &candidate.files.content_hash()).await,
        before
    );
    let after = manager
        .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.snapshot, snapshot.snapshot);
    assert_eq!(after.candidate, snapshot.candidate);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_refuses_missing_or_changed_sources_without_prefill() {
    for missing in [false, true] {
        let StoredCandidate {
            handle,
            candidate,
            key,
            tmp: _tmp,
            ..
        } = stored_candidate().await;
        let before = preparation(&handle, &candidate.files.content_hash()).await;
        let file = candidate.files.audio().next().unwrap();
        if missing {
            std::fs::remove_file(&file.path).unwrap();
        } else {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&file.path)
                .unwrap()
                .set_len(file.size + 1)
                .unwrap();
        }
        assert!(handle.reset_candidate_setup(&key).await.is_err());
        assert_eq!(
            preparation(&handle, &candidate.files.content_hash()).await,
            before
        );
        shut_down(handle).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_prepared_before_an_edit_cannot_replace_it_or_its_snapshot() {
    let StoredCandidate {
        mut handle,
        manager,
        candidate,
        key,
        tmp: _tmp,
    } = stored_candidate().await;
    manager.set_prefill_with_tags(true).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let resume = Arc::new(std::sync::Barrier::new(2));
    handle.file_tags = Arc::new(CountingFileTagReader::blocking(entered_tx, resume.clone()));
    let resetting = tokio::spawn({
        let handle = handle.clone();
        let key = key.clone();
        async move { handle.reset_candidate_setup(&key).await }
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("reset reads tags");
    let editing = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        handle.set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            "Newer edit".into(),
        ),
    )
    .await;
    resume.wait();
    editing
        .expect("reset preparation releases the commit lock")
        .unwrap();
    let before = preparation(&handle, &candidate.files.content_hash()).await;
    assert!(resetting
        .await
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("metadata changed"));
    assert_eq!(
        preparation(&handle, &candidate.files.content_hash()).await,
        before
    );
    assert_eq!(
        manager
            .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        None
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_invalidates_prepared_metadata_and_refuses_claimed_candidates() {
    let StoredCandidate {
        handle,
        candidate,
        key,
        tmp: _tmp,
        ..
    } = stored_candidate().await;
    let before = preparation(&handle, &candidate.files.content_hash()).await;
    let read = CandidateAsRead {
        content_hash: candidate.files.content_hash(),
        file_edit_revision: before.file_edit_revision,
        metadata_revision: before.metadata_revision,
    };
    let old_metadata = CandidateMetadataDraft {
        draft: before.draft.clone(),
        source_discogs_artist_ids: before.source_discogs_artist_ids.clone(),
        provenance: before.metadata_provenance.clone(),
        cover: before.cover.clone(),
        assets: before.assets.clone(),
    };
    handle.reset_candidate_setup(&key).await.unwrap();
    let reset = preparation(&handle, &candidate.files.content_hash()).await;
    assert!(handle
        .preparations
        .apply_source(&candidate.watched_folder_path, &read, &key, &old_metadata)
        .await
        .is_err());
    assert_eq!(
        preparation(&handle, &candidate.files.content_hash()).await,
        reset
    );
    handle.claim_candidate_for_import(&key).await;
    assert!(matches!(
        handle.reset_candidate_setup(&key).await,
        Err(crate::import::ImportError::CandidateImportInProgress)
    ));
    assert_eq!(
        preparation(&handle, &candidate.files.content_hash()).await,
        reset
    );
    handle.runtime.release_import_claim(&key);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial(musicbrainz)]
async fn reset_setup_discards_selected_release_assets_and_identification() {
    let StoredCandidate {
        handle,
        manager,
        candidate,
        key,
        tmp: _tmp,
    } = stored_candidate().await;
    manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();
    let id = "70000107";
    crate::discogs::client::seed_release_cache(
        id,
        serde_json::json!({
            "id":70000107,"title":"Selected Album","year":1996,
            "artists":[{"id":70000107,"name":"Selected Artist"}],
            "formats":[{"name":"CD"}],
            "tracklist":[
                {"position":"1","title":"Selected First","duration":"0:01","type_":"track"},
                {"position":"2","title":"Selected Second","duration":"0:01","type_":"track"}
            ]
        })
        .to_string(),
    );
    crate::musicbrainz::seed_discogs_url_lookup(id, None);
    crate::discogs::client::seed_artist_image_response(id, None);
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(crate::import::Catalog::Discogs, id),
                partners: vec![],
            },
        )
        .await
        .unwrap();
    let hash = candidate.files.content_hash();
    let selected = preparation(&handle, &hash).await;
    assert!(selected.assets.applied_source.is_some());
    assert!(!selected.assets.artist_images.is_empty());
    assert!(manager
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .unwrap()
        .identify
        .is_some());
    let read = CandidateAsRead {
        content_hash: hash.clone(),
        file_edit_revision: selected.file_edit_revision,
        metadata_revision: selected.metadata_revision,
    };
    let cover = crate::import::CoverSelection::Remote(
        "https://images.example/selected.jpg".into(),
        crate::import::Catalog::Discogs,
    );
    handle
        .preparations
        .set_prepared_cover(
            &candidate.watched_folder_path,
            &key,
            &read,
            &cover,
            Some(&crate::import::cover_art::RemoteImage {
                bytes: cover_jpeg(),
                content_type: crate::util::content_type::ContentType::Jpeg,
            }),
        )
        .await
        .unwrap();
    assert!(preparation(&handle, &hash)
        .await
        .assets
        .remote_cover
        .is_some());
    handle.reset_candidate_setup(&key).await.unwrap();
    let reset = preparation(&handle, &hash).await;
    assert_eq!(reset.assets, CandidatePreparedAssets::default());
    assert!(reset.source_discogs_artist_ids.is_empty());
    assert!(reset.draft.album_artist_assignments.is_empty());
    assert_eq!(reset.cover, None);
    assert_eq!(reset.metadata_provenance, None);
    assert!(reset
        .draft
        .tracks
        .iter()
        .all(|row| row.source_index.is_none()));
    let state = manager
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.identify, None);
    assert_eq!(state.signals, None);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_without_tags_rejects_an_unchanged_source_rescanned_after_preparation() {
    let StoredCandidate {
        handle,
        manager,
        candidate,
        key,
        tmp: _tmp,
    } = stored_candidate().await;
    let old = manager
        .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
        .await
        .unwrap()
        .unwrap();
    let hash = candidate.files.content_hash();
    let before = preparation(&handle, &hash).await;
    let read = CandidateAsRead {
        content_hash: hash.clone(),
        file_edit_revision: before.file_edit_revision,
        metadata_revision: before.metadata_revision,
    };
    let metadata = CandidateMetadataDraft {
        draft: old.candidate.blank_source().draft,
        provenance: None,
        cover: None,
        source_discogs_artist_ids: Default::default(),
        assets: CandidatePreparedAssets::default(),
    };
    rescan_into(&manager, candidate.clone()).await;
    let rescanned = manager
        .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(old.scan_generation, rescanned.scan_generation);
    assert_eq!(old.candidate, rescanned.candidate);
    assert_eq!(preparation(&handle, &hash).await, before);
    assert!(handle
        .preparations
        .reset_setup(
            &old.candidate,
            &read,
            old.scan_generation,
            crate::import::LookupChoices::default(),
            metadata,
            None,
            vec![(key.clone(), candidate.files.clone())]
        )
        .await
        .is_err());
    assert_eq!(preparation(&handle, &hash).await, before);
    assert_eq!(
        manager
            .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
            .await
            .unwrap()
            .unwrap(),
        rescanned
    );
    shut_down(handle).await;
}

async fn mirrored_combination_folder(
    manager: &LibraryManager,
    source: &crate::import::release_candidate::ReleaseCandidate,
    root: &Path,
) -> FolderCandidate {
    let mut files = source.files().clone();
    for entry in &mut files.files {
        let target = root.join(&entry.file.relative_path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        let metadata = std::fs::metadata(&entry.file.path).unwrap();
        std::fs::copy(&entry.file.path, &target).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&target)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
            .unwrap();
        entry.file.path = target;
    }
    let folder = FolderCandidate {
        path: root.to_owned(),
        file_root: root.to_owned(),
        name: "Mirrored collection".into(),
        files,
        watched_folder_path: root.to_string_lossy().into_owned(),
        scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: source.file_edit_revision(),
        display_path: "Mirrored collection".into(),
        resolved_boundaries: vec![],
        combine_ancestor_key: None,
    };
    assert_eq!(folder.files.content_hash(), source.files().content_hash());
    manager
        .add_watched_import_folder(&folder.watched_folder_path)
        .await
        .unwrap();
    rescan_into(manager, folder.clone()).await;
    folder
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_updates_compatible_folder_and_combination_identities_together() {
    for reset_folder in [false, true] {
        let (manager, _library) = setup_test_manager().await;
        let first_root = TempDir::new().unwrap();
        let second_root = TempDir::new().unwrap();
        let (_, first_key, _) = picked_candidate(&manager, &first_root, "Volume A").await;
        let (_, second_key, _) = picked_candidate(&manager, &second_root, "Volume B").await;
        let handle = manager
            .start_import_service(tokio::runtime::Handle::current())
            .await
            .unwrap();
        let combined_key = handle
            .combine_candidates(vec![first_key, second_key])
            .await
            .unwrap();
        let original = handle
            .get_release_candidate(&combined_key)
            .await
            .unwrap()
            .unwrap();
        let mirror = TempDir::new().unwrap();
        let folder = mirrored_combination_folder(&manager, &original, mirror.path()).await;
        let folder_key = folder.path.to_string_lossy().into_owned();
        let key = if reset_folder {
            &folder_key
        } else {
            &combined_key
        };
        let before = preparation(&handle, &folder.files.content_hash()).await;
        handle
            .drop_candidate_track(key, before.draft.tracks[1].edit.id.clone())
            .await
            .unwrap();
        let mut events = handle.subscribe_events();
        handle.reset_candidate_setup(key).await.unwrap();
        let after = preparation(&handle, &folder.files.content_hash()).await;
        assert_eq!(after.draft.tracks.len(), 4);
        for key in [&folder_key, &combined_key] {
            let candidate = handle.get_release_candidate(key).await.unwrap().unwrap();
            assert_eq!(candidate.file_edit_revision(), after.file_edit_revision);
            let available = crate::import::track_slots::audio_units(candidate.files());
            assert!(after
                .draft
                .tracks
                .iter()
                .all(|track| available.contains(&track.edit.file)));
            assert_eq!(pane(&handle, key).await.metadata_draft.tracks.len(), 4);
            if let crate::import::release_candidate::ReleaseCandidate::Combined(combined) =
                candidate
            {
                let crate::import::release_candidate::ReleaseCandidate::Combined(original) =
                    &original
                else {
                    unreachable!()
                };
                assert_eq!(combined.combination, original.combination);
            }
        }
        let mut folder_event = false;
        let mut combined_event = false;
        while let Ok(event) = events.try_recv() {
            match event {
                ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate })
                    if candidate.path == folder.path =>
                {
                    folder_event = true
                }
                ImportEvent::Scan(ScanEvent::CandidateMetadataChanged { candidate_key })
                    if candidate_key == combined_key =>
                {
                    combined_event = true
                }
                _ => {}
            }
        }
        assert!(folder_event && combined_event);
        shut_down(handle).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_refuses_audio_incompatible_with_a_frozen_combination() {
    let (manager, _library) = setup_test_manager().await;
    let first_root = TempDir::new().unwrap();
    let second_root = TempDir::new().unwrap();
    let (_, first_key, _) = picked_candidate(&manager, &first_root, "Volume A").await;
    let (_, second_key, _) = picked_candidate(&manager, &second_root, "Volume B").await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    handle
        .set_file_role(
            first_key.clone(),
            "01 Track.flac".into(),
            FileRoleChoice::NotATrack,
        )
        .await
        .unwrap();
    let combined_key = handle
        .combine_candidates(vec![first_key, second_key])
        .await
        .unwrap();
    let combined = handle
        .get_release_candidate(&combined_key)
        .await
        .unwrap()
        .unwrap();
    let mirror = TempDir::new().unwrap();
    let folder = mirrored_combination_folder(&manager, &combined, mirror.path()).await;
    let folder_key = folder.path.to_string_lossy().into_owned();
    let choices = crate::import::LookupChoices {
        disc_id_excluded: true,
        chosen_catalogs: vec!["S1001".into()],
        ..crate::import::LookupChoices::default()
    };
    handle
        .set_candidate_lookup_choices(&folder_key, choices.clone())
        .await
        .unwrap();
    let before = preparation(&handle, &folder.files.content_hash()).await;
    assert_eq!(before.draft.tracks.len(), 3);
    assert!(handle.reset_candidate_setup(&folder_key).await.is_err());
    assert_eq!(
        preparation(&handle, &folder.files.content_hash()).await,
        before
    );
    assert_eq!(
        handle
            .get_release_candidate(&combined_key)
            .await
            .unwrap()
            .unwrap(),
        combined
    );
    assert_eq!(
        handle
            .get_release_candidate(&folder_key)
            .await
            .unwrap()
            .unwrap()
            .files(),
        &folder.files
    );
    assert_eq!(pane(&handle, &folder_key).await.lookup_choices, choices);
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_clears_lookup_choices_while_reset_to_tags_keeps_them() {
    let StoredCandidate {
        handle,
        key,
        tmp: _tmp,
        ..
    } = stored_candidate().await;
    let choices = crate::import::LookupChoices {
        disc_id_excluded: true,
        excluded_barcodes: vec!["0123456789012".into()],
        chosen_catalogs: vec!["S1001".into()],
        discounted_catalogs: vec!["OTHER-1".into()],
    };
    handle
        .set_candidate_lookup_choices(&key, choices.clone())
        .await
        .unwrap();
    handle
        .select_candidate_metadata_provenance(key.clone(), MetadataProvenance::FileTags)
        .await
        .unwrap();
    assert_eq!(pane(&handle, &key).await.lookup_choices, choices);
    handle.reset_candidate_setup(&key).await.unwrap();
    assert_eq!(
        pane(&handle, &key).await.lookup_choices,
        crate::import::LookupChoices::default()
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_prepared_before_a_lookup_choice_cannot_erase_it() {
    let StoredCandidate {
        mut handle,
        manager,
        candidate,
        key,
        tmp: _tmp,
    } = stored_candidate().await;
    manager.set_prefill_with_tags(true).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let resume = Arc::new(std::sync::Barrier::new(2));
    handle.file_tags = Arc::new(CountingFileTagReader::blocking(entered_tx, resume.clone()));
    let resetting = tokio::spawn({
        let handle = handle.clone();
        let key = key.clone();
        async move { handle.reset_candidate_setup(&key).await }
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("reset reads tags");
    let choices = crate::import::LookupChoices {
        disc_id_excluded: true,
        chosen_catalogs: vec!["S1001".into()],
        ..crate::import::LookupChoices::default()
    };
    let editing = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        handle.set_candidate_lookup_choices(&key, choices.clone()),
    )
    .await;
    resume.wait();
    editing
        .expect("reset preparation releases the commit lock")
        .unwrap();
    let before = preparation(&handle, &candidate.files.content_hash()).await;
    assert!(
        resetting.await.unwrap().is_err(),
        "a later identification choice supersedes the prepared Reset"
    );
    assert_eq!(pane(&handle, &key).await.lookup_choices, choices);
    assert_eq!(
        preparation(&handle, &candidate.files.content_hash()).await,
        before
    );
    assert_eq!(
        manager
            .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        None
    );
    shut_down(handle).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_setup_without_tags_keeps_a_combination_snapshot_ineligible_until_reread() {
    let (manager, _library) = setup_test_manager().await;
    let first_root = TempDir::new().unwrap();
    let second_root = TempDir::new().unwrap();
    let (_, first_key, _) = picked_candidate(&manager, &first_root, "Volume A").await;
    let (_, second_key, _) = picked_candidate(&manager, &second_root, "Volume B").await;
    let mut handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let key = handle
        .combine_candidates(vec![first_key, second_key])
        .await
        .unwrap();
    manager.set_prefill_with_tags(true).unwrap();
    handle.file_tags = Arc::new(CountingFileTagReader::with_embedded_cover(cover_jpeg()));
    handle.reset_candidate_setup(&key).await.unwrap();
    let source = handle.get_release_candidate(&key).await.unwrap().unwrap();
    let with_tags = manager
        .load_candidate_file_tag_snapshot(source.watched_folder_path(), &key)
        .await
        .unwrap()
        .unwrap();
    assert!(with_tags
        .snapshot
        .as_ref()
        .unwrap()
        .embedded_cover
        .is_some());
    assert!(matches!(
        preparation(&handle, &source.files().content_hash())
            .await
            .cover,
        Some(crate::import::CoverSelection::Embedded(_))
    ));
    manager.set_prefill_with_tags(false).unwrap();
    handle.reset_candidate_setup(&key).await.unwrap();
    let reset = pane(&handle, &key).await;
    assert_eq!(reset.candidate.files(), source.files());
    assert_eq!(reset.metadata_provenance, None);
    let saved = preparation(&handle, &source.files().content_hash()).await;
    assert_eq!(saved.cover, None);
    assert!(matches!(
        reset.cover.unwrap().selection,
        crate::import::CoverSelection::Local(_)
    ));
    let stale = manager
        .load_candidate_file_tag_snapshot(source.watched_folder_path(), &key)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale.snapshot, with_tags.snapshot);
    assert_ne!(
        stale.snapshot.as_ref().unwrap().file_edit_revision,
        stale.candidate.file_edit_revision()
    );
    let reader = Arc::new(CountingFileTagReader::immediate());
    let (_, reread) = handle
        .file_tag_snapshot_with_reader(&key, reader.clone())
        .await
        .unwrap();
    assert_eq!(reader.read_count(), 4);
    assert_eq!(
        reread.file_edit_revision,
        reset.candidate.file_edit_revision()
    );
    assert_eq!(reread.embedded_cover, None);
    assert_eq!(
        preparation(&handle, &source.files().content_hash()).await,
        saved
    );
    shut_down(handle).await;
}
