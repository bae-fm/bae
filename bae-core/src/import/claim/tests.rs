//! What the evidence says: which signal turned the picked release up, read off
//! the candidate's identify state.

use super::*;
use crate::db::LibraryStatus;
use crate::identify::state::SignalsContext;
use crate::identify::ResultProvenance;
use crate::import::MetadataSource;
use crate::signals::DiscIdSignal;

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
        by_catalog: false,
    }
}

/// A settled `Found` over the given releases and their provenance.
fn found(entries: &[(&str, ResultProvenance)]) -> IdentifyState {
    IdentifyState::Found {
        matches: entries.iter().map(|(id, _)| result(id)).collect(),
        library_statuses: entries.iter().map(|(id, _)| status(id)).collect(),
        track_count: 14,
        provenance: entries.iter().map(|(_, p)| p.clone()).collect(),
        context: empty_context(),
    }
}

/// A settled state over signals that share no result: the reducer combines
/// them into one `Found` over their union.
fn disagreeing(discid: &[&str], barcode: &[&str]) -> IdentifyState {
    let mut context = empty_context();
    context.discid_results = discid.iter().map(|id| (result(id), status(id))).collect();
    context.barcode_results = barcode.iter().map(|id| (result(id), status(id))).collect();
    crate::identify::state::re_derive_for_tests(context)
}

fn empty_context() -> SignalsContext {
    SignalsContext {
        disc_id: DiscIdSignal::Absent { track_count: 14 },
        barcode_codes: Vec::new(),
        had_barcode_source: false,
        catalogs: Vec::new(),
        chosen_catalog: None,
        disc_excluded: false,
        barcode_excluded: false,
        discid_results: Vec::new(),
        barcode_results: Vec::new(),
        catalog_results: Vec::new(),
        discid_failure: None,
        barcode_failure: None,
        catalog_failure: None,
        matched_barcode: None,
        track_count: 14,
    }
}

/// The evidence is read off the candidate's identify state — which signal
/// turned the release up, and how many releases that signal turned up with it.
/// Every terminal state and both non-terminal ones are covered, so a new state
/// can't quietly default to claiming the disc was matched.
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
            "one of two releases the disc ID alone turned up",
            disagreeing(&[REL_A, REL_B], &[]),
            ClaimEvidence::DiscIdShared { match_count: 2 },
        ),
        (
            "the barcode's release, alongside a disc-ID one it shares nothing with",
            disagreeing(&[REL_B], &[REL_A]),
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

/// A stored pick is the identity it commits: a release pick turns into that
/// pressing's identity, the folder's own tags into Unknown. This is the
/// projection the bulk-import path and the triage row both read.
#[test]
fn a_stored_pick_is_the_identity_it_commits() {
    use crate::import::{IdentityChoice, IdentityPick};

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
