use super::*;

/// The tests are about how results bucket, pair and order by year, none of
/// which the candidate's text takes part in.
fn grouped(results: Vec<MetadataResult>) -> Vec<ReleaseGroup> {
    group_results(unranked(results))
}

fn mb(release_id: &str, group_id: Option<&str>, year: Option<i32>) -> MetadataResult {
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        barcode: None,
        cover_art: None,
        source_group_id: group_id.map(str::to_string),
        source_tracks: None,
    }
}

/// The same album on Discogs, whose group is a master and whose card URL
/// therefore differs from MusicBrainz's.
fn discogs(release_id: &str, group_id: Option<&str>, year: Option<i32>) -> MetadataResult {
    MetadataResult {
        source: MetadataSource::Discogs,
        ..mb(release_id, group_id, year)
    }
}

fn cover() -> RemoteCover {
    RemoteCover {
        url: "https://caa.example/front.jpg".to_string(),
        thumbnail_url: "https://caa.example/thumb.jpg".to_string(),
        label: MetadataSource::MusicBrainz.cover_source_label().to_string(),
        source: MetadataSource::MusicBrainz,
    }
}

fn lead_ids(group: &ReleaseGroup) -> Vec<Vec<&str>> {
    group
        .pressings
        .iter()
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
    assert_eq!(groups[0].pressings.len(), 2);
    assert_eq!(
        groups[0].sources,
        vec![ReleaseGroupSource {
            source: MetadataSource::MusicBrainz,
            group_url: Some("https://musicbrainz.org/release-group/group-x".to_string()),
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
    assert_eq!(groups[0].pressings.len(), 2);
    assert_eq!(groups[1].pressings.len(), 1);
}

#[test]
fn ungrouped_result_is_its_own_single_pressing_card() {
    let groups = grouped(vec![mb("rel-1", None, Some(1999))]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, "rel-1");
    assert_eq!(
        groups[0].sources,
        vec![ReleaseGroupSource {
            source: MetadataSource::MusicBrainz,
            group_url: None,
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

/// The two providers describing the same album become one card carrying
/// both, MusicBrainz first, each with its own editorial page.
#[test]
fn the_same_album_across_sources_merges_into_one_card() {
    let groups = grouped(vec![
        discogs("dg-1", Some("master-7"), Some(2001)),
        mb("mb-1", Some("group-x"), Some(1992)),
    ]);
    assert_eq!(groups.len(), 1);
    // The Discogs bucket was seen first, so the card sits at its position
    // — but MusicBrainz leads the sources and names the card.
    assert_eq!(groups[0].id, "group-x");
    assert_eq!(
        groups[0].sources,
        vec![
            ReleaseGroupSource {
                source: MetadataSource::MusicBrainz,
                group_url: Some("https://musicbrainz.org/release-group/group-x".to_string()),
            },
            ReleaseGroupSource {
                source: MetadataSource::Discogs,
                group_url: Some("https://www.discogs.com/master/master-7".to_string()),
            },
        ]
    );
    assert_eq!(groups[0].year_min, Some(1992));
    assert_eq!(groups[0].year_max, Some(2001));
}

#[test]
fn different_titles_across_sources_stay_apart() {
    let mut other = discogs("dg-1", Some("master-7"), None);
    other.title = "Another Album".to_string();
    let groups = grouped(vec![mb("mb-1", Some("group-x"), None), other]);
    assert_eq!(groups.len(), 2);
}

/// Casing and punctuation differences in the title are not different
/// albums; a different artist is.
#[test]
fn the_album_key_ignores_case_and_edge_punctuation() {
    let mut other = discogs("dg-1", Some("master-7"), None);
    other.title = "  album title!".to_string();
    let groups = grouped(vec![mb("mb-1", Some("group-x"), None), other]);
    assert_eq!(groups.len(), 1);

    let mut different_artist = discogs("dg-2", Some("master-8"), None);
    different_artist.artist = Some("Other Artist".to_string());
    let groups = grouped(vec![mb("mb-2", Some("group-y"), None), different_artist]);
    assert_eq!(groups.len(), 2);
}

/// A named artist and no artist at all are not the same album.
#[test]
fn an_absent_artist_matches_only_an_absent_artist() {
    let mut anonymous = discogs("dg-1", Some("master-7"), None);
    anonymous.artist = None;
    let groups = grouped(vec![mb("mb-1", Some("group-x"), None), anonymous.clone()]);
    assert_eq!(groups.len(), 2);

    let mut also_anonymous = mb("mb-2", Some("group-y"), None);
    also_anonymous.artist = None;
    let groups = grouped(vec![also_anonymous, anonymous]);
    assert_eq!(groups.len(), 1);
}

/// Only one bucket per source merges into a card: a second Discogs master
/// with the same title stays its own card rather than joining.
#[test]
fn each_bucket_merges_at_most_once() {
    let groups = grouped(vec![
        mb("mb-1", Some("group-x"), None),
        discogs("dg-1", Some("master-7"), None),
        discogs("dg-2", Some("master-8"), None),
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].sources.len(), 2);
    assert_eq!(groups[1].sources.len(), 1);
}

/// A row paired across both sources is picked whole: the lead is the
/// document the draft is read from, and the other source's record of the
/// same pressing rides along as a partner.
#[test]
fn a_paired_row_is_picked_with_its_partner() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcode = Some("012345678905".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcode = Some("012345678905".to_string());

    let groups = grouped(vec![one, other]);
    assert_eq!(
        groups[0].pressings[0].pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
            release_id: "mb-1".to_string(),
            partners: vec![crate::import::MetadataRef::new(
                "dg-1",
                MetadataSource::Discogs
            )],
        }
    );
}

/// A pressing only one source lists claims only that source.
#[test]
fn a_lone_row_is_picked_with_no_partner() {
    let groups = grouped(vec![discogs("dg-1", Some("master-7"), Some(1992))]);
    assert_eq!(
        groups[0].pressings[0].pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            source: MetadataSource::Discogs,
            release_id: "dg-1".to_string(),
            partners: vec![],
        }
    );
}

/// Barcodes the two sources punctuate differently still name one pressing.
#[test]
fn releases_sharing_a_barcode_are_one_pressing() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.barcode = Some("0 12345 67890 5".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.barcode = Some("012345678905".to_string());

    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
}

#[test]
fn releases_sharing_a_catalog_number_are_one_pressing() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.catalog_number = Some("CAT-7 ".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.catalog_number = Some("cat-7".to_string());

    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1", "dg-1"]]);
}

/// The sources punctuate multi-disc catalog numbers differently, and this
/// pairing is not clever enough to tell "the same number, spelled
/// differently" from "a different number" — so it declines to pair.
#[test]
fn a_formatting_difference_in_the_catalog_number_does_not_pair() {
    let mut one = mb("mb-1", Some("group-x"), Some(1992));
    one.catalog_number = Some("CAT 2 2".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1992));
    other.catalog_number = Some("CAT 2-2".to_string());

    let groups = grouped(vec![one, other]);
    assert_eq!(lead_ids(&groups[0]), vec![vec!["mb-1"], vec!["dg-1"]]);
}

/// A barcode is stronger evidence than a catalog number, so the barcode
/// pair is taken even though an earlier release shares a catalog number
/// with the same Discogs row.
#[test]
fn a_barcode_pair_outranks_a_catalog_pair_for_the_same_release() {
    let mut catalog_only = mb("mb-catalog", Some("group-x"), Some(1992));
    catalog_only.catalog_number = Some("CAT-7".to_string());
    let mut barcoded = mb("mb-barcode", Some("group-x"), Some(1994));
    barcoded.barcode = Some("012345678905".to_string());
    barcoded.catalog_number = Some("CAT-7".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1994));
    other.barcode = Some("012345678905".to_string());
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
        mb("mb-1", Some("group-x"), Some(1992)),
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
        label: MetadataSource::Discogs.cover_source_label().to_string(),
        source: MetadataSource::Discogs,
    });
    let mut mb_covered = mb("mb-1", Some("group-x"), Some(1992));
    mb_covered.cover_art = Some(cover());

    let groups = grouped(vec![discogs_covered, mb_covered]);
    assert_eq!(groups[0].cover_art, Some(cover()));
}

/// Two results carrying the same `source_group_id` string but different
/// sources are still bucketed apart; only the album key merges them, and
/// here the titles differ.
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
        vec![MetadataSource::MusicBrainz, MetadataSource::Discogs]
    );
}

// MARK: - Ranking

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
    mb_release.barcode = Some("0075678169328".to_string());
    let mut dg_release = discogs("dg-1", Some("master-7"), Some(1976));
    dg_release.barcode = Some("0075678169328".to_string());
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
    mb_release.barcode = Some("0075678169328".to_string());
    let mut dg_release = discogs("dg-1", Some("master-7"), Some(1976));
    dg_release.barcode = Some("0075678169328".to_string());
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
    lead.barcode = Some("4988014720311".to_string());
    let reissues: Vec<MetadataResult> = [Some(1991), Some(1988), None]
        .into_iter()
        .map(|year| {
            let mut release = discogs(
                &format!("dg-{}", year.map_or("undated".to_string(), |y| y.to_string())),
                Some("master-7"),
                year,
            );
            release.barcode = Some("4988014720311".to_string());
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

/// Two records neither of which states a year have no year to agree on, so
/// nothing is read into their both leaving it out.
#[test]
fn two_undated_records_pair_by_position_alone() {
    let mut lead = mb("mb-1", Some("group-x"), None);
    lead.barcode = Some("4988014720311".to_string());
    let mut first = discogs("dg-first", Some("master-7"), None);
    first.barcode = Some("4988014720311".to_string());
    let mut second = discogs("dg-second", Some("master-7"), None);
    second.barcode = Some("4988014720311".to_string());

    let groups = grouped(vec![lead, first, second]);
    assert_eq!(
        lead_ids(&groups[0]),
        vec![vec!["mb-1", "dg-first"], vec!["dg-second"]]
    );
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
    one.barcode = Some("0075678169328".to_string());
    let mut other = discogs("dg-1", Some("master-7"), Some(1976));
    other.barcode = Some("0075678169328".to_string());
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
            source: MetadataSource::Discogs,
            release_id: "dg-1".to_string(),
            partners: vec![crate::import::MetadataRef::new(
                "mb-1",
                MetadataSource::MusicBrainz
            )],
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

/// The chips under an album's title name its sources in the order its rows do,
/// so a card whose best row leads with Discogs names Discogs first.
#[test]
fn the_card_names_its_sources_in_the_order_its_rows_do() {
    let (mb_release, dg_release) = paired();

    let groups = group_results(vec![(mb_release, agreed(1)), (dg_release, agreed(3))]);

    assert_eq!(
        groups[0]
            .sources
            .iter()
            .map(|source| source.source)
            .collect::<Vec<_>>(),
        vec![MetadataSource::Discogs, MetadataSource::MusicBrainz]
    );
}
