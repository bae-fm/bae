use super::*;

/// Most of these are about how the sets intersect, which the candidate's
/// own text takes no part in: a candidate that states nothing offers every
/// answer, so nothing folds and the order is the one the signals gave.
///
/// The title search is not one of the sets here: it is asked only when the
/// three identifiers came back empty, so every case about how they intersect
/// is a case where it never ran.
/// What combine hands back: the findings, and each release's library status.
type Outcome = (Findings, LibraryStatuses);

fn combine(discid: Results, barcode: Results, catalog: Results) -> Outcome {
    combine_results(
        discid,
        barcode,
        catalog,
        Results::new(),
        Vec::new(),
        &CandidateText::default(),
        &RipEvidence::Unproven,
    )
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

fn narrowed((findings, _): Outcome) -> NarrowedOut {
    assert!(!findings.is_empty(), "expected findings, got none");
    findings.narrowed_out
}

fn found((findings, _): Outcome) -> (Vec<MetadataResult>, Vec<LookupProvenance>, Vec<u32>) {
    assert!(!findings.is_empty(), "expected findings, got none");
    (findings.matches, findings.provenance, findings.pressings)
}

#[test]
fn nothing_checked_or_nothing_found_yields_not_found_anywhere() {
    let (findings, statuses) = combine(vec![], vec![], vec![]);
    assert_eq!(findings, Findings::default());
    assert_eq!(statuses, LibraryStatuses::default());
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
/// real release, so every answer is kept, and each row says which signal
/// produced it.
#[test]
fn lookups_that_named_different_releases_each_keep_their_answer() {
    let discid = vec![pair("rel-a", Some("group-1"))];
    let barcode = vec![pair("rel-b", Some("group-2"))];
    let catalog = vec![pair("rel-c", Some("group-3"))];
    let outcome = combine(discid, barcode, catalog);
    let (matches, provenance, _) = found(outcome.clone());
    // A chosen catalog number was typed off the disc and names one pressing,
    // so its answer is offered. The disc ID names every pressing sharing its
    // table of contents, and a barcode is read off a photograph with nothing
    // else here standing behind its answer, so both are set aside.
    assert_eq!(ids(&matches), vec!["rel-c"]);
    assert!(provenance[0].by_catalog && !provenance[0].by_disc_id);
    let left_out = narrowed(outcome);
    assert_eq!(ids(&left_out.matches), vec!["rel-a", "rel-b"]);
    assert!(left_out.provenance[0].by_disc_id);
    assert!(left_out.provenance[1].by_barcode);
}

/// The union names each release once even when two signals both saw it —
/// which happens when a third signal is what emptied the intersection.
#[test]
fn the_union_names_each_release_once() {
    let discid = vec![pair("rel-a", None)];
    let barcode = vec![pair("rel-a", None)];
    let catalog = vec![pair("rel-b", None)];
    let outcome = combine(discid, barcode, catalog);
    let (matches, provenance, _) = found(outcome.clone());
    // Two lookups returned rel-a, one returned rel-b, so rel-a is offered and
    // rel-b waits under the disclosure. Each is named once, on one list.
    assert_eq!(ids(&matches), vec!["rel-a"]);
    assert!(provenance[0].by_disc_id && provenance[0].by_barcode);
    assert_eq!(ids(&narrowed(outcome).matches), vec!["rel-b"]);
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
    let outcome = combine(discid, barcode, vec![]);
    let (matches, _, _) = found(outcome.clone());
    // Two releases, never folded into one by their shared id. They are ranked
    // against each other like any other pair, so they land on the two lists.
    assert_eq!(matches.len() + narrowed(outcome).matches.len(), 2);
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
    assert_eq!(outcome.1.narrowed_out.len(), 3);

    let narrowed = narrowed(outcome);
    assert_eq!(ids(&narrowed.matches), vec!["rel-a", "rel-b", "rel-c"]);
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
fn a_lone_signal_narrows_nothing() {
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
            origin: crate::signals::TextOrigin::FolderName,
            file: None,
            region: None,
        })
        .collect();
    CandidateText::of(&pool, &[])
}

/// One pressing of an album as MusicBrainz states it, with the folder's
/// catalog number, label, year and country all matching it.
fn pressing_of_album_one(release_id: &str, year: i32) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Album One".to_string(),
            artist: Some("Artist One".to_string()),
            label: Some("Label One".to_string()),
            catalog_number: Some("L1-16033".to_string()),
            area: Some(crate::pressing::area("US")),
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
            year: Some(year),
            source_group_id: Some("rg-album-one".to_string()),
            ..mk_result(release_id, Some("rg-album-one"))
        },
        LibraryStatus::absent(release_id),
    )
}

/// Somebody else's record, which a misread barcode came back naming.
fn unrelated_record() -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Album Three".to_string(),
            artist: Some("Artist Three".to_string()),
            label: Some("Label Three".to_string()),
            catalog_number: Some("L3-44633".to_string()),
            area: Some(crate::pressing::area("FR")),
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
            year: Some(1998),
            source_group_id: Some("rg-album-three".to_string()),
            ..mk_result("rel-album-three", Some("rg-album-three"))
        },
        LibraryStatus::absent("rel-album-three"),
    )
}

/// The folder's text is what orders the rows: the pressing it names the
/// catalog number, label, year and country of leads, and the pressings the
/// disc ID alone named follow.
#[test]
fn the_pressing_the_folder_describes_leads_the_disc_id_s_others() {
    let text = folder(&["Artist One - Album One [L1-16033]", "Label One 1976 US"]);
    let discid = vec![
        pressing_of_album_one("rel-1994", 1994),
        pressing_of_album_one("rel-2003", 2003),
        pressing_of_album_one("rel-1976", 1976),
    ];
    let outcome = combine_results(
        discid,
        vec![],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (matches, provenance, _) = found(outcome);
    assert_eq!(ids(&matches), vec!["rel-1976", "rel-1994", "rel-2003"]);
    assert!(provenance.iter().all(|lookup| lookup.by_disc_id));
}

/// A barcode that came back naming somebody else's record read the wrong
/// digits: the folder says nothing about it, and it is offered under the
/// rest rather than beside them.
#[test]
fn a_barcode_naming_a_record_the_folder_never_mentions_folds() {
    let text = folder(&["Artist One - Album One [L1-16033]"]);
    let outcome = combine_results(
        vec![pressing_of_album_one("rel-1976", 1976)],
        vec![unrelated_record()],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-1976"]);
    let left_out = narrowed(outcome).matches;
    assert_eq!(ids(&left_out), vec!["rel-album-three"]);
}

/// Two pressings of one album that both identifiers name together: one disc
/// layout, one barcode, one label, and a different catalog number printed on
/// each. Nothing the lookups did tells them apart.
///
/// The folder states one of the two numbers, and a catalog number names a
/// single pressing, so that row is the answer and the other one waits under
/// the disclosure. This is the whole point of reading the number: without it
/// the person is asked to choose between two rows that no identifier
/// separates.
#[test]
fn the_pressing_whose_catalog_number_the_folder_states_folds_the_other() {
    let text = folder(&["1972 - Album One (Label One, L1-16033, Germany)"]);
    let stated = pressing_of_album_one("rel-stated", 1989);
    let mut other = pressing_of_album_one("rel-other", 1990);
    other.0.catalog_number = Some("L1-99999".to_string());
    let outcome = combine_results(
        vec![stated.clone(), other.clone()],
        vec![stated, other],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-stated"]);
    assert_eq!(ids(&narrowed(outcome).matches), vec!["rel-other"]);
}

/// Two pressings the folder states neither number of stay side by side: the
/// number folds a row only when it stands behind one of them.
#[test]
fn two_pressings_the_folder_names_no_number_of_both_stay() {
    let text = folder(&["1972 - Album One (Label One, Germany)"]);
    let first = pressing_of_album_one("rel-first", 1989);
    let mut second = pressing_of_album_one("rel-second", 1990);
    second.0.catalog_number = Some("L1-99999".to_string());
    let outcome = combine_results(
        vec![first.clone(), second.clone()],
        vec![first, second],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-first", "rel-second"]);
    assert!(narrowed(outcome).is_empty());
}

/// Three pressings of one album the disc ID named together, differing only
/// by the year each was pressed. The folder states one of the three years,
/// and a year names an album rather than a pressing, so all three stay on
/// the list with the stated one first.
///
/// This is why the text is read as one value: counting matched fields would
/// leave the other two pressings behind the disclosure, and a folder named
/// after the album's year would fold away the pressing on the desk.
#[test]
fn pressings_that_differ_only_by_year_all_stay_on_the_list() {
    let text = folder(&["Artist One - Album One [L1-16033]", "Label One 1976 US"]);
    let discid = vec![
        pressing_of_album_one("rel-1994", 1994),
        pressing_of_album_one("rel-2003", 2003),
        pressing_of_album_one("rel-1976", 1976),
    ];
    let outcome = combine_results(
        discid,
        vec![],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-1976", "rel-1994", "rel-2003"]);
    assert!(narrowed(outcome).is_empty());
}

/// A pressing the disc ID named whose label the folder spells differently
/// stays on the list beside the ones it matches: the disc ID is computed
/// from the audio, so nothing the text says folds it away.
#[test]
fn a_disc_id_s_pressing_stays_however_the_folder_spells_its_label() {
    let text = folder(&["Artist One - Album One (Label One)"]);
    let matched = pressing_of_album_one("rel-matched", 1976);
    let mut reissue = pressing_of_album_one("rel-reissue", 1994);
    reissue.0.label = Some("Label Four".to_string());
    reissue.0.catalog_number = None;
    let outcome = combine_results(
        vec![matched, reissue],
        vec![],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-matched", "rel-reissue"]);
    assert!(narrowed(outcome).is_empty());
}

/// Folding shortens the list; it never empties it. A barcode answering on
/// its own is the whole of what there is to offer, whatever the folder
/// says.
#[test]
fn a_barcode_answering_alone_is_offered_however_little_the_folder_says() {
    let text = folder(&["CD1"]);
    let (matches, _, _) = found(combine_results(
        vec![],
        vec![unrelated_record()],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    ));
    assert_eq!(ids(&matches), vec!["rel-album-three"]);
}

/// A candidate carrying no text at all was never asked about its answers,
/// so nothing it found is set aside for the text saying nothing.
#[test]
fn a_candidate_with_no_text_narrows_nothing_on_it() {
    // Both releases came back from the one lookup, so only the folder's text
    // could tell them apart, and there is none to read.
    let outcome = combine_results(
        vec![],
        vec![pressing_of_album_one("rel-1976", 1976), unrelated_record()],
        vec![],
        vec![],
        Vec::new(),
        &CandidateText::default(),
        &RipEvidence::Unproven,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(matches.len(), 2);
    assert!(narrowed(outcome).is_empty());
}

/// What the intersection left out and what the folder says nothing about
/// are one list.
#[test]
fn the_intersection_s_leftovers_and_the_folder_s_are_one_list() {
    let text = folder(&["Artist One - Album One [L1-16033]"]);
    let discid = vec![
        pressing_of_album_one("rel-1976", 1976),
        pressing_of_album_one("rel-1994", 1994),
    ];
    let barcode = vec![pressing_of_album_one("rel-1976", 1976), unrelated_record()];
    let outcome = combine_results(
        discid,
        barcode,
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-1976"]);
    let left_out = narrowed(outcome).matches;
    let mut left_out = ids(&left_out);
    left_out.sort_unstable();
    assert_eq!(left_out, vec!["rel-1994", "rel-album-three"]);
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
    crate::import::release_group::group_formed_rows(judged, pressings, Vec::new(), &[])
        .into_iter()
        .flat_map(crate::import::release_group::ReleaseGroup::into_pressings)
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

/// The Japanese pressing of an album as MusicBrainz has it, which is the
/// one the disc ID names: the folder's catalog number and label, its country
/// as a code, and the barcode the sleeve prints.
fn album_two_musicbrainz() -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Album Two".to_string(),
            artist: Some("Artist Two".to_string()),
            label: Some("Label Two".to_string()),
            catalog_number: Some("L2-2031".to_string()),
            area: Some(crate::pressing::area("JP")),
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
            barcodes: vec!["4988014720311".to_string()],
            media: crate::pressing::StatedMedia::Undescribed,
            links: Vec::new(),
            year: Some(1988),
            source_group_id: Some("rg-album-two".to_string()),
            ..mk_result("mb-album-two", Some("rg-album-two"))
        },
        LibraryStatus::absent("mb-album-two"),
    )
}

/// One of the four Discogs records of that same catalog number, all of
/// which the barcode lookup returned.
fn album_two_discogs(release_id: &str, year: Option<i32>) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            source: Catalog::Discogs,
            title: "Album Two".to_string(),
            artist: Some("Artist Two".to_string()),
            label: Some("Label Two".to_string()),
            catalog_number: Some("L2-2031".to_string()),
            area: crate::pressing::ReleaseArea::discogs("Japan"),
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
            barcodes: vec!["4988014720311".to_string()],
            media: crate::pressing::StatedMedia::Undescribed,
            links: Vec::new(),
            year,
            source_group_id: Some("master-album-two".to_string()),
            ..mk_result(release_id, Some("master-album-two"))
        },
        LibraryStatus::absent(release_id),
    )
}

/// The disc ID answers on MusicBrainz alone, so the Discogs record of the
/// pressing it names can only ever be the barcode's answer — never the
/// intersection's. Pairing before the narrowing is what keeps the two
/// together: the row the folder describes is offered carrying both
/// sources, and the other Discogs records that print the same barcode go
/// under the disclosure, one row each.
#[test]
fn the_discogs_record_of_the_pressing_the_disc_id_named_is_offered_with_it() {
    let text = folder(&["1979 - Album Two (Label Two, L2-2031, Japan)"]);
    let outcome = combine_results(
        vec![album_two_musicbrainz()],
        vec![
            album_two_musicbrainz(),
            album_two_discogs("dg-1991", Some(1991)),
            album_two_discogs("dg-1988", Some(1988)),
            album_two_discogs("dg-undated-a", None),
            album_two_discogs("dg-undated-b", None),
        ],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (matches, provenance, pressings) = found(outcome.clone());
    let offered = rows(&matches, &provenance, &pressings, &text);
    assert_eq!(offered.len(), 1, "one row, not two: {offered:?}");
    assert_eq!(
        ids(&offered[0].0.releases),
        vec!["mb-album-two", "dg-1988"],
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
                "mb-album-two".to_string()
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
        vec![0, 1, 2],
        "three records of one catalog stay three rows"
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
    let text = folder(&["1979 - Album Two (Label Two, L2-2031, Japan)"]);
    let outcome = combine_results(
        vec![album_two_musicbrainz()],
        vec![
            album_two_musicbrainz(),
            album_two_discogs("dg-1988", Some(1988)),
            album_two_discogs("dg-1991", Some(1991)),
        ],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
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
    let text = folder(&["Artist One - Album One [L1-16033]"]);
    let outcome = combine_results(
        vec![pressing_of_album_one("rel-1976", 1976)],
        vec![unrelated_record()],
        vec![],
        vec![],
        Vec::new(),
        &text,
        &RipEvidence::Unproven,
    );
    let (_, _, pressings) = found(outcome);
    assert_eq!(crate::import::release_group::row_count(&pressings), 1);
}

/// The title search is the only set that ever answers alone, because it is
/// asked only once the three identifiers have come back empty. What it found
/// is offered whole, each row carrying the search as what produced it.
#[test]
fn a_search_that_answered_alone_is_offered_whole() {
    let outcome = combine_results(
        vec![],
        vec![],
        vec![],
        vec![pair("rel-a", Some("g-x")), pair("rel-b", Some("g-y"))],
        Vec::new(),
        &CandidateText::default(),
        &RipEvidence::Unproven,
    );
    let (matches, provenance, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-a", "rel-b"]);
    assert!(provenance.iter().all(|lookup| lookup.by_search));
    assert!(provenance
        .iter()
        .all(|lookup| !lookup.by_disc_id && !lookup.by_barcode && !lookup.by_catalog));
    assert!(
        narrowed(outcome).is_empty(),
        "one set alone narrows nothing"
    );
}

/// A twin read through a disc ID answer's link sits on that answer's row and
/// counts for no lookup: the row ranks as the disc ID answer would alone, and
/// the twin's provenance names the release that named it. A twin whose
/// namer is not among the answers has nothing to sit beside and is left out.
#[test]
fn a_twin_counts_for_no_lookup_and_sits_on_its_namer_s_row() {
    let mut named = pair("mb-1", Some("group-1"));
    named.0.links = vec![crate::import::MetadataRef::new(Catalog::Discogs, "dg-twin")];
    let twin = |id: &str, named_by: &str| Twin {
        result: pair_src(Catalog::Discogs, id, Some("master-1")).0,
        named_by: crate::import::MetadataRef::new(Catalog::MusicBrainz, named_by),
        status: LibraryStatus::absent(id),
    };
    let outcome = combine_results(
        vec![named],
        vec![pair_src(Catalog::Discogs, "dg-barcode", Some("master-1"))],
        vec![],
        vec![],
        vec![twin("dg-twin", "mb-1"), twin("dg-orphan", "mb-gone")],
        &CandidateText::default(),
        &RipEvidence::Unproven,
    );
    let (matches, provenance, pressings) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["mb-1", "dg-twin"]);
    assert_eq!(pressings, vec![0, 0], "the twin shares its namer's row");
    assert!(provenance[0].by_disc_id && provenance[0].named_by.is_none());
    assert_eq!(
        provenance[1],
        LookupProvenance {
            named_by: Some(crate::import::MetadataRef::new(
                Catalog::MusicBrainz,
                "mb-1"
            )),
            ..LookupProvenance::CHOSEN
        }
    );
    // The disc ID's row outranks the barcode's on the disc ID alone, as it
    // would with no twin beside it.
    assert_eq!(ids(&narrowed(outcome).matches), vec!["dg-barcode"]);
}
