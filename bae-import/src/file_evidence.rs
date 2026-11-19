//! Barcodes and disc IDs with the candidate files they were read off,
//! whatever pressing is picked, as the pane's read selects them. Only the
//! automation interface lists them; no screen marks a file with the clue read
//! from it.

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
