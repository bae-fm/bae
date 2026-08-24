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
    /// here: a badge names the kind of surface, not the file. A value that has to
    /// point at the file it was read off carries it separately, on
    /// [`SourcedValue::origin_path`].
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
    /// The file the value was read off, as the candidate-relative path that
    /// addresses it — the same id a gallery tile and a file row are keyed by,
    /// so a surface showing this value can put it on the file it came from.
    ///
    /// `None` where the origin is not a file (the folder's own name), and for a
    /// re-identify pass over a library release, whose images are stored blobs
    /// rather than files of a scanned folder.
    ///
    /// Relative, never absolute: these rows sync, and a path from one device's
    /// disk means nothing on another's.
    pub origin_path: Option<String>,
}

impl SourcedValue {
    /// A value whose origin names no file to point at.
    pub fn new(value: String, origin: SignalOrigin) -> Self {
        Self {
            value,
            origin,
            origin_path: None,
        }
    }

    /// A value read off one of the candidate's files, addressed the way every
    /// other surface addresses it.
    pub fn in_file(value: String, origin: SignalOrigin, file_id: String) -> Self {
        Self {
            value,
            origin,
            origin_path: Some(file_id),
        }
    }
}
