//! How the rows and the cards are ordered, and which record of a row leads
//! it — what the candidate's own text agrees with about each release, which
//! the grouping tests beside these hold constant. The releases themselves are
//! built by those tests' fixtures.

use super::tests::*;
use super::*;

fn agreed(count: u32) -> Agreements {
    Agreements {
        disc_id: count >= 1,
        barcode: count >= 2,
        catalog: count >= 3,
        label: count >= 4,
        year: count >= 5,
        country: count >= 6,
    }
}

/// The rows the candidate's own text says most about lead, whatever year
/// they were pressed.
#[test]
fn rows_the_text_says_most_about_lead() {
    let groups = group_results(vec![
        (mb("rel-early", Some("group-x"), Some(1976)), agreed(1)),
        (mb("rel-late", Some("group-x"), Some(2003)), agreed(4)),
    ]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["rel-late"], vec!["rel-early"]]);
}

/// Rows the text says as much about keep the pressing-year order.
#[test]
fn rows_the_text_says_as_much_about_keep_the_year_order() {
    let groups = group_results(vec![
        (mb("rel-late", Some("group-x"), Some(2003)), agreed(2)),
        (mb("rel-early", Some("group-x"), Some(1976)), agreed(2)),
    ]);
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
    let groups = group_results(vec![
        (mb("mb-other", Some("group-x"), Some(1976)), agreed(2)),
        (mb_release, Agreements::NONE),
        (dg_release, agreed(4)),
    ]);
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
    let disc_id_only = Agreements {
        disc_id: true,
        ..Agreements::NONE
    };
    let catalog_only = Agreements {
        catalog: true,
        ..Agreements::NONE
    };
    let groups = group_results(vec![
        (mb_release, disc_id_only),
        (dg_release, catalog_only),
        (mb("mb-other", Some("group-x"), Some(1970)), disc_id_only),
    ]);
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["mb-1", "dg-1"], vec!["mb-other"]],
        "two agreements between them beat one, whatever the years say",
    );
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
        vec![
            vec!["mb-1988", "dg-1988"],
            vec!["dg-1991"],
            vec!["dg-undated"]
        ]
    );
}

/// Two Discogs records print the barcode and nothing tells them apart from
/// each other: a catalog listing one object twice. They are one pressing
/// with the MusicBrainz record, which claims the first of them for Discogs.
#[test]
fn records_nothing_tells_apart_are_one_pressing() {
    let mut lead = mb("mb-1", Some("group-x"), None);
    lead.barcodes = vec!["4988014720311".to_string()];
    let mut first = discogs("dg-first", Some("master-7"), None);
    first.barcodes = vec!["4988014720311".to_string()];
    let mut second = discogs("dg-second", Some("master-7"), None);
    second.barcodes = vec!["4988014720311".to_string()];

    let groups = grouped(vec![lead.clone(), first.clone(), second.clone()]);
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["mb-1", "dg-first", "dg-second"]]
    );
    assert_eq!(
        groups[0].pressings[0].pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-1".to_string()),
            partners: vec![crate::import::MetadataRef::new(Catalog::Discogs, "dg-first")],
        }
    );
    assert_eq!(pressing_count(vec![second, lead, first]), 1);
}

/// Two Discogs records print the barcode but contradict each other on the
/// year, so which one the undated MusicBrainz record is cannot be said: it
/// settles alone rather than the first listed being taken, and the two
/// Discogs records, which are not one object, stay apart.
#[test]
fn records_that_contradict_each_other_leave_their_common_match_ambiguous() {
    let mut lead = mb("mb-1", Some("group-x"), None);
    lead.barcodes = vec!["4988014720311".to_string()];
    let mut first = discogs("dg-first", Some("master-7"), Some(1991));
    first.barcodes = vec!["4988014720311".to_string()];
    let mut second = discogs("dg-second", Some("master-7"), Some(1993));
    second.barcodes = vec!["4988014720311".to_string()];

    let groups = grouped(vec![lead.clone(), first.clone(), second.clone()]);
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["dg-first"], vec!["dg-second"], vec!["mb-1"]]
    );
    assert_eq!(pressing_count(vec![second, lead, first]), 3);
}

/// Cards are ordered by their best row, so the album the folder describes
/// is the one at the top of the list.
#[test]
fn cards_are_ordered_by_their_best_row() {
    let groups = group_results(vec![
        (mb("rel-stranger", Some("group-stranger"), None), agreed(1)),
        (mb("rel-named", Some("group-named"), None), agreed(4)),
    ]);
    assert_eq!(
        groups.iter().map(|group| group.id.as_str()).collect::<Vec<_>>(),
        vec!["group-named", "group-stranger"],
    );
}

// MARK: - Which record of a pressing leads it

/// A tracklist as a source states it. What it says does not matter here; that
/// it was said is the tie-break.
fn listed() -> crate::import::search::SourceTracks {
    crate::import::search::SourceTracks::Listed {
        count: 9,
        total_duration_ms: Some(2_400_000),
    }
}

/// One pressing as both sources state it, paired by the barcode they share.
fn paired() -> (MetadataResult, MetadataResult) {
    let mut one = mb("mb-1", Some("group-x"), Some(1976));
    one.barcodes = vec!["0075678169328".to_string()];
    let mut other = discogs("dg-1", Some("master-7"), Some(1976));
    other.barcodes = vec!["0075678169328".to_string()];
    (one, other)
}

/// Both sources describe the disc and neither of them is the one the draft is
/// read from by name: the record the folder says more about leads the row, and
/// picking the row claims the other beside it.
#[test]
fn the_record_the_text_says_most_about_leads_its_pressing() {
    let (mb_release, dg_release) = paired();

    let groups = group_results(vec![(mb_release, agreed(1)), (dg_release, agreed(3))]);

    assert_eq!(lead_ids(&groups[0]), vec![vec!["dg-1", "mb-1"]]);
    assert_eq!(
        groups[0].pressings[0].pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::Discogs, "dg-1".to_string()),
            partners: vec![crate::import::MetadataRef::new(Catalog::MusicBrainz, "mb-1")],
        }
    );
}

/// Records the folder says as much about: the one that states a tracklist
/// leads, since the draft's rows and the settle's check of them against the
/// audio are read out of that tracklist. A source that answered and listed
/// nothing states none.
#[test]
fn a_stated_tracklist_leads_records_the_text_says_as_much_about() {
    let (mut mb_release, mut dg_release) = paired();
    mb_release.source_tracks = Some(crate::import::search::SourceTracks::Nothing);
    dg_release.source_tracks = Some(listed());

    let groups = group_results(vec![(mb_release, agreed(2)), (dg_release, agreed(2))]);

    assert_eq!(lead_ids(&groups[0]), vec![vec!["dg-1", "mb-1"]]);
}

/// Nothing the folder says tells the two records apart and both state a
/// tracklist, so the source name has the last word.
#[test]
fn records_nothing_tells_apart_lead_with_musicbrainz() {
    let (mut mb_release, mut dg_release) = paired();
    mb_release.source_tracks = Some(listed());
    dg_release.source_tracks = Some(listed());

    let groups = group_results(vec![(dg_release, agreed(2)), (mb_release, agreed(2))]);

    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
}

/// The chips under an album's title name its sources in the one order surfaces
/// list them in, whichever record the card's best row is read from.
#[test]
fn the_card_names_its_sources_in_the_order_surfaces_list_them() {
    let (mb_release, dg_release) = paired();

    let groups = group_results(vec![(mb_release, agreed(1)), (dg_release, agreed(3))]);

    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["dg-1", "mb-1"]],
        "the best row is read from the Discogs record",
    );
    assert_eq!(
        groups[0]
            .sources
            .iter()
            .map(|source| source.source)
            .collect::<Vec<_>>(),
        vec![Catalog::MusicBrainz, Catalog::Discogs]
    );
}
