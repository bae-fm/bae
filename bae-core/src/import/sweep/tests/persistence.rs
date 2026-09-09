#[test]
fn duplicate_content_hashes_share_one_identify_job() {
    let first = synthetic_candidate("/first", 321);
    let second = synthetic_candidate("/second", 321);
    assert_eq!(first.files.content_hash(), second.files.content_hash());

    let planned = Pass::new(
        vec![first.clone().into(), second.clone().into()],
        &HashMap::new(),
    );
    assert_eq!(planned.queued().len(), 1);
    assert_eq!(planned.queued()[0].candidates.len(), 2);
    assert_eq!(planned.identified(), 0);

    let stored = HashMap::from([(
        first.files.content_hash(),
        row_with_verdict(&first, TerminalVerdict::NotFoundAnywhere { ledger: None }),
    )]);
    let planned = Pass::new(vec![first.into(), second.into()], &stored);
    assert!(planned.queued().is_empty());
    assert_eq!(planned.identified(), 2);
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
        },
        watched_folder_path: "/".to_string(),
        scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: path.trim_start_matches('/').to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

fn row_with_verdict(
    candidate: &FolderCandidate,
    verdict: TerminalVerdict,
) -> DbImportCandidateState {
    DbImportCandidateState {
        content_hash: candidate.files.content_hash(),
        folder_path: candidate.path.to_string_lossy().into_owned(),
        identify: Some(crate::db::DbCandidateIdentifyResult {
            verdict,
            probed_total_duration_ms: 0,
            identified_at: fixed_now(),
        }),
        signals: None,
        lookup_choices: Default::default(),
        file_edits: Default::default(),
        metadata_provenance: None,
        metadata_author: crate::import::MetadataAuthor::Nobody,
        metadata_revision: 0,
    }
}

fn blank_metadata_for_dir(dir: &Path) -> crate::import::CandidateMetadataDraft {
    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        dir,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("the candidate folder is readable");
    crate::import::CandidateMetadataDraft {
        draft: crate::import::pane::blank_candidate_source(&files).draft,
        source_discogs_artist_ids: Default::default(),
        provenance: None,
        cover: None,
        assets: crate::import::CandidatePreparedAssets::default(),
    }
}

// ── 10. Lookup reuses a stored verdict ──────────────────────────────────────

/// A several-match verdict, as identification stores one: the pressing is the
/// open question, so no match carries a settled tracklist.
fn multi_match_verdict(release_ids: &[&str], group_id: &str) -> TerminalVerdict {
    TerminalVerdict::Found {
        matches: release_ids
            .iter()
            .map(|release_id| MetadataResult {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: release_id.to_string(),
                title: "Album".to_string(),
                artist: Some("Artist".to_string()),
                year: None,
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
                cover_art: None,
                source_group_id: Some(group_id.to_string()),
                source_tracks: None,
            })
            .collect(),
        track_count: 2,
        provenance: release_ids
            .iter()
            .map(|_| crate::identify::LookupProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
            })
            .collect(),
        narrowed_out: Vec::new(),
        narrowed_out_provenance: Vec::new(),
        ledger: None,
    }
}

/// A verdict is refused for a candidate an import has claimed. The claim and
/// the check share the folder-state commit lock, so by the time a claim
/// returns there is no interval left in which a verdict can be stored for a
/// candidate the user has already committed to importing — and a verdict that
/// did land would describe files the import is in the middle of consuming.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
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
        metadata: blank_metadata_for_dir(&dir),
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

/// Entering Lookup for an answered candidate starts nothing: no run, no
/// request, no event. Its stored verdict already supplies its identify state.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn explicit_lookup_for_an_answered_candidate_starts_nothing() {
    let fixture = Fixture::new("resume-answered").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;

    // Nothing is routed and nothing is seeded: any lookup would 404 its way
    // to a different state than the stored one.
    let verdict = multi_match_verdict(&["mb-resume-1", "mb-resume-2"], "rg-resume-1");
    let wrote = fixture
        .import
        .save_candidate_verdict_if_current(
            &dir.to_string_lossy(),
            IdentifyRunId::for_test(1),
            &NewImportCandidateVerdict {
                candidate: crate::import::CandidateAsRead {
                    content_hash: fixture.content_hash(&dir),
                    file_edit_revision: 0,
                    metadata_revision: 0,
                },
                folder_path: dir.to_string_lossy().into_owned(),
                verdict,
                signals: settled_signals(fixture.probed_durations(&dir)),
                metadata: blank_metadata_for_dir(&dir),
            },
        )
        .await
        .unwrap();
    assert!(wrote, "the seeded verdict lands");
    let mut events = fixture.import.subscribe_events();

    fixture.start_explicit_lookup(&dir);

    // The Lookup verdict check is a detached task; a run it wrongly
    // started would broadcast `IdentifyStateChanged` within this window.
    let started = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let ImportEvent::IdentifyStateChanged { .. } =
                events.recv().await.expect("bus stays open")
            {
                return;
            }
        }
    })
    .await;
    assert!(
        started.is_err(),
        "explicit Lookup for an answered candidate started a run"
    );
    assert!(
        fixture.provider.requests().is_empty(),
        "explicit Lookup for an answered candidate reached the wire: {:?}",
        fixture.provider.requests()
    );
}

/// A stale pane can still render Lookup as idle after the command has already
/// started its run. Repeating the entry command must leave that run registered:
/// `identify.start` supersedes, so starting again would cancel the work already
/// in flight and replace its run id.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn explicit_lookup_during_an_active_run_keeps_the_existing_run() {
    let fixture = Fixture::new("explicit-keeps-active-run").await;
    let dir = fixture.disc_id_candidate("Candidate");
    let key = dir.to_string_lossy().into_owned();
    fixture.provider.route("/discid/", 200, "{}");
    fixture.provider.hold("/discid/");
    fixture.scan(1).await;
    let mut events = fixture.import.subscribe_events();

    fixture.start_explicit_lookup_and_await_run(&dir).await;
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    let first_run = loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("the active run broadcasts its state")
            .expect("the import event bus remains open");
        if let ImportEvent::IdentifyStateChanged {
            candidate_key, run, ..
        } = event
        {
            if candidate_key == key {
                break run;
            }
        }
    };

    // The caller still sees Idle and repeats the ordinary entry command.
    fixture.start_explicit_lookup(&dir);

    let replacement = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let ImportEvent::IdentifyStateChanged {
                candidate_key, run, ..
            } = events
                .recv()
                .await
                .expect("the import event bus remains open")
            {
                if candidate_key == key && run != first_run {
                    return run;
                }
            }
        }
    })
    .await;
    fixture.provider.release();

    assert!(
        replacement.is_err(),
        "the repeated entry command replaced the active identify run"
    );
    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        1,
        "the repeated entry command started another provider lookup"
    );
    assert!(
        fixture.import.is_identifying(&key),
        "the original identify run remains registered"
    );
}

/// An ending ends the run it names and nothing else. A run's own terminal
/// state has already left `running` for `saving`, so an `Idle` behind it
/// cannot blank the answer waiting to be written — while a mid-run `Idle`,
/// which is the run being abandoned, empties the key.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn an_ending_ends_the_run_it_names_and_not_the_answer_being_saved() {
    let fixture = Fixture::new("teardown-keeps-state").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    let not_in_library =
        |result: &MetadataResult| crate::db::LibraryStatus::absent(&result.release_id);
    let found =
        multi_match_verdict(&["mb-teardown-1"], "rg-teardown-1").resume_state(&not_in_library, Default::default());
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
    let Some(IdentifyState::Found { matches, .. }) =
        &runtime.as_ref().and_then(|runtime| runtime.saving.clone())
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
        context: crate::identify::state::SignalsContext {
            providers: Vec::new(),
            artwork: crate::signals::ArtworkScan::Absent,
            disc: Default::default(),
            barcode: Default::default(),
            catalog: Default::default(),
            text: Default::default(),
            text_settled: false,
            track_count: 0,
        },
    };
    fixture.import.emit_event_for_test(changed(2, triangulating));
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
        .finish_identification_save(&key, crate::identify::IdentifyRunId::for_test(1));
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

/// Re-run on a candidate whose driver is gone starts a fresh interactive run
/// instead of no-op'ing — the stored answer is what a re-run exists to
/// replace, so it is not consulted.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_rerun_with_no_driver_runs_identification_again() {
    let fixture = Fixture::new("rerun-no-driver").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    fixture
        .store_settled_verdict(&dir, "mb-rerun-1", "rg-rerun-1", probed)
        .await;
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-rerun-2", "rg-rerun-2", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-rerun-2?",
        200,
        release_json("mb-rerun-2", "rg-rerun-2", &[probed, 0]),
    );

    fixture
        .sweep
        .rerun_for_explicit_lookup(dir.to_string_lossy().into_owned());

    wait_for_request(&fixture.provider, "/discid/", 1).await;
}

// ── 11. Re-stating a file decision changes nothing ──────────────────────────

/// The disc menu and the role picker fire on every selection, including of
/// the item already in force. A decision that re-states what is already true
/// writes nothing — above all it does not clear the stored verdict, which
/// would re-identify a folder whose shape did not change and blank the pane
/// over it.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn restating_a_file_decision_changes_nothing() {
    let fixture = Fixture::new("edit-noop").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    fixture
        .archive("mb-noop-1", "rg-noop-1", &[500, 500])
        .await;
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
