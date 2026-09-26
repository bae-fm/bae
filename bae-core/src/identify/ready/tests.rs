//! The Ready rule's own tests. What a candidate's verdict *is* comes from the
//! sweep's tests; these are about what the queue asks of the user given one.

use super::*;
use crate::identify::LookupProvenance;
use crate::import::search::SourceTracks;
use crate::import::Catalog;

fn result(release_id: &str, source_tracks: Option<SourceTracks>) -> MetadataResult {
    MetadataResult {
        source: Catalog::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album".to_string(),
        artist: None,
        year: None,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        barcodes: Vec::new(),
        media: crate::import::search::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some("rg-1".to_string()),
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
        source_tracks,
    }
}

fn found(matches: Vec<MetadataResult>, track_count: u32) -> TerminalVerdict {
    let provenance = matches
        .iter()
        .map(|_| LookupProvenance {
            by_disc_id: true,
            by_barcode: false,
            by_catalog: false,
            by_search: false,
            named_by: None,
        })
        .collect();
    let pressings = crate::import::release_group::form_rows(&matches);
    TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        pressings,
        narrowed_out: Vec::new(),
        narrowed_out_provenance: Vec::new(),
        narrowed_out_pressings: Vec::new(),
        ledger: None,
    }
}

/// The barcode printed on the sleeve, which both sources state.
const BARCODE: &str = "0123456789012";

/// `result`, stating the barcode its source printed.
fn barcoded(mut result: MetadataResult, barcode: &str) -> MetadataResult {
    result.barcodes = vec![barcode.to_string()];
    result
}

/// The Discogs record of a pressing: a source of its own, no group (Discogs
/// states a master or nothing), and the barcode that pairs it with the
/// MusicBrainz row. A Discogs search result never carries a tracklist.
fn discogs(release_id: &str, barcode: &str) -> MetadataResult {
    MetadataResult {
        source: Catalog::Discogs,
        source_group_id: None,
        ..barcoded(result(release_id, None), barcode)
    }
}

fn listing(count: u32) -> Option<SourceTracks> {
    Some(SourceTracks::Listed { count })
}

/// Every clause of the rule holding at once is the only way to Ready.
#[test]
fn one_verified_match_is_ready() {
    let verdict = found(vec![result("mb-1", listing(11))], 11);
    assert_eq!(classify(&verdict), QueueClassification::Ready);
}

/// A lone match the title search found is admitted like any other: the
/// tracklist check is what guards it.
#[test]
fn a_lone_match_found_by_title_is_ready() {
    let mut verdict = found(vec![result("mb-1", listing(11))], 11);
    let TerminalVerdict::Found { provenance, .. } = &mut verdict else {
        unreachable!("the fixture is a found verdict");
    };
    provenance[0] = LookupProvenance {
        by_disc_id: false,
        by_barcode: false,
        by_catalog: false,
        by_search: true,
        named_by: None,
    };
    assert_eq!(classify(&verdict), QueueClassification::Ready);
}

/// The releases agreement narrowed out are not answers: the rule counts the
/// matches alone, so a sole verified match is still Ready however many the
/// agreement discarded on the way to it.
#[test]
fn what_agreement_narrowed_out_is_not_a_match() {
    let TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        pressings,
        ..
    } = found(vec![result("mb-1", listing(11))], 11)
    else {
        panic!("a found verdict");
    };
    let verdict = TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        pressings,
        narrowed_out: vec![result("mb-2", listing(11))],
        narrowed_out_provenance: vec![LookupProvenance {
            by_disc_id: true,
            by_barcode: false,
            by_catalog: false,
            by_search: false,
            named_by: None,
        }],
        narrowed_out_pressings: vec![0],
        ledger: None,
    };
    assert_eq!(classify(&verdict), QueueClassification::Ready);
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
            barcoded(result("mb-1", listing(11)), BARCODE),
            discogs("d-1", BARCODE),
        ],
        11,
    );
    assert_eq!(classify(&verdict), QueueClassification::Ready);
}

/// The count is the rows the run recorded, not the rows this list would form
/// on its own. A run that kept two records apart because of a record in its
/// other list is not re-decided by the reader that counts them.
#[test]
fn the_pressing_count_is_the_rows_the_run_recorded() {
    let matches = vec![
        barcoded(result("mb-1", listing(11)), BARCODE),
        discogs("d-1", BARCODE),
    ];
    assert_eq!(
        crate::import::release_group::form_rows(&matches),
        vec![0, 0],
        "forming rows over this list alone makes the two one row"
    );
    let TerminalVerdict::Found {
        track_count,
        provenance,
        ..
    } = found(matches.clone(), 11)
    else {
        panic!("a found verdict");
    };
    let verdict = TerminalVerdict::Found {
        matches,
        track_count,
        provenance,
        pressings: vec![0, 1],
        narrowed_out: Vec::new(),
        narrowed_out_provenance: Vec::new(),
        narrowed_out_pressings: Vec::new(),
        ledger: None,
    };
    assert_eq!(VerdictSummary::of(&verdict).pressing_count, 2);
    assert_eq!(
        classify(&verdict),
        QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 })
    );
}

/// Two sources naming *different* pressings is still the user's choice, and
/// the count is rows on the list rather than records returned.
#[test]
fn two_sources_naming_different_pressings_stay_a_choice() {
    let verdict = found(
        vec![
            barcoded(result("mb-1", listing(11)), BARCODE),
            discogs("d-1", "9876543210987"),
        ],
        11,
    );
    assert_eq!(
        classify(&verdict),
        QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 })
    );
}

/// An exact signal is not a unique result: a disc ID routinely returns several
/// pressings of one release group, and choosing between them is not something
/// to do unattended.
#[test]
fn several_matches_are_a_choice_for_the_user() {
    let verdict = found(
        vec![result("mb-1", listing(11)), result("mb-2", listing(11))],
        11,
    );
    assert_eq!(
        classify(&verdict),
        QueueClassification::NeedsYou(NeedsYou::SeveralMatches { count: 2 })
    );
}

/// A release that lists no tracks has no count to check the folder's against,
/// whether nobody has asked the source yet or it answered with nothing. A
/// single match the source cannot corroborate goes to Needs you rather than
/// being admitted.
#[test]
fn a_match_listing_no_tracks_is_never_admitted() {
    for source_tracks in [None, Some(SourceTracks::Nothing)] {
        assert_eq!(
            classify(&found(vec![result("mb-1", source_tracks.clone())], 11)),
            QueueClassification::NeedsYou(NeedsYou::SourceTracksUnknown),
            "{source_tracks:?}"
        );
    }
}

/// The count is the whole check: a release listing a different number of
/// tracks names both counts.
#[test]
fn a_count_mismatch_names_both_counts() {
    assert_eq!(
        classify(&found(vec![result("mb-1", listing(12))], 11)),
        QueueClassification::NeedsYou(NeedsYou::TrackCountDisagrees {
            local: 11,
            source: 12
        })
    );
}

/// The other two terminal verdicts each ask their own question; neither can be
/// Ready, and neither collapses into the other.
#[test]
fn every_other_verdict_names_its_own_question() {
    assert_eq!(
        classify(&TerminalVerdict::NotFoundAnywhere { ledger: None }),
        QueueClassification::NeedsYou(NeedsYou::NoMatch)
    );
    assert_eq!(
        classify(&TerminalVerdict::ManualOnly {
            track_count: 11,
            ledger: None,
        },),
        QueueClassification::NeedsYou(NeedsYou::NothingToLookUp)
    );
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
        (found(vec![result("rel-a", listing(11))], 11), 1),
        (
            found(
                vec![result("rel-a", listing(11)), result("rel-b", listing(11))],
                11,
            ),
            2,
        ),
        (
            found(
                vec![
                    barcoded(result("rel-a", listing(11)), BARCODE),
                    discogs("rel-b", BARCODE),
                ],
                11,
            ),
            1,
        ),
        (TerminalVerdict::NotFoundAnywhere { ledger: None }, 0),
        (
            TerminalVerdict::ManualOnly {
                track_count: 11,
                ledger: None,
            },
            0,
        ),
        (
            TerminalVerdict::Failed {
                failures: vec![crate::identify::IdentifyFailure::DiscId(
                    crate::signals::LookupFailure::Network,
                )],
                track_count: 11,
                ledger: None,
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
            TerminalVerdict::NotFoundAnywhere { .. } => {
                assert_eq!(summary.kind, VerdictKind::NotFound);
                assert_eq!(summary.track_count, None);
            }
            TerminalVerdict::ManualOnly { track_count, .. } => {
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
        assert_eq!(classify_summary(&summary), classify(&verdict));
    }
}
