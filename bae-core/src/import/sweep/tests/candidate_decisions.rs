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
