//! Where what extraction read came from: the surface a line of text was read
//! off, and the file a barcode was read off.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use super::candidate_text::Source;

/// The surface a line of text was read off, which decides how ranking reads
/// the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOrigin {
    /// A CUE sheet field (`CATALOG`, `PERFORMER`/`TITLE`).
    CueSheet,
    /// Text recognized on a cover/artwork image.
    Artwork,
    /// The candidate's folder name — a path component or a bracketed tag.
    FolderName,
    /// A file's name.
    Filename,
    /// A `.txt` document.
    TextFile,
}

impl TextOrigin {
    const ALL: [TextOrigin; 5] = [
        Self::CueSheet,
        Self::Artwork,
        Self::FolderName,
        Self::Filename,
        Self::TextFile,
    ];

    /// The stored `origin` column value of a text line.
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

impl std::str::FromStr for TextOrigin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|origin| origin.as_str() == s)
            .ok_or_else(|| format!("unknown text origin: {s}"))
    }
}

/// Extraction, which reads the sources, is desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl TextOrigin {
    /// The kind of surface `source` is, without its file.
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

/// A barcode and the file it was read off; a code on two images is two of
/// these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedValue {
    pub value: String,
    /// The candidate-relative path of the file; `None` for a library
    /// release's stored images.
    pub origin_path: Option<String>,
}

impl SourcedValue {
    /// A value read off no file of the candidate's.
    pub fn new(value: String) -> Self {
        Self {
            value,
            origin_path: None,
        }
    }

    /// A value read off one of the candidate's files.
    pub fn in_file(value: String, file_id: String) -> Self {
        Self {
            value,
            origin_path: Some(file_id),
        }
    }

    /// Each value among `sightings` once, in first-seen order.
    pub fn values(sightings: &[SourcedValue]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for sighting in sightings {
            if !out.contains(&sighting.value) {
                out.push(sighting.value.clone());
            }
        }
        out
    }
}
