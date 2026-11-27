//! Fields addressed by metadata edit commands.

#[cfg(desktop)]
use super::CandidateDraft;

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
}

impl CandidateEditField {
    /// Put `value` in this field of the draft.
    #[cfg(desktop)]
    pub(crate) fn set(self, draft: &mut CandidateDraft, value: &str) {
        let slot = match self {
            Self::AlbumTitle => &mut draft.album_title,
            Self::AlbumYear => &mut draft.album_year,
            Self::PressingYear => &mut draft.pressing.year,
        };
        *slot = value.to_string();
    }
}

/// A choice of one of what the pressing is, from bae's vocabulary: the form
/// sets each with a control that offers only the values the vocabulary has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PressingFactEdit {
    Area(Option<bae_types::pressing::ReleaseArea>),
    /// How many of one carrier the pressing holds: a count of zero removes
    /// the carrier, and a carrier the pressing lacks is added at the end.
    MediumCount(bae_types::pressing::MediaCount),
    Status(Option<bae_types::pressing::ReleaseStatus>),
    Packaging(Option<bae_types::pressing::Packaging>),
    /// Add one Discogs detail after the others; one the pressing holds stays
    /// where it is.
    AddDiscogsDetail(bae_types::pressing::DiscogsDetail),
    /// Remove one Discogs detail; removing one the pressing lacks changes
    /// nothing.
    RemoveDiscogsDetail(bae_types::pressing::DiscogsDetail),
}

/// One edit of the metadata form's album-level fields: text typed into a
/// field, the label or barcode rows as the person left them, or a choice of
/// what the pressing is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftFieldEdit {
    Text {
        field: CandidateEditField,
        value: String,
    },
    /// Every label row, in order.
    Labels(Vec<super::RawLabelEdit>),
    /// Every barcode row, in order.
    Barcodes(Vec<String>),
    PressingFact(PressingFactEdit),
}

impl DraftFieldEdit {
    /// Put this edit in the draft.
    #[cfg(desktop)]
    pub(crate) fn apply(self, draft: &mut CandidateDraft) {
        match self {
            Self::Text { field, value } => field.set(draft, &value),
            Self::Labels(labels) => {
                draft.pressing.labels =
                    super::RawLabelEdit::kept_ids(labels, &draft.pressing.labels);
            }
            Self::Barcodes(barcodes) => draft.pressing.barcodes = barcodes,
            Self::PressingFact(choice) => choice.apply(&mut draft.pressing.facts),
        }
    }
}

impl PressingFactEdit {
    /// Put this choice in `facts`. A media count of zero is no medium, so the
    /// list keeps only carriers with a count; a detail is kept once.
    pub fn apply(self, facts: &mut bae_types::pressing::PressingFacts) {
        match self {
            Self::Area(area) => facts.area = area,
            Self::MediumCount(bae_types::pressing::MediaCount { medium, count }) => {
                match facts.media.iter().position(|counted| counted.medium == medium) {
                    Some(at) if count == 0 => {
                        facts.media.remove(at);
                    }
                    Some(at) => facts.media[at].count = count,
                    None if count > 0 => {
                        facts.media.push(bae_types::pressing::MediaCount { medium, count })
                    }
                    None => {}
                }
            }
            Self::Status(status) => facts.status = status,
            Self::Packaging(packaging) => facts.packaging = packaging,
            Self::AddDiscogsDetail(detail) => {
                if !facts.discogs_details.contains(&detail) {
                    facts.discogs_details.push(detail);
                }
            }
            Self::RemoveDiscogsDetail(detail) => {
                facts.discogs_details.retain(|held| *held != detail)
            }
        }
    }
}

#[cfg(test)]
#[path = "candidate_edit_field_tests.rs"]
mod tests;
