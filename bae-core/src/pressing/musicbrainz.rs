//! A MusicBrainz release's pressing facts, read into bae's vocabulary.
//!
//! A release document and a search result state the same fields — a country
//! code, a status, a packaging, a format per medium — so both are read here,
//! and a name outside MusicBrainz's lists is logged and left out rather than
//! carried as text.

use super::medium::Lookup;
use super::stated_media::StatedMedia;
use super::{Medium, Packaging, PressingFacts, ReleaseArea, ReleaseStatus};
use tracing::warn;

/// What a MusicBrainz release states about its pressing, as the document
/// writes it.
pub(crate) struct Stated<'a, Media> {
    pub(crate) release_id: &'a str,
    pub(crate) country: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) packaging: Option<&'a str>,
    /// Each medium's format, in medium order.
    pub(crate) media: Media,
}

/// A MusicBrainz release's pressing facts, and its media in their own shape.
pub(crate) fn read<'a>(
    stated: Stated<'a, impl IntoIterator<Item = Option<&'a str>>>,
) -> (PressingFacts, StatedMedia) {
    let release_id = stated.release_id;
    let media = media(release_id, stated.media);
    let facts = PressingFacts {
        area: stated.country.and_then(|code| {
            let area = ReleaseArea::musicbrainz(code);
            if area.is_none() {
                warn!(
                    musicbrainz_release_id = release_id,
                    country = code,
                    "a MusicBrainz release country is outside MusicBrainz's country list"
                );
            }
            area
        }),
        media: media.counts(),
        status: stated.status.and_then(|name| {
            let status = ReleaseStatus::musicbrainz(name);
            if status.is_none() {
                warn!(
                    musicbrainz_release_id = release_id,
                    status = name,
                    "a MusicBrainz release status is outside MusicBrainz's status list"
                );
            }
            status
        }),
        packaging: stated.packaging.and_then(|name| {
            let packaging = Packaging::musicbrainz(name);
            if packaging.is_none() {
                warn!(
                    musicbrainz_release_id = release_id,
                    packaging = name,
                    "a MusicBrainz release packaging is outside MusicBrainz's packaging list"
                );
            }
            packaging
        }),
        // Only Discogs states details beyond the fields.
        discogs_details: Vec::new(),
    };
    (facts, media)
}

/// A MusicBrainz release's media, one entry per medium it lists.
pub(crate) fn media<'a>(
    release_id: &str,
    formats: impl IntoIterator<Item = Option<&'a str>>,
) -> StatedMedia {
    let media: Vec<Option<Medium>> = formats
        .into_iter()
        .map(|format| medium(release_id, format))
        .collect();
    if media.is_empty() {
        StatedMedia::Undescribed
    } else {
        StatedMedia::PerMedium(media)
    }
}

/// The carrier one MusicBrainz medium's format names, `None` where it names
/// none bae knows.
pub(crate) fn medium(release_id: &str, format: Option<&str>) -> Option<Medium> {
    let format = format?;
    match Medium::musicbrainz(format) {
        Lookup::Carrier(medium) => Some(medium),
        Lookup::NamesNoCarrier => None,
        Lookup::Unrecognized => {
            warn!(
                musicbrainz_release_id = release_id,
                format,
                "a MusicBrainz medium format is outside MusicBrainz's format list"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pressing::{Country, MediaCount, Region};

    /// A release modelled on a real Japanese CD issue: an ISO code, the
    /// Official status, a jewel case, one CD.
    #[test]
    fn a_release_reads_as_typed_facts() {
        let (facts, media) = read(Stated {
            release_id: "r",
            country: Some("JP"),
            status: Some("Official"),
            packaging: Some("Jewel Case"),
            media: [Some("CD")],
        });
        assert_eq!(
            facts,
            PressingFacts {
                area: Some(ReleaseArea::Country(Country::from_code("JP").unwrap())),
                media: vec![MediaCount {
                    medium: Medium::Cd,
                    count: 1
                }],
                status: Some(ReleaseStatus::Official),
                packaging: Some(Packaging::JewelCase),
                discogs_details: Vec::new(),
            }
        );
        assert_eq!(media, StatedMedia::PerMedium(vec![Some(Medium::Cd)]));
    }

    #[test]
    fn every_medium_is_counted_not_only_the_first() {
        let (facts, _) = read(Stated {
            release_id: "r",
            country: Some("XE"),
            status: None,
            packaging: None,
            media: [Some("CD"), Some("CD"), Some("DVD-Video"), None],
        });
        assert_eq!(
            facts.media,
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
        assert_eq!(facts.area, Some(ReleaseArea::Region(Region::Europe)));
    }

    #[test]
    fn a_name_outside_the_lists_is_left_out() {
        let (facts, media) = read(Stated {
            release_id: "r",
            country: Some("ZZ"),
            status: Some("Leaked"),
            packaging: Some("Crate"),
            media: [Some("Wax Tablet")],
        });
        assert_eq!(facts, PressingFacts::default());
        assert_eq!(media, StatedMedia::PerMedium(vec![None]));
    }
}
