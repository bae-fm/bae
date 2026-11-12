//! How results become album cards: which albums share a card — a pressing
//! both catalogs' records name, or a statement that one album is the other —
//! how rows and cards are ordered by what the candidate's text agrees with,
//! and what a card's heading reads: its sources, label and cover.

use super::*;
use bae_catalogs::album_links::AlbumLinks;

/// The tests are about how results bucket, pair and order by year, none of
/// which the candidate's text takes part in.
pub(super) fn grouped(results: Vec<MetadataResult>) -> Vec<ReleaseGroup> {
    group_results(unranked(results), None)
}

pub(super) fn mb(release_id: &str, group_id: Option<&str>, year: Option<i32>) -> MetadataResult {
    MetadataResult {
        source: LookupCatalog::MusicBrainz,
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
        album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
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
    MetadataResult { source: LookupCatalog::Discogs, ..mb(release_id, group_id, year) }
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

pub(super) fn lead_ids(group: &ReleaseGroup) -> Vec<Vec<&str>> {
    group
        .pressings()
        .map(|pressing| {
            pressing.releases.iter().map(|release| release.release_id.as_str()).collect()
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
    assert_eq!(groups.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["group-b", "group-a"]);
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
    let groups =
        grouped(vec![mb("rel-1", Some("group-a"), None), mb("rel-2", Some("group-b"), None)]);
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
    let groups =
        grouped(vec![mb("mb-1", Some("group-x"), None), discogs("dg-1", Some("master-7"), None)]);
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
        groups[0].sources.iter().map(|source| source.source).collect::<Vec<_>>(),
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
                Some(("Album Title (Live)", Some("https://musicbrainz.org/release-group/group-y"))),
                vec![vec!["mb-2"]],
            ),
            (
                Some(("Album Title", Some("https://musicbrainz.org/release-group/group-x"))),
                vec![vec!["mb-1", "dg-1"]],
            ),
            (
                Some(("Album Title", Some("https://www.discogs.com/master/master-7"))),
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
    assert_eq!(lead_ids(&groups[0]), vec![vec!["rel-1992"], vec!["rel-2012"], vec!["rel-undated"]]);
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
        groups.iter().map(|group| group.sources[0].source).collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz, Catalog::Discogs]
    );
}

/// A run's offered rows and the rows its agreement set aside are grouped as
/// one list: an album is one card whichever side its rows are on, and a card
/// none of whose rows is offered comes after every card that offers one.
#[test]
fn rows_either_side_of_the_disclosure_are_one_card() {
    let groups = group_formed_rows(
        unranked(vec![linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7")]),
        &[0],
        unranked(vec![
            mb("mb-2", Some("group-z"), None),
            discogs("dg-1", Some("master-7"), Some(1994)),
        ]),
        &[0, 1],
        None,
    );
    assert_eq!(
        groups.iter().map(|group| group.id.as_str()).collect::<Vec<_>>(),
        vec!["group-x", "group-z"]
    );
    let ids = |pressings: &[Pressing]| -> Vec<String> {
        pressings.iter().map(|pressing| pressing.lead().release_id.clone()).collect()
    };
    assert_eq!(ids(&groups[0].sections[0].pressings), vec!["mb-1"]);
    assert_eq!(ids(&groups[0].sections[0].narrowed_out), vec!["dg-1"]);
    assert!(groups[1].sections[0].pressings.is_empty());
    assert_eq!(ids(&groups[1].sections[0].narrowed_out), vec!["mb-2"]);
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

/// Records that name no album are two albums, whatever their titles say.
#[test]
fn records_naming_no_album_are_two_albums() {
    let groups = grouped(vec![discogs("11", None, None), discogs("12", None, None)]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].id, "11");
    assert_eq!(groups[1].id, "12");
}

pub(super) fn agreed(count: u32) -> Agreements {
    Agreements {
        disc_id: count >= 1,
        barcode: count >= 2,
        catalog: count >= 3,
        label: count >= 4,
        year: count >= 5,
        country: count >= 6,
        ..Agreements::NONE
    }
}

/// The rows the candidate's own text says most about lead, whatever year
/// they were pressed.
#[test]
fn rows_the_text_says_most_about_lead() {
    let groups = group_results(
        vec![
            (mb("rel-early", Some("group-x"), Some(1976)), agreed(1)),
            (mb("rel-late", Some("group-x"), Some(2003)), agreed(4)),
        ],
        None,
    );
    assert_eq!(lead_ids(&groups[0]), vec![vec!["rel-late"], vec!["rel-early"]]);
}

/// Rows the text says as much about keep the pressing-year order.
#[test]
fn rows_the_text_says_as_much_about_keep_the_year_order() {
    let groups = group_results(
        vec![
            (mb("rel-late", Some("group-x"), Some(2003)), agreed(2)),
            (mb("rel-early", Some("group-x"), Some(1976)), agreed(2)),
        ],
        None,
    );
    assert_eq!(lead_ids(&groups[0]), vec![vec!["rel-early"], vec!["rel-late"]]);
}

/// A row is picked whole, so what the text says about the row is what it
/// says about every source's record of it — a record the text says nothing
/// about does not hold its partner back.
#[test]
fn a_paired_row_ranks_by_its_records_together() {
    let mut mb_release = mb("mb-1", Some("group-x"), Some(1976));
    mb_release.barcodes = vec!["0075678169328".to_string()];
    let mut dg_release = discogs("dg-1", Some("master-7"), Some(1976));
    dg_release.barcodes = vec!["0075678169328".to_string()];
    let groups = group_results(
        vec![
            (mb("mb-other", Some("group-x"), Some(1976)), agreed(2)),
            (mb_release, Agreements::NONE),
            (dg_release, agreed(4)),
        ],
        None,
    );
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["dg-1", "mb-1"], vec!["mb-other"]],
        "and the record the text does say something about leads the row",
    );
}

/// The two sources answer different questions about one object — a disc ID
/// is MusicBrainz's alone, a Discogs record states the catalog number the
/// sleeve prints — so a row outranks one that neither source says as much
/// about, even though neither of its own records does.
#[test]
fn a_row_outranks_by_what_its_records_add_up_to() {
    let mut mb_release = mb("mb-1", Some("group-x"), Some(1976));
    mb_release.barcodes = vec!["0075678169328".to_string()];
    let mut dg_release = discogs("dg-1", Some("master-7"), Some(1976));
    dg_release.barcodes = vec!["0075678169328".to_string()];
    let disc_id_only = Agreements { disc_id: true, ..Agreements::NONE };
    let catalog_only = Agreements { catalog: true, ..Agreements::NONE };
    let groups = group_results(
        vec![
            (mb_release, disc_id_only),
            (dg_release, catalog_only),
            (mb("mb-other", Some("group-x"), Some(1970)), disc_id_only),
        ],
        None,
    );
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["mb-1", "dg-1"], vec!["mb-other"]],
        "two agreements between them beat one, whatever the years say",
    );
}

/// Cards are ordered by their best row, so the album the folder describes
/// is the one at the top of the list.
#[test]
fn cards_are_ordered_by_their_best_row() {
    let groups = group_results(
        vec![
            (mb("rel-stranger", Some("group-stranger"), None), agreed(1)),
            (mb("rel-named", Some("group-named"), None), agreed(4)),
        ],
        None,
    );
    assert_eq!(
        groups.iter().map(|group| group.id.as_str()).collect::<Vec<_>>(),
        vec!["group-named", "group-stranger"],
    );
}

/// One pressing as both sources state it, paired by the barcode they share.
pub(super) fn paired() -> (MetadataResult, MetadataResult) {
    let mut one = mb("mb-1", Some("group-x"), Some(1976));
    one.barcodes = vec!["0075678169328".to_string()];
    let mut other = discogs("dg-1", Some("master-7"), Some(1976));
    other.barcodes = vec!["0075678169328".to_string()];
    (one, other)
}

/// The chips under an album's title name its sources in the one order surfaces
/// list them in, whichever record the card's best row is read from.
#[test]
fn the_card_names_its_sources_in_the_order_surfaces_list_them() {
    let (mb_release, dg_release) = paired();

    let groups = group_results(vec![(mb_release, agreed(1)), (dg_release, agreed(3))], None);

    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["dg-1", "mb-1"]],
        "the best row is read from the Discogs record",
    );
    assert_eq!(
        groups[0].sources.iter().map(|source| source.source).collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz, Catalog::Discogs]
    );
}

/// `release`'s group is `master` on Discogs because `release` links `twin`
/// and `twin`'s own document files it under `master`.
fn stated_through(mut release: MetadataResult, twin: &str, master: &str) -> MetadataResult {
    release.links = vec![MetadataRef::new(Catalog::Discogs, twin)];
    release.album_links = AlbumLinks::Read(vec![MetadataRef::new(Catalog::Discogs, master)]);
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
        groups[0].sources.iter().map(|source| source.source).collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz, Catalog::Discogs]
    );
    assert_eq!(rows(&groups), vec![vec!["mb-us", "dg-us"], vec!["dg-eu-1"], vec!["dg-eu-2"]]);
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

/// The album a record's page names on Discogs is on the card's header, with
/// its page, though no Discogs release is on the list.
#[test]
fn a_card_names_the_album_its_links_state_on_a_catalog_it_has_no_row_of() {
    let mut stated = mb("mb-1", Some("group-a"), Some(1986));
    stated.album_links = AlbumLinks::Read(vec![MetadataRef::new(Catalog::Discogs, "8522")]);
    let groups = grouped(vec![stated, mb("mb-2", Some("group-a"), None)]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0]
            .sources
            .iter()
            .map(|source| (source.source, source.group_url.clone()))
            .collect::<Vec<_>>(),
        vec![
            (
                Catalog::MusicBrainz,
                Some("https://musicbrainz.org/release-group/group-a".to_string())
            ),
            (Catalog::Discogs, Some("https://www.discogs.com/master/8522".to_string())),
        ]
    );
}

fn cover() -> RemoteCover {
    RemoteCover {
        image: bae_catalogs::remote_image::RemoteImageSet::with_copies(
            "https://caa.example/front.jpg".to_string(),
            vec![bae_catalogs::remote_image::DownscaledCopy {
                url: "https://caa.example/thumb.jpg".to_string(),
                max_edge: 250,
            }],
        ),
        label: bae_catalogs::remote_image::CoverLabel::Lead { release_group: false },
        source: Catalog::MusicBrainz,
        standing: bae_catalogs::remote_image::CoverStanding::Stated,
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
        image: bae_catalogs::remote_image::RemoteImageSet::with_copies(
            "https://discogs.example/front.jpg".to_string(),
            vec![bae_catalogs::remote_image::DownscaledCopy {
                url: "https://discogs.example/thumb.jpg".to_string(),
                max_edge: 150,
            }],
        ),
        label: bae_catalogs::remote_image::CoverLabel::Lead { release_group: false },
        source: Catalog::Discogs,
        standing: bae_catalogs::remote_image::CoverStanding::Stated,
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
        bae_catalogs::discogs::remote_cover_from_urls(
            Some(&format!("https://discogs.example/{id}.jpg")),
            Some(&format!("https://discogs.example/{id}-150.jpg")),
            "release",
            1,
        )
    };
    let cd = |mut release: MetadataResult, format: &str| {
        let medium = match format {
            "LP" => bae_types::pressing::Medium::Vinyl,
            _ => bae_types::pressing::Medium::Cd,
        };
        release.media =
            match release.source {
                LookupCatalog::Discogs => bae_types::pressing::StatedMedia::Formats(vec![
                    bae_types::pressing::StatedFormat { medium: Some(medium), quantity: 1 },
                ]),
                _ => bae_types::pressing::StatedMedia::PerMedium(vec![Some(medium)]),
            };
        release
    };
    let mut mb_2001 = cd(linked(mb("mb-2001", Some("group-x"), Some(2001)), "master-7"), "CD");
    mb_2001.barcodes = vec!["012345678905".to_string()];
    mb_2001.cover_art = Some(RemoteCover::musicbrainz_release("mb-2001"));
    let mut dg_2001 = cd(discogs("dg-2001", Some("master-7"), Some(2001)), "CD");
    dg_2001.barcodes = vec!["012345678905".to_string()];
    dg_2001.cover_art = discogs_cover("dg-2001");
    let mut dg_1988 = cd(discogs("dg-1988", Some("master-7"), Some(1988)), "LP");
    dg_1988.cover_art = discogs_cover("dg-1988");
    let mut mb_undated = cd(linked(mb("mb-undated", Some("group-x"), None), "master-7"), "CD");
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

    assert_eq!(groups[0].cover_art, Some(RemoteCover::musicbrainz_release("mb-search")));
}
