//! The rule the claim line rests on: picking a release claims that release,
//! and the evidence only explains what turned it up.

use super::*;
use crate::db::LibraryStatus;
use crate::identify::state::SignalsContext;
use crate::identify::{GroupKey, ResultProvenance};
use crate::import::search::ImportSearchReleaseDetail;
use crate::import::MetadataSource;
use crate::signals::DiscIdSignal;
use std::collections::HashSet;

const REL_A: &str = "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e";
const REL_B: &str = "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b";

fn mb_ref(release_id: &str) -> MetadataRef {
    MetadataRef::new(release_id, MetadataSource::MusicBrainz)
}

fn result(release_id: &str) -> MetadataResult {
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(2004),
        format: Some("CD".to_string()),
        label: None,
        catalog_number: None,
        country: Some("UK".to_string()),
        cover_art: None,
        source_group_id: Some("group-1".to_string()),
        source_tracks: None,
    }
}

fn status(release_id: &str) -> LibraryStatus {
    LibraryStatus {
        release_id: release_id.to_string(),
        release_in_library: false,
        album_in_library: false,
        album_title: None,
        album_id: None,
    }
}

fn provenance(by_disc_id: bool, by_barcode: bool) -> ResultProvenance {
    ResultProvenance {
        by_disc_id,
        by_barcode,
        matches_catalog: false,
    }
}

/// A settled `Found` over the given releases and their provenance.
fn found(entries: &[(&str, ResultProvenance)]) -> IdentifyState {
    IdentifyState::Found {
        matches: entries.iter().map(|(id, _)| result(id)).collect(),
        library_statuses: entries.iter().map(|(id, _)| status(id)).collect(),
        track_count: 14,
        group: GroupKey {
            source: MetadataSource::MusicBrainz,
            source_group_id: "group-1".to_string(),
        },
        provenance: entries.iter().map(|(_, p)| p.clone()).collect(),
        context: empty_context(),
    }
}

/// A settled `Conflict` carrying each signal's own results.
fn conflict(discid: &[&str], barcode: &[&str]) -> IdentifyState {
    let mut context = empty_context();
    context.discid_results = discid.iter().map(|id| (result(id), status(id))).collect();
    context.barcode_results = barcode.iter().map(|id| (result(id), status(id))).collect();
    IdentifyState::Conflict { context }
}

fn empty_context() -> SignalsContext {
    SignalsContext {
        disc_id: DiscIdSignal::Absent { track_count: 14 },
        barcode_codes: Vec::new(),
        had_barcode_source: false,
        catalogs: Vec::new(),
        excluded: HashSet::new(),
        discid_results: Vec::new(),
        barcode_results: Vec::new(),
        discid_failure: None,
        barcode_failure: None,
        matched_barcode: None,
        track_count: 14,
    }
}

/// The picked release as a fetched detail describes it.
fn picked(release_id: &str) -> ClaimRelease {
    ClaimRelease::from_detail(&ImportSearchReleaseDetail {
        release_id: release_id.to_string(),
        source: MetadataSource::MusicBrainz,
        source_group_id: Some("group-1".to_string()),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(2004),
        format: Some("CD".to_string()),
        label: Some("Label Name".to_string()),
        catalog_number: Some("CAT-1234".to_string()),
        country: Some("UK".to_string()),
        barcode: None,
        track_count: 14,
        tracks: Vec::new(),
        cover_art: Vec::new(),
    })
}

/// Re-identify picks straight off a result row and commits from it, so its
/// claim release is built from that row rather than a fetched detail — and a
/// row carries no tracklist, so the line names the release without a track
/// count.
#[test]
fn a_picked_row_describes_itself_without_a_fetch() {
    let row = ClaimRelease {
        release_ref: mb_ref(REL_A),
        year: Some(2004),
        format: Some("CD".to_string()),
        country: Some("UK".to_string()),
        catalog_number: Some("CAT-1234".to_string()),
        track_count: None,
    };
    let line = claim_line(&found(&[(REL_A, provenance(false, true))]), &row);
    assert_eq!(line.release.as_deref(), Some("CD · 2004 · UK · CAT-1234"));
    assert_eq!(line.track_count, None);
}

// ── 1. A pick claims the pressing, whatever found it ────────────────

/// Picking a release claims that release. The evidence behind it — a disc ID
/// that matched it alone, a disc ID it shares with others, a barcode, a typed
/// search — changes the clause that explains the pick and nothing else. This is
/// the rule the whole claim line rests on, so it is stated once, here, over
/// every kind of evidence there is.
#[test]
fn a_pick_claims_the_pressing_whatever_found_it() {
    let cases = [
        (
            "disc ID matched this release alone",
            found(&[(REL_A, provenance(true, false))]),
            ClaimEvidence::DiscIdAlone,
        ),
        (
            "disc ID matched two releases",
            found(&[
                (REL_A, provenance(true, false)),
                (REL_B, provenance(true, false)),
            ]),
            ClaimEvidence::DiscIdShared { match_count: 2 },
        ),
        (
            "barcode off the packaging",
            found(&[(REL_A, provenance(false, true))]),
            ClaimEvidence::Barcode,
        ),
        (
            "found by searching",
            found(&[(REL_B, provenance(true, false))]),
            ClaimEvidence::Search,
        ),
    ];
    for (name, state, evidence) in cases {
        let line = claim_line(&state, &picked(REL_A));
        assert_eq!(
            line.choice,
            IdentityChoice::Release {
                release_ref: mb_ref(REL_A)
            },
            "{name}: a pick claims the pressing"
        );
        assert_eq!(line.evidence, evidence, "{name}: explains the pick");
    }
}

/// The evidence itself is read off the candidate's identify state — which
/// signal turned the release up, and how many releases that signal turned up
/// with it. Every terminal state and both non-terminal ones are covered, so a
/// new state can't quietly default to claiming a pressing.
#[test]
fn evidence_comes_from_the_identify_state() {
    let both_by_disc = found(&[
        (REL_A, provenance(true, false)),
        (REL_B, provenance(true, false)),
    ]);
    let cases: [(&str, IdentifyState, ClaimEvidence); 8] = [
        (
            "sole disc-ID match",
            found(&[(REL_A, provenance(true, false))]),
            ClaimEvidence::DiscIdAlone,
        ),
        (
            "one of two disc-ID matches",
            both_by_disc,
            ClaimEvidence::DiscIdShared { match_count: 2 },
        ),
        (
            "barcode match alongside a disc-ID match for another release",
            found(&[
                (REL_B, provenance(true, false)),
                (REL_A, provenance(false, true)),
            ]),
            ClaimEvidence::Barcode,
        ),
        (
            "a release the found set doesn't mention",
            found(&[(REL_B, provenance(true, false))]),
            ClaimEvidence::Search,
        ),
        (
            "picked the disc-ID side of a conflict",
            conflict(&[REL_A, REL_B], &[]),
            ClaimEvidence::DiscIdShared { match_count: 2 },
        ),
        (
            "picked the barcode side of a conflict",
            conflict(&[REL_B], &[REL_A]),
            ClaimEvidence::Barcode,
        ),
        (
            "nothing matched anywhere",
            IdentifyState::NotFoundAnywhere {
                context: empty_context(),
            },
            ClaimEvidence::Search,
        ),
        (
            "nothing to look up",
            IdentifyState::ManualOnly {
                track_count: 14,
                context: empty_context(),
            },
            ClaimEvidence::Search,
        ),
    ];
    for (name, state, expected) in cases {
        assert_eq!(evidence_for(&state, &mb_ref(REL_A)), expected, "{name}");
    }
}

/// The same id string from a different source is a different release, so its
/// evidence doesn't transfer.
#[test]
fn evidence_does_not_cross_sources() {
    let state = found(&[(REL_A, provenance(true, false))]);
    let discogs = MetadataRef::new(REL_A, MetadataSource::Discogs);
    assert_eq!(evidence_for(&state, &discogs), ClaimEvidence::Search);
}

/// A release stating no pressing facts describes itself as nothing rather than
/// leaving an empty slot in the sentence or padding it with the album title,
/// which is not a pressing fact.
///
/// The track count is deliberately still known here, because that is the shape
/// a source stub actually arrives in: the fetch read the tracklist, so the
/// count is always there, and it is the *description* that can be missing.
#[test]
fn a_release_with_no_pressing_facts_describes_itself_as_nothing() {
    let mut bare = picked(REL_A);
    bare.year = None;
    bare.format = None;
    bare.country = None;
    bare.catalog_number = Some("   ".to_string());
    let line = claim_line(&found(&[(REL_A, provenance(false, true))]), &bare);
    assert_eq!(line.release, None);
    assert_eq!(
        line.track_count,
        Some(14),
        "a stub still states its track count; only the description is missing"
    );
}

// ── 2. Re-picking the release claims the new one ────────────────────

/// Picking a different release claims that one: the claim is an assertion
/// about a release, so it does not outlive the release it was made about.
#[test]
fn re_picking_the_release_claims_the_new_one() {
    let state = found(&[
        (REL_A, provenance(true, false)),
        (REL_B, provenance(false, true)),
    ]);

    assert_eq!(
        claim_line(&state, &picked(REL_A)).choice,
        IdentityChoice::Release {
            release_ref: mb_ref(REL_A)
        }
    );
    assert_eq!(
        claim_line(&state, &picked(REL_B)).choice,
        IdentityChoice::Release {
            release_ref: mb_ref(REL_B)
        }
    );
}

/// Editing the pressing fields the confirm form shows, or deriving the line
/// again, leaves the claim where it was — the claim is about which release is
/// held, not about what was typed.
#[test]
fn editing_the_pressing_fields_does_not_move_the_claim() {
    let state = found(&[(REL_A, provenance(false, true))]);
    let original = claim_line(&state, &picked(REL_A));

    assert_eq!(claim_line(&state, &picked(REL_A)), original);

    let mut edited = picked(REL_A);
    edited.year = Some(2015);
    edited.catalog_number = Some("OTHER-9".to_string());
    let after_edit = claim_line(&state, &edited);
    assert_eq!(
        after_edit.choice, original.choice,
        "editing pressing fields must not move the claim"
    );
    assert_eq!(after_edit.evidence, original.evidence);
}

// ── 3. A stored pick is the identity it commits ─────────────────────

/// The stored pick is the claim: a release pick turns into that pressing's
/// identity, the folder's own tags into Unknown. This is the projection the
/// bulk-import path and the triage row both read, so it is stated once here.
#[test]
fn a_stored_pick_is_the_identity_it_commits() {
    use crate::import::{IdentityPick, MetadataSource};

    let pick = IdentityPick::Release {
        source: MetadataSource::MusicBrainz,
        release_id: REL_A.to_string(),
    };
    assert_eq!(
        pick.choice(),
        IdentityChoice::Release {
            release_ref: mb_ref(REL_A)
        }
    );
    assert_eq!(IdentityPick::Unknown.choice(), IdentityChoice::Unknown);

    let stored = serde_json::to_string(&pick).expect("a pick encodes");
    let read_back: IdentityPick = serde_json::from_str(&stored).expect("a stored pick decodes");
    assert_eq!(read_back, pick);
}
