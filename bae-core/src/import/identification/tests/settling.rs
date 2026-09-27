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
async fn settling_a_lead_costs_one_release_lookup_whichever_signal_found_it() {
    let fixture = Fixture::new("settle-lead").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
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
    assert_eq!(
        fixture.judgement_for(&disc_dir).await,
        (true, None)
    );
    assert_eq!(
        fixture.judgement_for(&barcode_dir).await,
        (true, None)
    );
}

/// A lead whose document cannot be read is stored as its lookup returned it,
/// saying why, with no pick applied and no partial documents left behind.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_lead_is_stored_unsettled_without_partial_documents() {
    let fixture = Fixture::new("settle-ordering").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    // The disc-ID lookup answers; the release document does not.
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-order-1", "rg-order-1", &[probed, 0]),
    );
    fixture.provider.route("/release/mb-order-1?", 400, "{}");
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found { findings, .. } = &verdict else {
        panic!("an unreadable document does not fail the run, got {verdict:?}");
    };
    assert!(findings.matches[0].document_failure.is_some());
    assert_ne!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "no document, so no pick was applied"
    );
    assert!(
        fixture.stored_release("mb-order-1").await.is_none(),
        "and nothing half-written is left behind"
    );
}

/// Explicitly applying a settled release whose group failed to fetch fetches
/// the release again, parent and all. Opening the resulting candidate reads
/// the stored release offline.
#[tokio::test(flavor = "multi_thread")]
async fn applying_a_settled_candidate_refetches_a_missing_parent_then_reads_offline() {
    let fixture = Fixture::new("offline-open").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;

    // The stored pressing names its group as missing; both can be fetched.
    fixture.provider.route(
        "/release/mb-offline-1?",
        200,
        release_json("mb-offline-1", "rg-offline-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release-group/rg-offline-1?",
        200,
        serde_json::json!({
            "id": "rg-offline-1", "title": "Album", "relations": [],
            "first-release-date": "1981"
        })
        .to_string(),
    );
    fixture
        .archive_missing_its_group("mb-offline-1", "rg-offline-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-offline-1", "rg-offline-1",)
        .await;
    let before = fixture.provider.requests().len();

    fixture
        .import
        .select_candidate_metadata_provenance(
            dir.to_string_lossy().into_owned(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    "mb-offline-1".to_string(),
                ),
                partners: vec![],
            },
        )
        .await
        .expect("a settled candidate opens once its release is fetched again");

    let after_apply = fixture.provider.requests().len();
    let stored = fixture
        .stored_release("mb-offline-1")
        .await
        .expect("the release stays stored");
    assert!(
        stored.unfetched.is_empty(),
        "the group fetched this time: {:?}",
        stored.unfetched
    );
    let detail = fixture
        .pane(&dir)
        .await
        .expect("the picked candidate reads back");
    assert_eq!(
        fixture.provider.requests().len(),
        after_apply,
        "reading the candidate does not fetch metadata"
    );
    let release = detail.release.expect("the pick names a release");
    assert_eq!(release.release_id, "mb-offline-1");
    assert_eq!(release.tracks.len(), 2);
    let edit = detail.metadata_draft;
    assert_eq!(edit.album_title, "Album");
    assert_eq!(
        release
            .cover_art
            .iter()
            .map(|cover| cover.image.url.as_str())
            .collect::<Vec<_>>(),
        vec![format!(
            "{}/release-group/rg-offline-1/front",
            crate::import::cover_art::ARCHIVE
        )],
        "the pressing states no front image of its own, so the album's is the \
         only option — and it is read off the stored release, not asked for"
    );
    let requested = &fixture.provider.requests()[before..];
    assert_eq!(requested.len(), 3, "{requested:?}");
    assert!(
        requested[0].starts_with("/ws/2/release/mb-offline-1?"),
        "{requested:?}"
    );
    assert_eq!(
        &requested[1..],
        &[
            "/ws/2/release-group/rg-offline-1?inc=artist-credits+url-rels&fmt=json".to_string(),
            "/release-group/rg-offline-1/front".to_string(),
        ],
        "selection fetches the release again, parent and all, then the cover"
    );
}

/// A settled lead whose release is not stored is a broken invariant, not a
/// cold cache. Picking it fails loudly, and stores no pick — so nothing is
/// left naming a release the pane could not draw.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_lead_with_no_stored_release_fails_loud() {
    let fixture = Fixture::new("offline-miss").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    fixture
        .store_settled_lead_without_its_pick(&dir, "mb-missing-1", "rg-missing-1")
        .await;

    let error = fixture
        .import
        .select_candidate_metadata_provenance(
            dir.to_string_lossy().into_owned(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    "mb-missing-1".to_string(),
                ),
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
        fixture.stored_release("mb-manual-1").await.is_none(),
        "nothing has fetched this release yet"
    );

    let pick = || crate::import::MetadataProvenance::ExternalRelease {
        record: crate::import::MetadataRef::new(
            crate::import::Catalog::MusicBrainz,
            "mb-manual-1".to_string(),
        ),
        partners: vec![],
    };
    fixture
        .import
        .select_candidate_metadata_provenance(dir.to_string_lossy().into_owned(), pick())
        .await
        .expect("a manual pick fetches");

    assert!(
        fixture.stored_release("mb-manual-1").await.is_some(),
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
/// answer, and identification settles it. The pick names the MusicBrainz release the
/// draft is read from and the Discogs release beside it, and both sources'
/// documents are archived, so opening this candidate needs no network.
#[tokio::test(flavor = "multi_thread")]
async fn matches_that_pair_into_one_pressing_settle_as_one_pick() {
    let fixture = Fixture::new("paired-settle").await;
    fixture.use_discogs().await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
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
    fixture
        .provider
        .route("/releases/70000101", 200, discogs_release_json("70000101"));
    // Nothing links this synthetic Discogs release to a MusicBrainz one, which
    // is the answer the cross-reference lookup would come back with.
    fixture
        .manager
        .providers()
        .musicbrainz()
        .seed_discogs_url_lookup("70000101", None);
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the paired candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found {
        findings: crate::identify::Findings { matches, .. },
        ..
    } = &verdict
    else {
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
            record: crate::import::MetadataRef::new(
                crate::import::Catalog::MusicBrainz,
                "mb-paired-1".to_string()
            ),
            partners: vec![crate::import::MetadataRef::new(
                crate::import::Catalog::Discogs,
                "70000101"
            )],
        }),
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
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
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
        barcode_search_json(&[("mb-paired-2", "rg-paired-2", PAIRED_BARCODE)]),
    );
    fixture.provider.route(
        "/release/mb-paired-2?",
        200,
        release_json("mb-paired-2", "rg-paired-2", &[probed, 0]),
    );
    fixture.provider.route(
        "/database/search",
        200,
        discogs_search_json("70000102", PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT),
    );
    fixture
        .provider
        .route("/releases/70000102", 200, discogs_release_json("70000102"));
    fixture
        .manager
        .providers()
        .musicbrainz()
        .seed_discogs_url_lookup("70000102", None);
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found {
        findings:
            crate::identify::Findings {
                matches,
                narrowed_out,
                ..
            },
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
        row.metadata_provenance,
        Some(crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(
                crate::import::Catalog::MusicBrainz,
                "mb-paired-2".to_string()
            ),
            partners: vec![crate::import::MetadataRef::new(
                crate::import::Catalog::Discogs,
                "70000102"
            )],
        }),
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
/// here; only the Discogs record states a year, and the folder prints it — so
/// the Discogs record leads its row, the pick claims MusicBrainz beside it, and
/// the draft is read from the Discogs document.
#[tokio::test(flavor = "multi_thread")]
async fn the_record_the_folder_agrees_with_settles_as_the_lead() {
    let fixture = Fixture::new("evidence-lead").await;
    fixture.use_discogs().await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    // The folder prints the year the Discogs record states and the MusicBrainz
    // search hit does not.
    let dir = fixture.barcode_candidate("From Barcode 1996");
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[("mb-paired-3", "rg-paired-3", PAIRED_BARCODE)]),
    );
    fixture.provider.route(
        "/release/mb-paired-3?",
        200,
        release_json("mb-paired-3", "rg-paired-3", &[180_000, 0]),
    );
    fixture.provider.route(
        "/database/search",
        200,
        discogs_search_json("70000103", PAIRED_BARCODE_AS_DISCOGS_PRINTS_IT),
    );
    fixture.provider.route(
        "/releases/70000103",
        200,
        {
            let mut release: serde_json::Value = serde_json::from_str(&discogs_release_json("70000103")).unwrap();
            release["tracklist"].as_array_mut().unwrap().push(serde_json::json!({
                "position": "2", "title": "Track 2", "duration": "0:01", "type_": "track", "artists": []
            }));
            release.to_string()
        },
    );
    fixture
        .manager
        .providers()
        .musicbrainz()
        .seed_discogs_url_lookup("70000103", None);
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the paired candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found {
        findings: crate::identify::Findings { matches, .. },
        ..
    } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        matches[0].source,
        crate::import::Catalog::Discogs,
        "the folder agrees with the Discogs record about more: {matches:?}"
    );
    assert!(
        matches[0].source_tracks.is_some(),
        "and the tracklist was settled from it, not from its partner"
    );
    assert_eq!(
        row.metadata_provenance,
        Some(crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(
                crate::import::Catalog::Discogs,
                "70000103".to_string()
            ),
            partners: vec![crate::import::MetadataRef::new(
                crate::import::Catalog::MusicBrainz,
                "mb-paired-3"
            )],
        }),
        "so the pick names it primary and the MusicBrainz record its partner"
    );
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the settled candidate reads back")
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

/// Two pressings are a question, not an answer: which one is on disk is the
/// user's call. Both offered pressings' documents are read before the run
/// settles, so each row states its full tracklist and a pick later asks for
/// nothing; with both fitting the folder they stay tied, and the verdict
/// stores with no pick.
#[tokio::test(flavor = "multi_thread")]
async fn two_distinct_pressings_are_read_in_full_and_do_not_settle() {
    let fixture = Fixture::new("two-pressings").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    for id in ["mb-two-1", "mb-two-2"] {
        fixture.provider.route(
            &format!("/release/{id}?"),
            200,
            release_json(id, "rg-two-1", &[probed, 0]),
        );
    }
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[
            ("mb-two-1", "rg-two-1", PAIRED_BARCODE),
            ("mb-two-2", "rg-two-1", "9876543210987"),
        ]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found {
        findings: crate::identify::Findings { matches, .. },
        ..
    } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(matches.len(), 2);
    assert!(
        matches.iter().all(|result| result.source_tracks
            == Some(SourceTracks::Listed { count: 2 })),
        "every offered pressing states its full tracklist: {matches:?}"
    );
    assert_ne!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "a run that settled on no release wrote no draft, so the candidate's own stands"
    );
    assert_eq!(
        (
            fixture.count_release_lookups("mb-two-1"),
            fixture.count_release_lookups("mb-two-2")
        ),
        (1, 1),
        "each offered pressing's document was fetched once: {:?}",
        fixture.provider.requests()
    );
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (false, None)
    );

    // Picking an offered row reads the document the run stored.
    let before_pick = fixture.provider.requests().len();
    fixture
        .import
        .select_candidate_metadata_provenance(
            dir.to_string_lossy().into_owned(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    "mb-two-2".to_string(),
                ),
                partners: vec![],
            },
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
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release/mb-read?",
        200,
        release_json("mb-read", "rg-read", &[probed, 0]),
    );
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[
            ("mb-read", "rg-read", PAIRED_BARCODE),
            ("mb-unread", "rg-read", "9876543210987"),
        ]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
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
    assert!(
        unread.document_failure.is_some(),
        "it says its document could not be read"
    );
}

/// The folder's track count breaks a tie the lookups leave: of two pressings
/// a barcode names alike, the one whose tracklist holds as many tracks as the
/// folder is offered, and the run settles on it.
#[tokio::test(flavor = "multi_thread")]
async fn the_pressing_whose_tracklist_fits_the_folder_is_offered() {
    let fixture = Fixture::new("fitting-pressing").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: PAIRED_BARCODE.to_string(),
        }));
    let dir = fixture.barcode_candidate("From Barcode");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/release/mb-long?",
        200,
        release_json("mb-long", "rg-fit", &[probed, 0, 1_000]),
    );
    fixture.provider.route(
        "/release/mb-fits?",
        200,
        release_json("mb-fits", "rg-fit", &[probed, 0]),
    );
    fixture.provider.route(
        "/release?",
        200,
        barcode_search_json(&[
            ("mb-long", "rg-fit", PAIRED_BARCODE),
            ("mb-fits", "rg-fit", PAIRED_BARCODE),
        ]),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate stores a row");
    let verdict = identify_result(&row).verdict.clone();
    let TerminalVerdict::Found {
        findings:
            crate::identify::Findings {
                matches,
                narrowed_out,
                ..
            },
        ..
    } = &verdict
    else {
        panic!("expected a Found verdict, got {verdict:?}");
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-fits"]
    );
    assert_eq!(
        narrowed_out
            .matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-long"]
    );
    assert_eq!(
        row.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "the one fitting pressing settles the draft"
    );
    assert_eq!(fixture.count_release_lookups("mb-fits"), 1);
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
        release_json_with_catalog("mb-catalog-1", "rg-catalog-1", &[probed, 0], "NJ 8255"),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let pane = fixture
        .pane(&dir)
        .await
        .expect("the settled candidate has a pane");
    assert_eq!(
        pane.metadata_provenance,
        Some(crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(
                crate::import::Catalog::MusicBrainz,
                "mb-catalog-1".to_string()
            ),
            partners: vec![],
        })
    );
    assert_eq!(
        pane.lookup_choices,
        crate::import::LookupChoices::default(),
        "the number the record carries and the folder prints is not chosen"
    );
}

/// `release_json` for a release that carries `catalog_number`.
fn release_json_with_catalog(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
    catalog_number: &str,
) -> String {
    let mut release: serde_json::Value =
        serde_json::from_str(&release_json(release_id, group_id, track_lengths))
            .expect("release fixture parses");
    release["label-info"] = serde_json::json!([{ "catalog-number": catalog_number }]);
    release.to_string()
}

/// `discid_json` for a release that carries `catalog_number`.
fn discid_json_with_catalog(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
    catalog_number: &str,
) -> String {
    let mut answer: serde_json::Value =
        serde_json::from_str(&discid_json(release_id, group_id, track_lengths))
            .expect("the disc ID fixture parses");
    answer["releases"][0]["label-info"] = serde_json::json!([{ "catalog-number": catalog_number }]);
    answer.to_string()
}
