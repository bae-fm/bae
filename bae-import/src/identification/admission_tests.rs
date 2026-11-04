//! What puts a candidate on the queue on its own and what its run starts
//! from: a stored verdict or a skip keeps it off, and the run reads the
//! candidate's lookup choices and its draft's title.

use super::*;
use crate::identification::tests::*;
use crate::ImportCandidateSnapshot;
use bae_catalogs::test_releases::{mb_release_json, mb_search_json};
use bae_util::test_channels::HANG_GUARD;
use std::sync::Arc;

/// A folder found again under a new name — renamed, or moved within the
/// watched folder — holds files a run already answered. It is a release found
/// for the first time, but its answer is stored, so it is never handed to the
/// pipeline again: the provider sees requests only for the first name.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_verdict_is_not_re_fetched() {
    let fixture = Fixture::new("not-re-fetched").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cached-1", "rg-cached-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-cached-1?",
        200,
        mb_release_json("mb-cached-1", "rg-cached-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.drain_automatic().await;
    let after_first = fixture.provider.requests().len();
    assert!(after_first > 0, "the first name has to actually ask the provider");
    assert!(fixture.identified_for(&dir).await.is_some());
    let answered = fixture.content_hash(&dir);

    let renamed = fixture.root.join("Album Renamed");
    std::fs::rename(&dir, &renamed).unwrap();
    assert_eq!(
        fixture.content_hash(&renamed),
        answered,
        "the renamed folder holds the files that were answered"
    );
    fixture.rescan(&fixture.import, 1).await;
    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.requests().len(),
        after_first,
        "the folder under its new name asked the provider for nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(
        fixture
            .import
            .answerable_candidate(&crate::CandidateKey::folder(&renamed))
            .await
            .unwrap()
            .is_some(),
        "the scan found the folder under its new name"
    );
}

/// Skipped is a decision the user already made, so automatic identification
/// excludes it until the user explicitly unskips it.
#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_candidate_is_not_identified() {
    let fixture = Fixture::new("skipped").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-skipped-1", "rg-skipped-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-skipped-1?",
        200,
        mb_release_json("mb-skipped-1", "rg-skipped-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.import.set_candidate_skipped(crate::CandidateKey::folder(&dir), true).await.unwrap();

    fixture.drain_automatic().await;

    assert!(
        fixture.provider.requests().is_empty(),
        "a skipped candidate costs the provider nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&dir).await.is_none(), "and produces no identification result");
}

/// A release the catalogs hold under no code of its own — no barcode, no disc
/// ID — is still found: once the identifiers come back empty the run asks the
/// providers for the candidate's own album title, and what comes back is the
/// verdict.
///
/// A lone row found this way is applied like any single match: its documents
/// are fetched and identification writes its draft from them.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_no_identifier_names_is_found_by_its_title() {
    let fixture = Fixture::new("found-by-title").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: "0123456789012".to_string(),
    }));
    // The folder name is what the pre-filled draft calls the release, and so
    // what the title search asks about.
    let dir = fixture.barcode_candidate("Album Title");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route("query=barcode%3A", 200, r#"{"releases":[]}"#);
    fixture.provider.route("query=release%3A", 200, mb_search_json("mb-by-title", "rg-by-title"));
    fixture.provider.route(
        "/release/mb-by-title?",
        200,
        mb_release_json("mb-by-title", "rg-by-title", &[probed / 2, probed - probed / 2]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let verdict = identify_result(&fixture.stored_for(&dir).await.expect("a verdict is stored"))
        .verdict
        .clone();
    let TerminalVerdict::Found { findings: bae_identify::Findings { matches, .. }, .. } = &verdict
    else {
        panic!("expected the title search's release, got {verdict:?}");
    };
    assert_eq!(matches[0].release_id, "mb-by-title");
    assert_eq!(
        fixture.provider.count_containing("query=barcode%3A"),
        1,
        "the barcode was asked first, and once"
    );
    assert_eq!(
        fixture.provider.count_containing("query=release%3A"),
        1,
        "the title was asked once, after it"
    );
    assert_eq!(
        fixture.provider.count_containing("/release/mb-by-title?"),
        1,
        "the lone row's documents are fetched to settle it"
    );
    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    assert_eq!(
        pane.draft().unwrap().metadata_author,
        crate::MetadataAuthor::Identification,
        "identification wrote the draft from the lone row"
    );
    assert_eq!(
        pane.draft()
            .unwrap()
            .release_link
            .as_ref()
            .and_then(crate::ReleaseLink::pressing)
            .expect("the settled row is the candidate's release")
            .record
            .key,
        "mb-by-title"
    );
}

/// A run reads what the person decided it asks about. With the barcode left
/// out, no provider is asked about the codes the artwork carries — the disc ID
/// is asked about and answers alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_leaves_out_the_signals_the_candidate_says_to_leave_out() {
    let fixture = Fixture::new("choices-leave-out-barcode").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: "0123456789012".to_string(),
    }));
    // A rip log for the disc ID and an image for the barcode, so both signals
    // are there and only the choice decides which is asked about.
    let dir = fixture.disc_id_candidate("Album");
    std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-choice-1", "rg-choice-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.provider.route(
        "/release/mb-choice-1?",
        200,
        mb_release_json("mb-choice-1", "rg-choice-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.scan(1).await;
    fixture.use_discogs().await;
    fixture
        .import
        .edit_candidate_lookup_choices(
            &crate::CandidateKey::folder(&dir),
            bae_identify::LookupChoiceEdit::ToggleBarcode { code: "0123456789012".to_string() },
        )
        .await
        .unwrap();

    fixture.drain_automatic().await;

    fixture
        .await_identified_row(&dir)
        .await
        .identify
        .expect("the run still answers from the disc ID");
    let requests = fixture.provider.requests();
    assert!(
        requests.iter().any(|target| target.contains("/discid/")),
        "the disc ID is still asked about: {requests:?}"
    );
    assert!(
        !requests.iter().any(|target| target.contains("barcode")),
        "nothing asks about a barcode the candidate says to leave out: {requests:?}"
    );
    assert!(
        !requests.iter().any(|target| target.contains("/database/search")),
        "and Discogs is not asked about it either: {requests:?}"
    );
}

/// A draft read off the folder's own files is not an answer, whether the
/// pre-fill wrote it or a person asked for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_tags_draft_is_still_run() {
    for (name, reset_by_hand) in [("prefilled", false), ("reset-to-tags", true)] {
        let fixture = Fixture::new(name).await;
        let dir = fixture.disc_id_candidate("Candidate");
        let probed = fixture.probed_total_ms(&dir);
        fixture.provider.route(
            "/discid/",
            200,
            discid_json("mb-file-tags", "rg-file-tags", &[probed, 0]),
        );
        fixture.provider.route(
            "/release/mb-file-tags?",
            200,
            mb_release_json("mb-file-tags", "rg-file-tags", &[probed, 0]),
        );
        fixture.scan(1).await;
        if reset_by_hand {
            fixture
                .import
                .select_candidate_file_tags(crate::CandidateKey::folder(&dir))
                .await
                .unwrap();
        }
        let stored = fixture.stored_for(&dir).await.expect("the candidate is stored");
        assert_eq!(
            stored.metadata_provenance,
            Some(crate::MetadataProvenance::FileMetadata),
            "{name} starts from its file tags"
        );
        assert_eq!(
            stored.metadata_author,
            if reset_by_hand {
                crate::MetadataAuthor::Person
            } else {
                crate::MetadataAuthor::Prefill
            },
            "{name}: the draft says who read the tags into it"
        );

        if reset_by_hand {
            // The reset is a person's decision, which ends the found release's
            // identification; asking again runs it over the reset draft.
            fixture.drain_automatic().await;
            assert!(fixture.identified_for(&dir).await.is_none());
            fixture.start_explicit_lookup(&dir);
            fixture.await_identified_row(&dir).await;
        } else {
            fixture.drain_automatic().await;
        }

        assert!(
            fixture.identified_for(&dir).await.is_some(),
            "{name} was identified and stored its result"
        );
    }
}

/// A release a person chose is stored as the candidate's result, so the found
/// release is not identified on its own afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_stores_the_result_and_automatic_identification_leaves_it_alone() {
    let fixture = Fixture::new("pick-is-a-result").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    // Nothing is routed: a run would have to look the disc ID up.
    fixture.archive("mb-chosen", "rg-chosen", &[probed, 0]).await;
    assert!(fixture.identified_for(&dir).await.is_none());

    fixture
        .import
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    "mb-chosen".to_string(),
                ),
                partners: Vec::new(),
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("the pick lands");

    let picked = fixture.stored_for(&dir).await.expect("the pick is stored");
    let result = picked.identify.expect("the choice is the candidate's result");
    assert!(
        matches!(
            &result.verdict,
            bae_identify::TerminalVerdict::Found { findings: bae_identify::Findings { matches, .. }, .. }
                if matches.len() == 1 && matches[0].release_id == "mb-chosen"
        ),
        "the result names the release they chose: {:?}",
        result.verdict
    );

    let asked = fixture.provider.requests();

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.requests(),
        asked,
        "a candidate a person had answered was asked about"
    );
    assert_eq!(
        fixture.stored_for(&dir).await.expect("the candidate is still stored").release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-chosen".to_string()
            ),
            partners: Vec::new(),
        }))
    );
}

/// A re-scan announcing a candidate an import owns must not queue it again,
/// or the batch total would never come back down. Driven from the bus rather
/// than the filesystem so the event order does not depend on the watcher.
#[tokio::test(flavor = "multi_thread")]
async fn a_rescan_does_not_count_back_a_candidate_an_import_owns() {
    let fixture = Fixture::new("import-rescan").await;
    let remaining = fixture.disc_id_candidate("Remaining");
    let importing = fixture.disc_id_candidate("Importing");
    std::fs::write(importing.join("notes.txt"), "distinct candidate").unwrap();
    let probed = fixture.probed_total_ms(&remaining);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-import-rescan", "rg-import-rescan", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-import-rescan?",
        200,
        mb_release_json("mb-import-rescan", "rg-import-rescan", &[probed, 0]),
    );
    fixture.scan(2).await;
    fixture.provider.hold("/discid/");

    let mut counts = fixture.import.every_identification_count_for_test();
    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;
    start_import_for(&fixture, &importing).await;
    // The scan re-announces the claimed candidate, as a watcher re-scan does.
    let claimed = match fixture.import.get_candidate(&crate::CandidateKey::folder(&importing)).await
    {
        Ok(Some(ImportCandidateSnapshot::Folder { candidate, .. })) => candidate,
        other => panic!(
            "the claimed candidate is still a folder candidate: {:?}",
            other.map(|snapshot| snapshot.map(|_| "a candidate"))
        ),
    };
    fixture.import.emit_event_for_test(ImportEvent::Scan(ScanEvent::FolderCandidate {
        candidate: claimed,
        found_while_automatic: false,
    }));
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass).await.expect("pass finishes after the re-scan").unwrap();

    let progress: Vec<_> = std::iter::from_fn(|| counts.try_recv().ok()).collect();
    assert!(
        progress.iter().all(|(_, total)| *total <= 2),
        "the re-scan does not put the importing candidate back in the batch: {progress:?}"
    );
    assert_eq!(progress.last(), Some(&(0, 0)), "{progress:?}");
}
