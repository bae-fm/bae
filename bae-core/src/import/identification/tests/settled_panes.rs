// ── Where the pane stands once a result lands ──────────────────────────────

/// A run that applied its own pick and whose release passed every check
/// against the folder leaves the draft and its Import as all there is to see, so the stored pane moves to
/// the draft in the same write — from Find online, where the person was
/// watching the run.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_pick_that_asks_nothing_opens_the_pane_on_the_draft() {
    let fixture = Fixture::new("settled-draft").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture
        .archive("mb-settled-1", "rg-settled-1", &[probed, 0])
        .await;
    fixture
        .import
        .move_candidate_pane(&key, crate::import::PaneMove::FindOnline)
        .await
        .unwrap();

    fixture
        .store_settled_verdict(&dir, "mb-settled-1", "rg-settled-1",)
        .await;

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None)
    );
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the candidate reads back")
            .session
            .presentation,
        crate::import::MetadataPresentation::Draft
    );
}

/// A picked release that failed a check against the folder — here, it lists
/// three tracks against the folder's two — leaves the pane where the person
/// left it.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_pick_that_asks_something_leaves_the_pane_where_it_was() {
    let fixture = Fixture::new("settled-asks").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.scan(1).await;
    fixture
        .archive("mb-settled-2", "rg-settled-2", &[0, 0])
        .await;
    fixture
        .import
        .move_candidate_pane(&key, crate::import::PaneMove::FindOnline)
        .await
        .unwrap();

    fixture
        .store_settled_verdict_listing(
            &dir,
            "mb-settled-2",
            "rg-settled-2",
            crate::import::search::SourceTracks::Listed { count: 3 },
        )
        .await;

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, Some(crate::identify::FolderCheck::TrackCountDisagrees {
            local: 2,
            source: 3
        }))
    );
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the candidate reads back")
            .session
            .presentation,
        crate::import::MetadataPresentation::FindOnline
    );
}

// ── Automatic ──────────────────────────────────────────────────────────────

/// Asking for identification's results for a candidate with a stored verdict
/// shows that verdict as it stood, the picked release still the draft's, and
/// asks nothing of the providers: no run is owed.
#[tokio::test(flavor = "multi_thread")]
async fn automatic_shows_a_stored_verdict_without_asking_again() {
    let fixture = Fixture::new("automatic-stored").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture
        .archive("mb-automatic-1", "rg-automatic-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-automatic-1", "rg-automatic-1")
        .await;
    let asked = fixture.provider.requests().len();

    assert!(
        !fixture.import.open_automatic(&key).await.unwrap(),
        "a stored verdict owes no run"
    );

    assert_eq!(fixture.provider.requests().len(), asked);
    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    assert_eq!(pane.session.presentation, crate::import::MetadataPresentation::FindOnline);
    assert_eq!(
        pane.session.find_online_section,
        crate::import::FindOnlineSection::Automatic
    );
    assert!(
        !matches!(pane.resumed_identify_state, crate::identify::IdentifyState::Idle),
        "the stored verdict is what shows"
    );
    assert!(
        matches!(
            &pane.metadata_provenance,
            Some(crate::import::MetadataProvenance::ExternalRelease { record, .. })
                if record.key == "mb-automatic-1"
        ),
        "the saved pick stays the draft's, for the pane to mark"
    );
}

/// With no stored verdict, asking for identification's results owes a run.
#[tokio::test(flavor = "multi_thread")]
async fn automatic_with_no_stored_verdict_owes_a_run() {
    let fixture = Fixture::new("automatic-first").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    fixture.scan(1).await;

    assert!(fixture.import.open_automatic(&key).await.unwrap());
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the candidate reads back")
            .session
            .presentation,
        crate::import::MetadataPresentation::FindOnline
    );
}
