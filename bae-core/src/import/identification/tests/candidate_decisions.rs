#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn ignoring_a_cue_replaces_its_song_rows_with_whole_audio() {
    for prefill in [false, true] {
        let fixture = Fixture::new("cue-audio-replacement").await;
        fixture
            .manager
            .set_prefill_with_file_metadata(prefill)
            .unwrap();
        let dir = fixture.seed_cue_album("Album");
        fixture.scan(1).await;
        let hash = fixture.content_hash(&dir);
        let before = fixture
            .manager
            .load_import_candidate_preparation(&hash)
            .await
            .unwrap()
            .unwrap()
            .draft;
        assert_eq!(before.tracks.len(), 5);
        let unchanged = before
            .tracks
            .iter()
            .filter(|track| matches!(track.edit.file, crate::import::AudioFile::Standalone { .. }))
            .cloned()
            .collect::<Vec<_>>();

        fixture
            .import
            .set_sheet_disc(
                dir.to_string_lossy().into_owned(),
                "Test Album.cue".to_string(),
                crate::import::folder_scanner::SheetDisc::Ignored,
            )
            .await
            .unwrap();

        let after = fixture
            .manager
            .load_import_candidate_preparation(&hash)
            .await
            .unwrap()
            .unwrap()
            .draft;
        assert_eq!(after.tracks.len(), 3, "one image plus two loose tracks");
        assert!(after
            .tracks
            .iter()
            .all(|track| matches!(track.edit.file, crate::import::AudioFile::Standalone { .. })));
        for track in unchanged {
            assert!(
                after.tracks.contains(&track),
                "unaffected tracks retain identity and metadata"
            );
        }
        assert_eq!(after.album_title, before.album_title);
        let image = after
            .tracks
            .iter()
            .find(|track| {
                matches!(&track.edit.file,
                    crate::import::AudioFile::Standalone { file_id } if file_id == "Test Album.flac"
                )
            })
            .unwrap();
        assert_eq!(image.edit.title.is_empty(), !prefill);
        fixture
            .import
            .set_sheet_disc(
                dir.to_string_lossy().into_owned(),
                "Test Album.cue".into(),
                crate::import::folder_scanner::SheetDisc::Disc { number: 1 },
            )
            .await
            .unwrap();
        let restored = fixture
            .manager
            .load_import_candidate_preparation(&hash)
            .await
            .unwrap()
            .unwrap()
            .draft;
        assert_eq!(restored.tracks.len(), 5);
        let slices = restored
            .tracks
            .iter()
            .filter(|track| matches!(track.edit.file, crate::import::AudioFile::SheetSlice { .. }))
            .collect::<Vec<_>>();
        assert_eq!(slices.len(), 3);
        assert_eq!(
            slices[0].edit.title,
            if prefill { "Track One (Silence)" } else { "" }
        );
        assert!(restored
            .tracks
            .iter()
            .all(|track| track.edit.id != image.edit.id));
    }
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_claimed_candidate_refuses_sheet_and_role_decisions() {
    let fixture = Fixture::new("claimed-file-decisions").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    fixture
        .archive("mb-claimed-1", "rg-claimed-1", &[500, 500])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-claimed-1", "rg-claimed-1", 1_000)
        .await;
    let key = dir.to_string_lossy().into_owned();
    fixture.import.claim_candidate_for_import(&key).await;

    for result in [
        fixture
            .import
            .set_sheet_disc(
                key.clone(),
                "Test Album.cue".to_string(),
                crate::import::folder_scanner::SheetDisc::Disc { number: 1 },
            )
            .await,
        fixture
            .import
            .set_sheet_binding(
                key.clone(),
                "Test Album.cue".to_string(),
                "Test Album.flac".to_string(),
                Some("Test Album.flac".to_string()),
            )
            .await,
        fixture
            .import
            .set_file_role(
                key,
                "02 Test Artist - Track Two (White Noise).flac".to_string(),
                crate::import::folder_scanner::FileRoleChoice::Audio,
            )
            .await,
    ] {
        assert!(matches!(
            result,
            Err(crate::import::ImportError::CandidateImportInProgress)
        ));
    }
}

/// Clearing a sheet's binding and then naming its audio again is the ordinary
/// way back from "Describes nothing". The second decision lands like the
/// first: the sheet carves its container again and the pane redraws from it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_cleared_sheet_can_be_bound_again() {
    let fixture = Fixture::new("rebind-after-clear").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    fixture
        .archive("mb-rebind-1", "rg-rebind-1", &[500, 500])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-rebind-1", "rg-rebind-1", 1_000)
        .await;
    let key = dir.to_string_lossy().into_owned();

    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
            "Test Album.flac".to_string(),
            None,
        )
        .await
        .unwrap();
    // The queue looks the reshaped folder up again and finds nothing: the
    // draft is now the folder's own, one blank track per loose file.
    fixture.sweep_once().await;
    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the sweep answered the reshaped folder"
    );
    let options = fixture
        .import
        .sheet_binding_options(key.clone(), "Test Album.cue".to_string())
        .await
        .unwrap();
    assert!(
        options[0]
            .options
            .iter()
            .any(|option| option.file_id == "Test Album.flac"
                && option.offer == crate::import::folder_scanner::SheetBindingOffer::Offered),
        "the picker offers the container after the clear: {options:?}"
    );

    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
            "Test Album.flac".to_string(),
            Some("Test Album.flac".to_string()),
        )
        .await
        .expect("a cleared sheet binds again");

    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    let bound = pane
        .candidate
        .files()
        .track_sheets()
        .find(|sheet| sheet.file.relative_path == "Test Album.cue")
        .expect("the sheet is still a sheet")
        .binding
        .audio_files()
        .map(|files| {
            files
                .iter()
                .map(|file| file.file_id.clone())
                .collect::<Vec<_>>()
        });
    assert_eq!(bound, Some(vec!["Test Album.flac".to_string()]));

    // The draft grew with the slots: three blank tracks for three loose files
    // became the sheet's three entries plus the two loose files, each with a
    // mapping of its own.
    let preparation = fixture
        .manager
        .load_import_candidate_preparation(&fixture.content_hash(&dir))
        .await
        .unwrap()
        .expect("the rebound candidate is prepared");
    assert_eq!(preparation.draft.tracks.len(), 5);
}
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn deleting_audio_removes_the_row_and_metadata_cannot_restore_it() {
    let fixture = Fixture::new("delete-audio-row").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    let hash = fixture.content_hash(&dir);
    let key = dir.to_string_lossy().into_owned();
    let before = fixture
        .manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .unwrap()
        .draft;
    let removed = before.tracks[1].edit.id.clone();
    fixture
        .import
        .drop_candidate_track(&key, removed.clone())
        .await
        .unwrap();
    let after = fixture
        .manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .unwrap()
        .draft;
    assert_eq!(after.tracks.len(), before.tracks.len() - 1);
    assert!(after.tracks.iter().all(|track| track.edit.id != removed));
    fixture
        .import
        .select_candidate_metadata_provenance(key, crate::import::MetadataProvenance::FileMetadata)
        .await
        .unwrap();
    let reapplied = fixture
        .manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .unwrap()
        .draft;
    assert_eq!(reapplied.tracks.len(), after.tracks.len());
    for (actual, expected) in reapplied.tracks.iter().zip(&after.tracks) {
        assert_eq!(actual.edit.id, expected.edit.id);
        assert_eq!(actual.edit.file, expected.edit.file);
    }
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn changing_a_cues_disc_preserves_titles_but_updates_its_group() {
    let fixture = Fixture::new("cue-disc-change").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    let hash = fixture.content_hash(&dir);
    let key = dir.to_string_lossy().into_owned();
    let before = fixture
        .manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .unwrap()
        .draft;
    fixture
        .import
        .set_sheet_disc(
            key,
            "Test Album.cue".into(),
            crate::import::folder_scanner::SheetDisc::Disc { number: 2 },
        )
        .await
        .unwrap();
    let after = fixture
        .manager
        .load_import_candidate_preparation(&hash)
        .await
        .unwrap()
        .unwrap()
        .draft;
    for original in before.tracks {
        let actual = after
            .tracks
            .iter()
            .find(|track| track.edit.id == original.edit.id)
            .unwrap();
        assert_eq!(actual.edit.title, original.edit.title);
        assert_eq!(actual.edit.file, original.edit.file);
        if matches!(
            actual.edit.file,
            crate::import::AudioFile::SheetSlice { .. }
        ) {
            assert_eq!(actual.edit.side, Some(2));
        }
    }
}
