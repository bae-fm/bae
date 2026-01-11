//! Candidate display type conversion

use bae_core::import::{MatchCandidate, MatchSource};
use bae_ui::display_types::MatchSourceType;

/// Convert bae MatchCandidate to bae-ui display type
pub fn to_display_candidate(candidate: &MatchCandidate) -> bae_ui::display_types::MatchCandidate {
    let (source_type, format, country, label, catalog_number, original_year) =
        match &candidate.source {
            MatchSource::MusicBrainz(release) => (
                MatchSourceType::MusicBrainz,
                release.format.clone(),
                release.country.clone(),
                release.label.clone(),
                release.catalog_number.clone(),
                release.first_release_date.clone(),
            ),
            MatchSource::Discogs(result) => (
                MatchSourceType::Discogs,
                result.format.as_ref().map(|v| v.join(", ")),
                result.country.clone(),
                result.label.as_ref().map(|v| v.join(", ")),
                None, // Discogs search results don't have catalog number
                None,
            ),
        };

    bae_ui::display_types::MatchCandidate {
        title: candidate.title(),
        artist: match &candidate.source {
            MatchSource::MusicBrainz(r) => r.artist.clone(),
            MatchSource::Discogs(r) => r.title.split(" - ").next().unwrap_or("").to_string(),
        },
        year: candidate.year(),
        cover_url: candidate.cover_art_url(),
        format,
        country,
        label,
        catalog_number,
        source_type,
        original_year,
    }
}
