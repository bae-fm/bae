//! Fields addressed by metadata edit commands.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use super::RawReleaseEditOf;

/// One album-level field of the metadata form.
///
/// The form's own fields, not the wire edit's: `year` is text here because the
/// field is text, and the commit is what parses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateEditField {
    AlbumTitle,
    AlbumYear,
    PressingYear,
    Format,
    Label,
    CatalogNumber,
    Country,
    Barcode,
}

impl CandidateEditField {
    /// Put `value` in this field of the draft.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn set<Track>(self, draft: &mut RawReleaseEditOf<Track>, value: &str) {
        let slot = match self {
            Self::AlbumTitle => &mut draft.album_title,
            Self::AlbumYear => &mut draft.album_year,
            Self::PressingYear => &mut draft.pressing.year,
            Self::Format => &mut draft.pressing.format,
            Self::Label => &mut draft.pressing.label,
            Self::CatalogNumber => &mut draft.pressing.catalog_number,
            Self::Country => &mut draft.pressing.country,
            Self::Barcode => &mut draft.pressing.barcode,
        };
        *slot = value.to_string();
    }
}
