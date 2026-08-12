/// The row leads with the identity the candidate is settled on. A manual search
/// settles it on a release identification never named — nothing else refreshes
/// the row, so a projection that reads the verdict alone leaves the sidebar
/// showing the folder name and a placeholder while the pane shows the release.
#[test]
fn a_settled_pick_is_what_the_row_leads_with() {
    let snapshot = snapshot_of(vec![
        candidate("Release 01", false, false),
        candidate("Release 02", false, false),
    ]);
    let answers = answers_for(
        &snapshot,
        vec![
            // Nothing matched this folder; the user searched and picked.
            Some(answer(
                TerminalVerdict::NotFoundAnywhere,
                QueueClassification::NeedsYou(NeedsYou::NoMatch),
            )),
            // Identification found one, and the user overruled it.
            Some(answer(
                found(vec![result("rel-found")]),
                QueueClassification::Ready,
            )),
        ],
    );
    let picks = picks_for(
        &snapshot,
        vec![
            Some(picked_release("rel-picked")),
            Some(picked_release("rel-other")),
        ],
    );

    let queue = project(snapshot, &answers, &picks);
    let rows = candidate_rows(&queue);

    for (row, release_id) in rows.iter().zip(["rel-picked", "rel-other"]) {
        let matched = row
            .matched
            .as_ref()
            .expect("the row leads with the release the pick settled it on");
        assert_eq!(matched.release_id, release_id);
        assert_eq!(matched.title, "Picked Album Title");
        assert_eq!(matched.artist.as_deref(), Some("Picked Artist Name"));
        assert_eq!(
            matched.cover_thumbnail_url.as_deref(),
            Some("https://example.test/picked-thumb.jpg")
        );
    }
}

/// Reading the folder as its own tags settles it on no release, so the row
/// leads with the folder name — not with the match the user just rejected.
#[test]
fn reading_a_folder_as_its_own_tags_leaves_the_row_leading_with_the_folder() {
    let snapshot = snapshot_of(vec![candidate("Release 01", false, false)]);
    let answers = answers_for(
        &snapshot,
        vec![Some(answer(
            found(vec![result("rel-found")]),
            QueueClassification::Ready,
        ))],
    );
    let picks = picks_for(
        &snapshot,
        vec![Some(Picked {
            pick: crate::import::IdentityPick::Unknown,
            release: None,
        })],
    );

    let queue = project(snapshot, &answers, &picks);

    assert!(candidate_rows(&queue)[0].matched.is_none());
}

#[test]
fn a_tentative_candidate_hidden_by_a_boundary_is_not_a_row_or_count() {
    let mut snapshot = snapshot_of(vec![candidate("Release 01", false, false)]);
    snapshot.folder_candidates[0].actionable = false;

    let queue = project(snapshot, &HashMap::new(), &HashMap::new());

    assert_eq!(queue.counts, TriageTabCounts::default());
    assert!(queue.sections.is_empty());
}

fn signals_context() -> crate::identify::state::SignalsContext {
    crate::identify::state::SignalsContext {
        disc_id: crate::signals::DiscIdSignal::Absent { track_count: 11 },
        barcode_codes: vec![],
        had_barcode_source: false,
        catalogs: vec![],
        excluded: Default::default(),
        discid_results: vec![],
        barcode_results: vec![],
        discid_failure: None,
        barcode_failure: None,
        matched_barcode: None,
        track_count: 11,
    }
}

// ── 1. Tab membership is total and exclusive ────────────────────────────────

#[test]
fn ready_and_needs_you_share_the_pending_tab() {
    let ready = place(
        false,
        false,
        None,
        &CandidateAnswer::Classified(QueueClassification::Ready),
    );
    let needs_you = place(
        false,
        false,
        None,
        &CandidateAnswer::Classified(QueueClassification::NeedsYou(
            NeedsYou::SignalsConflict,
        )),
    );

    assert_eq!(ready.tab(), TriageTab::Pending);
    assert_eq!(needs_you.tab(), TriageTab::Pending);
}

/// Every combination of what is known, import status, `skipped` and `is_added`
/// lands in exactly one tab.
///
/// The expectation is written as three independent predicates rather than as a
/// second copy of `place`'s `if` chain: each is guarded by `!done` / `!skipped`
/// so the precedence is stated once, declaratively, and a reordering of the
/// checks in `place` breaks it.
#[test]
fn tab_membership_is_total_and_exclusive() {
    let mut combinations = 0;
    for answer in every_answer() {
        for import_status in every_import_status() {
            for skipped in [false, true] {
                for is_added in [false, true] {
                    combinations += 1;
                    let placement = place(skipped, is_added, import_status.as_ref(), &answer);

                    let importing = matches!(
                        import_status,
                        Some(CandidateImportStatusSnapshot::Importing { .. })
                    );
                    let done = !importing && (is_added || import_status.is_some());
                    let is_skipped = !importing && !done && skipped;
                    let pending = importing || (!done && !skipped);

                    let expected: Vec<TriageTab> = [
                        (TriageTab::Pending, pending),
                        (TriageTab::Done, done),
                        (TriageTab::Skipped, is_skipped),
                    ]
                    .into_iter()
                    .filter_map(|(tab, holds)| holds.then_some(tab))
                    .collect();

                    assert_eq!(
                        expected.len(),
                        1,
                        "the three tab rules must hold for exactly one tab \
                         (answer {answer:?}, import status {import_status:?}, \
                         skipped {skipped}, is_added {is_added})"
                    );
                    assert_eq!(
                        placement.tab(),
                        expected[0],
                        "wrong tab for answer {answer:?}, import status \
                         {import_status:?}, skipped {skipped}, is_added {is_added}"
                    );

                    // Under Needs you the row always carries a reason, and its
                    // group is the one that reason batches under — never a
                    // group without a reason or a reason without a group.
                    if let TriagePlacement::NeedsYou { group, reason } = &placement {
                        assert_eq!(*group, NeedsYouGroup::of(reason));
                    }
                }
            }
        }
    }
    assert_eq!(combinations, 13 * 4 * 2 * 2);
}

/// A candidate nobody has classified yet is Needs you, in the still-identifying
/// group — not Ready. The mockup stacks that group under Ready; this is the
/// resolution of that, and the reasoning is on `place`. Its phase rides on the
/// row, so the three kinds of "no verdict" do not all render alike.
#[test]
fn a_candidate_with_no_verdict_is_needs_you_not_ready() {
    for phase in every_phase() {
        let placement = place(false, false, None, &CandidateAnswer::Unanswered(phase));
        assert_eq!(placement.tab(), TriageTab::Pending);
        assert_eq!(
            placement,
            TriagePlacement::NeedsYou {
                group: NeedsYouGroup::StillIdentifying,
                reason: NeedsYouReason::StillIdentifying { phase },
            }
        );
    }
}

/// The phase comes off the live identify state, and a run that *finished*
/// without a storable verdict is not the same as one still going. Every
/// terminal state reads `NoAnswer`: `TerminalVerdict::try_from` refusing it is
/// exactly why there is no stored verdict to classify.
#[test]
fn the_phase_tells_queued_running_and_finished_apart() {
    assert_eq!(
        IdentifyPhase::of(&IdentifyState::Idle),
        IdentifyPhase::Queued
    );
    assert_eq!(
        IdentifyPhase::of(&IdentifyState::Triangulating {
            discid: crate::identify::DiscidProgress::Computing,
            barcode: crate::identify::BarcodeProgress::Scanning,
            context: signals_context(),
        }),
        IdentifyPhase::Running
    );
    assert_eq!(
        IdentifyPhase::of(&IdentifyState::NotFoundAnywhere {
            context: signals_context()
        }),
        IdentifyPhase::NoAnswer
    );
    assert_eq!(
        IdentifyPhase::of(&IdentifyState::ManualOnly {
            track_count: 11,
            context: signals_context()
        }),
        IdentifyPhase::NoAnswer
    );
}

// ── 2. Done and Skipped outrank classification ──────────────────────────────

/// The test that fails if someone orders the checks the other way round: a
/// `Ready` candidate that has been imported is Done, and a `Ready` candidate
/// the user skipped is Skipped. Neither is Ready, so neither can be swept into
/// a bulk import a second time.
#[test]
fn done_and_skipped_outrank_classification() {
    let ready = CandidateAnswer::Classified(QueueClassification::Ready);
    let complete = CandidateImportStatusSnapshot::Complete {
        release_id: "rel-1".to_string(),
        album_id: "alb-1".to_string(),
    };

    // Import status beats Ready.
    assert_eq!(
        place(false, false, Some(&complete), &ready),
        TriagePlacement::Done
    );
    // `is_added` beats Ready.
    assert_eq!(place(false, true, None, &ready), TriagePlacement::Done);
    // Skipped beats Ready.
    assert_eq!(place(true, false, None, &ready), TriagePlacement::Skipped);
    // Done beats Skipped: an imported candidate is not awaiting triage, whether
    // or not it was skipped along the way.
    assert_eq!(
        place(true, false, Some(&complete), &ready),
        TriagePlacement::Done
    );
    assert_eq!(place(true, true, None, &ready), TriagePlacement::Done);

    // And the same two rules over a Needs-you classification, so neither is
    // passing only because Ready is the last arm.
    let conflicted =
        CandidateAnswer::Classified(QueueClassification::NeedsYou(NeedsYou::SignalsConflict));
    assert_eq!(
        place(false, false, Some(&complete), &conflicted),
        TriagePlacement::Done
    );
    assert_eq!(
        place(true, false, None, &conflicted),
        TriagePlacement::Skipped
    );
}

/// An import that has started is not Done until it finishes. Done is the tab
/// that says "this folder is in the library"; a folder whose files are still
/// being copied is not, and a row that claims otherwise while the pane shows a
/// percentage is the sidebar contradicting the pane about the same import.
#[test]
fn an_import_in_flight_is_importing_not_done() {
    let ready = CandidateAnswer::Classified(QueueClassification::Ready);
    let importing = CandidateImportStatusSnapshot::Importing {
        progress_percent: 0,
        step: None,
    };

    assert_eq!(
        place(false, false, Some(&importing), &ready),
        TriagePlacement::Importing
    );
    // The library check flips as soon as the release row lands, which is
    // before the import reports itself complete. The in-flight status is the
    // more recent fact about this candidate, so it wins.
    assert_eq!(
        place(false, true, Some(&importing), &ready),
        TriagePlacement::Importing
    );
    // A candidate skipped before the import was started is still being
    // imported now.
    assert_eq!(
        place(true, false, Some(&importing), &ready),
        TriagePlacement::Importing
    );
    // And an in-flight import is not asking the user anything, so it does not
    // land in a Needs-you group either.
    assert_eq!(
        place(false, false, Some(&importing), &ready).tab(),
        TriageTab::Pending
    );

    // The finished states still are Done.
    let complete = CandidateImportStatusSnapshot::Complete {
        release_id: "rel-1".to_string(),
        album_id: "alb-1".to_string(),
    };
    let failed = CandidateImportStatusSnapshot::Error {
        error: "boom".to_string(),
    };
    assert_eq!(
        place(false, false, Some(&complete), &ready),
        TriagePlacement::Done
    );
    assert_eq!(
        place(false, false, Some(&failed), &ready),
        TriagePlacement::Done
    );
}

// ── 3. Tab counts equal the rows in each tab ────────────────────────────────

/// The counts and the grouping come out of one pass, so they cannot drift.
/// Invalid folders are the one thing counted without a row — they have nothing
/// to triage — so they travel on the queue and Skipped is checked against the
/// rows plus them.
#[test]
fn tab_counts_equal_the_rows_in_each_tab() {
    let mut snapshot = snapshot_of(vec![
        candidate("ready-one", false, false),
        candidate("ready-two", false, false),
        candidate("needs-you", false, false),
        candidate("unanswered", false, false),
        candidate("skipped", true, false),
        candidate("added", false, true),
        candidate("importing", false, false),
    ]);
    snapshot.folder_candidates[6].runtime.import_status =
        Some(CandidateImportStatusSnapshot::Importing {
            progress_percent: 10,
            step: None,
        });
    snapshot.invalid_candidates = vec![InvalidCandidate {
        path: PathBuf::from("/music/broken"),
        name: "broken".to_string(),
        watched_folder_path: "/music".to_string(),
        display_path: "broken".to_string(),
        resolved_boundaries: Vec::new(),
        reason: InvalidReason::NoValidAudio,
    }];

    let answers = answers_for(
        &snapshot,
        vec![
            Some(answer(
                found(vec![result("rel-1")]),
                QueueClassification::Ready,
            )),
            Some(answer(
                found(vec![result("rel-2")]),
                QueueClassification::Ready,
            )),
            Some(answer(
                found(vec![result("rel-3"), result("rel-4")]),
                QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 }),
            )),
            None,
            Some(answer(
                found(vec![result("rel-5")]),
                QueueClassification::Ready,
            )),
            Some(answer(
                found(vec![result("rel-6")]),
                QueueClassification::Ready,
            )),
            Some(answer(
                found(vec![result("rel-7")]),
                QueueClassification::Ready,
            )),
        ],
    );

    let queue = project(snapshot, &answers, &HashMap::new());

    let tally = |tab: TriageTab| {
        candidate_rows(&queue)
            .into_iter()
            .filter(|row| row.placement.tab() == tab)
            .count() as u32
    };
    assert_eq!(queue.counts.pending, tally(TriageTab::Pending));
    assert_eq!(queue.counts.done, tally(TriageTab::Done));
    assert_eq!(
        queue.counts.skipped,
        tally(TriageTab::Skipped) + invalid_candidates(&queue).len() as u32
    );

    // Pinned, so a projection that put everything in one tab and still agreed
    // with itself would fail.
    assert_eq!(
        queue.counts,
        TriageTabCounts {
            pending: 5,
            done: 1,
            skipped: 2,
        }
    );
    assert_eq!(candidate_rows(&queue).len(), 7);
    assert_eq!(invalid_candidates(&queue).len(), 1);
    // Only the Ready rows take a checkbox.
    assert_eq!(
        candidate_rows(&queue)
            .into_iter()
            .filter(|row| row.selectable)
            .count() as u32,
        2
    );
}

// ── 4. A row with no match leads with the folder ────────────────────────────

/// Nothing matched, so there is no release to render: the row's title is the
/// folder name and there are no release fields at all — not a handful of
/// independent `None`s a surface could half-read.
#[test]
fn a_row_with_no_match_carries_no_release_fields() {
    let snapshot = snapshot_of(vec![
        candidate("nothing-matched", false, false),
        candidate("signals-disagreed", false, false),
        candidate("matched", false, false),
    ]);
    let answers = answers_for(
        &snapshot,
        vec![
            Some(answer(
                TerminalVerdict::NotFoundAnywhere,
                QueueClassification::NeedsYou(NeedsYou::NoMatch),
            )),
            Some(answer(
                TerminalVerdict::Conflict {
                    discid_results: vec![result("rel-1")],
                    barcode_results: vec![result("rel-2")],
                    matched_barcode: None,
                    track_count: 11,
                },
                QueueClassification::NeedsYou(NeedsYou::SignalsConflict),
            )),
            Some(answer(
                found(vec![result("rel-3")]),
                QueueClassification::Ready,
            )),
        ],
    );

    let queue = project(snapshot.clone(), &answers, &HashMap::new());

    let row_named = |name: &str| {
        candidate_rows(&queue)
            .into_iter()
            .find(|row| row.folder_name == name)
            .expect("named candidate row")
    };
    assert!(row_named("nothing-matched").matched.is_none());
    // A conflict has results on both sides but no agreement on which is the
    // match, so it leads with nothing either.
    assert!(row_named("signals-disagreed").matched.is_none());

    // A candidate with no verdict at all is the third shape of "no match".
    let unanswered = project(snapshot, &HashMap::new(), &HashMap::new());
    assert!(candidate_rows(&unanswered)
        .into_iter()
        .all(|row| row.matched.is_none()));

    // The matched row does carry them, so the assertions above are not passing
    // on a projection that never populates a release.
    let matched = row_named("matched")
        .matched
        .as_ref()
        .expect("a Found row leads with its release");
    assert_eq!(matched.release_id, "rel-3");
    assert_eq!(matched.title, "Album Title");
    assert_eq!(matched.artist.as_deref(), Some("Artist Name"));
    assert_eq!(
        matched.cover_thumbnail_url.as_deref(),
        Some("https://example.test/thumb.jpg")
    );
    assert_eq!(
        matched.evidence,
        MatchEvidence {
            source: MetadataSource::MusicBrainz,
            signal: Some(MatchedSignal::DiscId),
        }
    );
    assert_eq!(
        matched.pressing,
        Some(MatchedPressing {
            year: Some(1999),
            format: Some("CD".to_string()),
            track_count: Some(11),
        })
    );
}

/// Several pressings matched: the lead still stands in for the album, but the
/// pressing is absent *as a whole* — year, format and track count are exactly
/// what differs between the editions, and stating one would be the app
/// answering the question the row is asking.
#[test]
fn several_matches_state_the_lead_but_not_the_pressing() {
    let snapshot = snapshot_of(vec![candidate("two-pressings", false, false)]);
    let answers = answers_for(
        &snapshot,
        vec![Some(answer(
            found(vec![result("rel-1"), result("rel-2")]),
            QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 }),
        ))],
    );

    let queue = project(snapshot, &answers, &HashMap::new());
    let matched = candidate_rows(&queue)[0]
        .matched
        .as_ref()
        .expect("the lead pressing still stands in for the album");
    assert_eq!(matched.title, "Album Title");
    assert_eq!(matched.artist.as_deref(), Some("Artist Name"));
    assert_eq!(matched.pressing, None);
}

/// Done and Skipped rows still lead with what they matched — the placement
/// decides where a row sits, never whether it has a release to show.
#[test]
fn done_and_skipped_rows_still_lead_with_their_release() {
    let snapshot = snapshot_of(vec![
        candidate("imported", false, true),
        candidate("set-aside", true, false),
    ]);
    let answers = answers_for(
        &snapshot,
        vec![
            Some(answer(
                found(vec![result("rel-1")]),
                QueueClassification::Ready,
            )),
            Some(answer(
                found(vec![result("rel-2")]),
                QueueClassification::Ready,
            )),
        ],
    );

    let queue = project(snapshot, &answers, &HashMap::new());
    assert_eq!(candidate_rows(&queue)[0].placement, TriagePlacement::Done);
    assert_eq!(
        candidate_rows(&queue)[1].placement,
        TriagePlacement::Skipped
    );
    for row in candidate_rows(&queue) {
        assert!(
            row.matched.is_some(),
            "{} lost its release",
            row.folder_name
        );
        assert!(!row.selectable);
    }
}

// ── 5. Group 3 keeps its variants ───────────────────────────────────────────

/// The four "the folder and the source disagree" variants share one group, so
/// like decisions batch — and each row still names which disagreement it is, so
/// the sentence on the row can be precise. Flattening them into the group would
/// lose the operands.
#[test]
fn group_three_shares_a_header_and_keeps_its_variants() {
    let snapshot = snapshot_of(vec![
        candidate("count-disagrees", false, false),
        candidate("durations-disagree", false, false),
    ]);
    let count_disagrees = NeedsYou::TrackCountDisagrees {
        local: 11,
        source: 12,
    };
    let durations_disagree = NeedsYou::DurationsDisagree {
        probed_ms: 2_400_000,
        source_ms: 2_500_000,
        tolerance_ms: 5_500,
    };
    let answers = answers_for(
        &snapshot,
        vec![
            Some(answer(
                found(vec![result("rel-1")]),
                QueueClassification::NeedsYou(count_disagrees.clone()),
            )),
            Some(answer(
                found(vec![result("rel-2")]),
                QueueClassification::NeedsYou(durations_disagree.clone()),
            )),
        ],
    );

    let queue = project(snapshot, &answers, &HashMap::new());

    assert_eq!(
        candidate_rows(&queue)[0].placement,
        TriagePlacement::NeedsYou {
            group: NeedsYouGroup::CountsOrLengthsDisagree,
            reason: NeedsYouReason::Disagreement(count_disagrees),
        }
    );
    assert_eq!(
        candidate_rows(&queue)[1].placement,
        TriagePlacement::NeedsYou {
            group: NeedsYouGroup::CountsOrLengthsDisagree,
            reason: NeedsYouReason::Disagreement(durations_disagree),
        }
    );

    // The other two variants of the same group, and the two of "no match",
    // so the collapse is pinned in both directions.
    for variant in [
        NeedsYou::SourceLengthsUnknown,
        NeedsYou::LocalDurationUnknown,
    ] {
        assert_eq!(
            NeedsYouGroup::of(&NeedsYouReason::Disagreement(variant)),
            NeedsYouGroup::CountsOrLengthsDisagree
        );
    }
    for variant in [NeedsYou::NoMatch, NeedsYou::NothingToLookUp] {
        assert_eq!(
            NeedsYouGroup::of(&NeedsYouReason::Disagreement(variant)),
            NeedsYouGroup::NoMatch
        );
    }
    // And the variants that do *not* share: each of these is its own question.
    assert_eq!(
        NeedsYouGroup::of(&NeedsYouReason::Disagreement(NeedsYou::SeveralMatches {
            count: 2
        })),
        NeedsYouGroup::PickAPressing
    );
    assert_eq!(
        NeedsYouGroup::of(&NeedsYouReason::Disagreement(NeedsYou::SignalsConflict)),
        NeedsYouGroup::SignalsDisagree
    );
    assert_eq!(
        NeedsYouGroup::of(&NeedsYouReason::Disagreement(NeedsYou::AlreadyInLibrary)),
        NeedsYouGroup::AlreadyInLibrary
    );
}

/// The stacking order is core's, and `IN_ORDER` is the whole enum exactly once
/// — a surface that iterates it renders every group and invents no order.
#[test]
fn the_group_order_is_stated_once_and_holds_every_group() {
    let mut sorted = NeedsYouGroup::IN_ORDER;
    sorted.sort();
    assert_eq!(
        sorted,
        NeedsYouGroup::IN_ORDER,
        "IN_ORDER must match the declaration order the variants are stacked in"
    );
    for group in NeedsYouGroup::IN_ORDER {
        assert_eq!(
            NeedsYouGroup::IN_ORDER
                .iter()
                .filter(|g| **g == group)
                .count(),
            1
        );
    }
}
