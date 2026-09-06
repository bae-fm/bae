use super::*;
use crate::import::combination::CombinationTrackOrder;

#[tokio::test(flavor = "multi_thread")]
async fn selected_folders_from_different_roots_import_as_one_release() {
    let (manager, _library) = setup_test_manager().await;
    let first_root = TempDir::new().unwrap();
    let second_root = TempDir::new().unwrap();
    let (first, first_key, _) = picked_candidate(&manager, &first_root).await;
    let (second, second_key, _) = picked_candidate(&manager, &second_root).await;
    let original_paths = first
        .files
        .release_files()
        .chain(second.files.release_files())
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    let keys = vec![second_key.clone(), first_key.clone()];
    let review = handle
        .review_candidate_combination(keys.clone())
        .await
        .unwrap();
    let key = handle
        .combine_reviewed_candidates(
            &review,
            keys.clone(),
            CombinationTrackOrder::SeparateDiscs,
            "Collected Volumes".into(),
        )
        .await
        .unwrap();
    assert_eq!(handle.candidate_source_folders(&key).await.unwrap(), keys);
    assert!(handle
        .get_release_candidate(&first_key)
        .await
        .unwrap()
        .is_none());
    assert!(handle
        .get_release_candidate(&second_key)
        .await
        .unwrap()
        .is_none());
    handle
        .set_candidate_metadata_provenance(key.clone(), crate::import::MetadataProvenance::FileTags)
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            "Collected Volumes".into(),
        )
        .await
        .unwrap();
    handle
        .set_candidate_album_artists(
            &key,
            vec![crate::import::ArtistAssignment::new("Combined Artist")],
        )
        .await
        .unwrap();
    let projected = pane(&handle, &key).await;
    assert_eq!(projected.metadata_draft.tracks.len(), 4);
    assert_eq!(
        projected
            .metadata_draft
            .tracks
            .iter()
            .map(|track| track.side)
            .collect::<Vec<_>>(),
        [1, 1, 2, 2]
    );
    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .unwrap();
    let release_id = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            match events.recv().await.unwrap() {
                ImportEvent::ImportProgress {
                    progress:
                        ImportProgress::Complete {
                            import_id: completed,
                            id,
                            ..
                        },
                    ..
                } if completed == import_id => break id,
                ImportEvent::ImportProgress {
                    progress:
                        ImportProgress::Failed {
                            import_id: failed,
                            error,
                        },
                    ..
                } if failed == import_id => panic!("combined import failed: {error}"),
                _ => {}
            }
        }
    })
    .await
    .expect("combined import reports a terminal result");
    let imported = handle
        .library_manager
        .release_edit_seed(&release_id)
        .await
        .unwrap();
    assert_eq!(imported.edit.album_title, "Collected Volumes");
    assert_eq!(imported.edit.tracks.len(), 4);
    assert_eq!(
        imported
            .edit
            .tracks
            .iter()
            .map(|track| (track.side, track.track_number))
            .collect::<Vec<_>>(),
        [(1, Some(1)), (1, Some(2)), (2, Some(1)), (2, Some(2))]
    );
    assert!(pane(&handle, &key).await.is_added);
    assert!(handle.separate_combined_candidate(&key).await.is_err());
    assert!(original_paths.iter().all(|path| path.is_file()));
    shut_down(handle).await;
}
