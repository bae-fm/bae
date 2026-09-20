use super::*;

#[test]
fn catalog_links_preserve_the_known_entity_kind() {
    use bae_core::import::{Catalog, MetadataRef, ReleaseRecord};
    for (catalog, key, album_url, release_url) in [
        (
            Catalog::MusicBrainz,
            "entity-1",
            "https://musicbrainz.org/release-group/entity-1",
            "https://musicbrainz.org/release/entity-1",
        ),
        (
            Catalog::Discogs,
            "42",
            "https://www.discogs.com/master/42",
            "https://www.discogs.com/release/42",
        ),
    ] {
        let identity = MetadataRef::new(catalog, key);
        let album = BridgeReleaseRecord::from_core(ReleaseRecord::album(&identity));
        let pressing = BridgeReleaseRecord::from_core(ReleaseRecord::new(&identity, None, true));
        assert_eq!(album.url, album_url);
        assert_eq!(pressing.url, release_url);
        assert_eq!(album.catalog, pressing.catalog);
    }
}
