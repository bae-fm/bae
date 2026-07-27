//! The rule the claim line rests on: a disc ID that matched one release claims
//! the pressing, and nothing else does.

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

/// The picked release as the confirm pane's prefetched detail describes it.
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
/// claim release is built from that row rather than a prefetched detail — and
/// a row carries no tracklist, so the metadata-from line names the release
/// without a track count.
#[test]
fn a_picked_row_describes_itself_without_a_prefetch() {
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
    assert!(line.shows_metadata_source());
}

// ── 1. The default follows the evidence ─────────────────────────────

/// A disc ID that matched one release claims the pressing. Every other kind of
/// evidence — a disc ID that matched several, a barcode, a search — claims the
/// album and leaves the pressing open. This is the rule the whole claim line
/// rests on, so it is stated once, here, over every variant.
#[test]
fn only_a_lone_disc_id_match_claims_the_pressing() {
    let cases = [
        (
            "disc ID matched this release alone",
            ClaimEvidence::DiscIdAlone,
            true,
        ),
        (
            "disc ID matched two releases",
            ClaimEvidence::DiscIdShared { match_count: 2 },
            false,
        ),
        ("barcode off the packaging", ClaimEvidence::Barcode, false),
        ("found by searching", ClaimEvidence::Search, false),
    ];
    for (name, evidence, claims_pressing) in cases {
        assert_eq!(
            evidence.claims_pressing(),
            claims_pressing,
            "{name}: claims_pressing"
        );
        let choice = evidence.default_choice(mb_ref(REL_A));
        match (&choice, claims_pressing) {
            (IdentityChoice::Exact { release_ref }, true)
            | (IdentityChoice::Approximate { release_ref }, false) => {
                assert_eq!(release_ref, &mb_ref(REL_A), "{name}: keeps the release ref");
            }
            _ => panic!("{name}: wrong claim {choice:?}"),
        }
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

// ── 2. The provenance line appears exactly when the facts differ ────

/// An album-level claim shows the second line naming the release the metadata
/// came from; a pressing claim does not, because its own sentence already names
/// it. The two facts differ in exactly one of these cases.
#[test]
fn the_metadata_source_line_appears_only_for_an_album_claim() {
    let pressing = claim_line(&found(&[(REL_A, provenance(true, false))]), &picked(REL_A));
    assert!(matches!(pressing.choice, IdentityChoice::Exact { .. }));
    assert!(
        !pressing.shows_metadata_source(),
        "a pressing claim already names the release it was read from"
    );

    let album = claim_line(&found(&[(REL_A, provenance(false, true))]), &picked(REL_A));
    assert!(matches!(album.choice, IdentityChoice::Approximate { .. }));
    assert!(
        album.shows_metadata_source(),
        "an album claim must still name where the metadata came from"
    );
    // The release is described either way — the whole point is that picking
    // the album level no longer hides what was picked.
    assert_eq!(album.release.as_deref(), Some("CD · 2004 · UK · CAT-1234"));
    assert_eq!(album.release, pressing.release);
    assert_eq!(album.track_count, Some(14));
}

/// A release stating no pressing facts describes itself as nothing rather than
/// leaving an empty slot in the sentence or padding it with the album title,
/// which is not a pressing fact.
///
/// The track count is deliberately still known here, because that is the shape
/// a source stub actually arrives in: the prefetch fetched the tracklist, so
/// the count is always there, and it is the *description* that can be missing.
/// The two travel independently, and a surface must key its "no description"
/// rendering on `release` alone — a line built from the count by itself
/// ("Metadata from 14 tracks") names no release at all.
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
    assert!(line.shows_metadata_source());
}

// ── 3. Re-picking the release moves the claim, and nothing else does ─

/// The claim tracks which release is picked. Picking the one the disc ID named
/// alone claims the pressing; picking its barcode-matched sibling out of the
/// same settled state claims the album — no mode switch, no second control.
#[test]
fn re_picking_the_release_moves_the_claim() {
    let state = found(&[
        (REL_A, provenance(true, false)),
        (REL_B, provenance(false, true)),
    ]);

    let first = claim_line(&state, &picked(REL_A));
    assert_eq!(
        first.choice,
        IdentityChoice::Exact {
            release_ref: mb_ref(REL_A)
        }
    );

    let second = claim_line(&state, &picked(REL_B));
    assert_eq!(
        second.choice,
        IdentityChoice::Approximate {
            release_ref: mb_ref(REL_B)
        }
    );
}

// ── 4. A user override survives the commit ─────────────────────────

/// The claim that lands in the library is the one the import command carries.
/// The evidence-derived default is computed once, when the release is picked,
/// and commit never recomputes it: a command carrying an album-level claim
/// writes one even for a candidate whose disc ID would have claimed the
/// pressing. If that were not so, the carried claim would be undone at the last
/// moment.
///
/// No desktop surface can produce that divergence any more — the claim is a
/// function of which release is picked, and re-picking re-derives the whole
/// line, which is what "not a question asked during import" means. The one
/// caller that can still hand commit a claim of its own choosing is
/// `bae-automation`, which builds an `ImportCommand` field by field. This is
/// the test that keeps that path honest, and the one that would fail first if
/// commit ever started re-deriving the claim from the candidate's evidence.
#[test]
fn the_claim_the_command_carries_is_what_commits() {
    use crate::import::service::apply_identity_choice;
    use crate::import::ReleaseIdentity;

    let state = found(&[(REL_A, provenance(true, false))]);
    let default = claim_line(&state, &picked(REL_A)).choice;
    assert!(
        matches!(default, IdentityChoice::Exact { .. }),
        "this candidate's evidence defaults to claiming the pressing"
    );

    let mapper_output = vec![ReleaseIdentity {
        source: MetadataSource::MusicBrainz,
        source_group_id: "group-1".to_string(),
        source_release_id: Some(REL_A.to_string()),
    }];

    // The user re-picked, so the command carries the album-level claim. That is
    // what gets written, default notwithstanding.
    let overridden = IdentityChoice::Approximate {
        release_ref: mb_ref(REL_A),
    };
    let rows = apply_identity_choice(&mapper_output, &overridden);
    assert!(
        rows.iter().all(|row| row.source_release_id.is_none()),
        "the command's claim must reach the identity rows, not the default: {rows:?}"
    );
    assert_eq!(rows[0].source_group_id, "group-1");
}

/// Nothing else moves it. Editing the pressing fields the confirm form shows,
/// or deriving the line again, leaves the claim where the pick put it — the
/// claim is a fact about which release was picked, not about what was typed.
#[test]
fn nothing_but_a_re_pick_moves_the_claim() {
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
