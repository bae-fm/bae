//! How records of the two catalogs pair into one pressing row: what the
//! evidence they carry decides, and what a paired row's pick claims.

use super::tests::*;
use super::*;
use bae_types::pressing::{Medium, StatedFormat, StatedMedia};

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
        crate::import::PressingLink {
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
        crate::import::PressingLink {
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
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(
        Some("Label Name Records"),
        None,
    )];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["012345678905".to_string()];
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("LN"), None)];

    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
    assert_eq!(groups[0].label.as_deref(), Some("Label Name Records"));

    // An agreeing label does not make a shared catalog number a pair.
    let mut one = mb("mb-2", Some("group-y"), None);
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(
        Some("Label Name Records"),
        Some("CAT-7"),
    )];
    let mut other = discogs("dg-2", Some("master-8"), None);
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(
        Some("Label Name"),
        Some("CAT-7"),
    )];
    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-2"], vec!["dg-2"]]);
}

/// A barcode in a different but equivalent representation, or a country as
/// its name rather than its code, is the same fact; a medium one catalog
/// lists per disc and the other as a format entry is the same medium.
#[test]
fn equivalent_representations_pair_and_distinct_identifiers_do_not() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["0 12345 67890 5".to_string()];
    one.area = Some(bae_types::pressing::area("JP"));
    one.media = StatedMedia::PerMedium(vec![Some(Medium::Cd)]);
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["0012345678905".to_string()];
    other.area = bae_types::pressing::ReleaseArea::discogs("Japan");
    other.media = StatedMedia::Formats(vec![StatedFormat {
        medium: Some(Medium::Cd),
        quantity: 1,
    }]);
    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);

    // A thirteen-digit code and an eight-digit one are never one code, and a
    // catalog number's letters and digits are its identity.
    let mut one = mb("mb-2", Some("group-y"), Some(1992));
    one.barcodes = vec!["5051961234567".to_string()];
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-72"))];
    let mut other = discogs("dg-2", Some("master-8"), Some(1992));
    other.barcodes = vec!["12345678".to_string()];
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7 2"))];
    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-2"], vec!["dg-2"]]);
}

/// A different country or a different year is a contradiction that an
/// inferred pair does not survive — the country only while the label and the
/// year do not both agree; a medium is compared through what the records are
/// known to contain.
#[test]
fn meaningful_conflicts_prevent_inferred_pairs() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["012345678905".to_string()];
    one.area = Some(bae_types::pressing::area("US"));
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["012345678905".to_string()];
    other.area = bae_types::pressing::ReleaseArea::discogs("Germany");
    let groups = grouped(vec![one.clone(), other.clone()]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);

    other.area = bae_types::pressing::ReleaseArea::discogs("US");
    other.year = Some(1993);
    let groups = grouped(vec![one.clone(), other.clone()]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);

    other.year = Some(1992);
    one.media = StatedMedia::PerMedium(vec![Some(Medium::Cd)]);
    other.media = StatedMedia::Formats(vec![StatedFormat {
        medium: Some(Medium::Vinyl),
        quantity: 1,
    }]);
    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// One pressing whose label is Italian and whose discs were made in
/// Germany: one catalog records Italy, the other Germany. The same barcode,
/// label and year make it one row.
#[test]
fn a_pressing_recorded_under_two_countries_is_one_row() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["4012345678901".to_string()];
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(
        Some("Door Records"),
        Some("DR 100"),
    )];
    one.area = Some(bae_types::pressing::area("IT"));
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["4012345678901".to_string()];
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(
        Some("Door"),
        Some("DR-100"),
    )];
    other.area = bae_types::pressing::ReleaseArea::discogs("Germany");
    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
}

/// A release issued as files is not the CD it was cut from, however much
/// the two records share: a reissue carries the CD's catalog number, its
/// year and its country, and the one thing that tells them apart is what
/// each catalog says they are made of.
#[test]
fn a_file_release_is_not_the_cd_whose_catalog_number_it_carries() {
    let mut cd = mb("mb-1", Some("group-x"), Some(2013));
    cd.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    cd.area = Some(bae_types::pressing::area("JP"));
    cd.media = StatedMedia::PerMedium(vec![Some(Medium::Cd)]);
    let mut download = discogs("dg-1", Some("master-7"), Some(2013));
    download.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    download.area = bae_types::pressing::ReleaseArea::discogs("Japan");
    download.media = StatedMedia::Formats(vec![StatedFormat {
        medium: Some(Medium::Digital),
        quantity: 1,
    }]);
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
    first.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    let mut second = mb("mb-second", Some("group-x"), Some(1993));
    second.barcodes = vec!["012345678905".to_string(), "5051961234567".to_string()];
    second.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    let mut contested = discogs("dg-contested", Some("master-7"), None);
    contested.barcodes = vec!["012345678905".to_string()];
    contested.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
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
        crate::import::PressingLink {
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
        crate::import::PressingLink {
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
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7 "))];
    one.area = Some(bae_types::pressing::area("JP"));
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(
        Some("Label Name"),
        None,
    )];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("cat-7"))];
    other.area = bae_types::pressing::ReleaseArea::discogs("Japan");
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(
        Some("Label Name"),
        None,
    )];

    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// A catalog number with nothing else stated pairs nothing either.
#[test]
fn a_catalog_number_alone_pairs_nothing() {
    let mut one = mb("mb-1", Some("group-x"), None);
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    let mut other = discogs("dg-1", Some("master-7"), None);
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];

    let groups = grouped(vec![one, other]);
    assert_eq!(rows(&groups), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// A catalog number the two sources share cannot pair records whose barcodes
/// contradict each other: the stronger evidence says they are two objects.
#[test]
fn a_shared_catalog_number_cannot_bypass_incompatible_barcodes() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    one.barcodes = vec!["012345678905".to_string()];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
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
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.title = "Album Title - Subtitle".to_string();
    other.barcodes = vec!["012345678905".to_string()];
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];

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
    catalog_only.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    let mut barcoded = mb("mb-barcode", Some("group-x"), Some(1994));
    barcoded.barcodes = vec!["012345678905".to_string()];
    barcoded.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];
    let mut other = discogs("dg-1", Some("master-7"), Some(1994));
    other.barcodes = vec!["012345678905".to_string()];
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-7"))];

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
