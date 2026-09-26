//! How a card's heading cover is picked from its pressings' covers.

use super::tests::{discogs, grouped, linked, mb, rows};
use super::*;

fn cover() -> RemoteCover {
    RemoteCover {
        image: crate::import::cover_art::RemoteImageSet::with_copies(
            "https://caa.example/front.jpg".to_string(),
            vec![crate::import::cover_art::DownscaledCopy {
                url: "https://caa.example/thumb.jpg".to_string(),
                max_edge: 250,
            }],
        ),
        label: Catalog::MusicBrainz.cover_source_label().to_string(),
        source: Catalog::MusicBrainz,
        standing: crate::import::cover_art::CoverStanding::Stated,
    }
}

#[test]
fn representative_cover_preserves_remote_cover_pair() {
    let cover = cover();
    let mut first = mb("rel-1", Some("group-x"), Some(1992));
    first.cover_art = Some(cover.clone());

    let groups = grouped(vec![first, mb("rel-2", Some("group-x"), Some(1994))]);

    assert_eq!(groups[0].cover_art, Some(cover));
}

/// A merged card takes its cover from MusicBrainz when both sources offer
/// one, whichever bucket was seen first.
#[test]
fn a_merged_card_prefers_the_musicbrainz_cover() {
    let mut discogs_covered = discogs("dg-1", Some("master-7"), Some(2001));
    discogs_covered.cover_art = Some(RemoteCover {
        image: crate::import::cover_art::RemoteImageSet::with_copies(
            "https://discogs.example/front.jpg".to_string(),
            vec![crate::import::cover_art::DownscaledCopy {
                url: "https://discogs.example/thumb.jpg".to_string(),
                max_edge: 150,
            }],
        ),
        label: Catalog::Discogs.cover_source_label().to_string(),
        source: Catalog::Discogs,
        standing: crate::import::cover_art::CoverStanding::Stated,
    });
    let mut mb_covered = linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7");
    mb_covered.cover_art = Some(cover());

    let groups = grouped(vec![discogs_covered, mb_covered]);
    assert_eq!(groups[0].cover_art, Some(cover()));
}

/// A card's heading shows a cover a record states over an address a record
/// said nothing about, whichever pressing surfaced first: here a MusicBrainz
/// search result leads the card, and its archive address — which may hold
/// nothing — gives way to the image a Discogs record lists.
#[test]
fn a_card_heading_prefers_a_stated_cover_to_an_unstated_address() {
    let discogs_cover = |id: &str| {
        crate::discogs::remote_cover_from_urls(
            Some(&format!("https://discogs.example/{id}.jpg")),
            Some(&format!("https://discogs.example/{id}-150.jpg")),
            "release",
            1,
        )
    };
    let cd = |mut release: MetadataResult, format: &str| {
        release.format = Some(format.to_string());
        release
    };
    let mut mb_2001 = cd(
        linked(mb("mb-2001", Some("group-x"), Some(2001)), "master-7"),
        "CD",
    );
    mb_2001.barcodes = vec!["012345678905".to_string()];
    mb_2001.cover_art = Some(RemoteCover::musicbrainz_release("mb-2001"));
    let mut dg_2001 = cd(discogs("dg-2001", Some("master-7"), Some(2001)), "CD");
    dg_2001.barcodes = vec!["012345678905".to_string()];
    dg_2001.cover_art = discogs_cover("dg-2001");
    let mut dg_1988 = cd(discogs("dg-1988", Some("master-7"), Some(1988)), "LP");
    dg_1988.cover_art = discogs_cover("dg-1988");
    let mut mb_undated = cd(
        linked(mb("mb-undated", Some("group-x"), None), "master-7"),
        "CD",
    );
    mb_undated.cover_art = Some(RemoteCover::musicbrainz_release("mb-undated"));

    let groups = grouped(vec![mb_2001, dg_2001.clone(), dg_1988, mb_undated]);

    assert_eq!(groups.len(), 1, "one album: {:?}", rows(&groups));
    assert_eq!(groups[0].cover_art, dg_2001.cover_art);
}

/// With no record stating a cover, the heading shows the first unstated
/// address; a record stating it has none offers nothing.
#[test]
fn a_card_heading_falls_back_to_an_unstated_address() {
    let mut stated_none = mb("mb-none", Some("group-x"), Some(1990));
    stated_none.cover_art = None;
    let mut unstated = mb("mb-search", Some("group-x"), Some(1991));
    unstated.cover_art = Some(RemoteCover::musicbrainz_release("mb-search"));

    let groups = grouped(vec![stated_none, unstated]);

    assert_eq!(
        groups[0].cover_art,
        Some(RemoteCover::musicbrainz_release("mb-search"))
    );
}
