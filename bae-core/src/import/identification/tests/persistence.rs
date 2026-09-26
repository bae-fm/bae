/// Candidates hashing the same are one job, and the answer one of them stores
/// answers all of them: the next admission finds both settled and asks nothing.
#[tokio::test(flavor = "multi_thread")]
async fn one_answer_covers_every_candidate_that_hashes_the_same() {
    let fixture = Fixture::new("shared-hash-answered").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    assert_eq!(
        fixture.content_hash(&first),
        fixture.content_hash(&second),
        "the two folders hold the same bytes"
    );
    let probed = fixture.probed_total_ms(&first);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-shared-1", "rg-shared-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-shared-1?",
        200,
        release_json("mb-shared-1", "rg-shared-1", &[probed, 0]),
    );
    fixture.scan(2).await;

    fixture.sweep_once().await;

    let asked = fixture.provider.count_containing("/discid/");
    assert_eq!(
        asked,
        1,
        "one run answered both: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&first).await.is_some());
    assert!(fixture.identified_for(&second).await.is_some());

    fixture.sweep_once().await;

    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        asked,
        "and a second admission finds both of them answered"
    );
}

// ── Synthetic candidates, for the pure planning tests ───────────────────────

fn synthetic_candidate(path: &str, size: u64) -> FolderCandidate {
    use crate::import::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
    FolderCandidate {
        path: PathBuf::from(path),
        file_root: PathBuf::from(path),
        name: path.trim_start_matches('/').to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                proposed_audio: true,
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
        watched_folder_path: "/".to_string(),
        scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: path.trim_start_matches('/').to_string(),
        grouping: None,
    }
}

// ── 10. Lookup reuses a stored verdict ──────────────────────────────────────

/// A several-match verdict, as identification stores one: the pressing is the
/// open question, so no match carries a settled tracklist.
fn multi_match_verdict(release_ids: &[&str], group_id: &str) -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: crate::identify::Findings {
            matches: release_ids
                .iter()
                .map(|release_id| MetadataResult {
                    source: crate::import::Catalog::MusicBrainz,
                    release_id: release_id.to_string(),
                    title: "Album".to_string(),
                    artist: Some("Artist".to_string()),
                    year: None,
                    label: None,
                    catalog_number: None,
                    area: None,
                    status: None,
                    packaging: None,
                    discogs_details: Vec::new(),
                    barcodes: Vec::new(),
                    media: crate::pressing::StatedMedia::Undescribed,
                    links: Vec::new(),
                    cover_art: None,
                    source_group_id: Some(group_id.to_string()),
                    album_links: crate::import::album_links::AlbumLinks::NotAsked,
                    source_tracks: None,
                })
                .collect(),
            provenance: release_ids
                .iter()
                .map(|_| crate::identify::LookupProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    by_catalog: false,
                    by_search: false,
                    named_by: None,
                })
                .collect(),
            // Each release is a pressing of its own: the group lists several, and
            // which one the folder is, is the open question.
            pressings: (0..release_ids.len() as u32).collect(),
            narrowed_out: crate::identify::NarrowedOut::default(),
        },
        track_count: 2,
        ledger: None,
    }
}

/// A verdict is refused for a candidate an import has claimed. The claim and
/// the check share the folder-state commit lock, so by the time a claim
/// returns there is no interval left in which a verdict can be stored for a
/// candidate the user has already committed to importing — and a verdict that
/// did land would describe files the import is in the middle of consuming.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_is_refused_for_a_claimed_candidate() {
    let fixture = Fixture::new("verdict-refused-when-claimed").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();
    let row = || NewImportCandidateVerdict {
        candidate: crate::import::CandidateAsRead {
            content_hash: fixture.content_hash(&dir),
            file_edit_revision: 0,
            metadata_revision: 0,
        },
        folder_path: key.clone(),
        verdict: multi_match_verdict(&["mb-claimed-1"], "rg-claimed-1"),
        signals: settled_signals(fixture.probed_durations(&dir)),
        metadata: None,
        owes_import: false,
    };

    assert!(
        fixture
            .import
            .save_candidate_verdict_if_current(&key, IdentifyRunId::for_test(1), &row())
            .await
            .unwrap(),
        "an unclaimed candidate still takes its verdict"
    );

    fixture.import.claim_candidate_for_import(&key).await;

    assert!(
        !fixture
            .import
            .save_candidate_verdict_if_current(&key, IdentifyRunId::for_test(1), &row())
            .await
            .unwrap(),
        "a claimed candidate refuses a verdict"
    );
}

/// An ending ends the run it names and nothing else. A run's own terminal
/// state has already left `running` for `saving`, so an `Idle` behind it
/// cannot blank the answer waiting to be written — while a mid-run `Idle`,
/// which is the run being abandoned, empties the key.
#[tokio::test(flavor = "multi_thread")]
async fn an_ending_ends_the_run_it_names_and_not_the_answer_being_saved() {
    let fixture = Fixture::new("teardown-keeps-state").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    let not_in_library =
        |result: &MetadataResult| crate::db::LibraryStatus::absent(&result.release_id);
    let found = multi_match_verdict(&["mb-teardown-1"], "rg-teardown-1")
        .resume_state(&not_in_library, Default::default());
    let changed = |run: u64, state: IdentifyState| ImportEvent::IdentifyStateChanged {
        candidate_key: key.clone(),
        run: crate::identify::IdentifyRunId::for_test(run),
        state,
        priority: CallPriority::Background,
    };
    let mut changes = fixture.import.subscribe_candidate_runtime().1;
    // The recorder publishes one change per event it records, and a torn-down
    // `Idle` after a terminal state records nothing. The bus is ordered, so an
    // unrelated event that does record marks that the `Idle` before it has
    // been seen.
    let marker = || ImportEvent::ImportProgress {
        candidate_key: "reidentify:marker".to_string(),
        progress: crate::import::ImportProgress::Preparing {
            import_id: "marker".to_string(),
            step: crate::import::PrepareStep::Queued,
            album_title: String::new(),
            artist_name: String::new(),
        },
    };
    let recorded = |change: Result<
        crate::import::CandidateRuntimeChange,
        tokio::sync::broadcast::error::RecvError,
    >| {
        let change = change.expect("runtime changes stay open");
        assert!(
            matches!(&change, crate::import::CandidateRuntimeChange::Updated { key, .. } if key == "reidentify:marker"),
            "expected the marker, got {change:?}"
        );
    };

    fixture.import.emit_event_for_test(changed(1, found));
    changes.recv().await.expect("runtime changes stay open");
    fixture
        .import
        .emit_event_for_test(changed(1, IdentifyState::Idle));
    fixture.import.emit_event_for_test(marker());
    recorded(changes.recv().await);
    let Ok(Some(ImportCandidateSnapshot::Folder { runtime, .. })) =
        fixture.import.get_candidate(&key).await
    else {
        panic!("the scanned candidate is readable");
    };
    let Some(IdentifyState::Found {
        findings: crate::identify::Findings { matches, .. },
        ..
    }) = &runtime.as_ref().and_then(|runtime| runtime.saving.clone())
    else {
        panic!("the answer stays where its write will find it, got {runtime:?}");
    };
    assert_eq!(matches[0].release_id, "mb-teardown-1");

    // A later run cancelled mid-flight is that run being abandoned. It ends
    // itself and leaves the answer still waiting to be written.
    let triangulating = IdentifyState::Triangulating {
        discid: crate::identify::DiscidProgress::Computing,
        barcode: crate::identify::BarcodeProgress::Scanning,
        catalog: crate::identify::CatalogProgress::Skipped,
        search: crate::identify::SearchProgress::Pending,
        context: crate::identify::state::SignalsContext {
            rip: crate::signals::RipEvidence::Unproven,
            providers: Vec::new(),
            steps: crate::config::IdentificationSteps::default(),
            artwork: crate::signals::ArtworkScan::Absent,
            disc: Default::default(),
            barcode: Default::default(),
            catalog: Default::default(),
            search: Default::default(),
            text: Default::default(),
            text_settled: false,
            track_count: 0,
            album_links: crate::identify::state::AlbumLinkReading::Pending,
        },
    };
    fixture
        .import
        .emit_event_for_test(changed(2, triangulating));
    changes.recv().await.expect("runtime changes stay open");
    fixture
        .import
        .emit_event_for_test(changed(2, IdentifyState::Idle));
    changes.recv().await.expect("runtime changes stay open");
    let Ok(Some(ImportCandidateSnapshot::Folder { runtime, .. })) =
        fixture.import.get_candidate(&key).await
    else {
        panic!("the scanned candidate is readable");
    };
    let runtime = runtime.expect("the answer is still waiting on its write");
    assert!(runtime.running.is_none());
    assert!(matches!(runtime.saving, Some(IdentifyState::Found { .. })));

    // Only the write of the run that answered ends that wait, and then
    // nothing is happening for the key at all.
    fixture
        .import
        .end_identification_answer(&key, crate::identify::IdentifyRunId::for_test(1));
    let Ok(Some(ImportCandidateSnapshot::Folder { runtime, .. })) =
        fixture.import.get_candidate(&key).await
    else {
        panic!("the scanned candidate is readable");
    };
    assert!(
        runtime.is_none(),
        "the finished write leaves nothing behind, got {runtime:?}"
    );
}

// ── 11. Re-stating a file decision changes nothing ──────────────────────────

/// The disc menu and the role picker fire on every selection, including of
/// the item already in force. A decision that re-states what is already true
/// writes nothing — above all it does not clear the stored verdict, which
/// would re-identify a folder whose shape did not change and blank the pane
/// over it.
#[tokio::test(flavor = "multi_thread")]
async fn restating_a_file_decision_changes_nothing() {
    let fixture = Fixture::new("edit-noop").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    fixture.archive("mb-noop-1", "rg-noop-1", &[500, 500]).await;
    fixture
        .store_settled_verdict(&dir, "mb-noop-1", "rg-noop-1", 1_000)
        .await;
    let key = dir.to_string_lossy().into_owned();
    let mut events = fixture.import.subscribe_events();

    // The sheet already carves disc one, the loose file is already audio, and
    // the sheet already binds its own container.
    fixture
        .import
        .set_sheet_disc(
            key.clone(),
            "Test Album.cue".to_string(),
            crate::import::folder_scanner::SheetDisc::Disc { number: 1 },
        )
        .await
        .unwrap();
    fixture
        .import
        .set_file_role(
            key.clone(),
            "02 Test Artist - Track Two (White Noise).flac".to_string(),
            crate::import::folder_scanner::FileRoleChoice::Audio,
        )
        .await
        .unwrap();
    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
            "Test Album.flac".to_string(),
            Some("Test Album.flac".to_string()),
        )
        .await
        .unwrap();

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate's row remains");
    assert!(
        row.identify.is_some(),
        "a re-stated decision must not clear the stored verdict"
    );
    assert!(
        !drain_events(&mut events).iter().any(|event| matches!(
            event,
            ImportEvent::Scan(ScanEvent::CandidateBindingChanged { .. })
        )),
        "and must not announce a changed candidate"
    );

    // A genuinely different decision still lands and still clears.
    fixture
        .import
        .set_sheet_disc(
            key,
            "Test Album.cue".to_string(),
            crate::import::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();
    assert!(
        fixture
            .stored_for(&dir)
            .await
            .is_none_or(|row| row.identify.is_none()),
        "a real change clears the verdict as before"
    );
}
