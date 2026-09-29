use super::*;

// Most of these are about how the sets intersect, with a candidate that states
// nothing, so the order is the one the signals gave.

/// What combine hands back: the findings, and each release's library status.
type Outcome = (Findings, LibraryStatuses);

fn combine(discid: Results, barcode: Results, catalog: Results) -> Outcome {
    combine_results(
        LookupAnswers {
            disc_id: discid,
            barcode,
            catalog,
            ..LookupAnswers::default()
        },
        Vec::new(),
        &CandidateText::default(),
        FolderAudio::UNPROVEN,
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

/// Signals that share no result each keep their answer, each row naming the
/// signal that produced it.
#[test]
fn lookups_that_named_different_releases_each_keep_their_answer() {
    let discid = vec![pair("rel-a", Some("group-1"))];
    let barcode = vec![pair("rel-b", Some("group-2"))];
    let (mut numbered, status) = pair("rel-c", Some("group-3"));
    numbered.labels = vec![crate::pressing::ReleaseLabel::of(None, Some("L3-100"))];
    let catalog = vec![(numbered, status)];
    let outcome = combine_results(
        LookupAnswers {
            disc_id: discid,
            barcode,
            catalog,
            ..LookupAnswers::default()
        },
        Vec::new(),
        &folder(&["Album [L3-100]"]),
        FolderAudio::UNPROVEN,
    );
    let (matches, provenance, _) = found(outcome.clone());
    // The folder states the chosen number, so its release is offered; the disc
    // ID's and the barcode's answers have nothing behind them and are set aside.
    assert_eq!(ids(&matches), vec!["rel-c"]);
    assert!(provenance[0].by_catalog && !provenance[0].by_disc_id);
    let left_out = narrowed(outcome);
    assert_eq!(ids(&left_out.matches), vec!["rel-a", "rel-b"]);
    assert!(left_out.provenance[0].by_disc_id);
    assert!(left_out.provenance[1].by_barcode);
}

/// The union names each release once even when two signals both saw it.
#[test]
fn the_union_names_each_release_once() {
    let discid = vec![pair("rel-a", None)];
    let barcode = vec![pair("rel-a", None)];
    let catalog = vec![pair("rel-b", None)];
    let outcome = combine(discid, barcode, catalog);
    let (matches, provenance, _) = found(outcome.clone());
    // Two lookups returned rel-a and one rel-b, so rel-a is offered.
    assert_eq!(ids(&matches), vec!["rel-a"]);
    assert!(provenance[0].by_disc_id && provenance[0].by_barcode);
    assert_eq!(ids(&narrowed(outcome).matches), vec!["rel-b"]);
}

/// A checked signal that found nothing takes no part.
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
    // Two releases, never folded into one by their shared id.
    assert_eq!(matches.len() + narrowed(outcome).matches.len(), 2);
}

/// What the intersection left out comes back beside the matches, in signal
/// order, each once, saying which signal named it.
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

/// One signal answering alone narrows nothing.
#[test]
fn a_lone_signal_narrows_nothing() {
    let alone = combine(
        vec![pair("rel-a", None), pair("rel-b", None)],
        vec![],
        vec![],
    );
    assert!(narrowed(alone).is_empty());
}

/// A result with no group id still stands, as its own card.
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
        })
        .collect();
    CandidateText::of(&pool, &[], &[])
}

/// A pressing of Album One whose catalog number, label and country the
/// folder states.
fn pressing_of_album_one(release_id: &str, year: i32) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Album One".to_string(),
            artist: Some("Artist One".to_string()),
            labels: vec![crate::pressing::ReleaseLabel::of(
                Some("Label One"),
                Some("L1-16033"),
            )],
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
            labels: vec![crate::pressing::ReleaseLabel::of(
                Some("Label Three"),
                Some("L3-44633"),
            )],
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

/// The pressing the folder's text describes leads the disc ID's others.
#[test]
fn the_pressing_the_folder_describes_leads_the_disc_id_s_others() {
    let text = folder(&["Artist One - Album One [L1-16033]", "Label One 1976 US"]);
    let discid = vec![
        pressing_of_album_one("rel-1994", 1994),
        pressing_of_album_one("rel-2003", 2003),
        pressing_of_album_one("rel-1976", 1976),
    ];
    let outcome = combine_results(
        LookupAnswers {
            disc_id: discid,
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (matches, provenance, _) = found(outcome);
    assert_eq!(ids(&matches), vec!["rel-1976", "rel-1994", "rel-2003"]);
    assert!(provenance.iter().all(|lookup| lookup.by_disc_id));
}

/// A barcode naming a record the folder never mentions is set aside.
#[test]
fn a_barcode_naming_a_record_the_folder_never_mentions_folds() {
    let text = folder(&["Artist One - Album One [L1-16033]"]);
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![pressing_of_album_one("rel-1976", 1976)],
            barcode: vec![unrelated_record()],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-1976"]);
    let left_out = narrowed(outcome).matches;
    assert_eq!(ids(&left_out), vec!["rel-album-three"]);
}

/// Two pressings both identifiers name, told apart only by catalog number:
/// the one the folder states is the answer and the other is set aside.
#[test]
fn the_pressing_whose_catalog_number_the_folder_states_folds_the_other() {
    let text = folder(&["1972 - Album One (Label One, L1-16033, Germany)"]);
    let stated = pressing_of_album_one("rel-stated", 1989);
    let mut other = pressing_of_album_one("rel-other", 1990);
    other.0.labels = vec![crate::pressing::ReleaseLabel::of(None, Some("L1-99999"))];
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![stated.clone(), other.clone()],
            barcode: vec![stated, other],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-stated"]);
    assert_eq!(ids(&narrowed(outcome).matches), vec!["rel-other"]);
}

/// Two pressings the folder states neither number of stay side by side.
#[test]
fn two_pressings_the_folder_names_no_number_of_both_stay() {
    let text = folder(&["1972 - Album One (Label One, Germany)"]);
    let first = pressing_of_album_one("rel-first", 1989);
    let mut second = pressing_of_album_one("rel-second", 1990);
    second.0.labels = vec![crate::pressing::ReleaseLabel::of(None, Some("L1-99999"))];
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![first.clone(), second.clone()],
            barcode: vec![first, second],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-first", "rel-second"]);
    assert!(narrowed(outcome).is_empty());
}

/// Pressings differing only by year all stay, the stated year first: a year
/// names an album, not a pressing.
#[test]
fn pressings_that_differ_only_by_year_all_stay_on_the_list() {
    let text = folder(&["Artist One - Album One [L1-16033]", "Label One 1976 US"]);
    let discid = vec![
        pressing_of_album_one("rel-1994", 1994),
        pressing_of_album_one("rel-2003", 2003),
        pressing_of_album_one("rel-1976", 1976),
    ];
    let outcome = combine_results(
        LookupAnswers {
            disc_id: discid,
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-1976", "rel-1994", "rel-2003"]);
    assert!(narrowed(outcome).is_empty());
}

/// A disc ID's pressing stays however the folder spells its label: the disc
/// ID comes from the audio.
#[test]
fn a_disc_id_s_pressing_stays_however_the_folder_spells_its_label() {
    let text = folder(&["Artist One - Album One (Label One)"]);
    let matched = pressing_of_album_one("rel-matched", 1976);
    let mut reissue = pressing_of_album_one("rel-reissue", 1994);
    reissue.0.labels = vec![crate::pressing::ReleaseLabel::of(Some("Label Four"), None)];
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![matched, reissue],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-matched", "rel-reissue"]);
    assert!(narrowed(outcome).is_empty());
}

/// Folding never empties the list: a barcode answering alone is offered.
#[test]
fn a_barcode_answering_alone_is_offered_however_little_the_folder_says() {
    let text = folder(&["CD1"]);
    let (matches, _, _) = found(combine_results(
        LookupAnswers {
            barcode: vec![unrelated_record()],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    ));
    assert_eq!(ids(&matches), vec!["rel-album-three"]);
}

/// A candidate with no text sets nothing aside for it.
#[test]
fn a_candidate_with_no_text_narrows_nothing_on_it() {
    let outcome = combine_results(
        LookupAnswers {
            barcode: vec![pressing_of_album_one("rel-1976", 1976), unrelated_record()],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &CandidateText::default(),
        FolderAudio::UNPROVEN,
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
        LookupAnswers {
            disc_id: discid,
            barcode,
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (matches, _, _) = found(outcome.clone());
    assert_eq!(ids(&matches), vec!["rel-1976"]);
    let left_out = narrowed(outcome).matches;
    let mut left_out = ids(&left_out);
    left_out.sort_unstable();
    assert_eq!(left_out, vec!["rel-1994", "rel-album-three"]);
}

// MARK: - The pressing is what is offered or set aside

/// The rows a surface draws from a list of matches, with their badges, read
/// off the rows the run recorded.
fn rows(
    matches: &[MetadataResult],
    provenance: &[LookupProvenance],
    pressings: &[u32],
    text: &CandidateText,
) -> Vec<(Pressing, crate::identify::agreements::Agreements)> {
    let facts = FolderFacts::of(text, matches);
    let judged: Vec<Judged> = matches
        .iter()
        .cloned()
        .zip(provenance.iter().cloned())
        .map(|(result, lookup)| {
            let agreements = agreements_of(&result, text, &facts, &lookup);
            (result, agreements)
        })
        .collect();
    let judgements = Judgements::of(&judged, None);
    crate::import::release_group::group_formed_rows(judged, pressings, Vec::new(), &[], None)
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

/// MusicBrainz's record of the Japanese pressing the disc ID names.
fn album_two_musicbrainz() -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Album Two".to_string(),
            artist: Some("Artist Two".to_string()),
            labels: vec![crate::pressing::ReleaseLabel::of(
                Some("Label Two"),
                Some("L2-2031"),
            )],
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

/// One of the Discogs records of that catalog number the barcode returned.
fn album_two_discogs(release_id: &str, year: Option<i32>) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            source: Catalog::Discogs,
            title: "Album Two".to_string(),
            artist: Some("Artist Two".to_string()),
            labels: vec![crate::pressing::ReleaseLabel::of(
                Some("Label Two"),
                Some("L2-2031"),
            )],
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

/// The Discogs record of the pressing the disc ID names is offered on its
/// row; the other Discogs records of the barcode are set aside, one row each.
#[test]
fn the_discogs_record_of_the_pressing_the_disc_id_named_is_offered_with_it() {
    let text = folder(&["1979 - Album Two (Label Two, L2-2031, Japan)"]);
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![album_two_musicbrainz()],
            barcode: vec![
                album_two_musicbrainz(),
                album_two_discogs("dg-1991", Some(1991)),
                album_two_discogs("dg-1988", Some(1988)),
                album_two_discogs("dg-undated-a", None),
                album_two_discogs("dg-undated-b", None),
            ],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
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

/// Two catalogs' records of one pressing can list different tracklists. The
/// record whose tracklist fits the folder is the one the row's draft is read
/// from, however much more of the folder's text the other agrees with — so
/// the row the ranking offers as fitting is the row the verdict picks.
#[test]
fn the_record_whose_tracklist_fits_the_folder_leads_its_pressing() {
    let text = folder(&["1979 - Album Two (Label Two, L2-2031, Japan)"]);
    let listing = |(mut result, status): (MetadataResult, LibraryStatus), count: u32| {
        result.source_tracks = Some(crate::import::search::SourceTracks::Listed { count });
        (result, status)
    };
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![listing(album_two_musicbrainz(), 12)],
            barcode: vec![
                listing(album_two_musicbrainz(), 12),
                listing(album_two_discogs("dg-1988", Some(1988)), 10),
            ],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio {
            track_count: 10,
            ..FolderAudio::UNPROVEN
        },
    );
    let (findings, _) = outcome;
    assert_eq!(ids(&findings.matches), vec!["dg-1988", "mb-album-two"]);
    let verdict = crate::identify::TerminalVerdict::Found {
        findings,
        track_count: 10,
        ledger: None,
    };
    assert_eq!(
        crate::identify::VerdictSummary::of(&verdict).judgement(),
        (true, None)
    );
}

/// A row is offered whole or set aside whole.
#[test]
fn a_pressing_never_splits_across_the_two_lists() {
    let text = folder(&["1979 - Album Two (Label Two, L2-2031, Japan)"]);
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![album_two_musicbrainz()],
            barcode: vec![
                album_two_musicbrainz(),
                album_two_discogs("dg-1988", Some(1988)),
                album_two_discogs("dg-1991", Some(1991)),
            ],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (_, _, pressings) = found(outcome.clone());
    assert_eq!(crate::import::release_group::row_count(&pressings), 1);
    assert_eq!(
        crate::import::release_group::row_count(&narrowed(outcome).pressings),
        1
    );
}

/// A lone pressing the folder describes is one row, even beside a barcode
/// naming somebody else.
#[test]
fn a_lone_pressing_the_folder_describes_is_the_sole_match() {
    let text = folder(&["Artist One - Album One [L1-16033]"]);
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![pressing_of_album_one("rel-1976", 1976)],
            barcode: vec![unrelated_record()],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &text,
        FolderAudio::UNPROVEN,
    );
    let (_, _, pressings) = found(outcome);
    assert_eq!(crate::import::release_group::row_count(&pressings), 1);
}

/// The title search, asked only when the identifiers found nothing, is
/// offered whole.
#[test]
fn a_search_that_answered_alone_is_offered_whole() {
    let outcome = combine_results(
        LookupAnswers {
            search: vec![pair("rel-a", Some("g-x")), pair("rel-b", Some("g-y"))],
            ..LookupAnswers::default()
        },
        Vec::new(),
        &CandidateText::default(),
        FolderAudio::UNPROVEN,
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

/// A twin sits on the row of the answer that named it and counts for no
/// lookup; one whose namer is not among the answers is left out.
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
        LookupAnswers {
            disc_id: vec![named],
            barcode: vec![pair_src(Catalog::Discogs, "dg-barcode", Some("master-1"))],
            ..LookupAnswers::default()
        },
        vec![twin("dg-twin", "mb-1"), twin("dg-orphan", "mb-gone")],
        &CandidateText::default(),
        FolderAudio::UNPROVEN,
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
    // The disc ID's row outranks the barcode's as it would with no twin.
    assert_eq!(ids(&narrowed(outcome).matches), vec!["dg-barcode"]);
}
