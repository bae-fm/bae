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
        TerminalVerdict::NotFoundAnywhere.resume_state(None, &LookupChoices::default(), &|_| {
            unreachable!("a no-match verdict names no release")
        }),
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(outcome, FinishCandidateOutcome::Failed { .. }));
    assert!(fixture.stored().await.is_empty());
}

/// Coven commits a write on its writer thread whether or not the future that
/// asked for it survives, and the candidate runtime holds a run's answer until
/// its write says the answer has landed. So the write has to end that wait
/// itself, not its caller: a task torn down the instant it has asked — the
/// app shutting down over a settle in flight — must still leave the key
/// stating what happened, or the row reads as a commit still pending, for
/// good, and offers the candidate nothing but Skip.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_verdict_write_ends_its_own_save_when_its_caller_is_torn_down() {
    use std::future::Future;

    let fixture = Fixture::new("torn-down-writer").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();
    let run = IdentifyRunId::for_test(1);
    let row = NewImportCandidateVerdict {
        candidate: crate::import::CandidateAsRead {
            content_hash: fixture.content_hash(&dir),
            file_edit_revision: 0,
            metadata_revision: 0,
        },
        folder_path: key.clone(),
        verdict: multi_match_verdict(&["mb-torn-1", "mb-torn-2"], "rg-torn-1"),
        signals: settled_signals(fixture.probed_durations(&dir)),
        metadata: blank_metadata_for_dir(&dir),
    };

    // The run's terminal state is what puts the key on a pending save.
    let not_in_library =
        |result: &MetadataResult| crate::db::LibraryStatus::absent(&result.release_id);
    fixture
        .import
        .emit_event_for_test(ImportEvent::IdentifyStateChanged {
            candidate_key: key.clone(),
            run,
            state: row
                .verdict
                .clone()
                .resume_state(None, &LookupChoices::default(), &not_in_library),
            priority: CallPriority::Background,
        });
    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture
            .import
            .candidate_runtime(&key)
            .is_none_or(|runtime| runtime.saving.is_none())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the terminal state is recorded as a pending save");

    // One poll asks for the write; dropping the future at the end of the block
    // is the caller being torn down.
    {
        let mut save = std::pin::pin!(fixture.import.save_candidate_verdict_if_current(
            &key,
            run,
            &row
        ));
        let first_poll =
            std::future::poll_fn(|cx| std::task::Poll::Ready(save.as_mut().poll(cx))).await;
        assert!(
            first_poll.is_pending(),
            "the first poll asks for the write and waits on it"
        );
    }

    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture
            .import
            .candidate_runtime(&key)
            .is_some_and(|runtime| runtime.saving.is_some())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the write ends the save it ran for");
    assert!(
        fixture.import.stored_verdict(&key).await.unwrap().is_some(),
        "the verdict landed"
    );
}
