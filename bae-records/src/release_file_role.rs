//! The job a release file does in its release, as the import's scan read it.

/// What a release file is to its release, as the scan of its folder read it
/// at import: the audio its tracks play from, a track sheet, an image, an
/// image whose bytes don't decode, a document of some kind, or anything else
/// the folder carried. The library keeps it because the file's content type
/// can't say: a damaged image is stored as plain bytes, and a CUE sheet, a
/// rip log and a playlist all as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseFileRole {
    /// Audio, which states its source facts.
    Audio,
    /// A CUE sheet that parsed.
    TrackSheet,
    Artwork,
    /// Named as an image, and its bytes do not decode as one.
    DamagedImage,
    /// Readable text, of the kind the scan read it as.
    Document(DocumentKind),
    /// Carried with the release, unrecognized.
    Other,
}

/// Which document a release file is: the scan reads each kind for what it
/// tells of the release. A rip log and an AccurateRip report prove a CD rip,
/// and a rip log's table of contents is the disc's Disc ID; a CUE sheet that
/// carves no tracks still says the audio was read off a disc; a text file is
/// read for the release's catalog numbers; a playlist only lists the audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DocumentKind {
    /// A CUE sheet read as text, not as a track sheet: one that did not
    /// parse, or one with no audio beside it.
    CueSheet,
    /// A ripper's log of how it read the disc.
    RipLog,
    /// The report of a rip checked against AccurateRip's database.
    AccurateRipReport,
    Playlist,
    /// Notes, a tracklist: any other text.
    Text,
}

impl ReleaseFileRole {
    /// The value the `role` column stores.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::TrackSheet => "track_sheet",
            Self::Artwork => "artwork",
            Self::DamagedImage => "damaged_image",
            Self::Document(_) => "document",
            Self::Other => "other",
        }
    }

    /// The value the `document_kind` column stores: a document's kind, and
    /// `None` for any other role.
    pub fn document_kind(self) -> Option<DocumentKind> {
        match self {
            Self::Document(kind) => Some(kind),
            Self::Audio | Self::TrackSheet | Self::Artwork | Self::DamagedImage | Self::Other => {
                None
            }
        }
    }

    /// The role the stored `role` and `document_kind` values name; `None` for
    /// a pair the columns' CHECKs refuse.
    pub fn from_db_values(role: &str, document_kind: Option<&str>) -> Option<Self> {
        match (role, document_kind) {
            ("audio", None) => Some(Self::Audio),
            ("track_sheet", None) => Some(Self::TrackSheet),
            ("artwork", None) => Some(Self::Artwork),
            ("damaged_image", None) => Some(Self::DamagedImage),
            ("document", Some(kind)) => DocumentKind::from_db_value(kind).map(Self::Document),
            ("other", None) => Some(Self::Other),
            _ => None,
        }
    }

    /// Whether the file reads as text in the document viewer: a document or
    /// a track sheet.
    pub fn is_readable_text(self) -> bool {
        matches!(self, Self::TrackSheet | Self::Document(_))
    }
}

impl DocumentKind {
    /// The value a `document_kind` column stores.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CueSheet => "cue_sheet",
            Self::RipLog => "rip_log",
            Self::AccurateRipReport => "accuraterip_report",
            Self::Playlist => "playlist",
            Self::Text => "text",
        }
    }

    /// The kind a stored `document_kind` value names; `None` for a value the
    /// column's CHECK refuses.
    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "cue_sheet" => Some(Self::CueSheet),
            "rip_log" => Some(Self::RipLog),
            "accuraterip_report" => Some(Self::AccurateRipReport),
            "playlist" => Some(Self::Playlist),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}
