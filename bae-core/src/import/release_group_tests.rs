use super::*;
use crate::import::album_links::AlbumLinks;
use bae_types::pressing::{Medium, StatedMedia};

/// The tests are about how results bucket, pair and order by year, none of
/// which the candidate's text takes part in.
pub(super) fn grouped(results: Vec<MetadataResult>) -> Vec<ReleaseGroup> {
    group_results(unranked(results), None)
}

pub(super) fn mb(release_id: &str, group_id: Option<&str>, year: Option<i32>) -> MetadataResult {
    MetadataResult {
        source: Catalog::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year,
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: bae_types::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: group_id.map(str::to_string),
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
        source_tracks: None,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
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
    release.album_links = AlbumLinks::Read(vec![crate::import::album_links::AlbumLink {
        album: MetadataRef::new(Catalog::Discogs, master),
        stated: crate::import::album_links::AlbumStatement::Page,
    }]);
    release
}

/// Every card's rows, card by card — what a test about pairing reads, since
/// pairing does not depend on which cards the rows land on.
pub(super) fn rows(groups: &[ReleaseGroup]) -> Vec<Vec<&str>> {
    groups.iter().flat_map(lead_ids).collect()
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
                section
                    .album
                    .as_ref()
                    .map(|album| (album.title.as_str(), album.source.group_url.as_deref())),
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

/// The card names the label every pressing naming one agrees on, however
/// each spells it; pressings naming different labels leave the card none,
/// never the first one's.
#[test]
fn the_card_label_is_the_one_its_pressings_agree_on() {
    let labelled = |id: &str, year: i32, names: &[&str]| {
        let mut record = mb(id, Some("group-x"), Some(year));
        record.labels = names
            .iter()
            .map(|name| bae_types::pressing::ReleaseLabel::of(Some(name), None))
            .collect();
        record
    };
    let groups = grouped(vec![
        labelled("rel-1", 1992, &[]),
        labelled("rel-2", 1994, &["Label Name"]),
        labelled("rel-3", 2012, &["Other Imprint", "LABEL NAME"]),
    ]);
    assert_eq!(groups[0].label.as_deref(), Some("Label Name"));

    let groups = grouped(vec![
        labelled("rel-2", 1994, &["Label Name"]),
        labelled("rel-3", 2012, &["Reissue Records"]),
    ]);
    assert_eq!(groups[0].label, None);
}

#[test]
fn a_card_whose_pressings_name_no_label_has_none() {
    let groups = grouped(vec![mb("rel-1", Some("group-x"), Some(1992))]);
    assert_eq!(groups[0].label, None);
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
        None,
    );
    assert_eq!(
        groups
            .iter()
            .map(|group| group.id.as_str())
            .collect::<Vec<_>>(),
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

/// A row of two records keeps each record's own facts: neither fills the
/// other, and what a pick applies is the lead's.
#[test]
fn a_pressings_records_keep_their_own_facts() {
    use bae_types::pressing::{area, DiscogsDetail, ReleaseStatus};
    let mut musicbrainz = mb("mb-1", Some("group-x"), Some(1992));
    musicbrainz.barcodes = vec!["012345678905".to_string()];
    musicbrainz.area = Some(area("JP"));
    musicbrainz.status = Some(ReleaseStatus::Official);
    musicbrainz.media = StatedMedia::PerMedium(vec![Some(Medium::Cd)]);
    let mut discogs = discogs("dg-1", Some("master-7"), Some(1992));
    discogs.barcodes = vec!["012345678905".to_string()];
    discogs.status = Some(ReleaseStatus::Promotion);
    discogs.discogs_details = vec![DiscogsDetail::Reissue];
    let groups = grouped(vec![musicbrainz, discogs]);
    let pressing = groups[0].sections[0].pressings[0].clone();
    assert_eq!(
        pressing.releases.len(),
        2,
        "the two records are one pressing"
    );
    let lead = pressing.lead().facts();
    assert_eq!(lead.area, Some(area("JP")));
    assert_eq!(lead.status, Some(ReleaseStatus::Official));
    assert!(
        lead.discogs_details.is_empty(),
        "Discogs's details stay on its record"
    );
    let other = pressing.releases[1].facts();
    assert_eq!(other.status, Some(ReleaseStatus::Promotion));
    assert_eq!(other.discogs_details, vec![DiscogsDetail::Reissue]);
    assert_eq!(other.area, None, "MusicBrainz's area stays on its record");
}
