//! Barcodes and disc IDs with the candidate files they were read off,
//! whatever pressing is picked. Only the automation interface lists them; no
//! screen marks a file with the clue read from it.

use bae_identify::signals::{DiscIdSignal, Signals};

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
    /// The candidate-relative path of the file.
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
#[path = "file_evidence_tests.rs"]
mod tests;
