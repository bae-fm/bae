use super::*;
use bae_catalogs::catalog::Catalog;

fn album(catalog: Catalog, key: &str) -> MetadataRef {
    MetadataRef::new(catalog, key)
}

#[test]
fn a_link_names_albums_in_catalog_order() {
    let link = AlbumLink::new([album(Catalog::MusicBrainz, "mb"), album(Catalog::Discogs, "dg")])
        .expect("albums in catalog order");
    assert_eq!(link.albums().len(), 2);
}

#[test]
fn a_link_refuses_albums_out_of_catalog_order() {
    assert_eq!(
        AlbumLink::new([album(Catalog::Discogs, "dg"), album(Catalog::MusicBrainz, "mb")]),
        None
    );
}

#[test]
fn a_link_refuses_no_album_and_two_albums_of_one_catalog() {
    assert_eq!(AlbumLink::new([]), None);
    assert_eq!(
        AlbumLink::new([album(Catalog::MusicBrainz, "a"), album(Catalog::MusicBrainz, "b")]),
        None
    );
}
