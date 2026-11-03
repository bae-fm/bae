//! A MusicBrainz release's pressing facts, read into bae's vocabulary.
//!
//! A release document and a search result state the same fields — a country
//! code, a status, a packaging, a format per medium — so both are read here,
//! and a name outside MusicBrainz's lists is logged and left out rather than
//! carried as text.

use bae_types::pressing::medium::Lookup;
use bae_types::pressing::stated_media::StatedMedia;
use bae_types::pressing::{Medium, Packaging, PressingFacts, ReleaseArea, ReleaseStatus};
use tracing::warn;

/// What a MusicBrainz release states about its pressing, as the document
/// writes it.
pub struct Stated<'a, Media> {
    pub release_id: &'a str,
    pub country: Option<&'a str>,
    pub status: Option<&'a str>,
    pub packaging: Option<&'a str>,
    /// Each medium's format, in medium order.
    pub media: Media,
}

/// A MusicBrainz release's pressing facts, and its media in their own shape.
pub fn read<'a>(
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
pub fn media<'a>(
    release_id: &str,
    formats: impl IntoIterator<Item = Option<&'a str>>,
) -> StatedMedia {
    let media: Vec<Option<Medium>> =
        formats.into_iter().map(|format| medium(release_id, format)).collect();
    if media.is_empty() {
        StatedMedia::Undescribed
    } else {
        StatedMedia::PerMedium(media)
    }
}

/// The carrier one MusicBrainz medium's format names, `None` where it names
/// none bae knows.
pub fn medium(release_id: &str, format: Option<&str>) -> Option<Medium> {
    let format = format?;
    match Medium::musicbrainz(format) {
        Lookup::Carrier(medium) => Some(medium),
        Lookup::NamesNoCarrier => None,
        Lookup::Unrecognized => {
            warn!(
                musicbrainz_release_id = release_id,
                format, "a MusicBrainz medium format is outside MusicBrainz's format list"
            );
            None
        }
    }
}

#[cfg(test)]
#[path = "pressing_tests.rs"]
mod tests;
