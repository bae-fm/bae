#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_claimed_candidate_refuses_sheet_and_role_decisions() {
    let fixture = Fixture::new("claimed-file-decisions").await;
    let dir = fixture.root.join("Album");
    std::fs::create_dir_all(&dir).unwrap();
    for name in [
        "Test Album.cue",
        "Test Album.flac",
        "02 Test Artist - Track Two (White Noise).flac",
        "03 Test Artist - Track Three (Brown Noise).flac",
    ] {
        std::fs::copy(
            Path::new("tests/fixtures/cue_flac").join(name),
            dir.join(name),
        )
        .unwrap();
    }
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
    let dir = fixture.root.join("Album");
    std::fs::create_dir_all(&dir).unwrap();
    for name in [
        "Test Album.cue",
        "Test Album.flac",
        "02 Test Artist - Track Two (White Noise).flac",
        "03 Test Artist - Track Three (Brown Noise).flac",
    ] {
        std::fs::copy(
            Path::new("tests/fixtures/cue_flac").join(name),
            dir.join(name),
        )
        .unwrap();
    }
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
        .set_sheet_binding(key.clone(), "Test Album.cue".to_string(), None)
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
        options.iter().any(|option| option.file_id == "Test Album.flac"
            && option.offer == crate::import::folder_scanner::SheetBindingOffer::Offered),
        "the picker offers the container after the clear: {options:?}"
    );

    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
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
        .map(|files| files.iter().map(|file| file.file_id.clone()).collect::<Vec<_>>());
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
