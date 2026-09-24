use super::*;
use crate::import::album_links::AlbumLinks;
use crate::import::search::StatedMedia;

/// The tests are about how results bucket, pair and order by year, none of
/// which the candidate's text takes part in.
pub(super) fn grouped(results: Vec<MetadataResult>) -> Vec<ReleaseGroup> {
    group_results(unranked(results))
}

pub(super) fn mb(release_id: &str, group_id: Option<&str>, year: Option<i32>) -> MetadataResult {
    MetadataResult {
        source: Catalog::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        barcodes: Vec::new(),
        media: crate::import::search::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: group_id.map(str::to_string),
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
        source_tracks: None,
    }
}

/// The same album on Discogs, whose group is a master and whose card URL
/// therefore differs from MusicBrainz's.
pub(super) fn discogs(
    release_id: &str,
    group_id: Option<&str>,
    year: Option<i32>,
) -> MetadataResult {
    MetadataResult {
        source: Catalog::Discogs,
        ..mb(release_id, group_id, year)
    }
}

/// `release` as its catalog states it: its album is `master` on Discogs.
pub(super) fn linked(mut release: MetadataResult, master: &str) -> MetadataResult {
    release.album_links = AlbumLinks::Read(vec![MetadataRef::new(Catalog::Discogs, master)]);
    release
}

/// Every card's rows, card by card — what a test about pairing reads, since
/// pairing does not depend on which cards the rows land on.
pub(super) fn rows(groups: &[ReleaseGroup]) -> Vec<Vec<&str>> {
    groups.iter().flat_map(lead_ids).collect()
}

pub(super) fn cover() -> RemoteCover {
    RemoteCover {
        url: "https://caa.example/front.jpg".to_string(),
        thumbnail_url: "https://caa.example/thumb.jpg".to_string(),
        label: Catalog::MusicBrainz.cover_source_label().to_string(),
        source: Catalog::MusicBrainz,
    }
}

pub(super) fn lead_ids(group: &ReleaseGroup) -> Vec<Vec<&str>> {
    group
        .pressings()
        .map(|pressing| {
            pressing
                .releases
                .iter()
                .map(|release| release.release_id.as_str())
                .collect()
        })
        .collect()
}

#[test]
fn same_group_collapses_into_one_card() {
    let groups = grouped(vec![
        mb("rel-1", Some("group-x"), Some(1992)),
        mb("rel-2", Some("group-x"), Some(2012)),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, "group-x");
    assert_eq!(groups[0].pressings().count(), 2);
    assert_eq!(
        groups[0].sources,
        vec![ReleaseGroupSource {
            source: Catalog::MusicBrainz,
            group_url: Some("https://musicbrainz.org/release-group/group-x".to_string()),
            album_links_unread: false,
        }]
    );
}

#[test]
fn distinct_groups_keep_first_seen_order() {
    let mut second = mb("rel-2", Some("group-a"), None);
    second.title = "Other Album".to_string();
    let groups = grouped(vec![
        mb("rel-1", Some("group-b"), None),
        second,
        mb("rel-3", Some("group-b"), None),
    ]);
    assert_eq!(
        groups.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
        ["group-b", "group-a"]
    );
    assert_eq!(groups[0].pressings().count(), 2);
    assert_eq!(groups[1].pressings().count(), 1);
}

#[test]
fn ungrouped_result_is_its_own_single_pressing_card() {
    let groups = grouped(vec![mb("rel-1", None, Some(1999))]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, "rel-1");
    assert_eq!(
        groups[0].sources,
        vec![ReleaseGroupSource {
            source: Catalog::MusicBrainz,
            group_url: None,
            album_links_unread: false,
        }]
    );
    assert_eq!(groups[0].year_min, Some(1999));
    assert_eq!(groups[0].year_max, Some(1999));
}

/// Two ungrouped MusicBrainz results are two albums as far as MusicBrainz
/// is concerned: the cross-source merge never merges one source with
/// itself, whatever the titles say.
#[test]
fn two_ungrouped_results_from_one_source_do_not_merge() {
    let groups = grouped(vec![mb("rel-1", None, None), mb("rel-2", None, None)]);
    assert_eq!(groups.len(), 2);
}

#[test]
fn two_musicbrainz_groups_never_merge_with_each_other() {
    let groups = grouped(vec![
        mb("rel-1", Some("group-a"), None),
        mb("rel-2", Some("group-b"), None),
    ]);
    assert_eq!(groups.len(), 2);
}

/// MusicBrainz linking its album to a Discogs master makes the two one card
/// carrying both, MusicBrainz first, each with its own editorial page.
#[test]
fn an_album_musicbrainz_links_to_a_master_is_one_card() {
    let groups = grouped(vec![
        discogs("dg-1", Some("master-7"), Some(2001)),
        linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7"),
    ]);
    assert_eq!(groups.len(), 1);
    // The Discogs bucket was seen first, so the card sits at its position
    // — but MusicBrainz leads the sources and names the card.
    assert_eq!(groups[0].id, "group-x");
    assert_eq!(
        groups[0].sources,
        vec![
            ReleaseGroupSource {
                source: Catalog::MusicBrainz,
                group_url: Some("https://musicbrainz.org/release-group/group-x".to_string()),
                album_links_unread: false,
            },
            ReleaseGroupSource {
                source: Catalog::Discogs,
                group_url: Some("https://www.discogs.com/master/master-7".to_string()),
                album_links_unread: false,
            },
        ]
    );
    assert_eq!(groups[0].year_min, Some(1992));
    assert_eq!(groups[0].year_max, Some(2001));
}

/// The case the grouping is for: the two catalogs credit the artist
/// differently, one Discogs release credits it as MusicBrainz does, and no
/// pressing pairs. MusicBrainz's link makes them one card, whichever order
/// the results arrive in; without it, identical text is no reason to join.
#[test]
fn a_linked_album_is_one_card_whatever_the_artist_text_and_the_order() {
    let group = "0f5d2a51-8c1e-4b7a-9e3d-6a2b4c8d1e7f";
    let master = "510001";
    let musicbrainz = |release_id: &str, year: i32| {
        let mut release = linked(mb(release_id, Some(group), Some(year)), master);
        release.title = "Album".to_string();
        release.artist = Some("Artist".to_string());
        release
    };
    let discogs_release = |release_id: &str, year: i32, artist: &str| {
        let mut release = discogs(release_id, Some(master), Some(year));
        release.title = "Album".to_string();
        release.artist = Some(artist.to_string());
        release
    };
    let results = vec![
        musicbrainz("a1b2c3d4-0000-4000-8000-000000000001", 1965),
        discogs_release("1001", 1965, "The Artists*"),
        musicbrainz("a1b2c3d4-0000-4000-8000-000000000002", 1970),
        discogs_release("1002", 1974, "Artist"),
    ];
    let mut reversed = results.clone();
    reversed.reverse();

    for order in [results.clone(), reversed] {
        let groups = grouped(order);
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(groups[0].id, group);
        assert_eq!(groups[0].sources.len(), 2);
        let mut rows = lead_ids(&groups[0]);
        rows.sort();
        assert_eq!(
            rows,
            vec![
                vec!["1001"],
                vec!["1002"],
                vec!["a1b2c3d4-0000-4000-8000-000000000001"],
                vec!["a1b2c3d4-0000-4000-8000-000000000002"],
            ]
        );
    }

    let unlinked: Vec<MetadataResult> = results
        .into_iter()
        .map(|mut release| {
            release.album_links = AlbumLinks::NotAsked;
            release
        })
        .collect();
    assert_eq!(grouped(unlinked).len(), 2);
}

/// Albums nothing links are two cards, however alike their text: the text is
/// no reason to join them.
#[test]
fn albums_nothing_links_are_two_cards_whatever_their_text() {
    let groups = grouped(vec![
        mb("mb-1", Some("group-x"), None),
        discogs("dg-1", Some("master-7"), None),
    ]);
    assert_eq!(groups.len(), 2);

    // A group whose page could not be read links nothing either, and its
    // card says so, since it may be the other card's album.
    let mut unread = mb("mb-2", Some("group-y"), None);
    unread.album_links = AlbumLinks::Unread;
    let groups = grouped(vec![unread, discogs("dg-2", Some("master-8"), None)]);
    assert_eq!(groups.len(), 2);
    assert!(groups[0].sources[0].album_links_unread);
    assert!(!groups[1].sources[0].album_links_unread);
}

/// Everything the links connect is one card: two MusicBrainz albums that both
/// link one master are one card with it.
#[test]
fn everything_the_links_connect_is_one_card() {
    let groups = grouped(vec![
        linked(mb("mb-1", Some("group-x"), None), "master-7"),
        discogs("dg-1", Some("master-7"), None),
        linked(mb("mb-2", Some("group-y"), None), "master-7"),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0]
            .sources
            .iter()
            .map(|source| source.source)
            .collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz, Catalog::MusicBrainz, Catalog::Discogs]
    );
}

/// A card holding two albums of one catalog lists each album's rows under
/// that album's own title and page, so what joined them can be seen. A row
/// with a MusicBrainz record sits under its MusicBrainz album; a row only
/// Discogs lists sits under its master.
#[test]
fn a_card_holding_two_albums_of_one_catalog_splits_its_rows_by_album() {
    let mut paired = linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7");
    paired.barcodes = vec!["012345678905".to_string()];
    let mut other_album = linked(mb("mb-2", Some("group-y"), Some(1994)), "master-7");
    other_album.title = "Album Title (Live)".to_string();
    let mut paired_discogs = discogs("dg-1", Some("master-7"), Some(1992));
    paired_discogs.barcodes = vec!["012345678905".to_string()];
    let discogs_only = discogs("dg-2", Some("master-7"), Some(2001));

    let groups = grouped(vec![discogs_only, other_album, paired_discogs, paired]);
    assert_eq!(groups.len(), 1);
    type Heading<'a> = Option<(&'a str, Option<&'a str>)>;
    let sections: Vec<(Heading, Vec<Vec<&str>>)> = groups[0]
        .sections
        .iter()
        .map(|section| {
            (
                section.album.as_ref().map(|album| {
                    (album.title.as_str(), album.source.group_url.as_deref())
                }),
                section
                    .pressings
                    .iter()
                    .map(|pressing| {
                        pressing
                            .releases
                            .iter()
                            .map(|release| release.release_id.as_str())
                            .collect()
                    })
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        sections,
        vec![
            (
                Some((
                    "Album Title (Live)",
                    Some("https://musicbrainz.org/release-group/group-y")
                )),
                vec![vec!["mb-2"]],
            ),
            (
                Some((
                    "Album Title",
                    Some("https://musicbrainz.org/release-group/group-x")
                )),
                vec![vec!["mb-1", "dg-1"]],
            ),
            (
                Some((
                    "Album Title",
                    Some("https://www.discogs.com/master/master-7")
                )),
                vec![vec!["dg-2"]],
            ),
        ]
    );
}

/// A card holding one album of each catalog lists its rows as one section
/// with no heading: the card's own title and pages are the album's.
#[test]
fn a_card_holding_one_album_per_catalog_is_one_section() {
    let groups = grouped(vec![
        linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7"),
        discogs("dg-1", Some("master-7"), Some(2001)),
        mb("mb-2", Some("group-x"), Some(1994)),
    ]);
    assert_eq!(groups[0].sections.len(), 1);
    assert_eq!(groups[0].sections[0].album, None);
    assert_eq!(groups[0].pressings().count(), 3);
}

/// A link joins the album it names and no other: a second master with the
/// same text stays its own card.
#[test]
fn a_link_joins_only_the_album_it_names() {
    let groups = grouped(vec![
        linked(mb("mb-1", Some("group-x"), None), "master-7"),
        discogs("dg-1", Some("master-7"), None),
        discogs("dg-2", Some("master-8"), None),
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].sources.len(), 2);
    assert_eq!(groups[1].id, "master-8");
}

/// An artist one source spells differently, or leaves out, does not keep two
/// records of one pressing apart: the barcode pairs them, and the pair joins
/// their cards.
#[test]
fn artist_spelling_does_not_block_established_identity() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["012345678905".to_string()];
    let mut respelled = discogs("dg-1", Some("master-7"), Some(1992));
    respelled.artist = Some("The Artist Name".to_string());
    respelled.barcodes = vec!["012345678905".to_string()];
    let groups = grouped(vec![one.clone(), respelled]);
    assert_eq!(groups.len(), 1);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);

    let mut anonymous = discogs("dg-2", Some("master-8"), Some(1992));
    anonymous.artist = None;
    anonymous.barcodes = vec!["012345678905".to_string()];
    let groups = grouped(vec![anonymous, one]);
    assert_eq!(groups.len(), 1);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-2"]]);
    assert_eq!(groups[0].artist.as_deref(), Some("Artist Name"));
}

/// A release document that names the other source's release as the same
/// release pairs with it whatever the text says, and a contradiction in the
/// facts does not overrule what the document states.
#[test]
fn a_stated_link_pairs_despite_the_text_and_the_facts() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.title = "Album Title: Subtitle".to_string();
    one.links = vec![crate::import::MetadataRef::new(Catalog::Discogs, "dg-1")];
    let mut other = discogs("dg-1", Some("master-7"), Some(1994));
    other.title = "Another Album".to_string();
    other.artist = Some("Other Artist".to_string());

    let groups = grouped(vec![other, one]);
    assert_eq!(groups.len(), 1);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
    assert_eq!(groups[0].id, "group-x");
    assert_eq!(groups[0].title, "Album Title: Subtitle");
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-1".to_string()),
            partners: vec![crate::import::MetadataRef::new(Catalog::Discogs, "dg-1")],
        }
    );
}

/// Two Discogs records print the MusicBrainz record's barcode, and the
/// MusicBrainz document links one of them. The linked one is its pressing;
/// the other is a separate Discogs record and stays its own row.
#[test]
fn a_stated_link_names_the_record_that_stands_for_its_catalog() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["012345678905".to_string()];
    one.links = vec![crate::import::MetadataRef::new(
        Catalog::Discogs,
        "dg-linked",
    )];
    let mut linked = discogs("dg-linked", Some("master-7"), Some(1992));
    linked.barcodes = vec!["012345678905".to_string()];
    let mut barcoded = discogs("dg-barcoded", Some("master-7"), Some(1992));
    barcoded.barcodes = vec!["012345678905".to_string()];

    let groups = grouped(vec![barcoded, one, linked]);
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["mb-1", "dg-linked"], vec!["dg-barcoded"]]
    );
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-1".to_string()),
            partners: vec![crate::import::MetadataRef::new(
                Catalog::Discogs,
                "dg-linked"
            )],
        },
        "one record per catalog is claimed"
    );
}

/// A link makes the two catalogs' albums one card and invents no pressing
/// correspondence: two rows, one per source.
#[test]
fn a_link_combines_the_album_without_pairing_pressings() {
    let groups = grouped(vec![
        linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7"),
        discogs("dg-1", Some("master-7"), Some(1992)),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].sources.len(), 2);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// Different labels as written are inconclusive, so the barcode pair stands;
/// a label alias is neither required nor invented — the card names the first
/// label as stated.
#[test]
fn label_spelling_neither_blocks_nor_fabricates_identity() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["012345678905".to_string()];
    one.label = Some("Label Name Records".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["012345678905".to_string()];
    other.label = Some("LN".to_string());

    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
    assert_eq!(groups[0].label.as_deref(), Some("Label Name Records"));

    // An agreeing label does not make a shared catalog number a pair.
    let mut one = mb("mb-2", Some("group-y"), None);
    one.catalog_number = Some("CAT-7".to_string());
    one.label = Some("Label Name Records".to_string());
    let mut other = discogs("dg-2", Some("master-8"), None);
    other.catalog_number = Some("CAT-7".to_string());
    other.label = Some("Label Name".to_string());
    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-2"], vec!["dg-2"]]);
}

/// A barcode in a different but equivalent representation, or a country as
/// its name rather than its code, is the same fact; a medium described with
/// qualifiers is the same medium.
#[test]
fn equivalent_representations_pair_and_distinct_identifiers_do_not() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["0 12345 67890 5".to_string()];
    one.country = Some("JP".to_string());
    one.media = StatedMedia::PerMedium(vec![Some("CD".to_string())]);
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["0012345678905".to_string()];
    other.country = Some("Japan".to_string());
    other.media = StatedMedia::Descriptors(vec![
        "CD".to_string(),
        "Album".to_string(),
        "Reissue".to_string(),
    ]);
    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);

    // A thirteen-digit code and an eight-digit one are never one code, and a
    // catalog number's letters and digits are its identity.
    let mut one = mb("mb-2", Some("group-y"), Some(1992));
    one.barcodes = vec!["5051961234567".to_string()];
    one.catalog_number = Some("CAT-72".to_string());
    let mut other = discogs("dg-2", Some("master-8"), Some(1992));
    other.barcodes = vec!["12345678".to_string()];
    other.catalog_number = Some("CAT-7 2".to_string());
    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-2"], vec!["dg-2"]]);
}

/// A different country or a different year is a contradiction that an
/// inferred pair does not survive; a medium is compared through what the
/// records are known to contain.
#[test]
fn meaningful_conflicts_prevent_inferred_pairs() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["012345678905".to_string()];
    one.country = Some("US".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["012345678905".to_string()];
    other.country = Some("Germany".to_string());
    let groups = grouped(vec![one.clone(), other.clone()]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);

    other.country = Some("United States".to_string());
    other.year = Some(1993);
    let groups = grouped(vec![one.clone(), other.clone()]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);

    other.year = Some(1992);
    one.media = StatedMedia::PerMedium(vec![Some("CD".to_string())]);
    other.media = StatedMedia::Descriptors(vec!["Vinyl".to_string(), "LP".to_string()]);
    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// A release issued as files is not the CD it was cut from, however much
/// the two records share: a reissue carries the CD's catalog number, its
/// year and its country, and the one thing that tells them apart is what
/// each catalog says they are made of.
#[test]
fn a_file_release_is_not_the_cd_whose_catalog_number_it_carries() {
    let mut cd = mb("mb-1", Some("group-x"), Some(2013));
    cd.catalog_number = Some("CAT-7".to_string());
    cd.country = Some("JP".to_string());
    cd.media = StatedMedia::PerMedium(vec![Some("CD".to_string())]);
    let mut download = discogs("dg-1", Some("master-7"), Some(2013));
    download.catalog_number = Some("CAT-7".to_string());
    download.country = Some("Japan".to_string());
    download.media = StatedMedia::Descriptors(vec![
        "File".to_string(),
        "FLAC".to_string(),
        "Album".to_string(),
        "Reissue".to_string(),
    ]);
    let groups = grouped(vec![cd, download]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// Reissues repeat a pressing's codes. Where the year tells the records
/// apart, each pairs with its own; where nothing does, none pairs — and
/// however the records arrive, the pairs are the same.
#[test]
fn reissues_pair_by_what_tells_them_apart_whatever_the_order() {
    let mut early = mb("mb-1988", Some("group-x"), Some(1988));
    early.barcodes = vec!["4988014720311".to_string()];
    let mut late = mb("mb-1991", Some("group-x"), Some(1991));
    late.barcodes = vec!["4988014720311".to_string()];
    let mut dg_early = discogs("dg-1988", Some("master-7"), Some(1988));
    dg_early.barcodes = vec!["4988014720311".to_string()];
    let mut dg_late = discogs("dg-1991", Some("master-7"), Some(1991));
    dg_late.barcodes = vec!["4988014720311".to_string()];
    let mut dg_undated = discogs("dg-undated", Some("master-7"), None);
    dg_undated.barcodes = vec!["4988014720311".to_string()];

    let records = [early, late, dg_early, dg_late, dg_undated];
    let orders: [[usize; 5]; 3] = [[0, 1, 2, 3, 4], [4, 3, 2, 1, 0], [2, 0, 4, 1, 3]];
    for order in orders {
        let groups = grouped(order.iter().map(|&at| records[at].clone()).collect());
        let mut rows = lead_ids(&groups[0]);
        rows.sort();
        assert_eq!(
            rows,
            vec![
                vec!["dg-undated"],
                vec!["mb-1988", "dg-1988"],
                vec!["mb-1991", "dg-1991"]
            ],
            "{order:?}"
        );
        assert_eq!(pressing_count(records.to_vec()), 3);
    }
}

/// Two MusicBrainz records pressed in different years compete for one
/// undated Discogs record with the same support: the Discogs record is
/// ambiguous and settles alone, and each rival stays open for a pressing
/// below.
#[test]
fn an_ambiguous_member_settles_alone_but_its_rivals_stay_open() {
    let mut first = mb("mb-first", Some("group-x"), Some(1992));
    first.barcodes = vec!["012345678905".to_string()];
    first.catalog_number = Some("CAT-7".to_string());
    let mut second = mb("mb-second", Some("group-x"), Some(1993));
    second.barcodes = vec!["012345678905".to_string(), "5051961234567".to_string()];
    second.catalog_number = Some("CAT-7".to_string());
    let mut contested = discogs("dg-contested", Some("master-7"), None);
    contested.barcodes = vec!["012345678905".to_string()];
    contested.catalog_number = Some("CAT-7".to_string());
    // Shares only the second's other barcode, and less else than the
    // contested record does, so its pair is taken at a level below.
    let mut below = discogs("dg-below", Some("master-7"), Some(1993));
    below.barcodes = vec!["5051961234567".to_string()];

    let groups = grouped(vec![first, second, contested, below]);
    let mut rows = lead_ids(&groups[0]);
    rows.sort();
    assert_eq!(
        rows,
        vec![
            vec!["dg-contested"],
            vec!["mb-first"],
            vec!["mb-second", "dg-below"]
        ]
    );
}

/// A pair joins two groups on the same card, and a source's other group
/// stays its own card: pairs decide the album grouping, not the shared group
/// id.
#[test]
fn pairs_join_groups_and_leave_the_rest_apart() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.title = "Album Title: Subtitle".to_string();
    one.barcodes = vec!["012345678905".to_string()];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.title = "Album Title - Subtitle".to_string();
    other.barcodes = vec!["012345678905".to_string()];
    let mut unrelated = discogs("dg-2", Some("master-8"), Some(2001));
    unrelated.title = "Album Title - Subtitle".to_string();

    let groups = grouped(vec![unrelated, one, other]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].id, "master-8");
    assert_eq!(groups[1].id, "group-x");
    assert_eq!(lead_ids(&groups[1]), vec![vec!["mb-1", "dg-1"]]);
}

/// A row paired across both sources is picked whole: the lead is the
/// document the draft is read from, and the other source's record of the
/// same pressing rides along as a partner.
#[test]
fn a_paired_row_is_picked_with_its_partner() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["012345678905".to_string()];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["012345678905".to_string()];

    let groups = grouped(vec![one, other]);
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-1".to_string()),
            partners: vec![crate::import::MetadataRef::new(Catalog::Discogs, "dg-1")],
        }
    );
}

/// A pressing only one source lists claims only that source.
#[test]
fn a_lone_row_is_picked_with_no_partner() {
    let groups = grouped(vec![discogs("dg-1", Some("master-7"), Some(1992))]);
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::Discogs, "dg-1".to_string()),
            partners: vec![],
        }
    );
}

/// Barcodes the two sources punctuate differently still name one pressing.
#[test]
fn releases_sharing_a_barcode_are_one_pressing() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["0 12345 67890 5".to_string()];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["012345678905".to_string()];

    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
}

/// A label can keep one catalog number on every reissue for decades, so a
/// shared number pairs nothing, even with the year, country and label
/// agreeing.
#[test]
fn releases_sharing_a_catalog_number_are_not_one_pressing() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.catalog_number = Some("CAT-7 ".to_string());
    one.country = Some("JP".to_string());
    one.label = Some("Label Name".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.catalog_number = Some("cat-7".to_string());
    other.country = Some("Japan".to_string());
    other.label = Some("Label Name".to_string());

    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// A catalog number with nothing else stated pairs nothing either.
#[test]
fn a_catalog_number_alone_pairs_nothing() {
    let mut one = mb("mb-1", Some("group-x"), None);
    one.catalog_number = Some("CAT-7".to_string());
    let mut other = discogs("dg-1", Some("master-7"), None);
    other.catalog_number = Some("CAT-7".to_string());

    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// A catalog number the two sources share cannot pair records whose barcodes
/// contradict each other: the stronger evidence says they are two objects.
#[test]
fn a_shared_catalog_number_cannot_bypass_incompatible_barcodes() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.catalog_number = Some("CAT-7".to_string());
    one.barcodes = vec!["012345678905".to_string()];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.catalog_number = Some("CAT-7".to_string());
    other.barcodes = vec!["5051961234567".to_string()];

    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// The album title is spelled with a colon on one source and a dash on the
/// other. That is presentation; the barcode, catalog number and year say the
/// two records are one pressing, and the card carries both sources.
#[test]
fn a_title_spelling_difference_does_not_block_a_barcode_pair() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.title = "Album Title: Subtitle".to_string();
    one.barcodes = vec!["012345678905".to_string()];
    one.catalog_number = Some("CAT-7".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.title = "Album Title - Subtitle".to_string();
    other.barcodes = vec!["012345678905".to_string()];
    other.catalog_number = Some("CAT-7".to_string());

    let groups = grouped(vec![one, other]);
    assert_eq!(groups.len(), 1);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
    assert_eq!(groups[0].title, "Album Title: Subtitle");
    assert_eq!(
        groups[0]
            .sources
            .iter()
            .map(|source| source.source)
            .collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz, Catalog::Discogs]
    );
    assert_eq!(
        pressing_count(
            groups
                .iter()
                .flat_map(ReleaseGroup::pressings)
                .flat_map(|pressing| pressing.releases.clone())
                .collect()
        ),
        1
    );
}

/// A barcode is stronger evidence than a catalog number, so the barcode
/// pair is taken even though an earlier release shares a catalog number
/// with the same Discogs row.
#[test]
fn a_barcode_pair_outranks_a_catalog_pair_for_the_same_release() {
    let mut catalog_only = mb("mb-catalog", Some("group-x"), Some(1992));
    catalog_only.catalog_number = Some("CAT-7".to_string());
    let mut barcoded = mb("mb-barcode", Some("group-x"), Some(1994));
    barcoded.barcodes = vec!["012345678905".to_string()];
    barcoded.catalog_number = Some("CAT-7".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1994));
    other.barcodes = vec!["012345678905".to_string()];
    other.catalog_number = Some("CAT-7".to_string());

    let groups = grouped(vec![catalog_only, barcoded, other]);
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["mb-catalog"], vec!["mb-barcode", "dg-1"]]
    );
}

/// Releases with nothing to pair on stay their own rows, and the Discogs
/// leftovers land as single-source pressings.
#[test]
fn unpaired_releases_are_single_source_pressings() {
    let groups = grouped(vec![
        linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7"),
        discogs("dg-1", Some("master-7"), Some(2001)),
    ]);
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["mb-1"], vec!["dg-1"]],
        "one card, two rows"
    );
}

#[test]
fn rows_are_ordered_by_pressing_year_with_unknown_years_last() {
    let groups = grouped(vec![
        mb("rel-undated", Some("group-x"), None),
        mb("rel-2012", Some("group-x"), Some(2012)),
        mb("rel-1992", Some("group-x"), Some(1992)),
    ]);
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["rel-1992"], vec!["rel-2012"], vec!["rel-undated"]]
    );
    assert_eq!(groups[0].year_min, Some(1992));
    assert_eq!(groups[0].year_max, Some(2012));
}

#[test]
fn year_span_is_none_when_no_pressing_carries_a_year() {
    let groups = grouped(vec![mb("rel-1", Some("group-x"), None)]);
    assert_eq!(groups[0].year_min, None);
    assert_eq!(groups[0].year_max, None);
}

#[test]
fn the_card_label_is_the_first_pressing_that_names_one() {
    let mut unlabelled = mb("rel-1", Some("group-x"), Some(1992));
    unlabelled.label = None;
    let mut labelled = mb("rel-2", Some("group-x"), Some(1994));
    labelled.label = Some("Label Name".to_string());
    let mut later = mb("rel-3", Some("group-x"), Some(2012));
    later.label = Some("Reissue Records".to_string());

    let groups = grouped(vec![unlabelled, labelled, later]);
    assert_eq!(groups[0].label.as_deref(), Some("Label Name"));
}

#[test]
fn a_card_whose_pressings_name_no_label_has_none() {
    let groups = grouped(vec![mb("rel-1", Some("group-x"), Some(1992))]);
    assert_eq!(groups[0].label, None);
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
        url: "https://discogs.example/front.jpg".to_string(),
        thumbnail_url: "https://discogs.example/thumb.jpg".to_string(),
        label: Catalog::Discogs.cover_source_label().to_string(),
        source: Catalog::Discogs,
    });
    let mut mb_covered = linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7");
    mb_covered.cover_art = Some(cover());

    let groups = grouped(vec![discogs_covered, mb_covered]);
    assert_eq!(groups[0].cover_art, Some(cover()));
}

/// Two results carrying the same `source_group_id` string but different
/// sources are still bucketed apart: only a link joins them, and none is
/// stated here.
#[test]
fn the_same_group_id_across_sources_does_not_collide() {
    let mut one = mb("rel-mb", Some("shared-id"), Some(2001));
    one.title = "Album One".to_string();
    let mut other = discogs("rel-dg", Some("shared-id"), Some(2001));
    other.title = "Album Two".to_string();

    let groups = grouped(vec![one, other]);

    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups
            .iter()
            .map(|group| group.sources[0].source)
            .collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz, Catalog::Discogs]
    );
}

/// A run's offered rows and the rows its agreement set aside are grouped as
/// one list: an album is one card whichever side its rows are on, and a card
/// none of whose rows is offered comes after every card that offers one.
#[test]
fn rows_either_side_of_the_disclosure_are_one_card() {
    let groups = group_formed_rows(
        unranked(vec![linked(
            mb("mb-1", Some("group-x"), Some(1992)),
            "master-7",
        )]),
        &[0],
        unranked(vec![
            mb("mb-2", Some("group-z"), None),
            discogs("dg-1", Some("master-7"), Some(1994)),
        ]),
        &[0, 1],
    );
    assert_eq!(
        groups.iter().map(|group| group.id.as_str()).collect::<Vec<_>>(),
        vec!["group-x", "group-z"]
    );
    let ids = |pressings: &[Pressing]| -> Vec<String> {
        pressings
            .iter()
            .map(|pressing| pressing.lead().release_id.clone())
            .collect()
    };
    assert_eq!(ids(&groups[0].sections[0].pressings), vec!["mb-1"]);
    assert_eq!(ids(&groups[0].sections[0].narrowed_out), vec!["dg-1"]);
    assert!(groups[1].sections[0].pressings.is_empty());
    assert_eq!(ids(&groups[1].sections[0].narrowed_out), vec!["mb-2"]);
}
