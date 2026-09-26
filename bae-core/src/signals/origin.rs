//! Where a signal value came from — the provenance a signal badge labels
//! itself with ("from Cover OCR", "from the folder name", …).
//!
//! Two origins, one inside the other. A line of text is read off one of the
//! candidate's text surfaces ([`TextOrigin`]). A value — a barcode or a
//! catalog number — is read out of such a line, or is a barcode the detector
//! decoded from the bars ([`SignalOrigin`]). Each type admits only what can
//! happen to what it describes: no line of text came from the bars.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use super::candidate_text::Source;

/// The surface a line of text was read off — a coarse, UI-facing projection
/// of the internal `Source`, so a badge can say where a value came from
/// without leaking file paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TextOrigin {
    /// A CUE sheet field (`CATALOG`, `PERFORMER`/`TITLE`).
    CueSheet,
    /// Text recognized on a cover/artwork image (OCR) — a catalog number, or
    /// the digits printed under a barcode's bars.
    Artwork,
    /// The candidate's folder name — a path component or a bracketed tag.
    FolderName,
    /// A file's name.
    Filename,
    /// A `.txt` document.
    TextFile,
}

/// Where a barcode or catalog number was read: out of a line of text, or —
/// for a barcode — from the bars themselves.
///
/// `Serialize`/`Deserialize`: carried on the ledger a run records, which
/// `identify::TerminalVerdict` persists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SignalOrigin {
    /// Read out of a line of text on this surface.
    Text(TextOrigin),
    /// A barcode the detector decoded from the bars on a cover/artwork image.
    ArtworkBarcode,
}

impl From<TextOrigin> for SignalOrigin {
    fn from(origin: TextOrigin) -> Self {
        Self::Text(origin)
    }
}

impl TextOrigin {
    const ALL: [TextOrigin; 5] = [
        Self::CueSheet,
        Self::Artwork,
        Self::FolderName,
        Self::Filename,
        Self::TextFile,
    ];

    /// The stored `origin` column value of a text line — and of a value read
    /// out of one.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CueSheet => "cue_sheet",
            Self::Artwork => "artwork",
            Self::FolderName => "folder_name",
            Self::Filename => "filename",
            Self::TextFile => "text_file",
        }
    }
}

impl SignalOrigin {
    /// The stored `origin` column value of a barcode or catalog number.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text(origin) => origin.as_str(),
            Self::ArtworkBarcode => "artwork_barcode",
        }
    }
}

impl std::str::FromStr for TextOrigin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|origin| origin.as_str() == s)
            .ok_or_else(|| format!("unknown text origin: {s}"))
    }
}

impl std::str::FromStr for SignalOrigin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == Self::ArtworkBarcode.as_str() {
            return Ok(Self::ArtworkBarcode);
        }
        s.parse::<TextOrigin>()
            .map(Self::Text)
            .map_err(|_| format!("unknown signal origin: {s}"))
    }
}

/// Reading a text source's origin belongs to the extraction pass, which is
/// desktop-only; the origins themselves travel everywhere.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl TextOrigin {
    /// The path payloads on `Artwork` / `FilenameGeneric` / `TextFile` are dropped
    /// here: a badge names the kind of surface, not the file. A value that has to
    /// point at the file it was read off carries it separately, on
    /// [`SourcedValue::origin_path`].
    pub fn of_source(source: &Source) -> Self {
        match source {
            Source::Artwork { .. } => Self::Artwork,
            Source::PathComponent => Self::FolderName,
            Source::FilenameGeneric { .. } => Self::Filename,
            Source::CueField { .. } => Self::CueSheet,
            Source::TextFile { .. } => Self::TextFile,
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
///
/// `Serialize`/`Deserialize`: carried on the ledger a run records, which
/// `identify::TerminalVerdict` persists. Reading one back goes through
/// [`ImageRegion::new`] like every other way in, so a stored value that does
/// not describe a box inside the image is refused rather than admitted.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "StoredImageRegion")]
pub struct ImageRegion {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A region's four fractions as they are stored.
#[derive(Debug, serde::Deserialize)]
struct StoredImageRegion {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl TryFrom<StoredImageRegion> for ImageRegion {
    type Error = String;

    fn try_from(stored: StoredImageRegion) -> Result<Self, Self::Error> {
        ImageRegion::new(stored.x, stored.y, stored.width, stored.height)
            .ok_or_else(|| format!("{stored:?} is not a region inside the image"))
    }
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    pub fn new(value: String, origin: impl Into<SignalOrigin>) -> Self {
        Self {
            value,
            origin: origin.into(),
            origin_path: None,
            region: None,
        }
    }

    /// A value read off one of the candidate's files, addressed the way every
    /// other surface addresses it.
    pub fn in_file(value: String, origin: impl Into<SignalOrigin>, file_id: String) -> Self {
        Self {
            value,
            origin: origin.into(),
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
