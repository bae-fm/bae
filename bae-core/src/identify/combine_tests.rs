use super::*;

/// Most of these are about how the sets intersect, which the candidate's
/// own text takes no part in: a candidate that states nothing offers every
/// answer, so nothing folds and the order is the one the signals gave.
fn combine(discid: Results, barcode: Results, catalog: Results) -> CombineOutcome {
    combine_results(discid, barcode, catalog, &CandidateText::default())
}

fn mk_result(release_id: &str, group_id: Option<&str>) -> MetadataResult {
    MetadataResult::for_test(Catalog::MusicBrainz, release_id, group_id)
}

fn pair(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
    (
        mk_result(release_id, group_id),
        LibraryStatus::absent(release_id),
    )
}

fn pair_src(
    source: Catalog,
    release_id: &str,
    group_id: Option<&str>,
) -> (MetadataResult, LibraryStatus) {
    let mut result = mk_result(release_id, group_id);
    result.source = source;
    (result, LibraryStatus::absent(release_id))
}

fn ids(matches: &[MetadataResult]) -> Vec<&str> {
    matches.iter().map(|m| m.release_id.as_str()).collect()
}

fn narrowed(outcome: CombineOutcome) -> NarrowedOut {
    match outcome {
        CombineOutcome::Found { narrowed_out, .. } => narrowed_out,
        other => panic!("expected Found, got {other:?}"),
    }
}

fn found(outcome: CombineOutcome) -> (Vec<MetadataResult>, Vec<LookupProvenance>, Vec<u32>) {
    match outcome {
        CombineOutcome::Found {
            matches,
            provenance,
            pressings,
            ..
        } => (matches, provenance, pressings),
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn nothing_checked_or_nothing_found_yields_not_found_anywhere() {
    let outcome = combine(vec![], vec![], vec![]);
    assert!(matches!(outcome, CombineOutcome::NotFoundAnywhere));
}

/// One checked signal answers on its own: there is nothing to agree with.
#[test]
fn one_set_alone_is_the_answer() {
    for (name, discid, barcode, catalog) in [
        (
            "disc id alone",
            vec![pair("rel-a", Some("group-1"))],
            vec![],
            vec![],
        ),
        (
            "barcode alone",
            vec![],
            vec![pair("rel-a", Some("group-1")), pair("rel-b", None)],
            vec![],
        ),
        (
            "catalog alone",
            vec![],
            vec![],
            vec![pair("rel-a", Some("group-1"))],
        ),
    ] {
        let expected = discid.len().max(barcode.len()).max(catalog.len());
        let (matches, _, _) = found(combine(discid, barcode, catalog));
        assert_eq!(matches.len(), expected, "{name}");
    }
}

/// Several pressings of one release group all stand: which one is on disk
/// is the user's call.
#[test]
fn every_pressing_the_signals_agree_on_stays() {
    let both = vec![
        pair("rel-a", Some("group-1")),
        pair("rel-b", Some("group-2")),
    ];
    let (matches, _, _) = found(combine(both.clone(), both, vec![]));
    assert_eq!(matches.len(), 2);
}

/// The intersection is what agreement looks like — the release both signals
/// name, in the first signal's order.
#[test]
fn two_checked_signals_intersect() {
    let discid = vec![
        pair("rel-a", Some("group-1")),
        pair("rel-b", Some("group-1")),
    ];
    let barcode = vec![pair("rel-b", Some("group-1"))];
    let (matches, provenance, _) = found(combine(discid, barcode, vec![]));
    assert_eq!(ids(&matches), vec!["rel-b"]);
    assert!(provenance[0].by_disc_id && provenance[0].by_barcode);
    assert!(!provenance[0].by_catalog);
}

/// Three checked signals have to all agree, not just two of them.
#[test]
fn three_checked_signals_intersect() {
    let discid = vec![pair("rel-a", None), pair("rel-b", None)];
    let barcode = vec![pair("rel-a", None), pair("rel-b", None)];
    let catalog = vec![pair("rel-b", None)];
    let (matches, provenance, _) = found(combine(discid, barcode, catalog));
    assert_eq!(ids(&matches), vec!["rel-b"]);
    assert!(provenance[0].by_disc_id);
    assert!(provenance[0].by_barcode);
    assert!(provenance[0].by_catalog);
}

/// Signals that share no result are not a failure to identify: each saw a
/// real release, so the set is their union, in signal order, and each row
/// says which signal produced it.
#[test]
fn an_empty_intersection_falls_through_to_the_union() {
    let discid = vec![pair("rel-a", Some("group-1"))];
    let barcode = vec![pair("rel-b", Some("group-2"))];
    let catalog = vec![pair("rel-c", Some("group-3"))];
    let (matches, provenance, _) = found(combine(discid, barcode, catalog));
    assert_eq!(ids(&matches), vec!["rel-a", "rel-b", "rel-c"]);
    assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
    assert!(provenance[1].by_barcode && !provenance[1].by_disc_id);
    assert!(provenance[2].by_catalog && !provenance[2].by_disc_id);
}

/// The union names each release once even when two signals both saw it —
/// which happens when a third signal is what emptied the intersection.
#[test]
fn the_union_names_each_release_once() {
    let discid = vec![pair("rel-a", None)];
    let barcode = vec![pair("rel-a", None)];
    let catalog = vec![pair("rel-b", None)];
    let (matches, provenance, _) = found(combine(discid, barcode, catalog));
    assert_eq!(ids(&matches), vec!["rel-a", "rel-b"]);
    assert!(provenance[0].by_disc_id && provenance[0].by_barcode);
}

/// A checked signal that found nothing takes no part: the rest still
/// answer, rather than the empty set emptying everything.
#[test]
fn a_signal_that_found_nothing_does_not_empty_the_set() {
    let barcode = vec![pair("rel-a", None)];
    let (matches, _, _) = found(combine(vec![], barcode, vec![]));
    assert_eq!(ids(&matches), vec!["rel-a"]);
}

/// Releases are told apart by source as well as id, so the same id on two
/// providers is two releases and never intersects by accident.
#[test]
fn the_same_id_on_two_providers_is_two_releases() {
    let discid = vec![pair_src(Catalog::MusicBrainz, "rel-a", None)];
    let barcode = vec![pair_src(Catalog::Discogs, "rel-a", None)];
    let (matches, _, _) = found(combine(discid, barcode, vec![]));
    assert_eq!(matches.len(), 2);
}

/// What agreement left out comes back beside the matches: every release a
/// signal named that the intersection does not hold, in signal order, each
/// once, saying which signal named it.
#[test]
fn an_intersection_hands_back_what_it_narrowed_out() {
    let discid = vec![
        pair("rel-a", None),
        pair("rel-shared", None),
        pair("rel-b", None),
    ];
    let barcode = vec![pair("rel-shared", None), pair("rel-c", None)];
    let outcome = combine(discid, barcode, vec![]);
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-shared"]);

    let narrowed = narrowed(outcome);
    assert_eq!(ids(&narrowed.matches), vec!["rel-a", "rel-b", "rel-c"]);
    assert_eq!(narrowed.library_statuses.len(), 3);
    assert!(narrowed.provenance[0].by_disc_id && !narrowed.provenance[0].by_barcode);
    assert!(narrowed.provenance[2].by_barcode && !narrowed.provenance[2].by_disc_id);
}

/// A release two signals both named, that a third narrowed out, is one
/// entry saying both named it.
#[test]
fn a_narrowed_out_release_two_signals_named_is_named_once() {
    let discid = vec![pair("rel-a", None), pair("rel-shared", None)];
    let barcode = vec![pair("rel-a", None), pair("rel-shared", None)];
    let catalog = vec![pair("rel-shared", None)];
    let narrowed = narrowed(combine(discid, barcode, catalog));
    assert_eq!(ids(&narrowed.matches), vec!["rel-a"]);
    assert!(narrowed.provenance[0].by_disc_id && narrowed.provenance[0].by_barcode);
    assert!(!narrowed.provenance[0].by_catalog);
}

/// Signals that share nothing already list everything they saw, and one
/// signal answering alone is the whole answer: neither narrowed anything.
#[test]
fn a_union_and_a_lone_signal_narrow_nothing() {
    let disagreeing = combine(vec![pair("rel-a", None)], vec![pair("rel-b", None)], vec![]);
    assert!(narrowed(disagreeing).is_empty());

    let alone = combine(
        vec![pair("rel-a", None), pair("rel-b", None)],
        vec![],
        vec![],
    );
    assert!(narrowed(alone).is_empty());
}

/// A result the source returned without a group id is still a release the
/// user can pick; it stands as its own single-pressing card.
#[test]
fn a_result_with_no_group_id_stays_in_the_set() {
    let results = vec![pair("rel-a", Some("group-x")), pair("rel-b", None)];
    let (matches, _, _) = found(combine(results, vec![], vec![]));
    assert_eq!(matches.len(), 2);
}

// MARK: - The candidate's own text judges what survived

fn folder(lines: &[&str]) -> CandidateText {
    let pool: Vec<crate::signals::TextLine> = lines
        .iter()
        .map(|text| crate::signals::TextLine {
            text: (*text).to_string(),
            origin: crate::signals::SignalOrigin::FolderName,
            file: None,
            region: None,
        })
        .collect();
    CandidateText::of(&pool, &[])
}

/// One pressing of AC/DC's *Dirty Deeds* as MusicBrainz states it: the
/// folder's catalog number, label, year and country all over it.
fn dirty_deeds(release_id: &str, year: i32) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Dirty Deeds Done Dirt Cheap".to_string(),
            artist: Some("AC/DC".to_string()),
            label: Some("Atlantic".to_string()),
            catalog_number: Some("16033-2".to_string()),
            country: Some("US".to_string()),
            year: Some(year),
            source_group_id: Some("rg-dirty-deeds".to_string()),
            ..mk_result(release_id, Some("rg-dirty-deeds"))
        },
        LibraryStatus::absent(release_id),
    )
}

/// Somebody else's record, which a misread barcode came back naming.
fn manu_chao() -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Clandestino".to_string(),
            artist: Some("Manu Chao".to_string()),
            label: Some("Virgin".to_string()),
            catalog_number: Some("724384463328".to_string()),
            country: Some("FR".to_string()),
            year: Some(1998),
            source_group_id: Some("rg-clandestino".to_string()),
            ..mk_result("rel-clandestino", Some("rg-clandestino"))
        },
        LibraryStatus::absent("rel-clandestino"),
    )
}

/// The folder's text is what orders the rows: the pressing it names the
/// catalog number, label, year and country of leads, and the pressings the
/// disc ID alone named follow.
#[test]
fn the_pressing_the_folder_describes_leads_the_disc_id_s_others() {
    let text = folder(&[
        "AC-DC - Dirty Deeds Done Dirt Cheap [16033-2]",
        "Atlantic 1976 US",
    ]);
    let discid = vec![
        dirty_deeds("rel-1994", 1994),
        dirty_deeds("rel-2003", 2003),
        dirty_deeds("rel-1976", 1976),
    ];
    let outcome = combine_results(discid, vec![], vec![], &text);
    let (matches, provenance, _) = found(outcome);
    assert_eq!(ids(&matches), vec!["rel-1976", "rel-1994", "rel-2003"]);
    assert!(provenance.iter().all(|lookup| lookup.by_disc_id));
}

/// A barcode that came back naming somebody else's record read the wrong
/// digits: the folder says nothing about it, and it is offered under the
/// rest rather than beside them.
#[test]
fn a_barcode_naming_a_record_the_folder_never_mentions_folds() {
    let text = folder(&["AC-DC - Dirty Deeds Done Dirt Cheap [16033-2]"]);
    let outcome = combine_results(
        vec![dirty_deeds("rel-1976", 1976)],
        vec![manu_chao()],
        vec![],
        &text,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-1976"]);
    let left_out = narrowed(outcome).matches;
    assert_eq!(ids(&left_out), vec!["rel-clandestino"]);
}

/// Folding shortens the list; it never empties it. A barcode answering on
/// its own is the whole of what there is to offer, whatever the folder
/// says.
#[test]
fn a_barcode_answering_alone_is_offered_however_little_the_folder_says() {
    let text = folder(&["CD1"]);
    let (matches, _, _) = found(combine_results(vec![], vec![manu_chao()], vec![], &text));
    assert_eq!(ids(&matches), vec!["rel-clandestino"]);
}

/// A candidate carrying no text at all was never asked, so nothing it
/// found is set aside on its silence.
#[test]
fn a_candidate_with_no_text_narrows_nothing_on_it() {
    let outcome = combine_results(
        vec![dirty_deeds("rel-1976", 1976)],
        vec![manu_chao()],
        vec![],
        &CandidateText::default(),
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(matches.len(), 2);
    assert!(narrowed(outcome).is_empty());
}

/// What the intersection left out and what the folder says nothing about
/// are one list.
#[test]
fn the_intersection_s_leftovers_and_the_folder_s_are_one_list() {
    let text = folder(&["AC-DC - Dirty Deeds Done Dirt Cheap [16033-2]"]);
    let discid = vec![dirty_deeds("rel-1976", 1976), dirty_deeds("rel-1994", 1994)];
    let barcode = vec![dirty_deeds("rel-1976", 1976), manu_chao()];
    let outcome = combine_results(discid, barcode, vec![], &text);
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-1976"]);
    let left_out = narrowed(outcome).matches;
    let mut left_out = ids(&left_out);
    left_out.sort_unstable();
    assert_eq!(left_out, vec!["rel-1994", "rel-clandestino"]);
}

// MARK: - The pressing is what is offered or set aside

/// The rows a surface draws from a list of matches, with the badges it
/// draws on them — judged as `combine` judged them and read off the rows
/// the run recorded, which is what `identify::view` does with a stored
/// verdict.
fn rows(
    matches: &[MetadataResult],
    provenance: &[LookupProvenance],
    pressings: &[u32],
    text: &CandidateText,
) -> Vec<(Pressing, crate::identify::agreements::Agreements)> {
    let judged: Vec<Judged> = matches
        .iter()
        .cloned()
        .zip(provenance.iter().cloned())
        .map(|(result, lookup)| {
            let agreements = agreements_of(&result, text, &lookup);
            (result, agreements)
        })
        .collect();
    let judgements = Judgements::of(&judged);
    crate::import::release_group::group_formed_rows(judged, pressings)
        .into_iter()
        .flat_map(|group| group.pressings)
        .map(|pressing| {
            let agreements = pressing.agreements(&judgements);
            (pressing, agreements)
        })
        .collect()
}

fn badges(agreements: &crate::identify::agreements::Agreements) -> Vec<&'static str> {
    [
        ("Disc ID", agreements.disc_id),
        ("Barcode", agreements.barcode),
        ("Catalog", agreements.catalog),
        ("Label", agreements.label),
        ("Year", agreements.year),
        ("Country", agreements.country),
    ]
    .into_iter()
    .filter_map(|(name, agreed)| agreed.then_some(name))
    .collect()
}

/// The Japanese pressing of *Van Halen II* the disc ID names, as
/// MusicBrainz has it: the folder's catalog number and label, its country
/// as a code, and the barcode the sleeve prints.
fn van_halen_musicbrainz() -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Van Halen II".to_string(),
            artist: Some("Van Halen".to_string()),
            label: Some("Warner Bros.".to_string()),
            catalog_number: Some("20P2-2031".to_string()),
            country: Some("JP".to_string()),
            barcodes: vec!["4988014720311".to_string()],
            media: crate::import::search::StatedMedia::Undescribed,
            links: Vec::new(),
            year: Some(1988),
            source_group_id: Some("rg-van-halen-ii".to_string()),
            ..mk_result("mb-van-halen-ii", Some("rg-van-halen-ii"))
        },
        LibraryStatus::absent("mb-van-halen-ii"),
    )
}

/// One of the four Discogs records of that same catalog number, all of
/// which the barcode lookup came back with.
fn van_halen_discogs(release_id: &str, year: Option<i32>) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            source: Catalog::Discogs,
            title: "Van Halen II".to_string(),
            artist: Some("Van Halen".to_string()),
            label: Some("Warner Bros.".to_string()),
            catalog_number: Some("20P2-2031".to_string()),
            country: Some("Japan".to_string()),
            barcodes: vec!["4988014720311".to_string()],
            media: crate::import::search::StatedMedia::Undescribed,
            links: Vec::new(),
            year,
            source_group_id: Some("master-van-halen-ii".to_string()),
            ..mk_result(release_id, Some("master-van-halen-ii"))
        },
        LibraryStatus::absent(release_id),
    )
}

/// The disc ID answers on MusicBrainz alone, so the Discogs record of the
/// pressing it names can only ever be the barcode's answer — never the
/// intersection's. Pairing before the narrowing is what keeps the two
/// together: the row the folder describes is offered carrying both
/// sources, and the reissues that merely print the same barcode go under
/// the disclosure whole.
#[test]
fn the_discogs_record_of_the_pressing_the_disc_id_named_is_offered_with_it() {
    let text = folder(&["1979 - Van Halen II (Warner Bros., 20P2-2031, Japan)"]);
    let outcome = combine_results(
        vec![van_halen_musicbrainz()],
        vec![
            van_halen_musicbrainz(),
            van_halen_discogs("dg-1991", Some(1991)),
            van_halen_discogs("dg-1988", Some(1988)),
            van_halen_discogs("dg-undated-a", None),
            van_halen_discogs("dg-undated-b", None),
        ],
        vec![],
        &text,
    );
    let (matches, provenance, pressings) = found(outcome.clone());
    let offered = rows(&matches, &provenance, &pressings, &text);
    assert_eq!(offered.len(), 1, "one row, not two: {offered:?}");
    assert_eq!(
        ids(&offered[0].0.releases),
        vec!["mb-van-halen-ii", "dg-1988"],
        "the Discogs record of the same pressing rides with it"
    );
    assert_eq!(
        badges(&offered[0].1),
        vec!["Disc ID", "Barcode", "Catalog", "Label", "Country"]
    );
    assert_eq!(
        offered[0].0.pick(),
        crate::import::MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(
                Catalog::MusicBrainz,
                "mb-van-halen-ii".to_string()
            ),
            partners: vec![crate::import::MetadataRef::new(Catalog::Discogs, "dg-1988")],
        },
        "so picking the row claims both sources"
    );

    let narrowed = narrowed(outcome);
    assert_eq!(
        ids(&narrowed.matches),
        vec!["dg-1991", "dg-undated-a", "dg-undated-b"]
    );
    let set_aside = rows(
        &narrowed.matches,
        &narrowed.provenance,
        &narrowed.pressings,
        &text,
    );
    assert_eq!(
        set_aside.len(),
        3,
        "the rows the run built, read back: {:?}",
        set_aside
            .iter()
            .map(|(pressing, _)| ids(&pressing.releases))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        crate::import::release_group::form_rows(&narrowed.matches),
        vec![0, 0, 0],
        "forming rows over the set-aside list alone rolls all three into \
         one, because what told them apart is in the other list"
    );
    for (pressing, agreements) in &set_aside {
        assert_eq!(
            badges(agreements),
            vec!["Barcode", "Catalog", "Label", "Country"],
            "{:?}",
            ids(&pressing.releases)
        );
    }
}

/// A row is offered whole or set aside whole, and each list records the
/// rows the run built — which is what the sweep's settle step and the
/// queue's pressing count both read.
#[test]
fn a_pressing_never_splits_across_the_two_lists() {
    let text = folder(&["1979 - Van Halen II (Warner Bros., 20P2-2031, Japan)"]);
    let outcome = combine_results(
        vec![van_halen_musicbrainz()],
        vec![
            van_halen_musicbrainz(),
            van_halen_discogs("dg-1988", Some(1988)),
            van_halen_discogs("dg-1991", Some(1991)),
        ],
        vec![],
        &text,
    );
    let (_, _, pressings) = found(outcome.clone());
    assert_eq!(crate::import::release_group::row_count(&pressings), 1);
    assert_eq!(
        crate::import::release_group::row_count(&narrowed(outcome).pressings),
        1
    );
}

/// The sole-match rule and the Ready classification both ask how many
/// pressings the matches make, and both read it off the rows this run
/// recorded — so a lone pressing the folder describes settles even though
/// a barcode came back naming somebody else.
#[test]
fn a_lone_pressing_the_folder_describes_is_the_sole_match() {
    let text = folder(&["AC-DC - Dirty Deeds Done Dirt Cheap [16033-2]"]);
    let outcome = combine_results(
        vec![dirty_deeds("rel-1976", 1976)],
        vec![manu_chao()],
        vec![],
        &text,
    );
    let (_, _, pressings) = found(outcome);
    assert_eq!(crate::import::release_group::row_count(&pressings), 1);
}
