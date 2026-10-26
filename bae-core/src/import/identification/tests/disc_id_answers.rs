// ── Releases a Disc ID lookup returned ──────────────────────────────────────
//
// A run reads a release its Disc ID lookup returned from that answer, which
// states everything ranking reads but the annotation; a pick reads the
// release itself before it applies anything.

/// A Disc ID answered by two pressings of the album the folder holds, so the
/// run offers both and picks neither. The first states a disambiguation, and
/// its own page adds an annotation.
async fn two_pressings_fixture(name: &str) -> (Fixture, PathBuf) {
    let fixture = Fixture::new(name).await;
    let dir = fixture.disc_id_candidate("Album");
    route_two_pressings(&fixture, fixture.probed_total_ms(&dir));
    (fixture, dir)
}

/// The routes of [`two_pressings_fixture`], for any fixture.
fn route_two_pressings(fixture: &Fixture, probed: u64) {
    let pressing = |release_id: &str, country: &str| {
        let mut release: serde_json::Value =
            serde_json::from_str(&release_json(release_id, "rg-disc", &[probed, 0])).unwrap();
        release["media"][0]["discs"] = serde_json::json!([{ "id": FIXTURE_DISC_ID }]);
        release["country"] = serde_json::json!(country);
        release
    };
    let mut first = pressing("mb-disc-de", "DE");
    first["disambiguation"] = serde_json::json!("club edition");
    let second = pressing("mb-disc-gb", "GB");
    let mut document = first.clone();
    document["annotation"] = serde_json::json!("Matrix: A1");
    fixture.provider.route(
        "/discid/",
        200,
        serde_json::json!({ "releases": [first, second.clone()] }).to_string(),
    );
    fixture
        .provider
        .route("/release/mb-disc-de?", 200, document.to_string());
    fixture
        .provider
        .route("/release/mb-disc-gb?", 200, second.to_string());
    fixture.provider.route(
        "/release-group/rg-disc?",
        200,
        serde_json::json!({
            "id": "rg-disc", "title": "Album", "first-release-date": "1981", "relations": []
        })
        .to_string(),
    );
}

/// The run reads the releases its Disc ID returned from that answer: it asks
/// MusicBrainz for their group and never for either release, and ranks them
/// on the tracklists and the notes the answer states. Neither is picked, so
/// nothing reads either further.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_reads_the_releases_its_disc_id_returned_from_that_answer() {
    let (fixture, dir) = two_pressings_fixture("disc-id-read").await;
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.count_containing("/ws/2/release/"),
        0,
        "{:?}",
        fixture.provider.requests()
    );
    assert_eq!(fixture.provider.count_containing("/release-group/rg-disc?"), 1);
    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings, .. } = &verdict else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    let mut offered: Vec<&str> = findings
        .matches
        .iter()
        .map(|result| result.release_id.as_str())
        .collect();
    offered.sort();
    assert_eq!(offered, vec!["mb-disc-de", "mb-disc-gb"]);
    for result in &findings.matches {
        assert_eq!(result.source_tracks, Some(SourceTracks::Listed { count: 2 }));
        assert_eq!(result.document_failure, None);
        assert_eq!(result.album_first_year, Some(1981), "the group's page was read");
    }
    let first = findings
        .matches
        .iter()
        .find(|result| result.release_id == "mb-disc-de")
        .unwrap();
    assert_eq!(first.notes, vec!["club edition".to_string()]);
    assert_ne!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "two pressings are offered and neither is picked"
    );

    for release_id in ["mb-disc-de", "mb-disc-gb"] {
        let stored = fixture
            .stored_release(release_id)
            .await
            .expect("the run stores what it read");
        assert_eq!(
            stored.unfetched,
            vec![crate::import::source_release::UnfetchedDocument {
                document: crate::import::PayloadSource::MusicBrainz,
                key: release_id.to_string(),
                reason: crate::import::source_release::UnfetchedReason::StoppedShort,
            }],
            "its own page is what the read stopped short of"
        );
    }
}

/// Picking a release the run read from its Disc ID answer asks for the
/// release itself before anything applies it, and stores the release a
/// complete read from scratch stores: its annotation among its notes.
#[tokio::test(flavor = "multi_thread")]
async fn picking_a_release_read_from_its_disc_id_answer_reads_it_in_full() {
    let (fixture, dir) = two_pressings_fixture("disc-id-pick").await;
    fixture.scan(1).await;
    fixture.drain_automatic().await;
    assert_eq!(fixture.count_release_lookups("mb-disc-de"), 0);
    let release =
        crate::import::MetadataRef::new(bae_catalogs::catalog::Catalog::MusicBrainz, "mb-disc-de");

    fixture
        .import
        .select_candidate_release(
            dir.to_string_lossy().into_owned(),
            crate::import::PressingLink {
                record: release.clone(),
                partners: vec![],
            },
        )
        .await
        .expect("the pick applies the release");

    assert_eq!(
        fixture.count_release_lookups("mb-disc-de"),
        1,
        "{:?}",
        fixture.provider.requests()
    );
    let picked = fixture
        .stored_release("mb-disc-de")
        .await
        .expect("the pick stores its release");
    assert_eq!(
        picked.notes,
        vec!["club edition".to_string(), "Matrix: A1".to_string()]
    );

    let scratch = Fixture::new("disc-id-complete").await;
    route_two_pressings(&scratch, fixture.probed_total_ms(&dir));
    let complete = crate::import::service::prepare_release(
        &scratch.manager,
        &release,
        crate::import::payloads::ReadDepth::Complete,
        CallPriority::Interactive,
    )
    .await
    .unwrap();
    assert_eq!(picked, complete);
}

/// A release only a barcode found has no Disc ID answer to be read from: the
/// run reads it with a release lookup, its own page and all.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_only_a_barcode_found_is_read_with_a_release_lookup() {
    let fixture = Fixture::new("barcode-read").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    for release_id in ["mb-barcode-de", "mb-barcode-gb"] {
        fixture.provider.route(
            &format!("/release/{release_id}?"),
            200,
            release_json(release_id, "rg-barcode", &[probed, 0]),
        );
    }
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json_placed(&[
            ("mb-barcode-de", "rg-barcode", PAIRED_BARCODE, "DE"),
            ("mb-barcode-gb", "rg-barcode", PAIRED_BARCODE, "GB"),
        ]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    for release_id in ["mb-barcode-de", "mb-barcode-gb"] {
        assert_eq!(fixture.count_release_lookups(release_id), 1);
        let stored = fixture
            .stored_release(release_id)
            .await
            .expect("the run stores what it read");
        assert!(
            !stored
                .unfetched
                .iter()
                .any(|document| document.document == crate::import::PayloadSource::MusicBrainz),
            "{:?}",
            stored.unfetched
        );
    }
    assert_ne!(
        fixture
            .stored_for(&dir)
            .await
            .expect("the candidate stores a row")
            .metadata_author,
        crate::import::MetadataAuthor::Identification,
        "two pressings are offered and neither is picked"
    );
}

/// A lead a Disc ID found is read from that answer, so the run offers it
/// read and picks it unattended. The pick's complete read is the first to ask
/// for the release itself; where that fails, the verdict fails with it and
/// nothing is applied. The release stays stored as the run read it, its own
/// page among what that read stopped short of.
#[tokio::test(flavor = "multi_thread")]
async fn a_disc_id_lead_whose_release_cannot_be_read_fails_at_its_pick() {
    let fixture = Fixture::new("settle-disc-id-unreadable").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-short-1", "rg-short-1", &[probed, 0]),
    );
    fixture.provider.route("/release/mb-short-1?", 400, "{}");
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Failed {
        failures, findings, ..
    } = &verdict
    else {
        panic!("the pick's failed read fails the verdict, got {verdict:?}");
    };
    assert!(
        matches!(
            failures.as_slice(),
            [crate::identify::IdentifyFailure::ReleaseDetails(
                crate::signals::LookupFailure::Provider { status: Some(400) }
            )]
        ),
        "{failures:?}"
    );
    assert_eq!(
        findings.matches[0].document_failure, None,
        "the run read the release from the Disc ID answer"
    );
    assert_eq!(
        fixture.count_release_lookups("mb-short-1"),
        1,
        "only the pick asked for the release: {:?}",
        fixture.provider.requests()
    );
    assert_ne!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "nothing was applied"
    );
    let stored = fixture
        .stored_release("mb-short-1")
        .await
        .expect("the run's read is stored");
    assert!(
        stored.unfetched.contains(&crate::import::source_release::UnfetchedDocument {
            document: crate::import::PayloadSource::MusicBrainz,
            key: "mb-short-1".to_string(),
            reason: crate::import::source_release::UnfetchedReason::StoppedShort,
        }),
        "{:?}",
        stored.unfetched
    );
}

