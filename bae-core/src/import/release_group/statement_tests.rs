//! How the statements that say what a MusicBrainz album is on Discogs put
//! the two catalogs' albums on one card — whichever statement it is, through
//! the same album link — and how a twin read through a release's link sits on
//! the row of the release that names it.

use super::tests::*;
use super::*;
use crate::import::album_links::{AlbumLink, AlbumLinks, AlbumStatement};

/// `release`'s group is `master` on Discogs because `release` links `twin`
/// and `twin`'s own document files it under `master`.
fn stated_through(mut release: MetadataResult, twin: &str, master: &str) -> MetadataResult {
    release.links = vec![MetadataRef::new(Catalog::Discogs, twin)];
    release.album_links = AlbumLinks::Read(vec![AlbumLink {
        album: MetadataRef::new(Catalog::Discogs, master),
        stated: AlbumStatement::Release {
            musicbrainz_release: release.release_id.clone(),
            twin: MetadataRef::new(Catalog::Discogs, twin),
        },
    }]);
    release
}

/// The case the statement exists for: the disc ID names one pressing on
/// MusicBrainz, the barcode names two others on Discogs, and the release
/// group's page links no master. The MusicBrainz release links its Discogs
/// record, which is filed under the barcode's master — so one card carries
/// both albums, and the twin is on the MusicBrainz release's row.
#[test]
fn a_release_link_s_master_joins_the_albums_and_its_twin_joins_the_row() {
    let groups = grouped(vec![
        stated_through(mb("mb-us", Some("group-a"), Some(2006)), "dg-us", "master-7"),
        discogs("dg-eu-1", Some("master-7"), Some(2006)),
        discogs("dg-eu-2", Some("master-7"), Some(2006)),
        discogs("dg-us", Some("master-7"), Some(2006)),
    ]);
    assert_eq!(groups.len(), 1, "one album, one card");
    assert_eq!(
        groups[0]
            .sources
            .iter()
            .map(|source| source.source)
            .collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz, Catalog::Discogs]
    );
    assert_eq!(
        rows(&groups),
        vec![vec!["mb-us", "dg-us"], vec!["dg-eu-1"], vec!["dg-eu-2"]]
    );
}

/// A twin filed under another master than any listed one joins its own
/// master's album to the MusicBrainz release group, and nothing else: the
/// listed master stays its own card.
#[test]
fn a_twin_under_another_master_joins_no_listed_album() {
    let groups = grouped(vec![
        stated_through(mb("mb-us", Some("group-a"), Some(2006)), "dg-us", "master-8"),
        discogs("dg-eu-1", Some("master-7"), Some(2006)),
        discogs("dg-us", Some("master-8"), Some(2006)),
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(rows(&groups[..1]), vec![vec!["mb-us", "dg-us"]]);
    assert_eq!(rows(&groups[1..]), vec![vec!["dg-eu-1"]]);
}

/// A Wikidata item's statement joins through the same album link a page's
/// does.
#[test]
fn a_wikidata_statement_joins_the_albums_as_a_page_does() {
    let mut stated = mb("mb-1", Some("group-a"), Some(2006));
    stated.album_links = AlbumLinks::Read(vec![AlbumLink {
        album: MetadataRef::new(Catalog::Discogs, "master-7"),
        stated: AlbumStatement::Wikidata {
            item: "Q1".to_string(),
        },
    }]);
    let groups = grouped(vec![stated, discogs("dg-1", Some("master-7"), Some(2007))]);
    assert_eq!(groups.len(), 1);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}
