#[test]
fn a_verdict_that_no_longer_decodes_is_rejected() {
    let stale = synthetic_candidate("/b", 222);
    let row = row_with_verdict(
        &stale,
        r#"{"ShapeFromAnOlderBuild":{"whatever":1}}"#.to_string(),
    );
    assert!(decode(&row).is_err());
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_malformed_verdict_on_a_late_candidate_aborts_without_panicking() {
    let fixture = Fixture::new("malformed-late-row").await;
    let running = fixture.disc_id_candidate("Running");
    let probed = fixture.probed_total_ms(&running);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-malformed-running", "rg-malformed-running", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-malformed-running?",
        200,
        release_json("mb-malformed-running", "rg-malformed-running", &[probed, 0]),
    );
    fixture.scan(1).await;

    fixture.provider.hold("/discid/");

    let context = fixture.context();
    let token = CancellationToken::new();
    let pass = tokio::spawn(async move { run_pass_for_test(&context, &token).await });
    wait_for_request(&fixture.provider, "/discid/", 1).await;
    let late = fixture.disc_id_candidate("Late");
    std::fs::write(late.join("late-playlist.m3u"), "late identity").unwrap();
    fixture
        .manager
        .save_import_candidate_verdict(&NewImportCandidateVerdict {
            content_hash: fixture.content_hash(&late),
            folder_path: late.to_string_lossy().into_owned(),
            verdict: r#"{"ShapeFromAnOlderBuild":{"whatever":1}}"#.to_string(),
            probed_total_duration_ms: 0,
            expected_edit_revision: 0,
            identity_pick: None,
        })
        .await
        .unwrap();
    fixture
        .import
        .refresh_watched_folder(fixture.root.to_string_lossy().into_owned())
        .await
        .unwrap();
    fixture.provider.release();
    tokio::time::timeout(Duration::from_secs(10), pass)
        .await
        .expect("malformed late row aborts the pass")
        .expect("malformed late row is handled without panic");
    assert!(!fixture
        .identify
        .is_running(running.to_string_lossy().as_ref()));
}

#[test]
fn duplicate_content_hashes_share_one_identify_job() {
    let first = synthetic_candidate("/first", 321);
    let second = synthetic_candidate("/second", 321);
    assert_eq!(first.files.content_hash(), second.files.content_hash());

    let planned = plan(vec![first.clone(), second.clone()], &HashMap::new(), 2);
    assert_eq!(planned.identify.len(), 1);
    assert_eq!(planned.identify[0].candidates.len(), 2);
    assert_eq!(planned.identified, 0);

    let stored = HashMap::from([(
        first.files.content_hash(),
        row_with_verdict(
            &first,
            serde_json::to_string(&TerminalVerdict::NotFoundAnywhere).unwrap(),
        ),
    )]);
    let planned = plan(vec![first, second], &stored, 2);
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
                ),
                role: FileRole::Audio,
            }],
            format_label: "FLAC".to_string(),
        },
        watched_folder_path: "/".to_string(),
        scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: path.trim_start_matches('/').to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

fn row_with_verdict(candidate: &FolderCandidate, verdict: String) -> DbImportCandidateState {
    DbImportCandidateState {
        content_hash: candidate.files.content_hash(),
        folder_path: candidate.path.to_string_lossy().into_owned(),
        identify: Some(crate::db::DbCandidateIdentifyResult {
            verdict,
            probed_total_duration_ms: 0,
            identified_at: fixed_now(),
        }),
        file_edits: Default::default(),
        identity_pick: None,
    }
}

// ── 10. Selection resumes a stored verdict ──────────────────────────────────

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
                cover_art: None,
                source_group_id: Some(group_id.to_string()),
                source_tracks: None,
            })
            .collect(),
        track_count: 2,
        group: crate::identify::GroupKey {
            source: crate::import::MetadataSource::MusicBrainz,
            source_group_id: group_id.to_string(),
        },
        provenance: release_ids
            .iter()
            .map(|_| crate::identify::ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                matches_catalog: false,
            })
            .collect(),
    }
}

/// Selecting an answered candidate stands its stored verdict back up as the
/// identify state — every stored match, at `Interactive`, with the provider
/// gone. This is what makes clicking a "several matches" row show those
/// matches instantly instead of re-running the whole pipeline.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn selecting_an_answered_candidate_resumes_its_verdict_with_the_provider_gone() {
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
                verdict: serde_json::to_string(&verdict).unwrap(),
                probed_total_duration_ms: fixture.probed_total_ms(&dir) as i64,
                expected_edit_revision: 0,
                identity_pick: None,
            },
        )
        .await
        .unwrap();
    assert!(wrote, "the seeded verdict lands");
    let mut events = fixture.import.subscribe_events();

    fixture.select(&dir);

    let state = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match events.recv().await.expect("bus stays open") {
                ImportEvent::IdentifyStateChanged { state, .. } => return state,
                _ => continue,
            }
        }
    })
    .await
    .expect("the resumed state is broadcast");
    let IdentifyState::Found { matches, .. } = &state else {
        panic!("expected the stored Found back, got {state:?}");
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-resume-1", "mb-resume-2"],
        "every stored match is in the resumed state"
    );
    assert!(
        fixture.provider.requests().is_empty(),
        "resuming reached the wire for nothing: {:?}",
        fixture.provider.requests()
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
        toolbar: Vec::new(),
        state,
        priority: CallPriority::Background,
    };

    fixture.import.record_candidate_event(&changed(found));
    fixture
        .import
        .record_candidate_event(&changed(IdentifyState::Idle));
    let Some(ImportCandidateSnapshot::Folder { runtime, .. }) = fixture.import.get_candidate(&key)
    else {
        panic!("the scanned candidate is readable");
    };
    let IdentifyState::Found { matches, .. } = &runtime.identify_state else {
        panic!(
            "a terminal state survives its driver's teardown, got {:?}",
            runtime.identify_state
        );
    };
    assert_eq!(matches[0].release_id, "mb-teardown-1");

    // A mid-run cancel is a different fact and still resets: the run was
    // abandoned, not answered.
    let triangulating = IdentifyState::Triangulating {
        discid: crate::identify::DiscidProgress::Computing,
        barcode: crate::identify::BarcodeProgress::Scanning,
        context: crate::identify::state::SignalsContext {
            disc_id: crate::signals::DiscIdSignal::Absent { track_count: 0 },
            barcode_codes: Vec::new(),
            had_barcode_source: false,
            catalogs: Vec::new(),
            excluded: Default::default(),
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            discid_failure: None,
            barcode_failure: None,
            matched_barcode: None,
            track_count: 0,
        },
    };
    fixture
        .import
        .record_candidate_event(&changed(triangulating));
    fixture
        .import
        .record_candidate_event(&changed(IdentifyState::Idle));
    let Some(ImportCandidateSnapshot::Folder { runtime, .. }) = fixture.import.get_candidate(&key)
    else {
        panic!("the scanned candidate is readable");
    };
    assert!(
        matches!(runtime.identify_state, IdentifyState::Idle),
        "a cancelled mid-run state resets as before, got {:?}",
        runtime.identify_state
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
        .rerun_for_selection(dir.to_string_lossy().into_owned());

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

// ── 12. The pick command and the answer query serve one payload ─────────────

/// Deciding an identity persists it, the row carries it back, and reading it
/// — the whole of "resume" — returns the same seeded answer with the
/// provider gone. A settled single match wrote the same record, so a Ready
/// candidate answers identically.
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

    let resumed = fixture
        .import
        .candidate_answer(key.clone())
        .await
        .expect("the stored decision reads back")
        .expect("a settled single match is a decision");
    let crate::import::DecidedIdentity::Release {
        release_id,
        prefetch,
        ..
    } = &resumed
    else {
        panic!("expected the settled release back, got Unknown");
    };
    assert_eq!(release_id, "mb-answer-1");
    assert_eq!(prefetch.detail.tracks.len(), 2);
    // Identification settling on one match is a pick, so it claims the
    // pressing exactly as a click on that release would.
    assert_eq!(prefetch.claim.level, crate::import::ClaimLevel::Exact);

    // The row carries the same decision for the sidebar's resume trigger.
    let queue = crate::import::triage::load(&fixture.import, &fixture.manager)
        .await
        .expect("the triage queue loads");
    let picked = queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .find_map(|entry| match entry {
            crate::import::triage::TriageEntry::Candidate(row) if row.candidate_key == key => {
                row.picked.clone()
            }
            _ => None,
        })
        .expect("the row carries the decision");
    assert_eq!(
        picked,
        crate::import::IdentityPick::Release {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "mb-answer-1".to_string(),
            claim: crate::import::ClaimLevel::Exact,
        }
    );

    // A person deciding Unknown replaces the record, and the query returns
    // exactly what the command did.
    let decided = fixture
        .import
        .pick_candidate_identity(key.clone(), crate::import::IdentityPick::Unknown)
        .await
        .expect("deciding Unknown succeeds");
    assert!(matches!(
        decided,
        crate::import::DecidedIdentity::Unknown { .. }
    ));
    let resumed = fixture
        .import
        .candidate_answer(key)
        .await
        .expect("the replaced decision reads back")
        .expect("Unknown is a decision");
    assert!(matches!(
        resumed,
        crate::import::DecidedIdentity::Unknown { .. }
    ));
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
        "/release/mb-picked-1",
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
                .pick_candidate_identity(
                    key,
                    crate::import::IdentityPick::Release {
                        source: crate::import::MetadataSource::MusicBrainz,
                        release_id: "mb-picked-1".to_string(),
                        claim: crate::import::ClaimLevel::Exact,
                    },
                )
                .await
        })
    };
    loop {
        let event = events.recv().await.expect("the pick raises an event");
        if matches!(
            &event,
            crate::import::ImportEvent::Scan(super::super::handle::ScanEvent::CandidateIdentityPicked {
                candidate_key,
            }) if *candidate_key == key
        ) {
            break;
        }
    }

    let queue = crate::import::triage::load(&fixture.import, &fixture.manager)
        .await
        .expect("the triage queue loads");
    picking
        .await
        .expect("the pick task runs")
        .expect("picking the searched release succeeds");
    let matched = queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .find_map(|entry| match entry {
            crate::import::triage::TriageEntry::Candidate(row) if row.candidate_key == key => {
                row.matched.clone()
            }
            _ => None,
        })
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

/// Lowering the claim is a decision like any other: it is written with the
/// pick, so the answer, the row's resume record and the identity a bulk import
/// would commit all come back at the album level after a restart. The evidence
/// here is a disc ID that matched one release — the sharpest there is — and it
/// still does not move the claim back.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_lowered_claim_reads_back_lowered() {
    let fixture = Fixture::new("pick-lowered").await;
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

    let lowered = crate::import::IdentityPick::Release {
        source: crate::import::MetadataSource::MusicBrainz,
        release_id: "mb-answer-1".to_string(),
        claim: crate::import::ClaimLevel::Approximate,
    };
    let decided = fixture
        .import
        .pick_candidate_identity(key.clone(), lowered.clone())
        .await
        .expect("lowering the claim succeeds");
    let crate::import::DecidedIdentity::Release { prefetch, .. } = &decided else {
        panic!("expected the picked release back, got Unknown");
    };
    assert_eq!(prefetch.claim.level, crate::import::ClaimLevel::Approximate);
    // The evidence is untouched: it says what identified the release, not what
    // the user claims about it.
    assert_eq!(
        prefetch.claim.evidence,
        crate::import::ClaimEvidence::DiscIdAlone
    );

    // The query serves what the command did, which is what a restart reads.
    let resumed = fixture
        .import
        .candidate_answer(key.clone())
        .await
        .expect("the lowered decision reads back")
        .expect("a lowered claim is still a decision");
    let crate::import::DecidedIdentity::Release { prefetch, .. } = &resumed else {
        panic!("expected the picked release back, got Unknown");
    };
    assert_eq!(prefetch.claim.level, crate::import::ClaimLevel::Approximate);

    // And the row carries it both ways: the pick the pane reopens on, and the
    // identity a bulk import of this row would commit.
    let queue = crate::import::triage::load(&fixture.import, &fixture.manager)
        .await
        .expect("the triage queue loads");
    let row = queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .find_map(|entry| match entry {
            crate::import::triage::TriageEntry::Candidate(row) if row.candidate_key == key => {
                Some(row.clone())
            }
            _ => None,
        })
        .expect("the row is in the queue");
    assert_eq!(row.picked, Some(lowered));
    assert_eq!(
        row.claim,
        Some(crate::import::IdentityChoice::Approximate {
            release_ref: crate::import::MetadataRef::new(
                "mb-answer-1",
                crate::import::MetadataSource::MusicBrainz
            ),
        })
    );
}
