//! The work a run's effects ask: the catalog searches it sends, and the
//! releases it reads to rank, each tracklist fitted to the folder.

use super::*;
use crate::identification::tests::*;
use bae_catalogs::search::SourceTracks;
use bae_catalogs::test_releases::{
    mb_barcode_search_json, mb_barcode_search_json_placed, mb_release_json,
    mb_release_json_with_catalog,
};
use bae_identify::TerminalVerdict;
use bae_util::test_channels::HANG_GUARD;
use std::path::Path;
use std::sync::Arc;

// What a run asks the catalogs by catalog number, read off the requests the
// fake provider received.

/// The query pairs of every request whose target contains `path` and names
/// `key`, decoded.
fn requests_naming(
    fixture: &Fixture,
    path: &str,
    key: &str,
) -> Vec<std::collections::HashMap<String, String>> {
    fixture
        .provider
        .requests()
        .iter()
        .filter(|target| target.contains(path))
        .map(|target| {
            reqwest::Url::parse(&format!("http://fake{target}"))
                .expect("a request target parses")
                .query_pairs()
                .into_owned()
                .collect::<std::collections::HashMap<_, _>>()
        })
        .filter(|query| query.contains_key(key))
        .collect()
}

/// The folder states the number the disc ID's release carries, so the
/// number comes into effect: Discogs is searched for it under the artist
/// that release credits, as its words beside `catno`, and MusicBrainz, which
/// answers in one page, for the number alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_rounds_catalog_number_search_names_the_artist_on_discogs_alone() {
    let fixture = Fixture::new("catalog-search-artist").await;
    fixture.use_discogs().await;
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
    fixture.provider.route("/database/search", 200, r#"{"results":[]}"#);
    fixture.scan(1).await;

    fixture.drain_automatic().await;
    fixture.await_identified_row(&dir).await;

    let musicbrainz: Vec<String> = requests_naming(&fixture, "/ws/2/release?", "query")
        .into_iter()
        .map(|query| query["query"].clone())
        .filter(|query| query.contains("catno:"))
        .collect();
    assert_eq!(
        musicbrainz,
        vec![r#"catno:"NJ 8255""#.to_string()],
        "MusicBrainz is asked for the number alone"
    );
    let discogs = requests_naming(&fixture, "/database/search", "catno");
    assert_eq!(discogs.len(), 1, "Discogs is asked once: {discogs:?}");
    assert_eq!(discogs[0]["catno"], "NJ 8255");
    assert_eq!(discogs[0].get("q").map(String::as_str), Some("Artist"));
    assert_eq!(discogs[0].get("label"), None, "a number in effect is searched under no label");
}

/// The disc ID's release prints a number under its label and no barcode, and
/// the folder does not state the number: the release's pressing is looked up
/// on Discogs by that number, under its label and the artist it credits.
#[tokio::test(flavor = "multi_thread")]
async fn a_pressing_lookup_by_catalog_number_names_the_label_and_the_artist() {
    let fixture = Fixture::new("pressing-search-label").await;
    fixture.use_discogs().await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    let label_info = serde_json::json!([
        { "catalog-number": "LN-100", "label": { "name": "Label Name" } }
    ]);
    let mut disc: serde_json::Value =
        serde_json::from_str(&discid_json("mb-pressing-1", "rg-pressing-1", &[probed, 0]))
            .expect("the disc ID fixture parses");
    disc["releases"][0]["label-info"] = label_info.clone();
    fixture.provider.route("/discid/", 200, disc.to_string());
    let mut release: serde_json::Value =
        serde_json::from_str(&mb_release_json("mb-pressing-1", "rg-pressing-1", &[probed, 0]))
            .expect("the release fixture parses");
    release["label-info"] = label_info;
    fixture.provider.route("/release/mb-pressing-1?", 200, release.to_string());
    fixture.provider.route("/database/search", 200, r#"{"results":[]}"#);
    fixture.scan(1).await;

    fixture.drain_automatic().await;
    fixture.await_identified_row(&dir).await;

    let discogs = requests_naming(&fixture, "/database/search", "catno");
    assert_eq!(discogs.len(), 1, "Discogs is asked once: {discogs:?}");
    assert_eq!(discogs[0]["catno"], "LN-100");
    assert_eq!(discogs[0].get("label").map(String::as_str), Some("Label Name"));
    assert_eq!(discogs[0].get("q").map(String::as_str), Some("Artist"));
}

/// The folder's track count breaks a tie the lookups leave: of two pressings
/// a barcode names alike, the one whose tracklist holds as many tracks as the
/// folder is offered, and the run settles on it. The other is no match, so it
/// is not set aside under the offered one either.
#[tokio::test(flavor = "multi_thread")]
async fn the_pressing_whose_tracklist_fits_the_folder_is_offered() {
    let fixture = Fixture::new("fitting-pressing").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release/mb-long?",
        200,
        mb_release_json("mb-long", "rg-fit", &[probed, 0, 1_000]),
    );
    fixture.provider.route(
        "/release/mb-fits?",
        200,
        mb_release_json("mb-fits", "rg-fit", &[probed, 0]),
    );
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json_placed(&[
            ("mb-long", "rg-fit", PAIRED_BARCODE, "GB"),
            ("mb-fits", "rg-fit", PAIRED_BARCODE, "DE"),
        ]),
    );
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
        matches.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["mb-fits"]
    );
    assert!(narrowed_out.is_empty(), "{narrowed_out:?}");
    assert_eq!(
        row.metadata_author,
        crate::MetadataAuthor::Identification,
        "the one fitting pressing settles the draft"
    );
    assert_eq!(fixture.count_release_lookups("mb-fits"), 1);
}

/// A folder that combines two ten-track albums, whose name writes the first
/// album's title and a catalog number. The barcode returns that album alone,
/// which the folder's name states more of, and the release that holds both
/// albums, whose title the name does not write; both carry the number. The
/// ten-track album ranks first and is read, lists other tracks than the
/// folder, and drops out; the two-album release is read next, fits, and is
/// offered alone and applied.
#[tokio::test(flavor = "multi_thread")]
async fn a_combined_folder_settles_on_the_release_that_holds_all_its_tracks() {
    let fixture = Fixture::new("combined-folder").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("Artist - Album One [CAT-100]");
    for at in 3..=20 {
        copy_fixture(
            &Path::new("tests/fixtures/flac").join(FLAC_FIXTURES[at % 2]),
            &dir.join(format!("{at:02} Test Track {at}.flac")),
        );
    }
    let lengths: Vec<u64> = (1..=20)
        .map(|at| {
            let name = match at {
                1 | 2 => FLAC_FIXTURES[at - 1].to_string(),
                _ => format!("{at:02} Test Track {at}.flac"),
            };
            bae_audio::probe_audio_from_path(&dir.join(name), bae_util::test_files::place())
                .expect("fixture FLAC probes")
                .duration
                .as_millis() as u64
        })
        .collect();
    let release = |release_id: &str, group_id: &str, title: &str, media: &[&[u64]]| {
        let media: Vec<serde_json::Value> = media
            .iter()
            .map(|lengths| {
                let tracks: Vec<serde_json::Value> = lengths
                    .iter()
                    .enumerate()
                    .map(|(at, length)| {
                        serde_json::json!({
                            "position": at + 1,
                            "number": (at + 1).to_string(),
                            "title": format!("Track {}", at + 1),
                            "length": length,
                        })
                    })
                    .collect();
                serde_json::json!({ "tracks": tracks })
            })
            .collect();
        serde_json::json!({
            "id": release_id,
            "title": title,
            "artist-credit": [{ "name": "Artist" }],
            "release-group": { "id": group_id },
            "barcode": PAIRED_BARCODE,
            "label-info": [{ "catalog-number": "CAT-100" }],
            "media": media,
            "relations": [],
            "cover-art-archive": { "front": false, "darkened": false },
        })
    };
    fixture.provider.route(
        "/release/mb-album-one?",
        200,
        release("mb-album-one", "rg-album-one", "Album One", &[&lengths[..10]]).to_string(),
    );
    fixture.provider.route(
        "/release/mb-both-albums?",
        200,
        release(
            "mb-both-albums",
            "rg-both-albums",
            "Album One & Album Two",
            &[&lengths[..10], &lengths[10..]],
        )
        .to_string(),
    );
    let found = |release: serde_json::Value| {
        let mut found = release;
        found
            .as_object_mut()
            .expect("a release is an object")
            .retain(|field, _| !matches!(field.as_str(), "media" | "relations"));
        found
    };
    fixture.provider.route(
        "/release?",
        200,
        serde_json::json!({
            "releases": [
                found(release("mb-album-one", "rg-album-one", "Album One", &[])),
                found(release(
                    "mb-both-albums",
                    "rg-both-albums",
                    "Album One & Album Two",
                    &[],
                )),
            ]
        })
        .to_string(),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings, .. } = &verdict else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        findings.matches.iter().map(|result| result.release_id.as_str()).collect::<Vec<_>>(),
        vec!["mb-both-albums"]
    );
    assert!(
        findings.narrowed_out.is_empty(),
        "the ten-track album is no match: {:?}",
        findings.narrowed_out
    );
    assert_eq!(fixture.count_release_lookups("mb-album-one"), 1, "it was read");
    assert_eq!(fixture.judgement_for(&dir).await, (true, None));
    assert_eq!(
        row.metadata_author,
        crate::MetadataAuthor::Identification,
        "the release that holds every track settles the draft"
    );
}

/// A sole release listing more tracks than the folder holds is no match: the
/// run reads it, finds nothing left, and the folder is not found.
#[tokio::test(flavor = "multi_thread")]
async fn a_sole_release_that_does_not_fit_the_folder_is_not_found() {
    let fixture = Fixture::new("unfit-sole").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release/mb-long?",
        200,
        mb_release_json("mb-long", "rg-long", &[probed, 0, 1_000]),
    );
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json(&[("mb-long", "rg-long", PAIRED_BARCODE)]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    assert!(
        matches!(verdict, TerminalVerdict::NotFoundAnywhere { .. }),
        "a release that does not fit is no match, got {verdict:?}"
    );
    assert_eq!(fixture.count_release_lookups("mb-long"), 1, "it was read");
    assert_ne!(row.metadata_author, crate::MetadataAuthor::Identification, "nothing is applied");
}

/// Two pressings are a question, not an answer: which one is on disk is the
/// user's call. Both offered pressings' documents are read before the run
/// settles, so each row states its full tracklist and a pick later asks for
/// nothing; with both fitting the folder they stay tied, and the verdict
/// stores with no pick.
#[tokio::test(flavor = "multi_thread")]
async fn two_distinct_pressings_are_read_in_full_and_do_not_settle() {
    let fixture = Fixture::new("two-pressings").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    for id in ["mb-two-1", "mb-two-2"] {
        fixture.provider.route(
            &format!("/release/{id}?"),
            200,
            mb_release_json(id, "rg-two-1", &[probed, 0]),
        );
    }
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json_placed(&[
            ("mb-two-1", "rg-two-1", PAIRED_BARCODE, "GB"),
            ("mb-two-2", "rg-two-1", "9876543210987", "DE"),
        ]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings: bae_identify::Findings { matches, .. }, .. } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(matches.len(), 2);
    assert!(
        matches
            .iter()
            .all(|result| result.source_tracks == Some(SourceTracks::Listed { count: 2 })),
        "every offered pressing states its full tracklist: {matches:?}"
    );
    assert_ne!(
        row.metadata_author,
        crate::MetadataAuthor::Identification,
        "a run that settled on no release wrote no draft, so the candidate's own stands"
    );
    assert_eq!(
        (fixture.count_release_lookups("mb-two-1"), fixture.count_release_lookups("mb-two-2")),
        (1, 1),
        "each offered pressing's document was fetched once: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(fixture.judgement_for(&dir).await, (false, None));

    // Picking an offered row reads the document the run stored.
    let before_pick = fixture.provider.requests().len();
    fixture
        .import
        .select_candidate_release(
            crate::CandidateKey::folder(&dir),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    "mb-two-2".to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("an offered row is picked from what the run stored");
    assert_eq!(
        fixture.provider.requests()[before_pick..]
            .iter()
            .filter(|request| request.contains("/release/"))
            .count(),
        0,
        "picking asked for no document again: {:?}",
        fixture.provider.requests()
    );
}

/// A document that cannot be read does not fail the run: its row keeps what
/// the lookup returned and stores why it could not be read, and the run
/// settles on the documents it did read.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_whose_document_cannot_be_read_keeps_its_search_facts() {
    let fixture = Fixture::new("unread-pressing").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release/mb-read?",
        200,
        mb_release_json("mb-read", "rg-read", &[probed, 0]),
    );
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json_placed(&[
            ("mb-read", "rg-read", PAIRED_BARCODE, "GB"),
            ("mb-unread", "rg-read", "9876543210987", "DE"),
        ]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings, .. } = &verdict else {
        panic!("the run settles on what it read, got {verdict:?}");
    };
    let stored = |id: &str| {
        findings
            .matches
            .iter()
            .chain(&findings.narrowed_out.matches)
            .find(|result| result.release_id == id)
            .unwrap_or_else(|| panic!("{id} is stored"))
            .clone()
    };
    let read = stored("mb-read");
    assert_eq!(read.source_tracks, Some(SourceTracks::Listed { count: 2 }));
    assert_eq!(read.document_failure, None);
    let unread = stored("mb-unread");
    assert_eq!(unread.source_tracks, None, "it keeps what the lookup returned");
    assert!(unread.document_failure.is_some(), "it says its document could not be read");
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
    let row = fixture.stored_for(&dir).await.expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings, .. } = &verdict else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    let mut offered: Vec<&str> =
        findings.matches.iter().map(|result| result.release_id.as_str()).collect();
    offered.sort();
    assert_eq!(offered, vec!["mb-disc-de", "mb-disc-gb"]);
    for result in &findings.matches {
        assert_eq!(result.source_tracks, Some(SourceTracks::Listed { count: 2 }));
        assert_eq!(result.document_failure, None);
        assert_eq!(result.album_first_year, Some(1981), "the group's page was read");
    }
    assert_ne!(
        row.metadata_author,
        crate::MetadataAuthor::Identification,
        "two pressings are offered and neither is picked"
    );

    for release_id in ["mb-disc-de", "mb-disc-gb"] {
        let stored = fixture.stored_release(release_id).await.expect("the run stores what it read");
        assert_eq!(
            stored.unfetched,
            vec![bae_catalogs::source_release::UnfetchedDocument {
                document: bae_catalogs::records::PayloadSource::MusicBrainz,
                key: release_id.to_string(),
                reason: bae_catalogs::source_release::UnfetchedReason::StoppedShort,
            }],
            "its own page is what the read stopped short of"
        );
    }
}

/// A release only a barcode found has no Disc ID answer to be read from: the
/// run reads it with a release lookup, its own page and all.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_only_a_barcode_found_is_read_with_a_release_lookup() {
    let fixture = Fixture::new("barcode-read").await;
    fixture.import.register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
        barcode: PAIRED_BARCODE.to_string(),
    }));
    let dir = fixture.barcode_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    for release_id in ["mb-barcode-de", "mb-barcode-gb"] {
        fixture.provider.route(
            &format!("/release/{release_id}?"),
            200,
            mb_release_json(release_id, "rg-barcode", &[probed, 0]),
        );
    }
    fixture.provider.route(
        "/release?",
        200,
        mb_barcode_search_json_placed(&[
            ("mb-barcode-de", "rg-barcode", PAIRED_BARCODE, "DE"),
            ("mb-barcode-gb", "rg-barcode", PAIRED_BARCODE, "GB"),
        ]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    for release_id in ["mb-barcode-de", "mb-barcode-gb"] {
        assert_eq!(fixture.count_release_lookups(release_id), 1);
        let stored = fixture.stored_release(release_id).await.expect("the run stores what it read");
        assert!(
            !stored
                .unfetched
                .iter()
                .any(|document| document.document
                    == bae_catalogs::records::PayloadSource::MusicBrainz),
            "{:?}",
            stored.unfetched
        );
    }
    assert_ne!(
        fixture.stored_for(&dir).await.expect("the candidate stores a row").metadata_author,
        crate::MetadataAuthor::Identification,
        "two pressings are offered and neither is picked"
    );
}

/// A release document can list a tracklist the candidate's draft cannot be
/// read from — here, a release crediting nobody. Its reading is unreadable,
/// so it is no match: the run rules its row out rather than picking it and
/// failing to apply it.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_its_draft_cannot_be_read_from_is_no_match() {
    let fixture = Fixture::new("interactive-projection-failure").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    let mut answer: serde_json::Value =
        serde_json::from_str(&discid_json("mb-projection-1", "rg-projection-1", &[probed, 0]))
            .unwrap();
    // No artist credits: the tracklist lists the folder's tracks, and the
    // reader refuses a release with nobody credited. The run reads the
    // release from this answer. The release group goes too — not because its
    // absence fails anything, but because a document that names one sends
    // the fetch after it, and this run has no group route to answer with.
    let incomplete = answer["releases"][0].as_object_mut().unwrap();
    incomplete.remove("artist-credit");
    incomplete.remove("release-group");
    fixture.provider.route("/discid/", 200, answer.to_string());
    // Identified only when asked: nothing found here is identified on its own.
    fixture.set_identify_automatically(false).await;
    fixture.scan(1).await;

    fixture.start_explicit_lookup(&dir);
    tokio::time::timeout(HANG_GUARD, fixture.identification().drained_for_test())
        .await
        .expect("the explicit run ends");

    let (admitted, _) = fixture.judgement_for(&dir).await;
    assert!(!admitted, "an unreadable release is never admitted");
    let key = crate::CandidateKey::folder(&dir);
    assert!(
        !matches!(
            fixture.identification_status(&key).await,
            Some(crate::IdentificationStatus::FinalizationFailed { .. })
        ),
        "nothing was picked to fail applying, got {:?}",
        fixture.identification_status(&key).await
    );
}

/// A cancelled run's lookup ends where it stands — here, waiting between
/// retries on a provider that never answers — rather than running on.
#[tokio::test]
async fn a_cancelled_run_drops_the_lookup_it_was_waiting_on() {
    struct Dropped(Option<tokio::sync::oneshot::Sender<()>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            let _ = self.0.take().map(|tx| tx.send(()));
        }
    }
    let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let token = CancellationToken::new();
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    spawn_until_cancelled(
        &tokio::runtime::Handle::current(),
        &bae_diagnostics::Diagnostics::noop(),
        &token,
        &event_tx,
        async move {
            let _guard = Dropped(Some(dropped_tx));
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the test is waiting on the start; a gone receiver has already failed it"
            )]
            let _ = started_tx.send(());
            std::future::pending::<()>().await;
        },
    );
    started_rx.await.expect("the lookup starts");
    token.cancel();
    tokio::time::timeout(HANG_GUARD, dropped_rx)
        .await
        .expect("the lookup is dropped once its run is cancelled")
        .expect("the guard reports its drop");
}

/// A lookup that panics ends its run as broken: the driver waiting on its
/// answer hears the run broke, and ends it there. The panic hook reported the
/// panic, so nothing here reports it again.
#[tokio::test]
async fn a_lookup_that_panics_ends_its_run_as_broken() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    spawn_until_cancelled(
        &tokio::runtime::Handle::current(),
        &diagnostics,
        &CancellationToken::new(),
        &event_tx,
        async {
            tokio::task::yield_now().await;
            bae_diagnostics::test_panic::panics("the lookup broke");
        },
    );

    let event = tokio::time::timeout(HANG_GUARD, event_rx.recv())
        .await
        .expect("the run hears the lookup end")
        .expect("the driver's channel is open");
    let IdentifyEvent::Broke { failure } = event else {
        panic!("a panicked lookup breaks its run, got {event:?}");
    };
    assert_eq!(failure.detail, "run an identify lookup panicked: the lookup broke");
    assert_eq!(bugs.reports(), Vec::new());
}
