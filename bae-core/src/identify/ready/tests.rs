//! The Ready rule's own tests. What a candidate's verdict *is* comes from the
//! sweep's tests; these are about what the queue asks of the user given one.

use super::*;
use crate::identify::{GroupKey, ResultProvenance};
use crate::import::search::SourceTracks;
use crate::import::MetadataSource;

fn result(release_id: &str, source_tracks: Option<SourceTracks>) -> MetadataResult {
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album".to_string(),
        artist: None,
        year: None,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        cover_art: None,
        source_group_id: Some("rg-1".to_string()),
        source_tracks,
    }
}

fn found(matches: Vec<MetadataResult>, track_count: u32) -> TerminalVerdict {
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
        track_count,
        group: GroupKey {
            source: MetadataSource::MusicBrainz,
            source_group_id: "rg-1".to_string(),
        },
        provenance,
    }
}

fn agreeing(count: u32, total_ms: u64) -> Option<SourceTracks> {
    Some(SourceTracks::Listed {
        count,
        total_duration_ms: Some(total_ms),
    })
}

fn status(release_id: &str, release_in_library: bool, album_in_library: bool) -> LibraryStatus {
    LibraryStatus {
        release_id: release_id.to_string(),
        release_in_library,
        album_in_library,
        album_title: None,
        album_id: None,
    }
}

/// Every clause of the rule holding at once is the only way to Ready.
#[test]
fn one_verified_match_not_in_the_library_is_ready() {
    let verdict = found(vec![result("mb-1", agreeing(11, 2_400_000))], 11);
    assert_eq!(
        classify(&verdict, 2_400_000, &[status("mb-1", false, false)]),
        QueueClassification::Ready
    );
}

/// An exact signal is not a unique result: a disc ID routinely returns several
/// pressings, and choosing between them is not something to do unattended.
#[test]
fn several_matches_are_a_choice_for_the_user() {
    let verdict = found(
        vec![
            result("mb-1", agreeing(11, 2_400_000)),
            result("mb-2", agreeing(11, 2_400_000)),
        ],
        11,
    );
    assert_eq!(
        classify(&verdict, 2_400_000, &[]),
        QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 })
    );
}

/// The clause that cannot be stored: another import landing flips this without
/// the candidate's own verdict changing, which is why the status is passed in
/// live rather than read out of the row.
#[test]
fn library_status_is_read_live_at_both_levels() {
    let verdict = found(vec![result("mb-1", agreeing(11, 2_400_000))], 11);
    for (release, album) in [(true, false), (false, true), (true, true)] {
        assert_eq!(
            classify(&verdict, 2_400_000, &[status("mb-1", release, album)]),
            QueueClassification::NeedsYou(NeedsYou::AlreadyInLibrary),
            "release_in_library={release}, album_in_library={album}"
        );
    }
}

/// Nothing to compare against is not the same as agreement. A single match the
/// sources cannot corroborate goes to Needs you rather than being admitted.
#[test]
fn an_unverifiable_match_is_never_admitted() {
    assert_eq!(
        classify(&found(vec![result("mb-1", None)], 11), 2_400_000, &[]),
        QueueClassification::NeedsYou(NeedsYou::SourceLengthsUnknown),
        "the source describes no tracklist at all"
    );
    assert_eq!(
        classify(
            &found(
                vec![result(
                    "mb-1",
                    Some(SourceTracks::Listed {
                        count: 11,
                        total_duration_ms: None,
                    })
                )],
                11
            ),
            2_400_000,
            &[]
        ),
        QueueClassification::NeedsYou(NeedsYou::SourceLengthsUnknown),
        "the source counts its tracks but does not time them"
    );
    assert_eq!(
        classify(
            &found(vec![result("mb-1", agreeing(11, 2_400_000))], 11),
            0,
            &[]
        ),
        QueueClassification::NeedsYou(NeedsYou::LocalDurationUnknown),
        "the candidate's own audio would not probe"
    );
}

/// The other three terminal verdicts each ask their own question; none of them
/// can be Ready, and none collapses into another.
#[test]
fn every_other_verdict_names_its_own_question() {
    assert_eq!(
        classify(
            &TerminalVerdict::Conflict {
                discid_results: vec![result("mb-1", None)],
                barcode_results: vec![result("mb-2", None)],
                matched_barcode: None,
                track_count: 11,
            },
            2_400_000,
            &[]
        ),
        QueueClassification::NeedsYou(NeedsYou::SignalsConflict)
    );
    assert_eq!(
        classify(&TerminalVerdict::NotFoundAnywhere, 2_400_000, &[]),
        QueueClassification::NeedsYou(NeedsYou::NoMatch)
    );
    assert_eq!(
        classify(
            &TerminalVerdict::ManualOnly { track_count: 11 },
            2_400_000,
            &[]
        ),
        QueueClassification::NeedsYou(NeedsYou::NothingToLookUp)
    );
}

/// The tolerance grows with the tracklist because per-track rounding does, and
/// stops at nothing else — so the number a change to it would move is pinned.
#[test]
fn the_tolerance_is_per_track_rounding_with_a_floor() {
    assert_eq!(
        duration_tolerance_ms(1),
        5_000,
        "the floor holds for a single"
    );
    assert_eq!(duration_tolerance_ms(10), 5_000, "and up to ten tracks");
    assert_eq!(
        duration_tolerance_ms(11),
        5_500,
        "past which rounding leads"
    );
    assert_eq!(duration_tolerance_ms(20), 10_000);
}

/// [`VerdictSummary`] is what the list classifies from, read off stored
/// columns rather than a rebuilt verdict — so every fact the rule consults has
/// to survive the reduction: which shape the verdict is, how many releases it
/// named, and the lead's own columns.
#[test]
fn a_summary_keeps_every_fact_the_rule_consults() {
    let verdicts = [
        found(vec![result("rel-a", agreeing(11, 2_400_000))], 11),
        found(
            vec![
                result("rel-a", agreeing(11, 2_400_000)),
                result("rel-b", agreeing(11, 2_400_000)),
            ],
            11,
        ),
        TerminalVerdict::Conflict {
            discid_results: vec![result("rel-a", None)],
            barcode_results: vec![result("rel-b", None)],
            matched_barcode: Some("1234567890123".to_string()),
            track_count: 11,
        },
        TerminalVerdict::NotFoundAnywhere,
        TerminalVerdict::ManualOnly { track_count: 11 },
    ];

    for verdict in verdicts {
        let summary = VerdictSummary::of(&verdict);
        match &verdict {
            TerminalVerdict::Found {
                matches,
                track_count,
                ..
            } => {
                assert_eq!(summary.kind, VerdictKind::Found);
                assert_eq!(summary.track_count, Some(*track_count));
                assert_eq!(summary.match_count, matches.len() as u32);
                let lead = summary.lead.as_ref().expect("a found verdict has a lead");
                assert_eq!(lead.release_id, matches[0].release_id);
                assert_eq!(lead.source_tracks, matches[0].source_tracks);
                assert!(lead.by_disc_id, "the lead carries its own provenance");
            }
            TerminalVerdict::Conflict { track_count, .. } => {
                assert_eq!(summary.kind, VerdictKind::Conflict);
                assert_eq!(summary.track_count, Some(*track_count));
                assert_eq!(summary.match_count, 0);
                assert!(summary.lead.is_none(), "a conflict leads with nothing");
            }
            TerminalVerdict::NotFoundAnywhere => {
                assert_eq!(summary.kind, VerdictKind::NotFound);
                assert_eq!(summary.track_count, None);
            }
            TerminalVerdict::ManualOnly { track_count } => {
                assert_eq!(summary.kind, VerdictKind::ManualOnly);
                assert_eq!(summary.track_count, Some(*track_count));
            }
        }

        // The rule reads the same answer either way — which is what lets the
        // list classify without rebuilding the verdict.
        let statuses = [status("rel-a", false, false), status("rel-b", false, false)];
        let lead_status = summary
            .lead
            .as_ref()
            .and_then(|lead| statuses.iter().find(|s| s.release_id == lead.release_id));
        assert_eq!(
            classify_summary(&summary, 2_400_000, lead_status),
            classify(&verdict, 2_400_000, &statuses)
        );
    }
}
