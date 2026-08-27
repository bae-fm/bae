//! Manual metadata seeding over a candidate's physical audio layout.

use super::assemble::{
    assemble_parsed_album, AlbumArtistScope, ArtistRef, ReleaseIr, TrackIr, TrackNumber,
};
use super::folder_scanner::CategorizedFiles;
use super::track_slots::manual_track_rows;
use super::ParsedAlbum;
use crate::db::{Pressing, ReleaseMetadataSource};
use coven::{Clock, IdProvider};

/// Build blank release metadata without reading filenames, CUE titles, embedded
/// tags, or provider documents. Only track slots, disc assignment, and track
/// numbering come from the candidate because they describe its audio layout.
pub(crate) fn map_manual_candidate_to_db(
    files: &CategorizedFiles,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> ParsedAlbum {
    let tracks = manual_track_rows(files)
        .into_iter()
        .map(|track| TrackIr {
            title: String::new(),
            side: track.side,
            number: TrackNumber::Explicit(track.track_number),
            source_position: None,
            events: Vec::new(),
        })
        .collect();

    assemble_parsed_album(
        ReleaseIr {
            album_title: String::new(),
            primary_artist: ArtistRef {
                name: String::new(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            },
            additional_artists: Vec::new(),
            album_year: None,
            is_compilation: false,
            pressing: Pressing {
                year: None,
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            metadata_source: ReleaseMetadataSource::Manual,
            metadata_source_release_id: None,
            album_artist_scope: AlbumArtistScope::ReleaseCredits,
            release_roles: Vec::new(),
            tracks,
            identities: Vec::new(),
        },
        clock,
        ids,
    )
}

#[cfg(test)]
#[path = "manual_mapper_tests.rs"]
mod tests;
