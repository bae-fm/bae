#![cfg(desktop)]

use super::*;
use bae_catalogs::search::MetadataResult;
use bae_identify::signals::{DiscIdSignal, SourcedValue};
use bae_identify::state::{DiscIdEvidence, SignalsContext};
use bae_identify::{
    BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState, LookupState, ProviderLookup,
    ValueLookup,
};
use bae_records::LibraryStatus;

/// A result with every field the mirrors render filled in.
fn metadata_result(release_id: &str, group_id: &str) -> MetadataResult {
    MetadataResult {
        title: "Album Title".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        year: Some(1999),
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label A"), Some("AB 100"))],
        area: Some(bae_types::pressing::ReleaseArea::Country(
            bae_types::pressing::Country::from_code("US").unwrap(),
        )),
        media: bae_types::pressing::StatedMedia::PerMedium(vec![Some(
            bae_types::pressing::Medium::Cd,
        )]),
        ..MetadataResult::for_test(LookupCatalog::MusicBrainz, release_id, Some(group_id))
    }
}

fn empty_context() -> SignalsContext {
    SignalsContext {
        providers: Vec::new(),
        artwork: bae_identify::signals::ArtworkScan::Absent,
        origin: bae_catalogs::AudioOrigin::default(),
        disc: Default::default(),
        barcode: Default::default(),
        catalog: Default::default(),
        search: Default::default(),
        text: Default::default(),
        text_settled: true,
        audio: Default::default(),
        isrc: Default::default(),
        documents: bae_identify::documents::DocumentReading::Pending,
        pressings: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// The folder's text, carrying the barcode `code` and nothing else.
fn text_carrying(code: &str) -> bae_identify::CandidateText {
    bae_identify::CandidateText::of(&[], &[], &[SourcedValue::new(code.to_string())])
}

/// `state` as the automation surface shows it, each release it names
/// read as not in the library.
fn shown(state: IdentifyState) -> AutomationIdentifyState {
    let view = bae_identify::IdentifyStateView::from(state);
    let statuses = library_checks(&view)
        .iter()
        .map(|check| LibraryStatus::absent(&check.release_id))
        .collect();
    automation_identify_state(view, statuses)
}

/// `result`, printing the barcode `code`.
fn printing(
    mut result: bae_catalogs::search::MetadataResult,
    code: &str,
) -> bae_catalogs::search::MetadataResult {
    result.barcodes = vec![code.to_string()];
    result
}

/// A run's ledger whose one disc ID's lookup found `found`: what the Disc
/// ID agreement is read from.
fn disc_id_ledger(found: &[bae_catalogs::search::MetadataResult]) -> bae_identify::IdentifyRunView {
    use bae_identify::release_group::{group_results, unranked};
    use bae_identify::{
        BarcodeStepView, CatalogStepView, DiscIdRow, DiscIdStepView, IdentifyRunView, IsrcStepView,
        LookupView, SearchStepView,
    };
    IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz],
        disc_id: DiscIdStepView::Read {
            discs: vec![DiscIdRow {
                disc_id: "disc-1".to_string(),
                lookup: LookupView::Found {
                    count: found.len() as u32,
                    groups: group_results(unranked(found.to_vec()), None),
                },
            }],
        },
        barcode: BarcodeStepView::Absent,
        catalog: CatalogStepView::NoneFound,
        isrc: IsrcStepView::Absent,
        search: SearchStepView::NotNeeded,
    }
}

#[test]
fn found_state_aligns_agreements_and_pressings_by_release_id() {
    let matches = vec![
        metadata_result("rel-1", "group-1"),
        printing(metadata_result("rel-2", "group-1"), "0123456789012"),
    ];
    let state = IdentifyState::Found {
        track_count: 12,
        findings: bae_identify::Findings {
            matches: matches.clone(),
            // Two pressings of one album: each release is its own row.
            pressings: vec![0, 1],
            narrowed_out: Default::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        ledger: Some(disc_id_ledger(&matches)),
        context: SignalsContext { text: text_carrying("0123456789012"), ..empty_context() },
    };

    let json = serde_json::to_value(shown(state)).unwrap();
    assert_eq!(json["kind"], "found");
    let groups = json["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "both matches share one release group");
    // `rel-2` prints the folder's barcode as well, so the rows come back
    // with `rel-2` on top.
    let pressings = groups[0]["sections"][0]["pressings"].as_array().unwrap();
    assert_eq!(pressings[0]["releases"][0]["release_id"], "rel-2");
    assert_eq!(pressings[1]["releases"][0]["release_id"], "rel-1");
    // Agreements and statuses are keyed by release id and travel in the
    // rows' order, so a reader aligns them by id, never by position.
    let by_release = |field: &str, release_id: &str| {
        json[field]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["release_id"] == release_id)
            .cloned()
            .unwrap_or_else(|| panic!("{field} names {release_id}"))
    };
    assert_eq!(by_release("agreements", "rel-1")["disc_id"], true);
    let second = by_release("agreements", "rel-2");
    assert_eq!(second["disc_id"], true);
    assert_eq!(second["barcode"], true);
    by_release("library_statuses", "rel-1");
    by_release("library_statuses", "rel-2");
}

/// Signals that share no result settle as one `Found`, each release on its
/// own card with its badges.
#[test]
fn disagreeing_signals_become_one_found_over_several_groups() {
    let state = IdentifyState::Found {
        track_count: 9,
        findings: bae_identify::Findings {
            matches: vec![
                metadata_result("rel-disc", "g-d"),
                printing(metadata_result("rel-bar", "g-b"), "0123456789012"),
            ],
            // One release each, so each is its own row.
            pressings: vec![0, 1],
            narrowed_out: Default::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        ledger: Some(disc_id_ledger(&[metadata_result("rel-disc", "g-d")])),
        context: SignalsContext { text: text_carrying("0123456789012"), ..empty_context() },
    };

    let json = serde_json::to_value(shown(state)).unwrap();
    assert_eq!(json["kind"], "found");
    let groups = json["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2, "the two releases are two release groups");
    assert_eq!(groups[0]["sections"][0]["pressings"][0]["releases"][0]["release_id"], "rel-disc");
    assert_eq!(groups[1]["sections"][0]["pressings"][0]["releases"][0]["release_id"], "rel-bar");
    let agreements = json["agreements"].as_array().unwrap();
    assert_eq!(agreements[0]["disc_id"], true);
    assert_eq!(agreements[1]["barcode"], true);
    assert_eq!(json["track_count"], 9);
}

/// A run in flight crosses as its ledger: one row per code, one cell per
/// provider.
#[test]
fn triangulating_lists_each_code_with_one_cell_per_provider() {
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Lookups {
            discs: vec![bae_identify::DiscLookup {
                disc_id: "disc-hash".to_string(),
                state: LookupState::LookingUp,
            }],
        },
        barcode: BarcodeProgress::Lookups {
            codes: vec![
                ValueLookup {
                    value: "0123456789012".to_string(),
                    providers: vec![
                        ProviderLookup {
                            source: LookupCatalog::MusicBrainz,
                            state: LookupState::Done { results: Vec::new() },
                        },
                        ProviderLookup {
                            source: LookupCatalog::Discogs,
                            state: LookupState::Done {
                                results: vec![printing(
                                    metadata_result("rel-dg", "group-1"),
                                    "0123456789012",
                                )],
                            },
                        },
                    ],
                },
                ValueLookup {
                    value: "9999999999999".to_string(),
                    providers: vec![
                        ProviderLookup {
                            source: LookupCatalog::MusicBrainz,
                            state: LookupState::LookingUp,
                        },
                        ProviderLookup {
                            source: LookupCatalog::Discogs,
                            state: LookupState::Done { results: Vec::new() },
                        },
                    ],
                },
            ],
        },
        catalog: CatalogProgress::Skipped,
        isrc: bae_identify::IsrcProgress::Skipped,
        search: bae_identify::SearchProgress::Pending,
        context: SignalsContext {
            providers: vec![LookupCatalog::MusicBrainz, LookupCatalog::Discogs],
            disc: DiscIdEvidence {
                signal: DiscIdSignal::Computed {
                    discs: vec![bae_identify::signals::ComputedDiscId {
                        disc_id: "disc-hash".to_string(),
                        source_file: Some("rip.log".to_string()),
                    }],
                },
                ..Default::default()
            },
            barcode: bae_identify::state::BarcodeEvidence {
                codes: vec![
                    SourcedValue::in_file("0123456789012".to_string(), "back.jpg".to_string()),
                    SourcedValue::in_file("9999999999999".to_string(), "inlay.jpg".to_string()),
                ],
                had_source: true,
                ..Default::default()
            },
            text: text_carrying("0123456789012"),
            ..empty_context()
        },
    };

    let json = serde_json::to_value(shown(state)).unwrap();
    assert_eq!(json["kind"], "triangulating");
    let run = &json["run"];
    assert_eq!(run["providers"], serde_json::json!(["music_brainz", "discogs"]));
    assert_eq!(run["disc_id"]["kind"], "read");
    assert_eq!(run["disc_id"]["discs"][0]["disc_id"], "disc-hash");
    assert_eq!(run["disc_id"]["discs"][0]["lookup"]["kind"], "looking_up");
    assert_eq!(run["barcode"]["kind"], "rows");
    assert_eq!(run["barcode"]["scanning"], false);
    let rows = run["barcode"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["value"], "0123456789012");
    assert_eq!(rows[0]["cells"][0]["source"], "music_brainz");
    assert_eq!(rows[0]["cells"][0]["lookup"]["kind"], "no_match");
    assert_eq!(rows[0]["cells"][1]["source"], "discogs");
    assert_eq!(rows[0]["cells"][1]["lookup"]["kind"], "found");
    assert_eq!(rows[0]["cells"][1]["lookup"]["count"], 1);
    assert_eq!(
        rows[0]["cells"][1]["lookup"]["groups"][0]["sections"][0]["pressings"][0]["releases"][0]
            ["release_id"],
        "rel-dg"
    );
    assert_eq!(rows[1]["value"], "9999999999999");
    assert_eq!(rows[1]["cells"][0]["lookup"]["kind"], "looking_up");
    assert_eq!(rows[1]["cells"][1]["lookup"]["kind"], "no_match");
    assert_eq!(run["catalog"]["kind"], "none_found");
    // Discogs's match is already on the list while MusicBrainz is out.
    let groups = json["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["sections"][0]["pressings"][0]["releases"][0]["release_id"], "rel-dg");
    assert_eq!(json["agreements"][0]["release_id"], "rel-dg");
    assert_eq!(json["agreements"][0]["barcode"], true);
}
