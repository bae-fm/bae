//! The verdict summary's judgements. What a candidate's verdict *is* comes from the
//! sweep's tests; these are about what the queue asks of the user given one.

use super::*;
use crate::identify::{Findings, LookupProvenance, NarrowedOut};
use crate::import::search::SourceTracks;
use crate::import::Catalog;

fn result(release_id: &str, source_tracks: Option<SourceTracks>) -> MetadataResult {
    MetadataResult {
        source: Catalog::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album".to_string(),
        artist: None,
        year: None,
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: crate::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some("rg-1".to_string()),
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
        source_tracks,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
    }
}

fn found(matches: Vec<MetadataResult>, track_count: u32) -> TerminalVerdict {
    let provenance = matches
        .iter()
        .map(|_| LookupProvenance {
            by_disc_id: true,
            by_barcode: false,
            by_catalog: false,
            by_isrc: false,
            by_search: false,
            named_by: None,
        })
        .collect();
    let pressings = crate::import::release_group::form_rows(&matches);
    TerminalVerdict::Found {
        findings: Findings {
            matches,
            provenance,
            pressings,
            narrowed_out: NarrowedOut::default(),
            medium_conflict: None,
        },
        track_count,
        ledger: None,
    }
}

/// What `found` would store, had a lookup failed beside the ones that
/// returned these matches.
fn failed_with_findings(matches: Vec<MetadataResult>, track_count: u32) -> TerminalVerdict {
    let TerminalVerdict::Found { findings, .. } = found(matches, track_count) else {
        unreachable!("`found` builds a found verdict");
    };
    TerminalVerdict::Failed {
        failures: vec![crate::identify::IdentifyFailure::Search(
            crate::import::search::SourceFailure {
                source: Catalog::MusicBrainz,
                failure: crate::signals::LookupFailure::Provider { status: Some(503) },
            },
        )],
        findings,
        track_count,
        ledger: None,
    }
}

/// A failed lookup keeps a candidate from being auto-importable however good what the other
/// lookups found looks: the one that failed may have named other pressings.
#[test]
fn a_failed_verdict_is_never_auto_importable_whatever_it_found() {
    let verdict = failed_with_findings(vec![result("rel-a", listing(11))], 11);
    assert_eq!(VerdictSummary::of(&verdict).judgement(), (false, None));
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

/// Every clause holding at once is the only way to be auto-importable.
#[test]
fn one_verified_match_is_auto_importable() {
    let verdict = found(vec![result("mb-1", listing(11))], 11);
    assert_eq!(VerdictSummary::of(&verdict).judgement(), (true, None));
}

/// A lone match the title search found is admitted like any other: the
/// tracklist check is what guards it.
#[test]
fn a_lone_match_found_by_title_is_auto_importable() {
    let mut verdict = found(vec![result("mb-1", listing(11))], 11);
    let TerminalVerdict::Found {
        findings: Findings { provenance, .. },
        ..
    } = &mut verdict
    else {
        unreachable!("the fixture is a found verdict");
    };
    provenance[0] = LookupProvenance {
        by_disc_id: false,
        by_barcode: false,
        by_catalog: false,
        by_isrc: false,
        by_search: true,
        named_by: None,
    };
    assert_eq!(VerdictSummary::of(&verdict).judgement(), (true, None));
}

/// The releases agreement narrowed out are not answers: only the matches
/// count, so a sole verified match is still auto-importable however many the
/// agreement discarded on the way to it.
#[test]
fn what_agreement_narrowed_out_is_not_a_match() {
    let TerminalVerdict::Found {
        track_count,
        findings:
            Findings {
                matches,
                provenance,
                pressings,
                ..
            },
        ..
    } = found(vec![result("mb-1", listing(11))], 11)
    else {
        panic!("a found verdict");
    };
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches,
            provenance,
            pressings,
            narrowed_out: NarrowedOut {
                matches: vec![result("mb-2", listing(11))],
                provenance: vec![LookupProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    by_catalog: false,
                    by_isrc: false,
                    by_search: false,
                    named_by: None,
                }],
                pressings: vec![0],
            },
            medium_conflict: None,
        },
        track_count,
        ledger: None,
    };
    assert_eq!(VerdictSummary::of(&verdict).judgement(), (true, None));
}

/// Two sources' records of one physical pressing are one row on the list,
/// picked whole — so a verdict naming both is one answer, not a choice, and a
/// settled lead makes it auto-importable exactly as a lone match does. The
/// Discogs row states no tracklist of its own and is not asked for one: the check reads the
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
    assert_eq!(VerdictSummary::of(&verdict).judgement(), (true, None));
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
        findings: Findings { provenance, .. },
        ..
    } = found(matches.clone(), 11)
    else {
        panic!("a found verdict");
    };
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches,
            provenance,
            pressings: vec![0, 1],
            narrowed_out: NarrowedOut::default(),
            medium_conflict: None,
        },
        track_count,
        ledger: None,
    };
    assert_eq!(VerdictSummary::of(&verdict).pressing_count, 2);
    assert_eq!(VerdictSummary::of(&verdict).judgement(), (false, None));
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
    assert_eq!(VerdictSummary::of(&verdict).judgement(), (false, None));
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
    assert_eq!(VerdictSummary::of(&verdict).judgement(), (false, None));
}

/// A release that lists no tracks has no count to check the folder's against,
/// whether nobody has asked the source yet or it answered with nothing. A
/// single match the source cannot corroborate is not auto-importable.
#[test]
fn a_match_listing_no_tracks_is_never_admitted() {
    for source_tracks in [None, Some(SourceTracks::Nothing)] {
        assert_eq!(
            VerdictSummary::of(&found(vec![result("mb-1", source_tracks.clone())], 11)).judgement(),
            (false, Some(FolderCheck::SourceTracksUnknown)),
            "{source_tracks:?}"
        );
    }
}

/// The count is the whole check: a release listing a different number of
/// tracks names both counts.
#[test]
fn a_count_mismatch_names_both_counts() {
    assert_eq!(
        VerdictSummary::of(&found(vec![result("mb-1", listing(12))], 11)).judgement(),
        (
            false,
            Some(FolderCheck::TrackCountDisagrees {
                local: 11,
                source: 12
            })
        )
    );
}

/// A pressing one of whose records' documents could not be read is not
/// picked unattended, however its count reads: what the document states was
/// never checked, and the pick cannot be applied without it.
#[test]
fn a_pressing_with_an_unread_document_is_not_picked_unattended() {
    let unread = |mut result: MetadataResult| {
        result.document_failure = Some(crate::signals::LookupFailure::Network);
        result
    };
    for matches in [
        vec![unread(result("mb-1", listing(11)))],
        vec![
            barcoded(result("mb-1", listing(11)), BARCODE),
            unread(discogs("d-1", BARCODE)),
        ],
    ] {
        assert_eq!(
            VerdictSummary::of(&found(matches.clone(), 11)).judgement(),
            (false, None),
            "{matches:?}"
        );
    }
}

/// A verdict that found nothing to check the folder against is never
/// auto-importable,
/// and names no folder check: the lookup result is the whole story, and the
/// identify steps already state it.
#[test]
fn a_verdict_that_found_nothing_fails_no_folder_check() {
    assert_eq!(
        VerdictSummary::of(&TerminalVerdict::NotFoundAnywhere { ledger: None }).judgement(),
        (false, None)
    );
    assert_eq!(
        VerdictSummary::of(&TerminalVerdict::ManualOnly {
            track_count: 11,
            ledger: None,
        },)
        .judgement(),
        (false, None)
    );
}

/// [`VerdictSummary`] is what the list judges from, read off stored columns
/// rather than a rebuilt verdict — so every fact the judgements consult has
/// to survive the reduction: which shape the verdict is, how many pressings it
/// named, and the lead's own columns. Each verdict is paired with the pressing
/// count it makes, which is the fact the reduction can no longer read off a
/// row count.
#[test]
fn a_summary_keeps_every_fact_the_judgements_consult() {
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
                findings: Findings::default(),
                track_count: 11,
                ledger: None,
            },
            0,
        ),
        // What the answering lookups found leads the failed row too.
        (
            failed_with_findings(vec![result("rel-a", listing(11))], 11),
            1,
        ),
    ];

    for (verdict, pressings) in verdicts {
        let summary = VerdictSummary::of(&verdict);
        assert_eq!(summary.pressing_count, pressings, "{verdict:?}");
        match &verdict {
            TerminalVerdict::Found {
                track_count,
                findings: Findings { matches, .. },
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
            TerminalVerdict::Failed {
                track_count,
                findings,
                ..
            } => {
                assert_eq!(summary.kind, VerdictKind::Failed);
                assert_eq!(summary.track_count, Some(*track_count));
                assert_eq!(
                    summary.lead.as_ref().map(|lead| lead.release_id.as_str()),
                    findings.matches.first().map(|m| m.release_id.as_str())
                );
            }
        }
    }
}

/// A single pressing whose tracklist fits is still not auto-importable when the
/// folder's own files rule it out: the person picks, or does not.
#[test]
fn a_release_the_folder_rules_out_fails_its_check() {
    assert_eq!(
        VerdictSummary::of(&ruled_out_by_the_folder(vec![result("mb-1", listing(11))])).judgement(),
        (
            false,
            Some(FolderCheck::MediumDisagrees {
                folder: crate::identify::MediumConflict::NotCdAudio
            })
        )
    );
}

/// The medium is checked before the pressing count, so a folder that rules
/// out every one of several pressings names that, rather than leaving the
/// choice between them unexplained.
#[test]
fn a_medium_the_folder_rules_out_is_named_over_several_pressings() {
    assert_eq!(
        VerdictSummary::of(&ruled_out_by_the_folder(vec![
            result("mb-1", listing(11)),
            result("mb-2", listing(11)),
        ]))
        .judgement(),
        (
            false,
            Some(FolderCheck::MediumDisagrees {
                folder: crate::identify::MediumConflict::NotCdAudio
            })
        )
    );
}

/// `found`, with the folder's 96 kHz audio ruling out every match as a CD.
fn ruled_out_by_the_folder(matches: Vec<MetadataResult>) -> TerminalVerdict {
    let TerminalVerdict::Found {
        mut findings,
        track_count,
        ledger,
    } = found(matches, 11)
    else {
        unreachable!("found builds a found verdict");
    };
    findings.medium_conflict = Some(crate::identify::MediumConflict::NotCdAudio);
    TerminalVerdict::Found {
        findings,
        track_count,
        ledger,
    }
}
