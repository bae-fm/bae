//! Triage projection tests — the rules, not the rendering.
//!
//! The first block is over the pure projection. The last drives the real
//! `load`, against a real `Database`, `LibraryManager` and `ImportService` in a
//! tempdir, because its failure modes — a library check that comes back short,
//! an undecodable stored row, a corrupt probed total — are all unreachable from
//! `place` and `project`.

use super::*;
use crate::identify::{GroupKey, ResultProvenance};
use crate::import::cover_art::RemoteCover;
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, InvalidReason, ScannedFile,
};
use crate::import::handle::CandidateRuntimeSnapshot;
use crate::import::WatchedFolder;
use std::path::PathBuf;

// ── Fixtures ────────────────────────────────────────────────────────────────

fn candidate_rows(queue: &TriageQueue) -> Vec<&TriageRow> {
    queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .filter_map(|entry| match entry {
            TriageEntry::Candidate(row) => Some(row),
            TriageEntry::Boundary(_) | TriageEntry::Invalid(_) => None,
        })
        .collect()
}

fn invalid_candidates(queue: &TriageQueue) -> Vec<&InvalidCandidate> {
    queue
        .sections
        .iter()
        .flat_map(|section| &section.entries)
        .filter_map(|entry| match entry {
            TriageEntry::Invalid(candidate) => Some(candidate),
            TriageEntry::Candidate(_) | TriageEntry::Boundary(_) => None,
        })
        .collect()
}

fn result(release_id: &str) -> MetadataResult {
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(1999),
        format: Some("CD".to_string()),
        label: None,
        catalog_number: None,
        country: None,
        cover_art: Some(RemoteCover {
            url: "https://example.test/cover.jpg".to_string(),
            thumbnail_url: "https://example.test/thumb.jpg".to_string(),
            label: "Front".to_string(),
            source: MetadataSource::MusicBrainz,
        }),
        source_group_id: Some("group-1".to_string()),
        source_tracks: Some(SourceTracks::Listed {
            count: 11,
            total_duration_ms: Some(2_400_000),
        }),
    }
}

fn found(matches: Vec<MetadataResult>) -> TerminalVerdict {
    let provenance = matches
        .iter()
        .map(|_| ResultProvenance {
            by_disc_id: true,
            by_barcode: false,
            matches_catalog: false,
        })
        .collect();
    TerminalVerdict::Found {
        matches,
        track_count: 11,
        group: GroupKey {
            source: MetadataSource::MusicBrainz,
            source_group_id: "group-1".to_string(),
        },
        provenance,
    }
}

/// Every `NeedsYou` variant, so a table-driven test walks the whole enum. The
/// exhaustive `match` is what makes adding a variant a compile error here.
fn every_needs_you() -> Vec<NeedsYou> {
    let all = vec![
        NeedsYou::AlreadyInLibrary,
        NeedsYou::SeveralMatches { count: 3 },
        NeedsYou::SignalsConflict,
        NeedsYou::NoMatch,
        NeedsYou::NothingToLookUp,
        NeedsYou::TrackCountDisagrees {
            local: 11,
            source: 12,
        },
        NeedsYou::DurationsDisagree {
            probed_ms: 2_400_000,
            source_ms: 2_500_000,
            tolerance_ms: 5_500,
        },
        NeedsYou::SourceLengthsUnknown,
        NeedsYou::LocalDurationUnknown,
    ];
    for variant in &all {
        // No `_` arm: a tenth variant fails to compile until it is listed above.
        match variant {
            NeedsYou::AlreadyInLibrary
            | NeedsYou::SeveralMatches { .. }
            | NeedsYou::SignalsConflict
            | NeedsYou::NoMatch
            | NeedsYou::NothingToLookUp
            | NeedsYou::TrackCountDisagrees { .. }
            | NeedsYou::DurationsDisagree { .. }
            | NeedsYou::SourceLengthsUnknown
            | NeedsYou::LocalDurationUnknown => {}
        }
    }
    all
}

/// Every identify phase, with the same no-`_` guard.
fn every_phase() -> Vec<IdentifyPhase> {
    let all = vec![
        IdentifyPhase::Queued,
        IdentifyPhase::Running,
        IdentifyPhase::NoAnswer,
    ];
    for phase in &all {
        match phase {
            IdentifyPhase::Queued | IdentifyPhase::Running | IdentifyPhase::NoAnswer => {}
        }
    }
    all
}

/// Everything that can be known about a candidate: each classification, and
/// each phase of not knowing yet.
fn every_answer() -> Vec<CandidateAnswer> {
    let mut all = vec![CandidateAnswer::Classified(QueueClassification::Ready)];
    all.extend(
        every_needs_you()
            .into_iter()
            .map(|needs_you| CandidateAnswer::Classified(QueueClassification::NeedsYou(needs_you))),
    );
    all.extend(every_phase().into_iter().map(CandidateAnswer::Unanswered));
    all
}

/// Every import status a candidate can be in, including none.
fn every_import_status() -> Vec<Option<CandidateImportStatusSnapshot>> {
    vec![
        None,
        Some(CandidateImportStatusSnapshot::Importing {
            progress_percent: 40,
            step: None,
        }),
        Some(CandidateImportStatusSnapshot::Complete {
            release_id: "rel-1".to_string(),
            album_id: "alb-1".to_string(),
        }),
        Some(CandidateImportStatusSnapshot::Error {
            error: "boom".to_string(),
        }),
    ]
}

fn candidate(folder: &str, skipped: bool, is_added: bool) -> (FolderCandidate, bool, bool) {
    (
        FolderCandidate {
            path: PathBuf::from(format!("/music/{folder}")),
            file_root: PathBuf::from(format!("/music/{folder}")),
            name: folder.to_string(),
            files: CategorizedFiles {
                // One file, named after the folder, so every fixture candidate has
                // its own content hash — the key the stored verdicts are under.
                files: vec![CandidateFile {
                    proposed_audio: true,
                    file: ScannedFile::new(
                        PathBuf::from(format!("/music/{folder}/01.flac")),
                        format!("{folder}-01.flac"),
                        1_000,
                    ),
                    role: FileRole::Audio,
                }],
                format_label: "FLAC".to_string(),
            },
            watched_folder_path: "/music".to_string(),
            scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            display_path: folder.to_string(),
            resolved_boundaries: Vec::new(),
            combine_ancestor_key: None,
        },
        skipped,
        is_added,
    )
}

fn snapshot_of(candidates: Vec<(FolderCandidate, bool, bool)>) -> ImportCandidatesSnapshot {
    ImportCandidatesSnapshot {
        watched_folders: vec![WatchedFolder {
            path: "/music".to_string(),
            name: "music".to_string(),
        }],
        folder_candidates: candidates
            .into_iter()
            .map(
                |(candidate, skipped, is_added)| FolderImportCandidateSnapshot {
                    candidate,
                    actionable: true,
                    skipped,
                    is_added,
                    runtime: CandidateRuntimeSnapshot {
                        identify_state: IdentifyState::Idle,
                        toolbar: vec![],
                        signals: None,
                        import_status: None,
                    },
                },
            )
            .collect(),
        invalid_candidates: vec![],
        boundaries: vec![],
        folder_scan_statuses: vec![],
    }
}

fn answer(verdict: TerminalVerdict, classification: QueueClassification) -> Answered {
    Answered {
        verdict,
        classification,
    }
}

fn answers_for(
    snapshot: &ImportCandidatesSnapshot,
    per_candidate: Vec<Option<Answered>>,
) -> HashMap<(String, u64), Answered> {
    snapshot
        .folder_candidates
        .iter()
        .zip(per_candidate)
        .filter_map(|(candidate, answer)| {
            answer.map(|answer| {
                (
                    (
                        candidate.candidate.files.content_hash(),
                        candidate.candidate.file_edit_revision,
                    ),
                    answer,
                )
            })
        })
        .collect()
}

fn picks_for(
    snapshot: &ImportCandidatesSnapshot,
    per_candidate: Vec<Option<Picked>>,
) -> HashMap<(String, u64), Picked> {
    snapshot
        .folder_candidates
        .iter()
        .zip(per_candidate)
        .filter_map(|(candidate, picked)| {
            picked.map(|picked| {
                (
                    (
                        candidate.candidate.files.content_hash(),
                        candidate.candidate.file_edit_revision,
                    ),
                    picked,
                )
            })
        })
        .collect()
}

/// A release the user picked out of a manual search, as its archived documents
/// describe it — a different release from anything `result` produces, so a row
/// leading with the verdict's match instead is a failure rather than a
/// coincidence.
fn picked_release(release_id: &str) -> Picked {
    Picked {
        pick: crate::import::IdentityPick::Release {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
            claim: crate::import::ClaimLevel::Exact,
        },
        release: Some(MatchedRelease::of_pick(
            MetadataSource::MusicBrainz,
            &crate::import::search::ImportSearchReleaseDetail {
                release_id: release_id.to_string(),
                source: MetadataSource::MusicBrainz,
                source_group_id: None,
                title: "Picked Album Title".to_string(),
                artist: Some("Picked Artist Name".to_string()),
                year: Some(1987),
                format: Some("LP".to_string()),
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
                track_count: 9,
                tracks: Vec::new(),
                cover_art: vec![RemoteCover {
                    url: "https://example.test/picked.jpg".to_string(),
                    thumbnail_url: "https://example.test/picked-thumb.jpg".to_string(),
                    label: "Front".to_string(),
                    source: MetadataSource::MusicBrainz,
                }],
            },
        )),
    }
}

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

/// Every combination of what is known, import status, `skipped` and `is_added`
/// lands in exactly one tab, and in the one the plan's rules name.
///
/// The expectation is written as four independent predicates rather than as a
/// second copy of `place`'s `if` chain: each is guarded by `!done` / `!skipped`
/// so the precedence is stated once, declaratively, and a reordering of the
/// checks in `place` breaks it.
#[test]
fn tab_membership_is_total_and_exclusive() {
    let ready = CandidateAnswer::Classified(QueueClassification::Ready);
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
                    let is_ready = !importing && !done && !skipped && answer == ready;
                    let needs_you = importing || (!done && !skipped && answer != ready);

                    let expected: Vec<TriageTab> = [
                        (TriageTab::Ready, is_ready),
                        (TriageTab::NeedsYou, needs_you),
                        (TriageTab::Done, done),
                        (TriageTab::Skipped, is_skipped),
                    ]
                    .into_iter()
                    .filter_map(|(tab, holds)| holds.then_some(tab))
                    .collect();

                    assert_eq!(
                        expected.len(),
                        1,
                        "the four tab rules must hold for exactly one tab \
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
        assert_eq!(placement.tab(), TriageTab::NeedsYou);
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
        TriageTab::NeedsYou
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
    assert_eq!(queue.counts.ready, tally(TriageTab::Ready));
    assert_eq!(queue.counts.needs_you, tally(TriageTab::NeedsYou));
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
            ready: 2,
            needs_you: 3,
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
        queue.counts.ready
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

#[test]
fn nested_candidates_form_a_collapsible_group_with_a_combine_target() {
    let snapshot = snapshot_of(vec![
        candidate("Group/Release One", false, false),
        candidate("Group/Wrapper/Release Two", false, false),
    ]);
    let queue = project(snapshot, &HashMap::new(), &HashMap::new());

    assert_eq!(queue.sections.len(), 1);
    let group = queue.sections[0].group.as_ref().expect("grouped section");
    assert_eq!(group.name, "Group");
    assert_eq!(
        group.key,
        FolderReleaseDecisionKey {
            watched_folder_path: "/music".to_string(),
            relative_folder_path: "Group".to_string(),
        }
    );
    assert_eq!(queue.sections[0].entries.len(), 2);
}

#[test]
fn direct_release_joins_its_top_level_descendant_group() {
    let snapshot = snapshot_of(vec![
        candidate("Artist", false, false),
        candidate("Artist/Album", false, false),
    ]);
    let queue = project(snapshot, &HashMap::new(), &HashMap::new());

    assert_eq!(queue.sections.len(), 1);
    let section = &queue.sections[0];
    assert_eq!(
        section.group.as_ref().map(|group| group.name.as_str()),
        Some("Artist")
    );
    assert_eq!(section.entries.len(), 2);
}

#[test]
fn candidate_and_boundary_entries_share_natural_path_order() {
    let mut snapshot = snapshot_of(vec![candidate("Group/Release 10", false, false)]);
    snapshot.boundaries.push(FolderReleaseBoundary {
        key: FolderReleaseDecisionKey {
            watched_folder_path: "/music".to_string(),
            relative_folder_path: "Group/Release 2".to_string(),
        },
        name: "Release 2".to_string(),
        display_path: "Group/Release 2".to_string(),
        shared_file_count: 0,
        tree_rows: Vec::new(),
        candidate_keys: Vec::new(),
    });

    let queue = project(snapshot, &HashMap::new(), &HashMap::new());
    assert_eq!(queue.sections.len(), 1);
    assert!(matches!(
        &queue.sections[0].entries[..],
        [TriageEntry::Boundary(boundary), TriageEntry::Candidate(row)]
            if boundary.display_path == "Group/Release 2"
                && row.display_path == "Group/Release 10"
    ));
}

#[test]
fn projected_entry_keys_are_stable_and_variant_distinct() {
    let mut snapshot = snapshot_of(vec![candidate("Group/Release", false, false)]);
    snapshot.boundaries.push(FolderReleaseBoundary {
        key: FolderReleaseDecisionKey {
            watched_folder_path: "/music".to_string(),
            relative_folder_path: "Group/Release".to_string(),
        },
        name: "Release".to_string(),
        display_path: "Group/Release".to_string(),
        shared_file_count: 0,
        tree_rows: Vec::new(),
        candidate_keys: Vec::new(),
    });

    let first = project(snapshot.clone(), &HashMap::new(), &HashMap::new());
    let second = project(snapshot, &HashMap::new(), &HashMap::new());
    let first_keys: Vec<_> = first.sections[0]
        .entries
        .iter()
        .map(TriageEntry::stable_key)
        .collect();
    let second_keys: Vec<_> = second.sections[0]
        .entries
        .iter()
        .map(TriageEntry::stable_key)
        .collect();

    assert_eq!(first_keys, second_keys);
    assert_eq!(first_keys.len(), 2);
    assert_ne!(first_keys[0], first_keys[1]);
}

// ── `load`: the real read ───────────────────────────────────────────────────

mod load {
    use super::*;
    use crate::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack, NewImportCandidateVerdict};
    use crate::import::{ImportService, ReleaseIdentity};
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;

    const ARTIST_ID: &str = "e36744a5-1a36-460f-891c-e7e558034edf";
    const FLAC_FIXTURES: [&str; 2] = ["01 Test Track 1.flac", "02 Test Track 2.flac"];

    /// A real database, library manager and import service over a tempdir. No
    /// provider is faked and nothing identifies: these tests seed the stored
    /// verdicts directly, because what is under test is the read that turns
    /// them into rows.
    struct Fixture {
        manager: LibraryManager,
        import: ImportServiceHandle,
        root: PathBuf,
        _temp: TempDir,
    }

    impl Fixture {
        async fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let clock: coven::ClockRef = Arc::new(coven::SystemClock);
            let ids: coven::IdRef = Arc::new(coven::UuidProvider);
            let database = Database::new_test(
                temp.path().join("test.db").to_str().unwrap(),
                clock.clone(),
                ids.clone(),
            )
            .await
            .unwrap();
            let library_dir = coven::StoreDir::new(temp.path());
            let library_id = format!("triage-{}", uuid::Uuid::new_v4());
            let config = crate::config::Config::with_defaults(
                library_id.clone(),
                "test-device".to_string(),
                library_dir,
                "Test Library".to_string(),
            );
            crate::config::install_test_keyring();
            let manager = LibraryManager::new(
                database,
                Arc::new(crate::config::ConfigHandle::new(config)),
                crate::keys::StoreKeys::bind(library_id),
                clock,
                ids,
                crate::diagnostics::Diagnostics::noop(),
                tokio::runtime::Handle::current(),
                crate::import::cover_art::CoverArtArchiveClient::hermetic(),
            );
            let import = ImportService::start(tokio::runtime::Handle::current(), manager.clone())
                .await
                .unwrap();
            let root = temp.path().join("watched");
            std::fs::create_dir_all(&root).unwrap();
            Fixture {
                manager,
                import,
                root,
                _temp: temp,
            }
        }

        /// A candidate folder with two real FLACs, so the scan produces a
        /// folder candidate with a real content hash.
        ///
        /// The rip log is named after the folder because the content hash is
        /// over relative paths and sizes: two folders holding the same files
        /// under the same names *are* one candidate as far as the stored
        /// verdicts are concerned, which is correct and not what these tests
        /// are about.
        fn candidate_dir(&self, folder: &str) -> PathBuf {
            let dir = self.root.join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            for name in FLAC_FIXTURES {
                std::fs::copy(Path::new("tests/fixtures/flac").join(name), dir.join(name)).unwrap();
            }
            std::fs::write(dir.join(format!("{folder}.txt")), folder).unwrap();
            dir
        }

        /// Watch the root and wait for the scan to surface every candidate.
        async fn scan(&self, expected: usize) {
            let mut events = self.import.subscribe_events();
            self.import
                .add_watched_folder(self.root.to_string_lossy().into_owned())
                .await
                .unwrap();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                let event = tokio::time::timeout(remaining, events.recv())
                    .await
                    .expect("the scan finishes")
                    .expect("the bus stays open");
                if matches!(
                    event,
                    crate::import::ImportEvent::Scan(crate::import::ScanEvent::Finished)
                ) && self.import.get_import_candidates().folder_candidates.len() == expected
                {
                    return;
                }
            }
        }

        fn content_hash(&self, dir: &Path) -> String {
            // No stored bindings: these fixtures never edit one, so every
            // sheet keeps the scan's own reading.
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                dir,
                crate::import::ReleaseFileScope::Recursive,
                &crate::import::folder_scanner::StoredCandidateEdits::none(),
            )
            .expect("the candidate folder is readable")
            .content_hash()
        }

        /// Seed the row a sweep would have written for this folder.
        async fn store(&self, dir: &Path, verdict: &str, probed_total_duration_ms: i64) {
            self.manager
                .save_import_candidate_verdict(&NewImportCandidateVerdict {
                    content_hash: self.content_hash(dir),
                    folder_path: dir.to_string_lossy().into_owned(),
                    verdict: verdict.to_string(),
                    probed_total_duration_ms,
                    expected_edit_revision: 0,
                    identity_pick: None,
                })
                .await
                .unwrap();
        }

        async fn store_verdict(&self, dir: &Path, verdict: &TerminalVerdict, probed: i64) {
            self.store(dir, &serde_json::to_string(verdict).unwrap(), probed)
                .await;
        }

        /// Put a release into the library under `mb_release_id`, so a live
        /// check answers "already in the library" for it.
        async fn own_release(&self, mb_group_id: &str, mb_release_id: &str) {
            let now = chrono::Utc::now();
            self.manager
                .insert_artist(&DbArtist {
                    id: ARTIST_ID.to_string(),
                    name: "Artist Name".to_string(),
                    sort_name: None,
                    discogs_artist_id: None,
                    musicbrainz_artist_id: None,
                    created_at: now,
                })
                .await
                .unwrap();
            let album = DbAlbum {
                id: uuid::Uuid::new_v4().to_string(),
                title: "Album Title".to_string(),
                artist_id: ARTIST_ID.to_string(),
                year: Some(1999),
                primary_release_id: None,
                is_compilation: false,
                created_at: now,
            };
            let release = DbRelease {
                id: uuid::Uuid::new_v4().to_string(),
                album_id: album.id.clone(),
                release_name: None,
                pressing: crate::db::Pressing {
                    year: Some(1999),
                    format: None,
                    label: None,
                    catalog_number: None,
                    country: None,
                    barcode: None,
                },
                disc_id: None,
                metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
                metadata_source_release_id: Some(mb_release_id.to_string()),
                remote: true,
                source_folder_name: None,
                content_hash: None,
                album_loudness_lufs: None,
                album_peak_linear: None,
                created_at: now,
            };
            let track = DbTrack {
                id: uuid::Uuid::new_v4().to_string(),
                release_id: release.id.clone(),
                title: "Track 1".to_string(),
                side: 1,
                track_number: Some(1),
                duration_ms: Some(180_000),
                discogs_position: None,
                created_at: now,
            };
            self.manager
                .insert_album_with_release_and_tracks(&album, &release, &[track], &[])
                .await
                .unwrap();
            self.manager
                .insert_release_identities(
                    &release.id,
                    &[ReleaseIdentity {
                        source: MetadataSource::MusicBrainz,
                        source_group_id: mb_group_id.to_string(),
                        source_release_id: Some(mb_release_id.to_string()),
                    }],
                )
                .await
                .unwrap();
        }

        async fn load(&self) -> Result<TriageQueue, LibraryError> {
            super::super::load(&self.import, &self.manager).await
        }
    }

    /// A verdict whose one match agrees with the folder on count and length —
    /// everything the Ready rule wants except the library check.
    fn agreeing_verdict(probed_ms: u64, release_id: &str, group_id: &str) -> TerminalVerdict {
        let mut only = result(release_id);
        only.source_group_id = Some(group_id.to_string());
        only.source_tracks = Some(SourceTracks::Listed {
            count: 2,
            total_duration_ms: Some(probed_ms),
        });
        let mut verdict = found(vec![only]);
        if let TerminalVerdict::Found {
            track_count, group, ..
        } = &mut verdict
        {
            *track_count = 2;
            group.source_group_id = group_id.to_string();
        }
        verdict
    }

    /// The probed total the fixture FLACs really have — the number a sweep
    /// would have stored, so the Ready rule's duration check passes on it.
    fn probed_total_ms(dir: &Path) -> u64 {
        FLAC_FIXTURES
            .iter()
            .map(|name| {
                crate::audio_codec::probe_audio_from_path(dir.join(name).to_str().unwrap())
                    .expect("the fixture FLAC probes")
                    .duration
                    .as_millis() as u64
            })
            .sum()
    }

    /// The control: a candidate whose stored verdict agrees with the folder and
    /// whose release is *not* in the library is Ready. Everything below is this
    /// case with one thing changed, so a failure there is that thing.
    #[tokio::test]
    async fn an_agreeing_verdict_not_in_the_library_is_ready() {
        let fixture = Fixture::new().await;
        let dir = fixture.candidate_dir("album");
        fixture.scan(1).await;
        let probed = probed_total_ms(&dir);
        fixture
            .store_verdict(
                &dir,
                &agreeing_verdict(probed, "mb-rel-1", "group-1"),
                probed as i64,
            )
            .await;

        let queue = fixture.load().await.unwrap();
        assert_eq!(candidate_rows(&queue).len(), 1);
        assert_eq!(candidate_rows(&queue)[0].placement, TriagePlacement::Ready);
        assert_eq!(queue.counts.ready, 1);
        assert!(candidate_rows(&queue)[0].selectable);
    }

    /// The same candidate with the release now in the library must not be
    /// Ready. A missing status reads as "not in the library", so this is the
    /// case `load` refuses to guess at when a check comes back short.
    #[tokio::test]
    async fn a_candidate_already_in_the_library_is_not_ready() {
        let fixture = Fixture::new().await;
        let dir = fixture.candidate_dir("album");
        fixture.scan(1).await;
        let probed = probed_total_ms(&dir);
        fixture
            .store_verdict(
                &dir,
                &agreeing_verdict(probed, "mb-rel-1", "group-1"),
                probed as i64,
            )
            .await;
        fixture.own_release("group-1", "mb-rel-1").await;

        let queue = fixture.load().await.unwrap();
        assert_eq!(
            candidate_rows(&queue)[0].placement,
            TriagePlacement::NeedsYou {
                group: NeedsYouGroup::AlreadyInLibrary,
                reason: NeedsYouReason::Disagreement(NeedsYou::AlreadyInLibrary),
            },
            "a release the library already holds must never be bulk-importable"
        );
        assert_eq!(queue.counts.ready, 0);
        assert_eq!(queue.counts.needs_you, 1);
        assert!(!candidate_rows(&queue)[0].selectable);
    }

    /// A stored row this build can no longer parse is corruption, not an absent
    /// answer. The queue read must fail instead of inventing a usable state.
    #[tokio::test]
    async fn an_undecodable_row_fails_the_read() {
        let fixture = Fixture::new().await;
        let dir = fixture.candidate_dir("album");
        fixture.scan(1).await;
        fixture
            .store(&dir, r#"{"Found":{"shape":"from a future build"}}"#, 0)
            .await;

        let error = fixture
            .load()
            .await
            .expect_err("an undecodable verdict cannot be treated as absent");
        assert!(error.to_string().contains("does not decode"));
    }

    /// A negative probed total cannot come from anything that writes the
    /// column. Clamping it to zero would classify the candidate
    /// `LocalDurationUnknown` — a believable answer standing in for a corrupt
    /// row — so the read fails instead.
    #[tokio::test]
    async fn a_negative_probed_total_is_rejected_by_the_write() {
        let fixture = Fixture::new().await;
        let dir = fixture.candidate_dir("album");
        fixture.scan(1).await;
        let verdict = agreeing_verdict(2_400_000, "mb-rel-1", "group-1");
        let error = fixture
            .manager
            .save_import_candidate_verdict(&NewImportCandidateVerdict {
                content_hash: fixture.content_hash(&dir),
                folder_path: dir.to_string_lossy().into_owned(),
                verdict: serde_json::to_string(&verdict).unwrap(),
                probed_total_duration_ms: -1,
                expected_edit_revision: 0,
                identity_pick: None,
            })
            .await
            .expect_err("a negative probed total cannot enter durable state");
        assert!(
            error.to_string().contains("CHECK constraint failed"),
            "unexpected error: {error}"
        );
    }

    /// Two candidates, one verdict each, one batched library check — and the
    /// statuses land on the right candidates rather than being transposed by
    /// the dedup that batches them.
    #[tokio::test]
    async fn one_batched_check_lands_on_the_right_candidates() {
        let fixture = Fixture::new().await;
        let owned = fixture.candidate_dir("owned");
        let fresh = fixture.candidate_dir("fresh");
        fixture.scan(2).await;

        let owned_probed = probed_total_ms(&owned);
        fixture
            .store_verdict(
                &owned,
                &agreeing_verdict(owned_probed, "mb-rel-owned", "group-owned"),
                owned_probed as i64,
            )
            .await;
        let fresh_probed = probed_total_ms(&fresh);
        fixture
            .store_verdict(
                &fresh,
                &agreeing_verdict(fresh_probed, "mb-rel-fresh", "group-fresh"),
                fresh_probed as i64,
            )
            .await;
        fixture.own_release("group-owned", "mb-rel-owned").await;

        let queue = fixture.load().await.unwrap();
        let placement_of = |name: &str| {
            candidate_rows(&queue)
                .into_iter()
                .find(|row| row.folder_name == name)
                .unwrap_or_else(|| panic!("no row for {name}"))
                .placement
                .clone()
        };
        assert_eq!(
            placement_of("owned"),
            TriagePlacement::NeedsYou {
                group: NeedsYouGroup::AlreadyInLibrary,
                reason: NeedsYouReason::Disagreement(NeedsYou::AlreadyInLibrary),
            }
        );
        assert_eq!(placement_of("fresh"), TriagePlacement::Ready);
        assert_eq!(queue.counts.ready, 1);
        assert_eq!(queue.counts.needs_you, 1);
    }
}
