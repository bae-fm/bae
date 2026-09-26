//! Fields addressed by metadata edit commands.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use super::RawReleaseEditOf;

/// One album-level text field of the metadata form.
///
/// The form's own fields, not the wire edit's: `year` is text here because the
/// field is text, and the commit is what parses it. What the pressing is is
/// chosen rather than typed; [`PressingFactEdit`] sets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateEditField {
    AlbumTitle,
    AlbumYear,
    PressingYear,
    Label,
    CatalogNumber,
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
            Self::Label => &mut draft.pressing.label,
            Self::CatalogNumber => &mut draft.pressing.catalog_number,
            Self::Barcode => &mut draft.pressing.barcode,
        };
        *slot = value.to_string();
    }
}

/// A choice of one of what the pressing is, from bae's vocabulary: the form
/// sets each with a control that offers only the values the vocabulary has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PressingFactEdit {
    Area(Option<crate::pressing::ReleaseArea>),
    Media(Vec<crate::pressing::MediaCount>),
    Status(Option<crate::pressing::ReleaseStatus>),
    Packaging(Option<crate::pressing::Packaging>),
    DiscogsDetails(Vec<crate::pressing::DiscogsDetail>),
}

/// One edit of the metadata form's album-level fields: text typed into a
/// field, or a choice of what the pressing is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftFieldEdit {
    Text {
        field: CandidateEditField,
        value: String,
    },
    PressingFact(PressingFactEdit),
}

impl DraftFieldEdit {
    /// Put this edit in the draft.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn apply<Track>(self, draft: &mut RawReleaseEditOf<Track>) {
        match self {
            Self::Text { field, value } => field.set(draft, &value),
            Self::PressingFact(choice) => choice.apply(&mut draft.pressing.facts),
        }
    }
}

impl PressingFactEdit {
    /// Put this choice in `facts`. A media count of zero is no medium, so the
    /// list keeps only carriers with a count; a detail is kept once.
    pub fn apply(self, facts: &mut crate::pressing::PressingFacts) {
        match self {
            Self::Area(area) => facts.area = area,
            Self::Media(media) => {
                facts.media = crate::pressing::MediaCount::tally(
                    media
                        .into_iter()
                        .filter(|counted| counted.count > 0)
                        .map(|counted| (counted.medium, counted.count)),
                )
            }
            Self::Status(status) => facts.status = status,
            Self::Packaging(packaging) => facts.packaging = packaging,
            Self::DiscogsDetails(details) => {
                let mut kept = Vec::new();
                for detail in details {
                    if !kept.contains(&detail) {
                        kept.push(detail);
                    }
                }
                facts.discogs_details = kept;
            }
        }
    }
}
