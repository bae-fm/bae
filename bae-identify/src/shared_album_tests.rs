use super::*;
use crate::release_group::{group_results, unranked};
use bae_catalogs::search::MetadataResult;

fn release(
    catalog: LookupCatalog,
    release_id: &str,
    album: Option<&str>,
    year: i32,
) -> MetadataResult {
    MetadataResult {
        source: catalog,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        year: Some(year),
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: bae_types::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: album.map(str::to_string),
        album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
        source_tracks: None,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    }
}

fn cards(results: Vec<MetadataResult>) -> Vec<ReleaseGroup> {
    group_results(unranked(results), None)
}

#[test]
fn several_pressings_of_one_album_name_it_in_each_catalog() {
    let shared = SharedAlbum::of(&cards(vec![
        release(LookupCatalog::MusicBrainz, "mb-1", Some("group-1"), 1990),
        release(LookupCatalog::MusicBrainz, "mb-2", Some("group-1"), 2001),
    ]))
    .expect("two pressings of one album");
    assert_eq!(shared.link.albums(), [MetadataRef::new(Catalog::MusicBrainz, "group-1")]);
    assert_eq!(
        shared.leads,
        vec![
            LookupRef::new(LookupCatalog::MusicBrainz, "mb-1"),
            LookupRef::new(LookupCatalog::MusicBrainz, "mb-2"),
        ],
        "the rows as the list orders them"
    );
}

#[test]
fn one_pressing_offers_no_album_to_be_unsure_about() {
    assert_eq!(
        SharedAlbum::of(&cards(vec![release(
            LookupCatalog::MusicBrainz,
            "mb-1",
            Some("group-1"),
            1990
        )])),
        None
    );
}

#[test]
fn pressings_of_two_albums_share_none() {
    assert_eq!(
        SharedAlbum::of(&cards(vec![
            release(LookupCatalog::MusicBrainz, "mb-1", Some("group-1"), 1990),
            release(LookupCatalog::MusicBrainz, "mb-2", Some("group-2"), 2001),
        ])),
        None
    );
}

#[test]
fn pressings_no_catalog_files_under_an_album_share_none() {
    assert_eq!(
        SharedAlbum::of(&cards(vec![
            release(LookupCatalog::Discogs, "dg-1", None, 1990),
            release(LookupCatalog::Discogs, "dg-2", None, 2001),
        ])),
        None
    );
}
