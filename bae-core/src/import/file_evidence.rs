//! Barcodes and disc IDs shown on the candidate files they were read off,
//! whatever pressing is picked.

use crate::signals::{DiscIdSignal, Signals};

/// An extracted signal that can name the file it was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceSignal {
    /// A barcode read off one of the folder's images or CUE sheets.
    Barcode,
    /// A disc ID computed from a rip log or cue sheet.
    DiscId,
}

/// One extracted identifying signal pinned to the candidate file it came
/// from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEvidence {
    pub signal: EvidenceSignal,
    /// The extracted barcode digits or disc ID.
    pub value: String,
    /// The candidate-relative path used by the gallery tile and file row.
    pub file_id: String,
}

/// Every extracted signal that names one of the candidate's files; a library
/// release's, which name none, are left out.
pub fn file_evidence(signals: &Signals) -> Vec<FileEvidence> {
    let mut evidence = Vec::new();
    if let DiscIdSignal::Computed { discs } = &signals.disc_id {
        evidence.extend(discs.iter().filter_map(|disc| {
            Some(FileEvidence {
                signal: EvidenceSignal::DiscId,
                value: disc.disc_id.clone(),
                file_id: disc.source_file.clone()?,
            })
        }));
    }
    evidence.extend(signals.barcode.codes().iter().filter_map(|code| {
        Some(FileEvidence {
            signal: EvidenceSignal::Barcode,
            value: code.value.clone(),
            file_id: code.origin_path.clone()?,
        })
    }));
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::{BarcodeSignal, ComputedDiscId, SourcedValue, TextSignal};

    fn signals() -> Signals {
        Signals {
            origin: crate::signals::AudioOrigin::default(),
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
            text: TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
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
        fileless.barcode = BarcodeSignal::Settled {
            codes: vec![SourcedValue::new("5099969394522".to_string())],
        };

        assert!(file_evidence(&fileless).is_empty());
    }
}
