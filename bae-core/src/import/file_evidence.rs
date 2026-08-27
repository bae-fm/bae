//! File-backed identifying signals exposed on the candidate files they came
//! from.
//!
//! This provenance belongs to extraction, not to a selected pressing. A
//! barcode stays a fact about the image it was read from and a disc ID stays a
//! fact about its rip log or cue sheet before a pressing is picked, after File
//! Tags is chosen, and when a manually selected pressing was supported by
//! neither signal.

use crate::signals::{DiscIdSignal, Signals};

/// An extracted signal that can name the file it was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceSignal {
    /// A barcode read from one of the folder's images.
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

/// Every extracted identifying signal that names one of the candidate's
/// files.
///
/// Fileless values are omitted because a library re-identification or a value
/// read from the folder name has no candidate tile or row to carry a badge.
pub fn file_evidence(signals: &Signals) -> Vec<FileEvidence> {
    let mut evidence = Vec::new();
    if let DiscIdSignal::Computed {
        disc_id,
        source_file: Some(file_id),
        ..
    } = &signals.disc_id
    {
        evidence.push(FileEvidence {
            signal: EvidenceSignal::DiscId,
            value: disc_id.clone(),
            file_id: file_id.clone(),
        });
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
    use crate::signals::{BarcodeSignal, SignalOrigin, SourcedValue, TextSignal};

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
                        "5099969394539".to_string(),
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

    #[test]
    fn every_file_backed_signal_names_its_exact_source() {
        assert_eq!(
            file_evidence(&signals()),
            vec![
                FileEvidence {
                    signal: EvidenceSignal::DiscId,
                    value: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-".to_string(),
                    file_id: "Album.log".to_string(),
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

        assert!(file_evidence(&fileless).is_empty());
    }
}
