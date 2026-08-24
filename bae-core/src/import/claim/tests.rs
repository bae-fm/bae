//! Where the evidence sits: which signal turned the picked release up, and
//! which of the candidate's files that signal was read off.

use super::*;
use crate::db::LibraryStatus;
use crate::identify::state::SignalsContext;
use crate::identify::ResultProvenance;
use crate::import::MetadataSource;
use crate::signals::{
    BarcodeSignal, DiscIdSignal, SignalOrigin, Signals, SourcedValue, TextSignal,
};

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

/// A settled `Found` over the given releases and their provenance, with the
/// barcode the lookup ran against.
fn found(entries: &[(&str, ResultProvenance)], matched_barcode: Option<&str>) -> IdentifyState {
    IdentifyState::Found {
        matches: entries.iter().map(|(id, _)| result(id)).collect(),
        library_statuses: entries.iter().map(|(id, _)| status(id)).collect(),
        track_count: 14,
        provenance: entries.iter().map(|(_, p)| p.clone()).collect(),
        context: SignalsContext {
            matched_barcode: matched_barcode.map(str::to_string),
            ..empty_context()
        },
    }
}

/// Signals as a scanned folder settles them: a disc ID off a rip log, and two
/// barcodes off two different images.
fn signals() -> Signals {
    Signals {
        disc_id: DiscIdSignal::Computed {
            disc_id: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-".to_string(),
            track_count: 14,
            source_file: Some("Album.log".to_string()),
        },
        barcode: BarcodeSignal::Settled {
            codes: vec![
                SourcedValue::in_file(
                    "5099969394522".to_string(),
                    SignalOrigin::Artwork,
                    "Back.jpg".to_string(),
                ),
                SourcedValue::in_file(
                    "0602527336459".to_string(),
                    SignalOrigin::Artwork,
                    "Inlay.jpg".to_string(),
                ),
            ],
        },
        text: TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        durations: Default::default(),
    }
}

/// A disc-ID match points at the file the ID was computed from; a barcode
/// match points at the image that barcode — and not the folder's other one —
/// was read off.
#[test]
fn evidence_names_the_file_it_was_read_off() {
    let by_disc = found(&[(REL_A, provenance(true, false))], None);
    assert_eq!(
        file_evidence(&by_disc, &mb_ref(REL_A), &signals()),
        vec![FileEvidence {
            signal: EvidenceSignal::DiscId,
            value: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-".to_string(),
            file_id: "Album.log".to_string(),
        }]
    );

    let by_barcode = found(&[(REL_A, provenance(false, true))], Some("0602527336459"));
    assert_eq!(
        file_evidence(&by_barcode, &mb_ref(REL_A), &signals()),
        vec![FileEvidence {
            signal: EvidenceSignal::Barcode,
            value: "0602527336459".to_string(),
            file_id: "Inlay.jpg".to_string(),
        }]
    );

    let by_both = found(&[(REL_A, provenance(true, true))], Some("5099969394522"));
    assert_eq!(
        file_evidence(&by_both, &mb_ref(REL_A), &signals())
            .into_iter()
            .map(|evidence| (evidence.signal, evidence.file_id))
            .collect::<Vec<_>>(),
        vec![
            (EvidenceSignal::DiscId, "Album.log".to_string()),
            (EvidenceSignal::Barcode, "Back.jpg".to_string()),
        ]
    );
}

/// Evidence with no file behind it has nothing to sit on. A disc ID computed
/// from stored tracks rather than a folder's file, a barcode read off the
/// folder's own name, and a release found by a catalog number or a typed
/// search all state nothing.
#[test]
fn evidence_with_no_file_states_nothing() {
    let mut fileless = signals();
    fileless.disc_id = DiscIdSignal::Computed {
        disc_id: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-".to_string(),
        track_count: 14,
        source_file: None,
    };
    fileless.barcode = BarcodeSignal::Settled {
        codes: vec![SourcedValue::new(
            "5099969394522".to_string(),
            SignalOrigin::FolderName,
        )],
    };
    let state = found(&[(REL_A, provenance(true, true))], Some("5099969394522"));
    assert_eq!(file_evidence(&state, &mb_ref(REL_A), &fileless), vec![]);

    let by_catalog = found(&[(REL_A, provenance(false, false))], None);
    assert_eq!(
        file_evidence(&by_catalog, &mb_ref(REL_A), &signals()),
        vec![]
    );
}

/// A release the state doesn't name was found some other way — a typed search,
/// or a pick made before the pipeline settled — and gets no chip. Nor do the
/// states that never matched anything.
#[test]
fn a_release_the_state_does_not_name_gets_nothing() {
    let other = found(&[(REL_B, provenance(true, false))], None);
    assert_eq!(file_evidence(&other, &mb_ref(REL_A), &signals()), vec![]);

    // The same id string from a different source is a different release.
    let same_id = found(&[(REL_A, provenance(true, false))], None);
    let discogs = MetadataRef::new(REL_A, MetadataSource::Discogs);
    assert_eq!(file_evidence(&same_id, &discogs, &signals()), vec![]);

    for state in [
        IdentifyState::Idle,
        IdentifyState::NotFoundAnywhere {
            context: empty_context(),
        },
        IdentifyState::ManualOnly {
            track_count: 14,
            context: empty_context(),
        },
    ] {
        assert_eq!(file_evidence(&state, &mb_ref(REL_A), &signals()), vec![]);
    }
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
