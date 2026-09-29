// ── Applying the release a run picked ──────────────────────────────────────
//
// A verdict that picks a release unattended applies it in the same write: the
// draft read from it, with the artist images and cover it needs. A catalog
// that cannot give one of those is a lookup that failed, stored as the run's.

/// A small PNG, which is what an image host serves.
fn cover_png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(16, 16)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("the test cover encodes");
    bytes.into_inner()
}

/// The cover host failing while the run's pick is applied stores the run as
/// a lookup that failed on the cover, naming the catalog that offered it:
/// nothing of the pick is applied, the row stands as a lookup error to
/// retry, and the pane reads the failure back from the store. Retrying once
/// the host answers applies the release with its cover.
#[tokio::test(flavor = "multi_thread")]
async fn a_cover_the_host_fails_to_serve_is_stored_as_the_runs_lookup_failure() {
    let fixture = Fixture::new("cover-fails").await;
    let dir = fixture.disc_id_candidate("Album");
    let key = dir.to_string_lossy().into_owned();
    let probed = fixture.probed_total_ms(&dir);
    let mut release: serde_json::Value =
        serde_json::from_str(&release_json("mb-cover-1", "rg-cover-1", &[probed, 0]))
            .expect("the release fixture parses");
    // The release's document states the archive serves its front, so that is
    // the cover the pick fetches.
    release["cover-art-archive"]["front"] = serde_json::json!(true);
    let mut discid: serde_json::Value = serde_json::from_str(&discid_json(
        "mb-cover-1",
        "rg-cover-1",
        &[probed, 0],
    ))
    .expect("the disc ID fixture parses");
    discid["releases"][0]["cover-art-archive"]["front"] = serde_json::json!(true);
    fixture
        .provider
        .route("/release/mb-cover-1/front", 500, "server error");
    fixture.provider.route("/discid/", 200, discid.to_string());
    fixture
        .provider
        .route("/release/mb-cover-1?", 200, release.to_string());
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let TerminalVerdict::Failed {
        failures, findings, ..
    } = &identify_result(&row).verdict
    else {
        panic!(
            "a cover the host fails to serve fails the run, got {:?}",
            identify_result(&row).verdict
        );
    };
    assert_eq!(
        failures,
        &vec![crate::identify::IdentifyFailure::Cover(
            crate::import::search::SourceFailure {
                source: crate::import::Catalog::MusicBrainz,
                failure: crate::signals::LookupFailure::Provider { status: Some(500) },
            }
        )],
    );
    assert_eq!(
        findings.matches[0].release_id, "mb-cover-1",
        "what the run found stays beside the failure"
    );
    assert_eq!(row.release_link, None, "no link is stored without its draft");
    assert_ne!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "and no draft is applied"
    );
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, None),
        "a failed run is never imported unattended"
    );

    let listed = queue_row(&fixture, &key).await;
    assert_eq!(
        listed.action_basis.standing,
        Some(crate::import::PendingStanding::LookupError)
    );
    assert!(
        listed
            .action_basis
            .actions(&crate::import::TriageRuntimeFacts::default())
            .contains(&crate::import::CandidateAction::RetryIdentification),
        "a lookup error offers Retry"
    );
    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    assert!(
        matches!(
            &pane.resumed_identify_state,
            IdentifyState::Failed { failures: shown, .. } if shown == failures
        ),
        "the pane states the stored failure, got {:?}",
        pane.resumed_identify_state
    );

    // The host recovers; Retry runs the candidate again.
    fixture
        .provider
        .reroute("/release/mb-cover-1/front", 200, cover_png());
    fixture.start_explicit_lookup(&dir);
    tokio::time::timeout(
        Duration::from_secs(30),
        fixture.identification().drained_for_test(),
    )
    .await
    .expect("the retried run ends");

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate keeps its row");
    assert!(
        matches!(&identify_result(&row).verdict, TerminalVerdict::Found { .. }),
        "the retried run finds the release, got {:?}",
        identify_result(&row).verdict
    );
    assert_eq!(
        row.release_link,
        Some(crate::import::ReleaseLink::Pressing(crate::import::PressingLink {
            record: crate::import::MetadataRef::new(
                crate::import::Catalog::MusicBrainz,
                "mb-cover-1",
            ),
            partners: vec![],
        })),
    );
    assert_eq!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification
    );
    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    let cover = pane.cover.expect("the applied release brings its cover");
    let crate::import::CoverSelection::Remote(image, source) = &cover.selection else {
        panic!("the cover is the catalog's, got {:?}", cover.selection);
    };
    assert_eq!(*source, crate::import::Catalog::MusicBrainz);
    assert!(
        image.url.ends_with("/release/mb-cover-1/front"),
        "{}",
        image.url
    );
    let assets = fixture
        .manager
        .load_import_candidate_prepared_assets(&fixture.content_hash(&dir))
        .await
        .unwrap();
    assert_eq!(
        assets.remote_cover.map(|image| image.bytes),
        Some(cover_png()),
        "the cover's bytes are stored with the draft"
    );
}
