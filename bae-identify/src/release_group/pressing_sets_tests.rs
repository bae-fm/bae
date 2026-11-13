//! How records of the two catalogs pair into one pressing row: what the
//! evidence they carry decides, whatever order the records arrive in.

use crate::release_group::tests::*;
use crate::release_group::*;
use bae_types::pressing::{Medium, StatedFormat, StatedMedia};

/// An artist one source spells differently, or leaves out, does not keep two
/// records of one pressing apart: the barcode pairs them, and the pair joins
/// their cards.
#[test]
fn artist_spelling_does_not_block_established_identity() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcodes = vec!["012345678905".to_string()];
    let mut respelled = discogs("dg-1", Some("master-7"), Some(1992));
    respelled.artist = Some(bae_types::credits::CreditLine::of_name("The Artist Name"));
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
    assert_eq!(
        groups[0]
            .artists
            .iter()
            .map(bae_types::credits::CreditLine::plain_text)
            .collect::<Vec<_>>(),
        vec!["Artist Name".to_string()]
    );
}

/// A card whose records title the album differently lists every title, and
/// one artist the records spell alike once; a title only punctuated
/// differently is the same title.
#[test]
fn a_card_lists_every_title_its_records_state() {
    let record = |source: bae_catalogs::catalog::LookupCatalog, id: &str, title: &str| {
        let mut record = match source {
            bae_catalogs::catalog::LookupCatalog::MusicBrainz => {
                mb(id, Some("group-x"), Some(1992))
            }
            bae_catalogs::catalog::LookupCatalog::Discogs => {
                discogs(id, Some("master-7"), Some(1992))
            }
        };
        record.title = title.to_string();
        record.artist = Some(bae_types::credits::CreditLine::of_name("Mara Quill"));
        record.barcodes = vec!["012345678905".to_string()];
        record
    };
    use bae_catalogs::catalog::LookupCatalog::{Discogs, MusicBrainz};

    let groups = grouped(vec![
        record(MusicBrainz, "mb-1", "Low Tide Hours"),
        record(Discogs, "dg-1", "Low Tide Hours (Remastered)"),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].titles,
        vec!["Low Tide Hours".to_string(), "Low Tide Hours (Remastered)".to_string()]
    );
    assert_eq!(groups[0].artists.len(), 1);

    let groups = grouped(vec![
        record(MusicBrainz, "mb-1", "Low Tide: Hours"),
        record(Discogs, "dg-1", "Low Tide - Hours"),
    ]);
    assert_eq!(groups[0].titles, vec!["Low Tide: Hours".to_string()]);
}

/// A release document that names the other source's release as the same
/// release pairs with it whatever the text says, and a contradiction in the
/// facts does not overrule what the document states.
#[test]
fn a_stated_link_pairs_despite_the_text_and_the_facts() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.title = "Album Title: Subtitle".to_string();
    one.links = vec![bae_catalogs::records::MetadataRef::new(Catalog::Discogs, "dg-1")];
    let mut other = discogs("dg-1", Some("master-7"), Some(1994));
    other.title = "Another Album".to_string();
    other.artist = Some(bae_types::credits::CreditLine::of_name("Other Artist"));

    let groups = grouped(vec![other, one]);
    assert_eq!(groups.len(), 1);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
    assert_eq!(groups[0].id, "group-x");
    assert_eq!(
        groups[0].titles,
        vec!["Album Title: Subtitle".to_string(), "Another Album".to_string()]
    );
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                LookupCatalog::MusicBrainz,
                "mb-1".to_string()
            ),
            partners: vec![bae_catalogs::records::LookupRef::new(LookupCatalog::Discogs, "dg-1")],
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
    one.links = vec![bae_catalogs::records::MetadataRef::new(Catalog::Discogs, "dg-linked")];
    let mut linked = discogs("dg-linked", Some("master-7"), Some(1992));
    linked.barcodes = vec!["012345678905".to_string()];
    let mut barcoded = discogs("dg-barcoded", Some("master-7"), Some(1992));
    barcoded.barcodes = vec!["012345678905".to_string()];

    let groups = grouped(vec![barcoded, one, linked]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-linked"], vec!["dg-barcoded"]]);
    assert_eq!(
        groups[0].sections[0].pressings[0].pick(),
        crate::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                LookupCatalog::MusicBrainz,
                "mb-1".to_string()
            ),
            partners: vec![bae_catalogs::records::LookupRef::new(
                LookupCatalog::Discogs,
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
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("Label Name Records"), None)];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["012345678905".to_string()];
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("LN"), None)];

    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
    assert_eq!(groups[0].labels, vec!["Label Name Records".to_string()]);

    // An agreeing label does not make a shared catalog number a pair.
    let mut one = mb("mb-2", Some("group-y"), None);
    one.labels =
        vec![bae_types::pressing::ReleaseLabel::of(Some("Label Name Records"), Some("CAT-7"))];
    let mut other = discogs("dg-2", Some("master-8"), None);
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("Label Name"), Some("CAT-7"))];
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
    other.media =
        StatedMedia::Formats(vec![StatedFormat { medium: Some(Medium::Cd), quantity: 1 }]);
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
    other.media =
        StatedMedia::Formats(vec![StatedFormat { medium: Some(Medium::Vinyl), quantity: 1 }]);
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
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("Door Records"), Some("DR 100"))];
    one.area = Some(bae_types::pressing::area("IT"));
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcodes = vec!["4012345678901".to_string()];
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("Door"), Some("DR-100"))];
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
    download.media =
        StatedMedia::Formats(vec![StatedFormat { medium: Some(Medium::Digital), quantity: 1 }]);
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
            vec![vec!["dg-undated"], vec!["mb-1988", "dg-1988"], vec!["mb-1991", "dg-1991"]],
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
    assert_eq!(rows, vec![vec!["dg-contested"], vec!["mb-first"], vec!["mb-second", "dg-below"]]);
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
    one.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("Label Name"), None)];
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("cat-7"))];
    other.area = bae_types::pressing::ReleaseArea::discogs("Japan");
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("Label Name"), None)];

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
    assert_eq!(groups[0].titles, vec!["Album Title: Subtitle".to_string()]);
    assert_eq!(
        groups[0].sources.iter().map(|source| source.source).collect::<Vec<_>>(),
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
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-catalog"], vec!["mb-barcode", "dg-1"]]);
}

/// Releases with nothing to pair on stay their own rows, and the Discogs
/// leftovers land as single-source pressings.
#[test]
fn unpaired_releases_are_single_source_pressings() {
    let groups = grouped(vec![
        linked(mb("mb-1", Some("group-x"), Some(1992)), "master-7"),
        discogs("dg-1", Some("master-7"), Some(2001)),
    ]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1"], vec!["dg-1"]], "one card, two rows");
}

/// A MusicBrainz record printing the second of the barcodes a Discogs record
/// lists is the same pressing.
#[test]
fn a_barcode_listed_second_pairs_the_records() {
    let mut musicbrainz = mb("mb-1", Some("group-x"), Some(1992));
    musicbrainz.barcodes = vec!["5051961234567".to_string()];
    let mut listing_two = discogs("dg-1", Some("7"), Some(1992));
    listing_two.barcodes = vec!["0 12345 67890 5".to_string(), "5051961234567".to_string()];
    let groups = grouped(vec![musicbrainz, listing_two]);
    assert_eq!(groups.len(), 1);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
}

/// A record of a CD and a vinyl is one object with the other catalog's
/// record naming the same two media, whichever it lists first, and not with
/// one naming a cassette.
#[test]
fn a_pressing_of_two_media_pairs_only_with_a_record_of_both() {
    let barcode = "012345678905";
    let musicbrainz = |media: Vec<Medium>| {
        let mut record = mb("mb-1", Some("group-x"), Some(1992));
        record.barcodes = vec![barcode.to_string()];
        record.media = StatedMedia::PerMedium(media.into_iter().map(Some).collect());
        record
    };
    let discogs_of = |media: &[Medium]| {
        let mut record = discogs("dg-1", Some("7"), Some(1992));
        record.barcodes = vec![barcode.to_string()];
        record.media = StatedMedia::Formats(
            media
                .iter()
                .map(|&medium| StatedFormat { medium: Some(medium), quantity: 1 })
                .collect(),
        );
        record
    };
    let cassette = discogs_of(&[Medium::Cassette]);
    let both = discogs_of(&[Medium::Vinyl, Medium::Cd]);
    for record in
        [musicbrainz(vec![Medium::Vinyl, Medium::Cd]), musicbrainz(vec![Medium::Cd, Medium::Vinyl])]
    {
        assert_eq!(
            pressing_count(vec![record.clone(), cassette.clone()]),
            2,
            "a cassette is not part of a CD-plus-vinyl object"
        );
        assert_eq!(
            pressing_count(vec![record, both.clone()]),
            1,
            "the record naming both media is the same object"
        );
    }
}

/// A label's reissues print the pressing's barcode and catalog number
/// again, so a code can name several of the other source's records. The
/// pressing year is what tells them apart, ahead of the order the source
/// listed them in.
#[test]
fn a_shared_barcode_pairs_with_the_record_pressed_the_same_year() {
    let mut lead = mb("mb-1988", Some("group-x"), Some(1988));
    lead.barcodes = vec!["4988014720311".to_string()];
    let reissues: Vec<MetadataResult> = [Some(1991), Some(1988), None]
        .into_iter()
        .map(|year| {
            let mut release = discogs(
                &format!("dg-{}", year.map_or("undated".to_string(), |y| y.to_string())),
                Some("master-7"),
                year,
            );
            release.barcodes = vec!["4988014720311".to_string()];
            release
        })
        .collect();

    let groups = grouped(std::iter::once(lead).chain(reissues).collect());
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["mb-1988", "dg-1988"], vec!["dg-1991"], vec!["dg-undated"]]
    );
}

/// Two Discogs records print the MusicBrainz record's barcode and nothing
/// else tells them apart. They are two records of one catalog, so they stay
/// two rows, and which of them the MusicBrainz record is cannot be said: it
/// settles alone.
#[test]
fn two_records_of_one_catalog_leave_their_common_match_alone() {
    let mut lead = linked(mb("mb-1", Some("group-x"), None), "master-7");
    lead.barcodes = vec!["4988014720311".to_string()];
    let mut first = discogs("dg-first", Some("master-7"), None);
    first.barcodes = vec!["4988014720311".to_string()];
    let mut second = discogs("dg-second", Some("master-7"), None);
    second.barcodes = vec!["4988014720311".to_string()];

    let groups = grouped(vec![lead.clone(), first.clone(), second.clone()]);
    let mut rows = lead_ids(&groups[0]);
    rows.sort();
    assert_eq!(rows, vec![vec!["dg-first"], vec!["dg-second"], vec!["mb-1"]]);
    assert_eq!(pressing_count(vec![second, lead, first]), 3);
}

/// Two Discogs records print the barcode but contradict each other on the
/// year, so which one the undated MusicBrainz record is cannot be said: it
/// settles alone rather than the first listed being taken, and the two
/// Discogs records, which are not one object, stay apart.
#[test]
fn records_that_contradict_each_other_leave_their_common_match_ambiguous() {
    let mut lead = linked(mb("mb-1", Some("group-x"), None), "master-7");
    lead.barcodes = vec!["4988014720311".to_string()];
    let mut first = discogs("dg-first", Some("master-7"), Some(1991));
    first.barcodes = vec!["4988014720311".to_string()];
    let mut second = discogs("dg-second", Some("master-7"), Some(1993));
    second.barcodes = vec!["4988014720311".to_string()];

    let groups = grouped(vec![lead.clone(), first.clone(), second.clone()]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["dg-first"], vec!["dg-second"], vec!["mb-1"]]);
    assert_eq!(pressing_count(vec![second, lead, first]), 3);
}
