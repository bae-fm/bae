//! What kind of file a release file is, and where it sits in the release's
//! folder, as a file's details show them.

use bae_util::content_type::ContentType;

/// What kind of file a release file is, from its content type and, for the
/// text files a rip leaves, its extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    /// Audio, with its format's name ("FLAC").
    Audio {
        format: String,
    },
    /// An image, with its format's name ("JPEG").
    Image {
        format: String,
    },
    CueSheet,
    /// A ripper's log of how it read the disc.
    RipLog,
    Text,
    Pdf,
    Other,
}

impl FileKind {
    pub(crate) fn of(content_type: &ContentType, filename: &str) -> Self {
        let extension = std::path::Path::new(filename)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        match (content_type, extension.as_deref()) {
            (content_type, _) if content_type.is_audio() => Self::Audio {
                format: content_type.display_name().to_string(),
            },
            (content_type, _) if content_type.is_image() => Self::Image {
                format: content_type.display_name().to_string(),
            },
            (_, Some("cue")) => Self::CueSheet,
            (_, Some("log")) => Self::RipLog,
            (ContentType::Pdf, _) => Self::Pdf,
            (ContentType::PlainText, _) | (_, Some("txt")) => Self::Text,
            _ => Self::Other,
        }
    }
}

/// A file's place in its release's folder: its own name, and the folders
/// above it (`Disc 1`), `None` for a file at the folder's top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePlace {
    pub name: String,
    pub folder: Option<String>,
}

impl FilePlace {
    /// `original_filename` is the file's path in its release's folder, with
    /// `/` between folders, as import stores it.
    pub(crate) fn of(original_filename: &str) -> Self {
        match original_filename.rsplit_once('/') {
            Some((folder, name)) => Self {
                name: name.to_string(),
                folder: Some(folder.to_string()),
            },
            None => Self {
                name: original_filename.to_string(),
                folder: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_kind_reads_its_content_type_and_a_rips_extensions() {
        assert_eq!(
            FileKind::of(&ContentType::Flac, "01 Track Title.flac"),
            FileKind::Audio {
                format: "FLAC".into()
            }
        );
        assert_eq!(
            FileKind::of(&ContentType::Jpeg, "front.jpg"),
            FileKind::Image {
                format: "JPEG".into()
            }
        );
        assert_eq!(
            FileKind::of(&ContentType::PlainText, "Album Title.cue"),
            FileKind::CueSheet
        );
        assert_eq!(
            FileKind::of(&ContentType::OctetStream, "Album Title.LOG"),
            FileKind::RipLog
        );
        assert_eq!(
            FileKind::of(&ContentType::PlainText, "notes.txt"),
            FileKind::Text
        );
        assert_eq!(
            FileKind::of(&ContentType::Pdf, "booklet.pdf"),
            FileKind::Pdf
        );
        assert_eq!(
            FileKind::of(&ContentType::OctetStream, "data.bin"),
            FileKind::Other
        );
    }

    #[test]
    fn a_file_place_splits_its_folders_from_its_name() {
        assert_eq!(
            FilePlace::of("Disc 1/07 Track Title.flac"),
            FilePlace {
                name: "07 Track Title.flac".into(),
                folder: Some("Disc 1".into()),
            }
        );
        assert_eq!(
            FilePlace::of("Scans/Booklet/page 1.jpg").folder.as_deref(),
            Some("Scans/Booklet")
        );
        assert_eq!(
            FilePlace::of("cover.jpg"),
            FilePlace {
                name: "cover.jpg".into(),
                folder: None,
            }
        );
    }
}
