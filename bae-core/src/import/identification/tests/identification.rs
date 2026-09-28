/// The whole point of the task: nothing is selected, no view is open, and the
/// candidate still ends up with a stored verdict that is auto-importable.
///
/// The provider answers the disc-ID lookup with exactly one release listing as
/// many tracks as the fixture holds, so every clause of the auto-import check is
/// exercised for real: one match, counts agreeing.
#[tokio::test(flavor = "multi_thread")]
async fn a_candidate_nobody_selected_acquires_a_verdict() {
    let fixture = Fixture::new("acquires-verdict").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json(
            "mb-auto-1",
            "rg-auto-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.provider.route(
        "/release/mb-auto-1?",
        200,
        release_json(
            "mb-auto-1",
            "rg-auto-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.scan(1).await;

    // Nobody asks: it is identified because it was found.
    fixture.drain_automatic().await;

    let row = fixture.stored_for(&dir).await.expect("a verdict is stored");
    assert_eq!(
        row.folder_path,
        dir.to_string_lossy(),
        "the row names where the candidate was last seen"
    );
    let identify = identify_result(&row);
    assert_eq!(
        identify.identified_at,
        fixed_now(),
        "the row is stamped from the injected clock"
    );
    assert_eq!(
        fixture.judgement_for(&dir).await,
        (true, None),
        "one match, counts agreeing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_planned_candidate_is_queued_before_its_driver_reports() {
    let fixture = Fixture::new("queued-before-driver").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let mut changes = fixture.import.subscribe_candidate_runtime().1;

    let pass = fixture.drain_automatic_task();

    let change = tokio::time::timeout(Duration::from_secs(10), changes.recv())
        .await
        .expect("the queue state is published before identification")
        .expect("candidate runtime remains open");
    let crate::import::CandidateRuntimeChange::Reset { runtimes } = change else {
        panic!("the queue publishes its admission before starting drivers");
    };
    assert_eq!(
        crate::import::TriageRuntimeFacts::of(&runtimes[&key]).identification,
        Some(crate::import::IdentificationStatus::Queued)
    );

    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), pass)
        .await
        .expect("the pass finishes after the provider resumes")
        .unwrap();
}

/// Candidates hashing the same share one job, and one of them runs it. The
/// rest are not told that run's states — each key is its own run, and what
/// they are waiting for is the answer this one stores, which covers them. So
/// they stay queued until it does.
#[tokio::test(flavor = "multi_thread")]
async fn a_shared_identify_job_runs_one_member_and_leaves_the_rest_queued() {
    let fixture = Fixture::new("shared-job-status").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    let keys = [
        first.to_string_lossy().into_owned(),
        second.to_string_lossy().into_owned(),
    ];
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(2).await;

    let pass = fixture.drain_automatic_task();

    if tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let runtimes = fixture.import.candidate_runtimes();
            let statuses: Vec<Option<crate::import::IdentificationStatus>> = keys
                .iter()
                .map(|key| {
                    runtimes.get(key).and_then(|runtime| {
                        crate::import::TriageRuntimeFacts::of(runtime).identification
                    })
                })
                .collect();
            let running = statuses
                .iter()
                .filter(|status| **status == Some(crate::import::IdentificationStatus::Running))
                .count();
            let queued = statuses
                .iter()
                .filter(|status| **status == Some(crate::import::IdentificationStatus::Queued))
                .count();
            if running == 1 && queued == 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_err()
    {
        panic!(
            "one member runs the shared job and the other waits on its answer; \
             runtimes: {:?}; requests: {:?}; identifying: {:?}",
            fixture.import.candidate_runtimes(),
            fixture.provider.requests(),
            keys.iter()
                .map(|key| fixture.import.is_identifying(key))
                .collect::<Vec<_>>()
        );
    }

    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(20), pass)
        .await
        .expect("the pass finishes after the provider resumes")
        .unwrap();
}

// ── 2. A stored verdict is not re-fetched ───────────────────────────────────

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
        release_json("mb-cached-1", "rg-cached-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture.drain_automatic().await;
    let after_first = fixture.provider.requests().len();
    assert!(
        after_first > 0,
        "the first name has to actually ask the provider"
    );
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
            .answerable_candidate(&renamed.to_string_lossy())
            .await
            .unwrap()
            .is_some(),
        "the scan found the folder under its new name"
    );
}

// ── 3. A transport failure is stored ────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn a_transport_failure_is_stored() {
    let fixture = Fixture::new("failure-stored").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture
        .provider
        .set_routes(vec![("/discid/", 400, "{}".to_string())]);
    fixture.scan(1).await;

    fixture.drain_automatic().await;
    let stored = fixture
        .identified_for(&dir)
        .await
        .expect("the failed outcome is stored");
    assert!(matches!(stored.verdict, TerminalVerdict::Failed { .. }));
}

// ── 4. The interactive path is not delayed by automatic identification ──────

/// With automatic identification's background lookups queued on the shared
/// limiter, a search the user typed is admitted next rather than after all of
/// them. Each of eight candidates has its own barcode, so each queues a lookup.
///
/// Wall time, because the fake provider is a real socket and a paused clock
/// would time every lookup out; assertions around the measurement check that
/// background work really was in flight.
#[tokio::test(flavor = "multi_thread")]
async fn the_interactive_path_is_not_delayed_by_automatic_identification() {
    let fixture = Fixture::new("interactive-not-delayed").await;
    fixture
        .import
        .register_artwork_analyzer(Arc::new(PerFolderBarcodeAnalyzer));
    let mut dirs = Vec::new();
    for i in 0..8 {
        let dir = fixture.barcode_candidate(&format!("Album {i}"));
        std::fs::write(
            dir.join(format!("playlist-{i}.m3u")),
            format!("candidate {i}"),
        )
        .unwrap();
        dirs.push(dir);
    }
    let probed = fixture.probed_total_ms(&dirs[0]);
    // Added first, so the typed-search route below catches only what the
    // barcode lookups leave.
    fixture.provider.route(
        "/release?query=barcode",
        200,
        search_json("mb-flood-0", "rg-flood-0"),
    );
    fixture.provider.route(
        "/release/mb-flood-0?",
        200,
        release_json("mb-flood-0", "rg-flood-0", &[probed, 0]),
    );
    fixture
        .provider
        .route("/release?", 200, search_json("mb-typed", "rg-typed"));
    fixture.scan(8).await;

    let sweep = fixture.drain_automatic_task();

    // Let the first lookup go out and the rest queue behind it.
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    let background_before = fixture.provider.count_containing("query=barcode");
    assert!(
        (1..8).contains(&background_before),
        "background lookups must be queued when the search is timed: \
         {background_before} of 8 done"
    );

    let started = std::time::Instant::now();
    let typed = fixture
        .manager
        .search_musicbrainz(
            crate::musicbrainz::ReleaseSearchParams {
                artist: Some("Artist".to_string()),
                album: Some("Album".to_string()),
                ..Default::default()
            },
            CallPriority::Interactive,
        )
        .await
        .expect("the typed search succeeds");
    let waited = started.elapsed();

    assert!(
        !sweep.is_finished(),
        "background work must still be going across the measurement"
    );
    fixture.identification().shut_down();
    let _ = tokio::time::timeout(Duration::from_secs(20), sweep).await;

    assert_eq!(typed.len(), 1);
    assert!(
        waited < Duration::from_millis(2_000),
        "an interactive search waited {waited:?} behind background lookups; \
         with priority it is admitted within about one interval"
    );
}

// ── 5. The track count decides, never the lengths ───────────────────────────

fn found_verdict(track_count: u32, source: Option<SourceTracks>) -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: crate::identify::Findings {
            matches: vec![MetadataResult {
                source: crate::import::Catalog::MusicBrainz,
                release_id: "mb-1".to_string(),
                title: "Album".to_string(),
                artist: None,
                year: None,
                labels: Vec::new(),
                area: None,
                status: None,
                packaging: None,
                discogs_details: Vec::new(),
                barcodes: Vec::new(),
                media: crate::pressing::StatedMedia::Undescribed,
                links: Vec::new(),
                cover_art: None,
                source_group_id: Some("rg-1".to_string()),
                album_links: crate::import::album_links::AlbumLinks::NotAsked,
                source_tracks: source,
                document_failure: None,
                album_first_year: None,
            }],
            provenance: vec![crate::identify::LookupProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
                by_isrc: false,
                by_search: false,
                named_by: None,
            }],
            pressings: vec![0],
            narrowed_out: crate::identify::NarrowedOut::default(),
            medium_conflict: None,
        },
        track_count,
        ledger: None,
    }
}

/// A source's lengths never keep a match from being auto-importable. The release parsed here
/// states lengths that match nothing about the rip, and all the rule reads off
/// it is the count: three tracks, as the folder holds.
#[test]
fn the_lengths_a_source_states_do_not_decide() {
    let payloads: crate::import::payloads::ReleasePayloads =
        crate::import::payloads::ReleasePayloads::for_test(
            crate::import::MetadataRef::new(crate::import::Catalog::MusicBrainz, "mb-1"),
            release_json("mb-1", "rg-1", &[200_000, 100_000, 300_000]),
            Vec::new(),
        );
    let source = payloads.extract().unwrap().source_tracks_for_audio(&[]);
    assert_eq!(source, SourceTracks::Listed { count: 3 });
    assert_eq!(
        crate::identify::VerdictSummary::of(&found_verdict(3, Some(source))).judgement(),
        (true, None),
        "the counts agree, whatever the lengths"
    );
}

/// A count disagreement is named as one, with both counts.
#[test]
fn a_count_disagreement_is_named_as_one() {
    assert_eq!(
        crate::identify::VerdictSummary::of(&found_verdict(11, Some(SourceTracks::Listed { count: 12 })),).judgement(),
        (false, Some(FolderCheck::TrackCountDisagrees {
            local: 11,
            source: 12
        }))
    );
}

// ── 6. A skipped candidate is not identified ────────────────────────────────

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
        release_json("mb-skipped-1", "rg-skipped-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    fixture
        .import
        .set_candidate_skipped(dir.to_string_lossy().into_owned(), true)
        .await
        .unwrap();

    fixture.drain_automatic().await;

    assert!(
        fixture.provider.requests().is_empty(),
        "a skipped candidate costs the provider nothing: {:?}",
        fixture.provider.requests()
    );
    assert!(
        fixture.identified_for(&dir).await.is_none(),
        "and produces no identification result"
    );
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
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-bare-1", "rg-bare-1", &lengths),
    );
    fixture.provider.route(
        "/release/mb-bare-1?",
        200,
        release_json("mb-bare-1", "rg-bare-1", &lengths),
    );
    fixture.scan(1).await;

    let folders_own = crate::import::CoverSelection::Local("cover.jpg".to_string());
    assert_eq!(
        fixture
            .manager
            .load_import_candidate_preparation(&fixture.content_hash(&dir))
            .await
            .unwrap()
            .expect("the scanned candidate is prepared")
            .cover,
        Some(folders_own.clone()),
        "the scan stores the cover the folder gives the candidate"
    );

    fixture.drain_automatic().await;

    assert_eq!(
        fixture
            .manager
            .load_import_candidate_preparation(&fixture.content_hash(&dir))
            .await
            .unwrap()
            .expect("the identified candidate is prepared")
            .cover,
        Some(folders_own.clone()),
        "the settled release brought no image, so the folder's cover stands"
    );
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the identified candidate reads back")
            .cover
            .expect("the pane shows the stored selection")
            .selection,
        folders_own
    );
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
    fixture
        .import
        .register_artwork_analyzer(Arc::new(BarcodeAnalyzer {
            barcode: "0123456789012".to_string(),
        }));
    // The folder name is what the pre-filled draft calls the release, and so
    // what the title search asks about.
    let dir = fixture.barcode_candidate("Album Title");
    let probed = fixture.probed_total_ms(&dir);
    fixture
        .provider
        .route("query=barcode%3A", 200, r#"{"releases":[]}"#);
    fixture.provider.route(
        "query=release%3A",
        200,
        search_json("mb-by-title", "rg-by-title"),
    );
    fixture.provider.route(
        "/release/mb-by-title?",
        200,
        release_json(
            "mb-by-title",
            "rg-by-title",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.scan(1).await;

    fixture.drain_automatic().await;

    let verdict = identify_result(&fixture.stored_for(&dir).await.expect("a verdict is stored"))
        .verdict
        .clone();
    let TerminalVerdict::Found {
        findings:
            crate::identify::Findings {
                matches,
                provenance,
                ..
            },
        ..
    } = &verdict
    else {
        panic!("expected the title search's release, got {verdict:?}");
    };
    assert_eq!(matches[0].release_id, "mb-by-title");
    assert!(
        provenance[0].by_search,
        "the row records the title search as what produced it"
    );
    assert!(!provenance[0].by_barcode && !provenance[0].by_disc_id);
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
        pane.metadata_author,
        crate::import::MetadataAuthor::Identification,
        "identification wrote the draft from the lone row"
    );
    assert_eq!(
        pane.release
            .expect("the settled row is the candidate's release")
            .release_id,
        "mb-by-title"
    );
}
