//! Where a signal value came from — the provenance a signal badge labels
//! itself with ("from Cover OCR", "from the folder name", …).

use super::candidate_text::Source;

/// The surface a signal value was harvested from — a coarse, UI-facing projection of
/// the internal `Source` (plus the inherent origins of the disc-ID and CUE-`CATALOG`
/// signals), so a badge can say where its value came from without leaking file paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalOrigin {
    /// The disc's table of contents (LOG/CUE).
    DiscToc,
    /// A CUE sheet field (`CATALOG`, `PERFORMER`/`TITLE`).
    CueSheet,
    /// OCR of a cover/artwork image.
    Artwork,
    /// The candidate's folder name — a path component or a bracketed tag.
    FolderName,
    /// A file's name.
    Filename,
    /// A `.txt` document.
    TextFile,
}

impl SignalOrigin {
    /// The path payloads on `Artwork` / `FilenameGeneric` / `TextFile` are dropped
    /// here: a badge names the kind of surface, not the file.
    pub fn from_text_source(source: &Source) -> Self {
        match source {
            Source::Artwork(_) => SignalOrigin::Artwork,
            Source::PathComponent => SignalOrigin::FolderName,
            Source::FilenameGeneric(_) => SignalOrigin::Filename,
            Source::CueField => SignalOrigin::CueSheet,
            Source::TextFile(_) => SignalOrigin::TextFile,
        }
    }

    pub fn can_confirm_catalog(self) -> bool {
        !matches!(self, SignalOrigin::Artwork)
    }
}

/// A catalog number or barcode paired with where it was harvested from, so a badge
/// can show its `value` and explain its `origin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedValue {
    pub value: String,
    pub origin: SignalOrigin,
}

impl SourcedValue {
    pub fn new(value: String, origin: SignalOrigin) -> Self {
        Self { value, origin }
    }
}
