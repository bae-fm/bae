use super::*;
use bae_identify::signals::{BarcodeSignal, ComputedDiscId, SourcedValue, TextSignal};

fn signals() -> Signals {
    Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: DiscIdSignal::Computed {
            discs: vec![
                ComputedDiscId {
                    disc_id: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-".to_string(),
                    source_file: Some("Disc 1/Album.log".to_string()),
                },
                ComputedDiscId {
                    disc_id: "aB7cD9eFgH2iJkL3mN4oP5qR6sT-".to_string(),
                    source_file: Some("Disc 2/Album.log".to_string()),
                },
            ],
        },
        barcode: BarcodeSignal::Settled {
            codes: vec![
                SourcedValue::in_file("5099969394522".to_string(), "Back.jpg".to_string()),
                SourcedValue::in_file("5099969394539".to_string(), "Back.jpg".to_string()),
                SourcedValue::in_file("0602527336459".to_string(), "Inlay.jpg".to_string()),
            ],
        },
        text: TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

#[test]
fn every_file_backed_signal_names_its_exact_source() {
    assert_eq!(
        file_evidence(&signals()),
        vec![
            FileEvidence {
                signal: EvidenceSignal::DiscId,
                value: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-".to_string(),
                file_id: "Disc 1/Album.log".to_string(),
            },
            FileEvidence {
                signal: EvidenceSignal::DiscId,
                value: "aB7cD9eFgH2iJkL3mN4oP5qR6sT-".to_string(),
                file_id: "Disc 2/Album.log".to_string(),
            },
            FileEvidence {
                signal: EvidenceSignal::Barcode,
                value: "5099969394522".to_string(),
                file_id: "Back.jpg".to_string(),
            },
            FileEvidence {
                signal: EvidenceSignal::Barcode,
                value: "5099969394539".to_string(),
                file_id: "Back.jpg".to_string(),
            },
            FileEvidence {
                signal: EvidenceSignal::Barcode,
                value: "0602527336459".to_string(),
                file_id: "Inlay.jpg".to_string(),
            },
        ]
    );
}

#[test]
fn signals_without_candidate_files_have_no_file_evidence() {
    let mut fileless = signals();
    fileless.disc_id = DiscIdSignal::Computed {
        discs: vec![ComputedDiscId {
            disc_id: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-".to_string(),
            source_file: None,
        }],
    };
    fileless.barcode =
        BarcodeSignal::Settled { codes: vec![SourcedValue::new("5099969394522".to_string())] };

    assert!(file_evidence(&fileless).is_empty());
}
