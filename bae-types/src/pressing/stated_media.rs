//! What a record says a pressing is made of, as it says it.
//!
//! The two catalogs state media in two shapes: MusicBrainz lists every
//! medium with one format each, Discogs lists format entries each with a
//! quantity. Both are kept in their own shape because the shape is evidence —
//! a MusicBrainz list accounts for every medium, a Discogs list names every
//! format the release has — and [`StatedMedia::counts`] is what a surface
//! shows of either.

use super::{MediaCount, Medium};

/// What a record says a pressing is made of.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StatedMedia {
    /// The response describes no media: a `ws/2/release?query=` result with
    /// no media, or a Discogs record with no formats.
    Undescribed,
    /// One entry per medium the record lists, in its order: the carrier its
    /// format names, `None` where it names none bae knows — no format, a
    /// format that names no carrier, or a name outside MusicBrainz's list. A
    /// MusicBrainz release.
    PerMedium(Vec<Option<Medium>>),
    /// One entry per format entry of the record that is a medium, in its
    /// order, with the quantity it states. A Discogs release.
    Formats(Vec<StatedFormat>),
}

/// One Discogs format entry that is a medium: the carrier it names, `None`
/// where it names none bae knows — a hybrid disc whose kind is not stated, a
/// name outside Discogs's list — and how many of it the release holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StatedFormat {
    pub medium: Option<Medium>,
    pub quantity: u32,
}

impl StatedMedia {
    /// The carriers the record names, each with how many of it there are, in
    /// the order each first appears. An entry that names no carrier bae knows
    /// is not counted: there is nothing to call it.
    pub fn counts(&self) -> Vec<MediaCount> {
        match self {
            Self::Undescribed => Vec::new(),
            Self::PerMedium(media) => MediaCount::tally(media.iter().flatten().map(|m| (*m, 1))),
            Self::Formats(formats) => MediaCount::tally(
                formats
                    .iter()
                    .filter_map(|format| Some((format.medium?, format.quantity))),
            ),
        }
    }

    /// Every entry the record lists, as the carrier it names — `None` where
    /// it names none bae knows. Empty for a record that describes no media.
    pub fn entries(&self) -> Vec<Option<Medium>> {
        match self {
            Self::Undescribed => Vec::new(),
            Self::PerMedium(media) => media.clone(),
            Self::Formats(formats) => formats.iter().map(|format| format.medium).collect(),
        }
    }

    /// Whether any stated carrier is one `is` picks out.
    pub fn any(&self, is: impl Fn(Medium) -> bool) -> bool {
        match self {
            Self::Undescribed => false,
            Self::PerMedium(media) => media.iter().flatten().any(|medium| is(*medium)),
            Self::Formats(formats) => formats.iter().filter_map(|format| format.medium).any(is),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_add_up_each_carrier_in_first_appearance_order() {
        let per_medium = StatedMedia::PerMedium(vec![
            Some(Medium::Cd),
            None,
            Some(Medium::Dvd),
            Some(Medium::Cd),
        ]);
        assert_eq!(
            per_medium.counts(),
            vec![
                MediaCount {
                    medium: Medium::Cd,
                    count: 2
                },
                MediaCount {
                    medium: Medium::Dvd,
                    count: 1
                },
            ]
        );
        let formats = StatedMedia::Formats(vec![
            StatedFormat {
                medium: Some(Medium::Vinyl),
                quantity: 1,
            },
            StatedFormat {
                medium: None,
                quantity: 1,
            },
            StatedFormat {
                medium: Some(Medium::Vinyl),
                quantity: 3,
            },
        ]);
        assert_eq!(
            formats.counts(),
            vec![MediaCount {
                medium: Medium::Vinyl,
                count: 4
            }]
        );
        assert!(StatedMedia::Undescribed.counts().is_empty());
    }
}
