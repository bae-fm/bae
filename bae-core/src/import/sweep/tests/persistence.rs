#[test]
fn duplicate_content_hashes_share_one_identify_job() {
    let first = synthetic_candidate("/first", 321);
    let second = synthetic_candidate("/second", 321);
    assert_eq!(first.files.content_hash(), second.files.content_hash());

    let planned = plan(vec![first.clone().into(), second.clone().into()], &HashMap::new(), 2);
    assert_eq!(planned.identify.len(), 1);
    assert_eq!(planned.identify[0].candidates.len(), 2);
    assert_eq!(planned.identified, 0);

    let stored = HashMap::from([(
        first.files.content_hash(),
        row_with_verdict(&first, TerminalVerdict::NotFoundAnywhere),
    )]);
    let planned = plan(vec![first.into(), second.into()], &stored, 2);
    assert!(planned.identify.is_empty());
    assert_eq!(planned.identified, 2);
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
        file_edits: Default::default(),
        metadata_provenance: None,
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
            .map(|_| crate::identify::ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
            })
            .collect(),
        matched_barcode: None,
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
        content_hash: fixture.content_hash(&dir),
        folder_path: key.clone(),
        verdict: multi_match_verdict(&["mb-claimed-1"], "rg-claimed-1"),
        signals: settled_signals(fixture.probed_durations(&dir)),
        expected_edit_revision: 0,
        expected_metadata_revision: 0,
        metadata: blank_metadata_for_dir(&dir),
    };

    assert!(
        fixture
            .import
            .save_candidate_verdict_if_current(&key, &row())
            .await
            .unwrap(),
        "an unclaimed candidate still takes its verdict"
    );

    fixture.import.claim_candidate_for_import(&key).await;

    assert!(
        !fixture
            .import
            .save_candidate_verdict_if_current(&key, &row())
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
            &NewImportCandidateVerdict {
                content_hash: fixture.content_hash(&dir),
                folder_path: dir.to_string_lossy().into_owned(),
                verdict,
                signals: settled_signals(fixture.probed_durations(&dir)),
                expected_edit_revision: 0,
                expected_metadata_revision: 0,
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
        fixture.identify.is_running(&key),
        "the original identify run remains registered"
    );
}

/// A driver being torn down after settling broadcasts `Idle` on its way out —
/// the sweep cancels its own drivers once they settle. The recorded runtime
/// keeps the terminal state: the candidate's answer doesn't stop being its
/// answer because the machinery that produced it exited. A genuine mid-run
/// cancel still resets.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_settled_runs_teardown_does_not_blank_its_recorded_state() {
    let fixture = Fixture::new("teardown-keeps-state").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    let not_in_library = |result: &MetadataResult| crate::db::LibraryStatus {
        release_id: result.release_id.clone(),
        release_in_library: false,
        album_in_library: false,
        album_title: None,
        album_id: None,
    };
    let found =
        multi_match_verdict(&["mb-teardown-1"], "rg-teardown-1").resume_state(&not_in_library);
    let changed = |state: IdentifyState| ImportEvent::IdentifyStateChanged {
        candidate_key: key.clone(),
        run: crate::identify::IdentifyRunId::for_test(0),
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

    fixture.import.emit_event_for_test(changed(found));
    changes.recv().await.expect("runtime changes stay open");
    fixture
        .import
        .emit_event_for_test(changed(IdentifyState::Idle));
    fixture.import.emit_event_for_test(marker());
    recorded(changes.recv().await);
    let Ok(Some(ImportCandidateSnapshot::Folder { runtime, .. })) =
        fixture.import.get_candidate(&key).await
    else {
        panic!("the scanned candidate is readable");
    };
    let Some(IdentifyState::Found { matches, .. }) = &runtime
        .as_ref()
        .and_then(|runtime| runtime.identify.clone())
        .and_then(crate::import::CandidateIdentifyRuntime::into_state)
    else {
        panic!("a terminal state survives its driver's teardown, got {runtime:?}");
    };
    assert_eq!(matches[0].release_id, "mb-teardown-1");

    // A mid-run cancel is a different fact and still resets: the run was
    // abandoned, not answered.
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
            track_count: 0,
        },
    };
    fixture.import.emit_event_for_test(changed(triangulating));
    changes.recv().await.expect("runtime changes stay open");
    fixture
        .import
        .emit_event_for_test(changed(IdentifyState::Idle));
    changes.recv().await.expect("runtime changes stay open");
    let Ok(Some(ImportCandidateSnapshot::Folder { runtime, .. })) =
        fixture.import.get_candidate(&key).await
    else {
        panic!("the scanned candidate is readable");
    };
    assert!(
        runtime.is_none(),
        "a cancelled mid-run state leaves nothing behind, got {runtime:?}"
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
    let dir = fixture.root.join("Album");
    std::fs::create_dir_all(&dir).unwrap();
    for name in [
        "Test Album.cue",
        "Test Album.flac",
        "02 Test Artist - Track Two (White Noise).flac",
        "03 Test Artist - Track Three (Brown Noise).flac",
    ] {
        std::fs::copy(
            Path::new("tests/fixtures/cue_flac").join(name),
            dir.join(name),
        )
        .unwrap();
    }
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

// ── 12. A stored pick is what the pane reads back ──────────────────────────

/// Deciding an identity persists it and the pane reads it back — the whole of
/// "resume" — with the provider gone. A settled single match wrote the same
/// record, so a Ready candidate reads identically.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_reads_back_as_the_same_answer() {
    let fixture = Fixture::new("pick-answer").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    // The settled verdict wrote the pick; nothing is routed, so everything
    // below is served from what identification archived.
    fixture
        .archive("mb-answer-1", "rg-answer-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1", probed)
        .await;

    let resumed = fixture.pane(&dir).await.expect("the candidate reads back");
    let release = resumed.release.expect("a settled single match is a decision");
    assert_eq!(release.release_id, "mb-answer-1");
    assert_eq!(release.tracks.len(), 2);
    assert_eq!(
        resumed.file_evidence,
        vec![crate::import::FileEvidence {
            signal: crate::import::EvidenceSignal::DiscId,
            value: SEEDED_DISC_ID.to_string(),
            file_id: SEEDED_DISC_ID_FILE.to_string(),
        }],
        "the chip says which signal turned the release up, on the file it came from"
    );

    // The row carries the same decision for the sidebar's resume trigger.
    let picked = queue_row(&fixture, &key)
        .await
        .metadata_provenance
        .expect("the row carries the decision");
    assert_eq!(
        picked,
        crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-answer-1".to_string(),
            partners: vec![],
        }
    );

    // A person deciding File Tags replaces the record, and the pane reads the
    // folder's own files instead of a release.
    fixture
        .import
        .select_candidate_metadata_provenance(key.clone(), crate::import::MetadataProvenance::FileTags)
        .await
        .expect("deciding File Tags succeeds");
    let resumed = fixture.pane(&dir).await.expect("the candidate reads back");
    assert!(resumed.release.is_none(), "File Tags names no external release");
    assert!(
        !resumed.metadata_draft.is_blank(),
        "and still draws a form, seeded from the folder's own tags"
    );
    assert_eq!(
        resumed.file_evidence,
        vec![crate::import::FileEvidence {
            signal: crate::import::EvidenceSignal::DiscId,
            value: SEEDED_DISC_ID.to_string(),
            file_id: SEEDED_DISC_ID_FILE.to_string(),
        }],
        "the extracted Disc ID still names its source file without a release pick"
    );
    assert!(
        fixture.provider.requests().is_empty(),
        "every answer came from the archive: {:?}",
        fixture.provider.requests()
    );
}

/// The sidebar row leads with the identity the candidate is settled on. A
/// manual search settles it on a release identification never named, and the
/// pick is the only record of that — a row reading the stored verdict alone
/// goes on showing the folder name and a placeholder while the pane shows the
/// release, with nothing to move it off.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_picked_release_is_what_the_row_leads_with() {
    let fixture = Fixture::new("pick-row").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    // Identification settled on one release; the user searched and picked
    // another, whose documents the search archived.
    fixture
        .archive("mb-answer-1", "rg-answer-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1", probed)
        .await;
    // The picked release is one identification never fetched, which is what a
    // manual search result is: its documents are archived by the pick itself.
    fixture.provider.route(
        "/release/mb-picked-1?",
        200,
        titled_release_json(
            "mb-picked-1",
            "rg-picked-1",
            "Picked Album Title",
            "Picked Artist Name",
        ),
    );

    // Read the queue on the event the surfaces refresh on, not after the pick
    // has finished settling: the row has to be right the moment it lands.
    let mut events = fixture.import.subscribe_events();
    let picking = {
        let import = fixture.import.clone();
        let key = key.clone();
        tokio::spawn(async move {
            import
                .select_candidate_metadata_provenance(
                    key,
                    crate::import::MetadataProvenance::ExternalRelease {
                        source: crate::import::MetadataSource::MusicBrainz,
                        release_id: "mb-picked-1".to_string(),
                        partners: vec![],
                    },
                )
                .await
        })
    };
    loop {
        let event = events.recv().await.expect("the pick raises an event");
        if matches!(
            &event,
            crate::import::ImportEvent::Scan(super::super::handle::ScanEvent::CandidateMetadataChanged {
                candidate_key,
            }) if *candidate_key == key
        ) {
            break;
        }
    }

    picking
        .await
        .expect("the pick task runs")
        .expect("picking the searched release succeeds");
    assert_eq!(
        fixture
            .pane(&dir)
            .await
            .expect("the candidate reads back")
            .file_evidence,
        vec![crate::import::FileEvidence {
            signal: crate::import::EvidenceSignal::DiscId,
            value: SEEDED_DISC_ID.to_string(),
            file_id: SEEDED_DISC_ID_FILE.to_string(),
        }],
        "a manual pick does not erase the candidate's extracted signal source"
    );
    let matched = queue_row(&fixture, &key)
        .await
        .matched
        .expect("the row leads with the release the pick settled it on");
    assert_eq!(matched.release_id, "mb-picked-1");
    assert_eq!(matched.title, "Picked Album Title");
    assert_eq!(matched.artist.as_deref(), Some("Picked Artist Name"));
    let thumbnail = matched
        .cover_thumbnail_url
        .as_deref()
        .expect("the picked release's document says the archive holds a front image");
    assert!(
        thumbnail.ends_with("/release/mb-picked-1/front-250"),
        "the row's thumbnail is the archive's address for the picked release's \
         front image, got {thumbnail}"
    );
}

/// A pick is written with the candidate, so the answer, the row's resume
/// record and the identity a bulk import would commit all come back naming the
/// same pressing after a restart — while the evidence keeps saying what
/// identified it, here a disc ID that matched that one release.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_reads_back_as_the_identity_it_commits() {
    let fixture = Fixture::new("pick-reads-back").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    fixture
        .archive("mb-answer-1", "rg-answer-1", &[probed, 0])
        .await;
    fixture
        .store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1", probed)
        .await;

    let pick = crate::import::MetadataProvenance::ExternalRelease {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-answer-1".to_string(),
        partners: vec![],
    };
    fixture
        .import
        .select_candidate_metadata_provenance(key.clone(), pick.clone())
        .await
        .expect("picking the release succeeds");

    // Extracted signal provenance belongs to the candidate files, so picking a
    // release does not change it.
    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    assert_eq!(
        pane.file_evidence,
        vec![crate::import::FileEvidence {
            signal: crate::import::EvidenceSignal::DiscId,
            value: SEEDED_DISC_ID.to_string(),
            file_id: SEEDED_DISC_ID_FILE.to_string(),
        }]
    );

    // The row carries the draft and provenance the pane and bulk import consume.
    let row = queue_row(&fixture, &key).await;
    assert_eq!(row.metadata_provenance, Some(pick));
}

/// Once a run's verdict lands in its row, the recorded runtime state clears:
/// the row owns the answer, and Lookup serves it from there. Nothing in memory
/// is left to shadow a row that later changes.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_stored_verdict_takes_over_from_the_recorded_runtime_state() {
    let fixture = Fixture::new("verdict-takes-over").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-own-1", "rg-own-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-own-1?",
        200,
        release_json("mb-own-1", "rg-own-1", &[probed, 0]),
    );
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();

    fixture.sweep_once().await;

    assert!(
        fixture.identified_for(&dir).await.is_some(),
        "the candidate really was identified"
    );
    let pane = fixture
        .pane(&dir)
        .await
        .expect("the identified candidate reads back");
    assert_eq!(
        pane.metadata_draft.album_title, "Album",
        "the automatic release choice seeds the editable metadata draft"
    );
    // The write's event reaches the recorder through the bus; poll for it.
    let cleared = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(Some(ImportCandidateSnapshot::Folder { runtime, .. })) =
                fixture.import.get_candidate(&key).await
            {
                if runtime.is_none() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        cleared.is_ok(),
        "the recorded terminal state clears once its verdict is stored"
    );
}

/// The list's row for one candidate, over every tab — which tab it lands in is
/// not what these tests are about.
async fn queue_row(fixture: &Fixture, key: &str) -> crate::import::TriageRow {
    for tab in [
        crate::import::TriageTab::Pending,
        crate::import::TriageTab::Done,
        crate::import::TriageTab::Skipped,
    ] {
        let view = crate::import::ImportListView {
            tab,
            ..crate::import::ImportListView::default()
        };
        let projection = fixture
            .import
            .wait_for_list(view, |_| true)
            .await;
        let row = projection
            .windows
            .iter()
            .flat_map(|window| &window.items)
            .find_map(|item| match item {
                crate::import::ImportListItem::Candidate { row, .. }
                    if row.candidate_key == key =>
                {
                    Some(row.clone())
                }
                _ => None,
            });
        if let Some(row) = row {
            return row;
        }
    }
    panic!("the row is in the queue");
}

/// A verdict lands with everything identification learned: what each of the
/// folder's audio units plays for, and the signals it settled on. The pane
/// reads those back instead of opening the folder or extracting again.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_stored_verdict_carries_its_durations_and_signals() {
    let fixture = Fixture::new("verdict-carries-signals").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json(
            "mb-signals-1",
            "rg-signals-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.provider.route(
        "/release/mb-signals-1?",
        200,
        release_json(
            "mb-signals-1",
            "rg-signals-1",
            &[probed / 2, probed - probed / 2],
        ),
    );
    fixture.scan(1).await;

    fixture.sweep_once().await;

    let row = fixture.stored_for(&dir).await.expect("a verdict is stored");
    assert_eq!(
        row.identify
            .as_ref()
            .expect("the verdict is stored")
            .probed_total_duration_ms,
        probed
    );
    let signals = row.signals.expect("the settled signals are stored");
    assert!(
        matches!(
            signals.disc_id,
            crate::signals::DiscIdSignal::Computed { .. }
        ),
        "the disc ID the lookup used reads back: {:?}",
        signals.disc_id
    );
    assert!(
        signals.durations.units.is_empty(),
        "source durations are derived from the candidate scan, not duplicated in identify state"
    );
}

/// A terminal state without its settled signals cannot be committed, and the
/// finalizer reports that failure instead of pretending the candidate remains
/// queued for another pass.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_verdict_with_no_signals_reports_a_finalization_failure() {
    let fixture = Fixture::new("verdict-without-signals").await;
    let candidate = synthetic_candidate("/missing-signals", 321);
    let entry = InFlight {
        job: IdentifyJob {
            identity: candidate_identity(&candidate.clone().into()),
            candidates: vec![candidate.into()],
        },
        run: IdentifyRunId::for_test(1),
        signals: None,
        expected_metadata_revision: 0,
    };

    let outcome = finish_candidate(
        &fixture.context(),
        &entry,
        TerminalVerdict::NotFoundAnywhere.resume_state(&|_| {
            unreachable!("a no-match verdict names no release")
        }),
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(outcome, FinishCandidateOutcome::Failed { .. }));
    assert!(fixture.stored().await.is_empty());
}
