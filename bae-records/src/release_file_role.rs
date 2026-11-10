//! The job a release file does in its release, as the import's scan read it.

/// What a release file is to its release, as the scan of its folder read it
/// at import: the audio its tracks play from, a track sheet, an image, an
/// image whose bytes don't decode, a document, or anything else the folder
/// carried. The library keeps it because the file's content type can't say:
/// a damaged image is stored as plain bytes, and a CUE sheet as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseFileRole {
    /// Audio, which states its source facts.
    Audio,
    /// A CUE sheet that parsed.
    TrackSheet,
    Artwork,
    /// Named as an image, and its bytes do not decode as one.
    DamagedImage,
    /// Readable text: a rip log, a tracklist, a playlist, or a CUE sheet that
    /// did not parse.
    Document,
    /// Carried with the release, unrecognized.
    Other,
}

impl ReleaseFileRole {
    /// The value the `role` column stores.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::TrackSheet => "track_sheet",
            Self::Artwork => "artwork",
            Self::DamagedImage => "damaged_image",
            Self::Document => "document",
            Self::Other => "other",
        }
    }

    /// The role a stored `role` value names; `None` for a value the column's
    /// CHECK refuses.
    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "audio" => Some(Self::Audio),
            "track_sheet" => Some(Self::TrackSheet),
            "artwork" => Some(Self::Artwork),
            "damaged_image" => Some(Self::DamagedImage),
            "document" => Some(Self::Document),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    /// Whether the file reads as text in the document viewer: a document or
    /// a track sheet.
    pub fn is_readable_text(self) -> bool {
        matches!(self, Self::TrackSheet | Self::Document)
    }
}
