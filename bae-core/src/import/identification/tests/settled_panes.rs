// ── Where the pane stands once a result lands ──────────────────────────────

/// A run that applied its own pick and whose result asks nothing leaves the
/// draft and its Import as all there is to see, so the stored pane moves to
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
        .set_candidate_presentation(&key, crate::import::MetadataPresentation::FindOnline)
        .await
        .unwrap();

    fixture
        .store_settled_verdict(&dir, "mb-settled-1", "rg-settled-1", probed)
        .await;

    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready
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

/// A result that asks something — here, the release lists three tracks
/// against the folder's two — leaves the pane where the person left it.
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
        .set_candidate_presentation(&key, crate::import::MetadataPresentation::FindOnline)
        .await
        .unwrap();

    fixture
        .store_settled_verdict_listing(
            &dir,
            "mb-settled-2",
            "rg-settled-2",
            1_000,
            crate::import::search::SourceTracks::Listed { count: 3 },
        )
        .await;

    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::NeedsYou(crate::identify::NeedsYou::TrackCountDisagrees {
            local: 2,
            source: 3
        })
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
