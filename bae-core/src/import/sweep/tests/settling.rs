// ── 7. Settling a lead ──────────────────────────────────────────────────────
//
// The documents a verdict buys before it is stored, and which verdicts have a
// lead to settle at all.

/// One release lookup per settled lead, whichever signal found it.
///
/// The disc-ID response already carries a tracklist, but not the rest of what
/// opening the candidate needs — the release-level relations the commit maps,
/// the release group, the cover options. A lead is settled by fetching the
/// release itself, once, and both candidates in this pass cost exactly that.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn settling_a_lead_costs_one_release_lookup_whichever_signal_found_it() {
    let fixture = Fixture::new("settle-lead").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let disc_dir = fixture.disc_id_candidate("From Disc Id");
    let barcode_dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&disc_dir);

    for (id, group) in [("mb-disc-1", "rg-disc-1"), ("mb-barcode-1", "rg-barcode-1")] {
        fixture.provider.route(
            &format!("/release/{id}?"),
            200,
            release_json(id, group, &[probed, 0]),
        );
    }
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-disc-1", "rg-disc-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-barcode-1", "rg-barcode-1"),
    );
    fixture.scan(2).await;

    fixture.sweep_once().await;

    assert_eq!(
        fixture.count_release_lookups("mb-disc-1"),
        1,
        "the disc-ID lead is settled once: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.count_release_lookups("mb-barcode-1"),
        1,
        "and so is the search lead: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.classification_for(&disc_dir).await,
        QueueClassification::Ready
    );
    assert_eq!(
        fixture.classification_for(&barcode_dir).await,
        QueueClassification::Ready
    );
}

/// The write ordering, from the outside: a failed lead fetch stores the failure
/// without storing partial release documents, and automatic passes leave it
/// alone until an explicit re-run.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_failed_settle_is_stored_without_partial_documents() {
    let fixture = Fixture::new("settle-ordering").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    // The disc-ID lookup answers; the release lookup that settles the lead does
    // not. The failed terminal answer must be stored without pretending the
    // release itself was settled.
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-order-1", "rg-order-1", &[probed, 0]),
    );
    fixture.provider.route("/release/mb-order-1?", 400, "{}");
    fixture.scan(1).await;

    fixture.sweep_once().await;

    assert!(matches!(
        fixture.identified_for(&dir).await.map(|row| row.verdict),
        Some(TerminalVerdict::Failed { .. })
    ));
    assert!(
        fixture.archived("mb-order-1").await.is_none(),
        "and nothing half-written is left behind"
    );

    let requests_after_failure = fixture.provider.requests().len();
    // The provider comes back, but an automatic pass does not replace the
    // stored failure.
    fixture.provider.set_routes(vec![
        (
            "/discid/",
            200,
            discid_json("mb-order-1", "rg-order-1", &[probed, 0]),
        ),
        (
            "/release/mb-order-1?",
            200,
            release_json("mb-order-1", "rg-order-1", &[probed, 0]),
        ),
    ]);
    fixture.sweep_once().await;

    assert_eq!(
        fixture.provider.requests().len(),
        requests_after_failure,
        "the stored failure is not retried automatically"
    );
    assert!(
        fixture.archived("mb-order-1").await.is_none(),
        "an automatic pass leaves the failed settle untouched"
    );
}

/// Explicit Lookup settles a candidate too. A person's own run answers the
/// candidate for good, and "answered" means the next launch opens it with no
/// network — so the same step runs here, before the verdict is written.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn explicit_lookup_settles_its_lead_before_storing_the_verdict() {
    let fixture = Fixture::new("interactive-settles").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        search_json("mb-interactive-1", "rg-interactive-1"),
    );
    fixture.provider.route(
        "/release/mb-interactive-1?",
        200,
        release_json("mb-interactive-1", "rg-interactive-1", &[probed, 0]),
    );
    fixture.scan(1).await;

    // Exactly what the explicit Lookup action does.
    fixture.start_explicit_lookup(&dir);
    let row = tokio::time::timeout(
        Duration::from_secs(20),
        fixture.await_identified_row(&dir),
    )
        .await
        .expect("the explicit Lookup recorder stores the verdict");

    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { matches, .. } = &verdict else {
        panic!("expected a single-match Found, got {verdict:?}");
    };
    assert!(
        matches[0].source_tracks.is_some(),
        "the lead was settled before the verdict was written"
    );
    assert!(
        fixture.archived("mb-interactive-1").await.is_some(),
        "and its documents are archived under the release they describe"
    );
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::Ready,
        "so the row is admitted on evidence that was actually checked"
    );

    // A later sweep pass finds nothing left to buy.
    let after_lookup = fixture.provider.requests().len();
    fixture.sweep_once().await;
    assert_eq!(
        fixture.provider.requests().len(),
        after_lookup,
        "a settled row is finished: {:?}",
        fixture.provider.requests()
    );
}

/// A release document can carry a readable tracklist yet still be impossible
/// to project into candidate metadata. An explicit run stores that terminal
/// release-details failure instead of waiting for another state event that
/// will never arrive.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn explicit_lookup_stores_a_metadata_projection_failure() {
    let fixture = Fixture::new("interactive-projection-failure").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-projection-1", "rg-projection-1", &[probed, 0]),
    );
    let mut incomplete: serde_json::Value = serde_json::from_str(&release_json(
        "mb-projection-1",
        "rg-projection-1",
        &[probed, 0],
    ))
    .unwrap();
    incomplete
        .as_object_mut()
        .unwrap()
        .remove("release-group");
    fixture.provider.route(
        "/release/mb-projection-1?",
        200,
        incomplete.to_string(),
    );
    fixture.scan(1).await;

    fixture.start_explicit_lookup(&dir);
    let row = tokio::time::timeout(Duration::from_secs(2), fixture.await_identified_row(&dir))
        .await
        .expect("the explicit recorder stores the projection failure");

    assert!(matches!(
        identify_result(&row).verdict,
        TerminalVerdict::Failed {
            ref failures,
            ..
        } if matches!(failures.as_slice(), [crate::identify::IdentifyFailure::ReleaseDetails(_)])
    ));
}

/// Picking a candidate whose lead is settled reads its release document from
/// the archive, then resolves the offered cover before storing the prepared
/// candidate. The metadata provider is not consulted again.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_candidate_uses_archived_metadata_and_prepares_its_cover() {
    let fixture = Fixture::new("offline-open").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;

    // Nothing is routed: the archived document is the only place this release
    // exists, while the cover endpoint answers that no image is available.
    fixture
        .archive("mb-offline-1", "rg-offline-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-offline-1", "rg-offline-1", probed)
        .await;
    let before = fixture.provider.requests().len();

    fixture
        .import
        .select_candidate_metadata_provenance(
            dir.to_string_lossy().into_owned(),
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-offline-1".to_string(),
                partners: vec![],
            },
        )
        .await
        .expect("a settled candidate opens from what identification archived");

    let detail = fixture
        .pane(&dir)
        .await
        .expect("the picked candidate reads back");
    let release = detail.release.expect("the pick names a release");
    assert_eq!(release.release_id, "mb-offline-1");
    assert_eq!(release.tracks.len(), 2);
    let edit = detail.metadata_draft;
    assert_eq!(edit.album_title, "Album");
    assert_eq!(
        release
            .cover_art
            .iter()
            .map(|cover| cover.url.as_str())
            .collect::<Vec<_>>(),
        vec![format!(
            "{}/release-group/rg-offline-1/front",
            crate::import::cover_art::archive_base_for_test()
        )],
        "the pressing states no front image of its own, so the album's is the \
         only option — and it is read off the stored document, not asked for"
    );
    assert_eq!(
        &fixture.provider.requests()[before..],
        &["/release-group/rg-offline-1/front".to_string()],
        "selection resolves the offered cover without re-fetching metadata"
    );
}

/// A settled lead whose documents are missing is a broken invariant, not a cold
/// cache. Picking it fails loudly, and stores no pick — so nothing is left
/// naming a release the pane could not draw.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_lead_with_no_documents_fails_loud() {
    let fixture = Fixture::new("offline-miss").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture
        .store_settled_verdict(&dir, "mb-missing-1", "rg-missing-1", probed)
        .await;

    let error = fixture
        .import
        .select_candidate_metadata_provenance(
            dir.to_string_lossy().into_owned(),
            crate::import::MetadataProvenance::ExternalRelease {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-missing-1".to_string(),
                partners: vec![],
            },
        )
        .await
        .expect_err("a settled lead with nothing archived must not silently re-fetch");

    assert!(
        matches!(&error, crate::import::ImportError::Internal { detail }
            if detail.contains("mb-missing-1")),
        "unexpected error: {error}"
    );
}

/// A pick identification never made — another pressing on the list, a manual
/// search hit — fetches, and archives what it fetched, so opening it again is
/// local too.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_outside_the_verdict_archives_what_it_fetched() {
    let fixture = Fixture::new("manual-pick").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture.provider.route(
        "/release/mb-manual-1?",
        200,
        release_json("mb-manual-1", "rg-manual-1", &[probed, 0]),
    );

    assert!(
        fixture.archived("mb-manual-1").await.is_none(),
        "nothing has fetched this release yet"
    );

    let pick = || crate::import::MetadataProvenance::ExternalRelease {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-manual-1".to_string(),
        partners: vec![],
    };
    fixture
        .import
        .select_candidate_metadata_provenance(dir.to_string_lossy().into_owned(), pick())
        .await
        .expect("a manual pick fetches");

    assert!(
        fixture.archived("mb-manual-1").await.is_some(),
        "and archives the release it fetched"
    );

    // Re-picking it costs nothing.
    let before = fixture.provider.requests().len();
    fixture
        .import
        .select_candidate_metadata_provenance(dir.to_string_lossy().into_owned(), pick())
        .await
        .expect("re-picking reads what the first pick archived");
    assert_eq!(
        fixture.provider.requests().len(),
        before,
        "the second pick reached the wire: {:?}",
        fixture.provider.requests()
    );
}

/// The barcode both sources print on the sleeve, as each of them spaces it.
/// Only the digits are comparable, which is what pairs the two records.
const PAIRED_BARCODE: &str = "0123456789012";
const PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT: &str = "012 345 678901 2";

/// Two sources' records of one physical pressing are one row on the Find
/// online list, picked whole — so a verdict that groups into a single row is an
/// answer, and the sweep settles it. The pick names the MusicBrainz release the
/// draft is read from and the Discogs release beside it, and both sources'
/// documents are archived, so opening this candidate needs no network.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn matches_that_pair_into_one_pressing_settle_as_one_pick() {
    let fixture = Fixture::new("paired-settle").await;
    fixture.use_discogs();
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[("mb-paired-1", "rg-paired-1", PAIRED_BARCODE)]),
    );
    fixture.provider.route(
        "/release/mb-paired-1?",
        200,
        release_json("mb-paired-1", "rg-paired-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/database/search",
        200,
        discogs_search_json("70000101", PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT),
    );
    fixture.provider.route(
        "/releases/70000101",
        200,
        discogs_release_json("70000101"),
    );
    // Nothing links this synthetic Discogs release to a MusicBrainz one, which
    // is the answer the cross-reference lookup would come back with.
    crate::musicbrainz::seed_discogs_url_lookup("70000101", None);
    fixture.scan(1).await;

    fixture.sweep_once().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the paired candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { matches, .. } = &verdict else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        matches.len(),
        2,
        "both sources answered the barcode: {matches:?}"
    );
    assert!(
        matches[0].source_tracks.is_some(),
        "the pressing's lead was settled before the verdict was written"
    );
    assert_eq!(
        row.metadata_provenance,
        Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-paired-1".to_string(),
            partners: vec![crate::import::MetadataRef::new(
                "70000101",
                crate::import::MetadataSource::Discogs,
            )],
        }),
        "the stored pick claims the Discogs record of the same pressing"
    );
    assert!(
        fixture.archived("mb-paired-1").await.is_some(),
        "the primary's documents are archived"
    );
    assert!(
        fixture.archived_discogs("70000101").await.is_some(),
        "and so are the partner's, so every source the pick claims reads offline"
    );
}

/// Two pressings are a question, not an answer: which one is on disk is the
/// user's call, and buying every pressing's documents would settle nothing. The
/// verdict stores with no pick and no release lookups behind it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn two_distinct_pressings_do_not_settle() {
    let fixture = Fixture::new("two-pressings").await;
    fixture
        .extraction
        .register_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[
            ("mb-two-1", "rg-two-1", PAIRED_BARCODE),
            ("mb-two-2", "rg-two-1", "9876543210987"),
        ]),
    );
    fixture.scan(1).await;

    fixture.sweep_once().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { matches, .. } = &verdict else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(matches.len(), 2);
    assert!(
        matches.iter().all(|result| result.source_tracks.is_none()),
        "nothing was settled: {matches:?}"
    );
    assert_eq!(row.metadata_provenance, None, "and no pick was stored");
    assert_eq!(
        fixture.count_release_lookups("mb-two-1") + fixture.count_release_lookups("mb-two-2"),
        0,
        "no pressing's documents were bought: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.classification_for(&dir).await,
        QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 })
    );
}
