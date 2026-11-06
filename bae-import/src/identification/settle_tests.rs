//! Settling a run's answer: the documents a verdict reads before it is
//! stored, the lead it settles on, the cover its pick is applied with, and
//! what a failed, cancelled or panicking settle stores.

use super::*;
use crate::identification::tests::*;
use crate::FolderCandidate;
use bae_catalogs::test_releases::{
    discogs_release_json, discogs_search_json, mb_barcode_search_json, mb_release_json,
    mb_release_json_with_catalog, mb_search_json,
};
use bae_identify::FolderCheck;
use bae_util::test_channels::HANG_GUARD;
use std::path::PathBuf;
use std::sync::Arc;

fn synthetic_candidate(path: &str, size: u64) -> FolderCandidate {
    use crate::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
    FolderCandidate {
        path: PathBuf::from(path),
        file_root: PathBuf::from(path),
        name: path.trim_start_matches('/').to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                file: ScannedFile::new(
                    PathBuf::from(format!("{path}/01.flac")),
                    "01.flac".to_string(),
                    size,
                    1,
                )
                .with_test_flac_audio(),
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        watched_folder_path: std::path::PathBuf::from("/"),
        scope: crate::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: crate::RelativeFolder::test(path.trim_start_matches('/')),
        grouping: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_transport_failure_is_stored() {
    let fixture = Fixture::new("failure-stored").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.provider.set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;

    fixture.drain_automatic().await;
    let stored = fixture.identified_for(&dir).await.expect("the failed outcome is stored");
    assert!(matches!(stored.verdict, TerminalVerdict::Failed { .. }));
}

/// An identification that settles on a release the catalogs hold no artwork
/// for leaves the candidate the cover its folder gave it. A folder holding
/// `cover.jpg` does not import bare because the record it was matched to had
/// no image.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_run_with_no_artwork_keeps_the_folders_own_cover() {
    let fixture = Fixture::new("keeps-folder-cover").await;
    let dir = fixture.disc_id_candidate("Album");
    std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    let probed = fixture.probed_total_ms(&dir);
    let lengths = [probed / 2, probed - probed / 2];
    fixture.provider.route("/discid/", 200, discid_json("mb-bare-1", "rg-bare-1", &lengths));
    fixture.provider.route(
        "/release/mb-bare-1?",
        200,
        mb_release_json("mb-bare-1", "rg-bare-1", &lengths),
    );
    fixture.scan(1).await;

    let folders_own = crate::cover_art::PreparedCover::Local("cover.jpg".to_string());
    assert_eq!(
        crate::preparations::stored_preparation(
            &fixture.context.tables,
            &fixture.content_hash(&dir)
        )
        .await
        .unwrap()
        .expect("the scanned candidate is prepared")
        .metadata
        .cover,
        Some(folders_own.clone()),
        "the scan stores the cover the folder gives the candidate"
    );

    fixture.drain_automatic().await;

    assert_eq!(
        crate::preparations::stored_preparation(
            &fixture.context.tables,
            &fixture.content_hash(&dir)
        )
        .await
        .unwrap()
        .expect("the identified candidate is prepared")
        .metadata
        .cover,
        Some(folders_own.clone()),
        "the settled release brought no image, so the folder's cover stands"
    );
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the identified candidate reads back")
            .draft()
            .unwrap()
            .cover
            .clone()
            .expect("the pane shows the stored selection")
            .selection(),
        folders_own.selection()
    );
}

/// One release lookup per settled lead, whichever signal found it.
///
/// The Disc ID answer is what the run reads a release it found from, but it
/// lacks what applying the release needs — the annotation, and the recording,
/// work and artist relations the commit maps. A lead is settled by fetching
/// the release itself, once, and both candidates in this pass cost exactly
/// that: the search lead's read by the run, the Disc ID lead's by its pick.
#[tokio::test(flavor = "multi_thread")]
async fn settling_a_lead_costs_one_release_lookup_whichever_signal_found_it() {
    let fixture = Fixture::new("settle-lead").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: "0123456789012".to_string(),
    }));
    let disc_dir = fixture.disc_id_candidate("From Disc Id");
    let barcode_dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&disc_dir);

    for (id, group) in [("mb-disc-1", "rg-disc-1"), ("mb-barcode-1", "rg-barcode-1")] {
        fixture.provider.route(
            &format!("/release/{id}?"),
            200,
            mb_release_json(id, group, &[probed, 0]),
        );
    }
    fixture.provider.route("/discid/", 200, discid_json("mb-disc-1", "rg-disc-1", &[probed, 0]));
    fixture.provider.route("/release?", 200, mb_search_json("mb-barcode-1", "rg-barcode-1"));
    fixture.scan(2).await;

    fixture.drain_automatic().await;

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
    assert_eq!(fixture.judgement_for(&disc_dir).await, (true, None));
    assert_eq!(fixture.judgement_for(&barcode_dir).await, (true, None));
}

/// A lead whose document cannot be read is stored as its lookup returned it,
/// saying why, with no pick applied and no partial documents left behind.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_lead_is_stored_unsettled_without_partial_documents() {
    let fixture = Fixture::new("settle-ordering").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: "0123456789012".to_string(),
    }));
    let dir = fixture.barcode_candidate("Album");
    // The barcode search answers; the release document does not.
    fixture.provider.route("/release?", 200, mb_search_json("mb-order-1", "rg-order-1"));
    fixture.provider.route("/release/mb-order-1?", 400, "{}");
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings, .. } = &verdict else {
        panic!("an unreadable document does not fail the run, got {verdict:?}");
    };
    assert!(findings.matches[0].document_failure.is_some());
    assert_ne!(
        row.metadata_author,
        crate::MetadataAuthor::Identification,
        "no document, so no pick was applied"
    );
    assert!(
        fixture.stored_release("mb-order-1").await.is_none(),
        "and nothing half-written is left behind"
    );
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, None),
        "a lead not applied is not imported unattended either"
    );
}

/// Two sources' records of one physical pressing are one row on the Find
/// online list, picked whole — so a verdict that groups into a single row is an
/// answer, and identification settles it. The pick names the MusicBrainz release the
/// draft is read from and the Discogs release beside it, and both sources'
/// documents are archived, so opening this candidate needs no network.
#[tokio::test(flavor = "multi_thread")]
async fn matches_that_pair_into_one_pressing_settle_as_one_pick() {
    let fixture = Fixture::new("paired-settle").await;
    fixture.use_discogs().await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json(&[("mb-paired-1", "rg-paired-1", PAIRED_BARCODE)]),
    );
    fixture.provider.route(
        "/release/mb-paired-1?",
        200,
        mb_release_json("mb-paired-1", "rg-paired-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/database/search",
        200,
        discogs_search_json("70000101", PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT),
    );
    fixture.provider.route("/releases/70000101", 200, discogs_release_json("70000101"));
    // Nothing links this synthetic Discogs release to a MusicBrainz one, which
    // is the answer the cross-reference lookup would come back with.
    fixture.musicbrainz.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release("70000101".to_string()),
        None,
    )]);
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the paired candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings: bae_identify::Findings { matches, .. }, .. } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(matches.len(), 2, "both sources answered the barcode: {matches:?}");
    assert!(
        matches[0].source_tracks.is_some(),
        "the pressing's lead was settled before the verdict was written"
    );
    assert_eq!(
        row.release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-paired-1".to_string()
            ),
            partners: vec![bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::Discogs,
                "70000101"
            )],
        })),
        "the stored pick claims the Discogs record of the same pressing"
    );
    assert!(
        fixture.stored_release("mb-paired-1").await.is_some(),
        "the primary's documents are archived"
    );
    assert!(
        fixture.stored_discogs_release("70000101").await.is_some(),
        "and so are the partner's, so every source the pick claims reads offline"
    );
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None),
        "one pressing, counts agreeing — the two \
         records are one row, so nothing is left to ask"
    );
}

/// A disc ID is a question only MusicBrainz answers, so the Discogs record of
/// the pressing it names can only ever come back from the barcode — never from
/// the disc ID as well. Agreement reads whole rows, so the two records of the
/// one pressing survive the narrowing together, and the pick identification stores
/// claims both sources.
#[tokio::test(flavor = "multi_thread")]
async fn a_disc_id_lead_settles_with_the_discogs_record_of_its_pressing() {
    let fixture = Fixture::new("disc-id-partner").await;
    fixture.use_discogs().await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.disc_id_candidate("From Disc Id And Barcode");
    std::fs::write(dir.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    let probed = fixture.probed_total_ms(&dir);

    fixture.provider.route(
        "/discid/",
        200,
        discid_json_stating_barcode("mb-paired-2", "rg-paired-2", &[probed, 0], PAIRED_BARCODE),
    );
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json(&[("mb-paired-2", "rg-paired-2", PAIRED_BARCODE)]),
    );
    fixture.provider.route(
        "/release/mb-paired-2?",
        200,
        mb_release_json("mb-paired-2", "rg-paired-2", &[probed, 0]),
    );
    fixture.provider.route(
        "/database/search",
        200,
        discogs_search_json("70000102", PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT),
    );
    fixture.provider.route("/releases/70000102", 200, discogs_release_json("70000102"));
    fixture.musicbrainz.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release("70000102".to_string()),
        None,
    )]);
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found {
        findings: bae_identify::Findings { matches, narrowed_out, .. },
        ..
    } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        matches.len(),
        2,
        "the disc ID's release and the Discogs record of the same pressing: {matches:?}"
    );
    assert!(
        narrowed_out.is_empty(),
        "neither of them is what agreement left out: {narrowed_out:?}"
    );
    assert_eq!(
        row.release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-paired-2".to_string()
            ),
            partners: vec![bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::Discogs,
                "70000102"
            )],
        })),
        "the stored pick claims the Discogs record the barcode alone found"
    );
    assert!(
        fixture.stored_discogs_release("70000102").await.is_some(),
        "and the partner's documents are archived with the primary's"
    );
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None),
        "one pressing, counts agreeing — nothing is left to ask"
    );
}

/// Which record of a pressing fills the draft is decided by what the folder
/// says about each, not by the source's name. Both sources answer the barcode
/// here; only the Discogs record states a year, and the folder names its
/// pressing by it — so the Discogs record leads its row, the pick claims
/// MusicBrainz beside it, and the draft is read from the Discogs document.
#[tokio::test(flavor = "multi_thread")]
async fn the_record_the_folder_agrees_with_settles_as_the_lead() {
    let fixture = Fixture::new("evidence-lead").await;
    fixture.use_discogs().await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    // The folder writes the album's year and, later, its pressing's, which
    // the Discogs record states and the MusicBrainz search hit does not. A
    // lone year would name no pressing: no record states the album's first
    // year, and a lone year is as often the album's.
    let dir = fixture.barcode_candidate("From Barcode 1971 (1996 Reissue)");
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json(&[("mb-paired-3", "rg-paired-3", PAIRED_BARCODE)]),
    );
    fixture.provider.route(
        "/release/mb-paired-3?",
        200,
        mb_release_json("mb-paired-3", "rg-paired-3", &[180_000, 0]),
    );
    fixture.provider.route(
        "/database/search",
        200,
        discogs_search_json("70000103", PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT),
    );
    fixture.provider.route("/releases/70000103", 200, {
        let mut release: serde_json::Value =
            serde_json::from_str(&discogs_release_json("70000103")).unwrap();
        release["tracklist"].as_array_mut().unwrap().push(serde_json::json!({
            "position": "2", "title": "Track 2", "duration": "0:01", "type_": "track", "artists": []
        }));
        release.to_string()
    });
    fixture.musicbrainz.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release("70000103".to_string()),
        None,
    )]);
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the paired candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings: bae_identify::Findings { matches, .. }, .. } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        matches[0].source,
        bae_catalogs::catalog::LookupCatalog::Discogs,
        "the folder agrees with the Discogs record about more: {matches:?}"
    );
    assert!(
        matches[0].source_tracks.is_some(),
        "and the tracklist was settled from it, not from its partner"
    );
    assert_eq!(
        row.release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::Discogs,
                "70000103".to_string()
            ),
            partners: vec![bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-paired-3"
            )],
        })),
        "so the pick names it primary and the MusicBrainz record its partner"
    );
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the settled candidate reads back")
            .draft()
            .unwrap()
            .metadata_draft
            .album_year,
        "1996",
        "and the draft carries the year only the Discogs document states"
    );
    assert!(
        fixture.stored_discogs_release("70000103").await.is_some()
            && fixture.stored_release("mb-paired-3").await.is_some(),
        "both sources the pick claims are archived, whichever of them leads"
    );
}

/// A settle writes the result, never what the next run asks: a disc-ID lead
/// whose release carries a number the folder prints lands with the lookup
/// choices exactly as they were, so running again asks the same lookups and
/// finds the same list.
#[tokio::test(flavor = "multi_thread")]
async fn settling_a_lead_leaves_the_lookup_choices_alone() {
    let fixture = Fixture::new("settle-chooses-catalog").await;
    let dir = fixture.disc_id_candidate("NJ-8255");
    let probed = fixture.probed_total_ms(&dir);

    fixture.provider.route(
        "/discid/",
        200,
        discid_json_with_catalog("mb-catalog-1", "rg-catalog-1", &[probed, 0], "NJ 8255"),
    );
    fixture.provider.route(
        "/release/mb-catalog-1?",
        200,
        mb_release_json_with_catalog("mb-catalog-1", "rg-catalog-1", &[probed, 0], "NJ 8255"),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let pane = fixture.pane(&dir).await.expect("the settled candidate has a pane");
    assert_eq!(
        pane.draft().unwrap().release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-catalog-1".to_string()
            ),
            partners: vec![],
        }))
    );
    assert_eq!(
        pane.draft().unwrap().lookup_choices,
        bae_identify::LookupChoices::default(),
        "the number the record carries and the folder prints is not chosen"
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
    fixture.provider.route("/discid/", 200, discid_json("mb-short-1", "rg-short-1", &[probed, 0]));
    fixture.provider.route("/release/mb-short-1?", 400, "{}");
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Failed { failures, findings, .. } = &verdict else {
        panic!("the pick's failed read fails the verdict, got {verdict:?}");
    };
    assert!(
        matches!(
            failures.as_slice(),
            [bae_identify::IdentifyFailure::ReleaseDetails(
                bae_catalogs::LookupFailure::Provider { status: Some(400) }
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
    assert_ne!(row.metadata_author, crate::MetadataAuthor::Identification, "nothing was applied");
    let stored = fixture.stored_release("mb-short-1").await.expect("the run's read is stored");
    assert!(
        stored.unfetched.contains(&bae_catalogs::source_release::UnfetchedDocument {
            document: bae_catalogs::records::PayloadSource::MusicBrainz,
            key: "mb-short-1".to_string(),
            reason: bae_catalogs::source_release::UnfetchedReason::StoppedShort,
        }),
        "{:?}",
        stored.unfetched
    );
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
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    let mut release: serde_json::Value =
        serde_json::from_str(&mb_release_json("mb-cover-1", "rg-cover-1", &[probed, 0]))
            .expect("the release fixture parses");
    // The release's document states the archive serves its front, so that is
    // the cover the pick fetches.
    release["cover-art-archive"]["front"] = serde_json::json!(true);
    let mut discid: serde_json::Value =
        serde_json::from_str(&discid_json("mb-cover-1", "rg-cover-1", &[probed, 0]))
            .expect("the disc ID fixture parses");
    discid["releases"][0]["cover-art-archive"]["front"] = serde_json::json!(true);
    fixture.provider.route("/release/mb-cover-1/front", 500, "server error");
    fixture.provider.route("/discid/", 200, discid.to_string());
    fixture.provider.route("/release/mb-cover-1?", 200, release.to_string());
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let TerminalVerdict::Failed { failures, findings, .. } = &identify_result(&row).verdict else {
        panic!(
            "a cover the host fails to serve fails the run, got {:?}",
            identify_result(&row).verdict
        );
    };
    assert_eq!(
        failures,
        &vec![bae_identify::IdentifyFailure::Cover(bae_catalogs::search::SourceFailure {
            source: bae_catalogs::catalog::Catalog::MusicBrainz,
            failure: bae_catalogs::LookupFailure::Provider { status: Some(500) },
        })],
    );
    assert_eq!(
        findings.matches[0].release_id, "mb-cover-1",
        "what the run found stays beside the failure"
    );
    assert_eq!(row.release_link, None, "no link is stored without its draft");
    assert_ne!(
        row.metadata_author,
        crate::MetadataAuthor::Identification,
        "and no draft is applied"
    );
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, None),
        "a failed run is never imported unattended"
    );

    let (listed, live) = queue_item(&fixture, &key).await;
    assert_eq!(live.standing, Some(crate::PendingStanding::LookupError));
    assert!(
        listed
            .action_basis
            .actions(&crate::TriageRuntimeFacts::default())
            .contains(&crate::CandidateAction::RetryIdentification),
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
    fixture.provider.reroute("/release/mb-cover-1/front", 200, cover_png());
    fixture.start_explicit_lookup(&dir);
    tokio::time::timeout(HANG_GUARD, fixture.identification().drained_for_test())
        .await
        .expect("the retried run ends");

    let row = fixture.stored_for(&dir).await.expect("the candidate keeps its row");
    assert!(
        matches!(&identify_result(&row).verdict, TerminalVerdict::Found { .. }),
        "the retried run finds the release, got {:?}",
        identify_result(&row).verdict
    );
    assert_eq!(
        row.release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-cover-1",
            ),
            partners: vec![],
        })),
    );
    assert_eq!(row.metadata_author, crate::MetadataAuthor::Identification);
    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    let cover = pane.draft().unwrap().cover.clone().expect("the applied release brings its cover");
    let crate::CoverSelection::Remote(image, source) = &cover.selection() else {
        panic!("the cover is the catalog's, got {:?}", cover.selection());
    };
    assert_eq!(*source, bae_catalogs::catalog::Catalog::MusicBrainz);
    assert!(image.url.ends_with("/release/mb-cover-1/front"), "{}", image.url);
    let stored = crate::preparations::stored_preparation(
        &fixture.context.tables,
        &fixture.content_hash(&dir),
    )
    .await
    .unwrap()
    .expect("the identified candidate is prepared");
    let Some(crate::cover_art::PreparedCover::Remote { fetched, .. }) = stored.metadata.cover
    else {
        panic!("the draft's cover is the catalog's, got {:?}", stored.metadata.cover);
    };
    assert_eq!(fetched.bytes, cover_png(), "the cover's bytes are stored with the draft");
}

/// A folder its rip log proves is a CD rip, and a disc ID answered by a
/// release whose record states vinyl: the one pressing is offered, but the
/// folder's own files rule it out, so nothing applies it or imports it.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_the_folder_rules_out_is_neither_applied_nor_imported() {
    let fixture = Fixture::importing("auto-import-medium").await;
    fixture.set_import_when_identified(true).await;
    let dir = fixture.disc_id_candidate("Album");
    let key = crate::CandidateKey::folder(&dir);
    let probed = fixture.probed_total_ms(&dir);
    let mut answer: serde_json::Value =
        serde_json::from_str(&discid_json("mb-vinyl", "rg-vinyl", &[probed, 0]))
            .expect("the disc ID fixture parses");
    answer["releases"][0]["media"][0]["format"] = serde_json::json!("12\" Vinyl");
    fixture.provider.route("/discid/", 200, answer.to_string());
    fixture.scan(1).await;
    let mut events = fixture.import.every_event();

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, Some(FolderCheck::MediumDisagrees { folder: bae_identify::MediumConflict::CdRip }))
    );
    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    assert_ne!(
        row.metadata_author,
        crate::MetadataAuthor::Identification,
        "the release the folder rules out was not applied to the draft"
    );
    assert_eq!(
        fixture.count_release_lookups("mb-vinyl"),
        0,
        "the offered row was read from its Disc ID answer, and nothing picked it"
    );
    assert_no_import(&fixture, &mut events, &key, "the folder ruled the release out").await;
}

/// Explicit Lookup settles a candidate too. A person's own run answers the
/// candidate for good, and "answered" means the next launch opens it with no
/// network — so the same step runs here, before the verdict is written.
#[tokio::test(flavor = "multi_thread")]
async fn explicit_lookup_settles_its_lead_before_storing_the_verdict() {
    let fixture = Fixture::new("interactive-settles").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: "0123456789012".to_string(),
    }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release?",
        200,
        mb_search_json("mb-interactive-1", "rg-interactive-1"),
    );
    fixture.provider.route(
        "/release/mb-interactive-1?",
        200,
        mb_release_json("mb-interactive-1", "rg-interactive-1", &[probed, 0]),
    );
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    // Exactly what a person asking to identify the candidate does.
    fixture.start_explicit_lookup(&dir);
    let row = tokio::time::timeout(HANG_GUARD, fixture.await_identified_row(&dir))
        .await
        .expect("the explicit Lookup recorder stores the verdict");

    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings: bae_identify::Findings { matches, .. }, .. } = &verdict
    else {
        panic!("expected a single-match Found, got {verdict:?}");
    };
    assert!(
        matches[0].source_tracks.is_some(),
        "the lead was settled before the verdict was written"
    );
    assert!(
        fixture.stored_release("mb-interactive-1").await.is_some(),
        "and its documents are archived under the release they describe"
    );
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None),
        "so the row is admitted on evidence that was actually checked"
    );
}

/// Shutdown writes nothing, and `save` checks the token right before writing
/// so a cancel during the settle's release fetch leaves no row.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_candidate_writes_no_row() {
    let fixture = Fixture::new("cancelled-writes-nothing").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-cancel-1", "rg-cancel-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-cancel-1?",
        200,
        mb_release_json("mb-cancel-1", "rg-cancel-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    // Hold the disc-ID response so the shutdown lands mid-identification.
    fixture.provider.hold("/discid/");

    let pass = fixture.drain_automatic_task();
    fixture.provider.wait_for_request("/discid/", 1).await;
    fixture.identification().shut_down();
    fixture.provider.release();
    tokio::time::timeout(HANG_GUARD, pass).await.expect("a cancelled queue returns").unwrap();

    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "a cancelled candidate writes no identification result: {:?}",
        fixture.stored().await.keys().collect::<Vec<_>>()
    );

    // `save` itself writes nothing under a cancelled token.
    let verdict = TerminalVerdict::NotFoundAnywhere { ledger: None };
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let candidate = fixture
        .import
        .answerable_candidate(&crate::CandidateKey::folder(&dir))
        .await
        .unwrap()
        .expect("the scanned candidate is answerable");
    assert!(matches!(
        save(
            &fixture.context(),
            &cancelled,
            IdentifyRunId::for_test(1),
            &candidate,
            &verdict,
            bae_identify::signals::Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: bae_identify::signals::DiscIdSignal::Absent,
                barcode: bae_identify::signals::BarcodeSignal::Absent,
                text: bae_identify::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            None,
            Vec::new(),
        )
        .await,
        Settled::Abandoned
    ));
    assert!(
        fixture.stored().await.values().all(|row| row.identify.is_none()),
        "cancellation preserves the discovered draft without writing an identification result"
    );
}

/// A verdict lands with everything identification learned: what each of the
/// folder's audio units plays for, and the signals it settled on. The pane
/// reads those back instead of opening the folder or extracting again.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_verdict_carries_its_durations_and_signals() {
    let fixture = Fixture::new("verdict-carries-signals").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-signals-1", "rg-signals-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.provider.route(
        "/release/mb-signals-1?",
        200,
        mb_release_json("mb-signals-1", "rg-signals-1", &[probed / 2, probed - probed / 2]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("a verdict is stored");
    assert!(row.identify.is_some(), "the verdict is stored");
    let signals = row.signals.expect("the settled signals are stored");
    assert!(
        matches!(signals.disc_id, bae_identify::signals::DiscIdSignal::Computed { .. }),
        "the disc ID the lookup used reads back: {:?}",
        signals.disc_id
    );
}

/// A terminal state without its settled signals cannot be committed, and the
/// settle reports that failure instead of pretending the candidate remains
/// queued for another run.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_with_no_signals_reports_a_finalization_failure() {
    let fixture = Fixture::new("verdict-without-signals").await;
    let candidate: crate::folder_scanner::FolderCandidate =
        synthetic_candidate("/missing-signals", 321);

    let finished = settle_answer(
        fixture.context(),
        crate::CandidateFiles::of(&candidate),
        candidate,
        IdentifyRunId::for_test(1),
        TerminalVerdict::NotFoundAnywhere { ledger: None }.resume_state(
            None,
            &bae_identify::LookupChoices::default(),
            bae_identify::signals::AudioFacts::default(),
        ),
        CallClass::BACKGROUND,
        CancellationToken::new(),
    )
    .await;

    assert!(matches!(finished.settled, Settled::Unwritable { .. }));
    assert!(fixture.stored().await.is_empty());
}

/// A settle that panics fails its job's identification as a write that did
/// not land does, with the panic's message, so the job ends rather than
/// staying "being written".
#[tokio::test(flavor = "multi_thread")]
async fn a_settle_that_panics_fails_as_an_answer_not_written() {
    let fixture = Fixture::new("settle-panics").await;
    let candidate: crate::folder_scanner::FolderCandidate =
        synthetic_candidate("/settle-panics", 321);

    let finished = settled_or_failed(
        &fixture.context(),
        crate::CandidateFiles::of(&candidate),
        candidate.key(),
        IdentifyRunId::for_test(1),
        async { panic!("the settle broke") },
    )
    .await;

    assert_eq!(finished.representative_key, candidate.key());
    assert!(
        matches!(
            &finished.settled,
            Settled::WriteFailed { error }
                if error == "settle an identification answer panicked: the settle broke"
        ),
        "a panicked settle fails as an unwritten answer"
    );
}
