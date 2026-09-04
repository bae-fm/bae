//! The Ready rule's own tests. What a candidate's verdict *is* comes from the
//! sweep's tests; these are about what the queue asks of the user given one.

use super::*;
use crate::identify::ResultProvenance;
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
        barcode: None,
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
            by_catalog: false,
        })
        .collect();
    TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        matched_barcode: None,
    }
}

/// The barcode printed on the sleeve, which both sources state.
const BARCODE: &str = "0123456789012";

/// `result`, stating the barcode its source printed.
fn barcoded(mut result: MetadataResult, barcode: &str) -> MetadataResult {
    result.barcode = Some(barcode.to_string());
    result
}

/// The Discogs record of a pressing: a source of its own, no group (Discogs
/// states a master or nothing), and the barcode that pairs it with the
/// MusicBrainz row. A Discogs search result never carries a tracklist.
fn discogs(release_id: &str, barcode: &str) -> MetadataResult {
    MetadataResult {
        source: MetadataSource::Discogs,
        source_group_id: None,
        ..barcoded(result(release_id, None), barcode)
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

/// Two sources' records of one physical pressing are one row on the list,
/// picked whole — so a verdict naming both is one answer, not a choice, and a
/// settled lead makes it Ready exactly as a lone match does. The Discogs row
/// states no tracklist of its own and is not asked for one: the rule reads the
/// release the draft is read from.
#[test]
fn two_sources_agreeing_on_a_barcode_are_one_pressing() {
    let verdict = found(
        vec![
            barcoded(result("mb-1", agreeing(11, 2_400_000)), BARCODE),
            discogs("d-1", BARCODE),
        ],
        11,
    );
    assert_eq!(
        classify(&verdict, 2_400_000, &[status("mb-1", false, false)]),
        QueueClassification::Ready
    );
}

/// Two sources naming *different* pressings is still the user's choice, and
/// the count is rows on the list rather than records returned.
#[test]
fn two_sources_naming_different_pressings_stay_a_choice() {
    let verdict = found(
        vec![
            barcoded(result("mb-1", agreeing(11, 2_400_000)), BARCODE),
            discogs("d-1", "9876543210987"),
        ],
        11,
    );
    assert_eq!(
        classify(&verdict, 2_400_000, &[]),
        QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 })
    );
}

/// An exact signal is not a unique result: a disc ID routinely returns several
/// pressings of one release group, and choosing between them is not something
/// to do unattended.
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

/// The other two terminal verdicts each ask their own question; neither can be
/// Ready, and neither collapses into the other.
#[test]
fn every_other_verdict_names_its_own_question() {
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
/// to survive the reduction: which shape the verdict is, how many pressings it
/// named, and the lead's own columns. Each verdict is paired with the pressing
/// count it makes, which is the fact the reduction can no longer read off a
/// row count.
#[test]
fn a_summary_keeps_every_fact_the_rule_consults() {
    let verdicts = [
        (found(vec![result("rel-a", agreeing(11, 2_400_000))], 11), 1),
        (
            found(
                vec![
                    result("rel-a", agreeing(11, 2_400_000)),
                    result("rel-b", agreeing(11, 2_400_000)),
                ],
                11,
            ),
            2,
        ),
        (
            found(
                vec![
                    barcoded(result("rel-a", agreeing(11, 2_400_000)), BARCODE),
                    discogs("rel-b", BARCODE),
                ],
                11,
            ),
            1,
        ),
        (TerminalVerdict::NotFoundAnywhere, 0),
        (TerminalVerdict::ManualOnly { track_count: 11 }, 0),
        (
            TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::DiscId(
                    crate::signals::LookupFailure::Network,
                )],
                track_count: 11,
            },
            0,
        ),
    ];

    for (verdict, pressings) in verdicts {
        let summary = VerdictSummary::of(&verdict);
        assert_eq!(summary.pressing_count, pressings, "{verdict:?}");
        match &verdict {
            TerminalVerdict::Found {
                matches,
                track_count,
                ..
            } => {
                assert_eq!(summary.kind, VerdictKind::Found);
                assert_eq!(summary.track_count, Some(*track_count));
                let lead = summary.lead.as_ref().expect("a found verdict has a lead");
                assert_eq!(lead.release_id, matches[0].release_id);
                assert_eq!(lead.source_tracks, matches[0].source_tracks);
                assert!(lead.by_disc_id, "the lead carries its own provenance");
            }
            TerminalVerdict::NotFoundAnywhere => {
                assert_eq!(summary.kind, VerdictKind::NotFound);
                assert_eq!(summary.track_count, None);
            }
            TerminalVerdict::ManualOnly { track_count } => {
                assert_eq!(summary.kind, VerdictKind::ManualOnly);
                assert_eq!(summary.track_count, Some(*track_count));
            }
            TerminalVerdict::Failed { track_count, .. } => {
                assert_eq!(summary.kind, VerdictKind::Failed);
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
