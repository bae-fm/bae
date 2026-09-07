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
            Source::Artwork { .. } => SignalOrigin::Artwork,
            Source::PathComponent => SignalOrigin::FolderName,
            Source::FilenameGeneric { .. } => SignalOrigin::Filename,
            Source::CueField => SignalOrigin::CueSheet,
            Source::TextFile { .. } => SignalOrigin::TextFile,
        }
    }
}

/// Where on an image a value was read: the box the detector drew around the
/// barcode or the line of text, as fractions of the image's width and height
/// with the origin at the top-left corner. A surface crops the image to it to
/// show the printed value itself rather than the whole scan.
///
/// Built only through [`ImageRegion::new`], which admits only finite fractions
/// inside the image, so two regions compare exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageRegion {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Eq for ImageRegion {}

impl ImageRegion {
    /// A region inside the image, or `None` for one a detector reported
    /// outside it or as no number at all — a crop of that would show nothing.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Option<Self> {
        let unit = 0.0..=1.0;
        let inside = [x, y, width, height]
            .iter()
            .all(|value| value.is_finite() && unit.contains(value));
        if !inside || width <= 0.0 || height <= 0.0 || x + width > 1.0 || y + height > 1.0 {
            return None;
        }
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }
}

/// A catalog number or barcode paired with where it was harvested from, so a badge
/// can show its `value` and explain its `origin`.
///
/// One sighting: the same value read off two images is two of these, each
/// naming its own file and region. A surface that lists values folds the
/// sightings of one value together; a surface that puts chips on files reads
/// them one by one.
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
    /// Where on the image the value was read, for an origin that is an image
    /// and a detector that reports where it looked. `None` for every other
    /// origin, and for a detector that reports payloads alone.
    pub region: Option<ImageRegion>,
}

impl SourcedValue {
    /// A value whose origin names no file to point at.
    pub fn new(value: String, origin: SignalOrigin) -> Self {
        Self {
            value,
            origin,
            origin_path: None,
            region: None,
        }
    }

    /// A value read off one of the candidate's files, addressed the way every
    /// other surface addresses it.
    pub fn in_file(value: String, origin: SignalOrigin, file_id: String) -> Self {
        Self {
            value,
            origin,
            origin_path: Some(file_id),
            region: None,
        }
    }

    /// The same sighting, with where on its image it was read.
    pub fn at(mut self, region: Option<ImageRegion>) -> Self {
        self.region = region;
        self
    }
}
