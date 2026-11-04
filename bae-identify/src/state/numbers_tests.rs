//! Which catalog numbers a run searches, under which artists, and when it
//! stops.

use super::*;
use crate::documents::ReleaseDocument;
use crate::signals::DiscIdSignal;
use crate::state::tests::step;
use crate::state::tests::*;
use crate::state::*;
use bae_catalogs::records::LookupRef;

/// The search of `catalog` on `source` a round asks for a release by
/// "Artist Name", the artist [`labelled`] credits: as it stands on
/// MusicBrainz, under the artist on Discogs.
fn catalog_lookup(source: LookupCatalog, catalog: &str) -> Effect {
    let artist = (source == DG).then_some("Artist Name");
    catalog_lookup_under(source, catalog, artist)
}

fn catalog_lookup_under(source: LookupCatalog, catalog: &str, artist: Option<&str>) -> Effect {
    Effect::LookupCatalog {
        source,
        catalog: catalog.to_string(),
        artist: artist.map(str::to_string),
    }
}

/// The disc ID names MusicBrainz releases alone, and the folder prints the
/// number one of them carries: that number comes into effect and is asked
/// of MusicBrainz as it stands and of Discogs under the artist the release
/// credits, and the Discogs release it finds is pooled with the rest.
#[test]
fn a_number_the_text_prints_that_a_found_release_carries_is_searched_on_both_catalogs() {
    let state = disc_run_of_folder("1986 Germany Label AB 12345-2", &["AB 12345-2"]);
    let (state, effects) = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                labelled(MB, "mb-1", "Label", "AB 12345-2"),
                pair("mb-2", Some("g")),
            ])),
        },
    );
    assert!(
        effects.contains(&catalog_lookup(MB, "AB 12345-2"))
            && effects.contains(&catalog_lookup(DG, "AB 12345-2")),
        "the number is searched on both catalogs: {effects:?}"
    );
    let IdentifyState::Triangulating { context, .. } = &state else {
        panic!("the run goes round: {state:?}");
    };
    assert_eq!(context.catalog.confirmed, vec!["AB 12345-2".to_string()]);
    assert!(context.catalog.chosen.is_empty(), "nothing was picked");

    let (state, _) = crate::state::step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "AB 12345-2".to_string(),
            artist: None,
            outcome: Ok(vec![labelled(MB, "mb-1", "Label", "AB 12345-2")]),
        },
    );
    let reading = crate::state::step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: DG,
            for_catalog: "AB 12345-2".to_string(),
            artist: Some("Artist Name".to_string()),
            outcome: Ok(vec![labelled(DG, "dg-1", "Label", "AB 12345-2")]),
        },
    );
    let (state, _, _) = answer_rounds(
        (reading.0, effects.into_iter().chain(reading.1).collect()),
        |release| ReleaseDocument {
            labels: vec![bae_types::pressing::ReleaseLabel::of(
                Some("Label"),
                Some(if release.key == "mb-2" { "ZZ-1" } else { "AB 12345-2" }),
            )],
            ..plain_document(&[])
        },
        |_, _| Vec::new(),
    );
    let rows = rows_of(&state);
    let discogs = rows
        .iter()
        .find(|(result, _, _)| result.release_id == "dg-1")
        .unwrap_or_else(|| panic!("the Discogs answer is pooled: {rows:?}"));
    assert!(!discogs.1, "no disc ID's lookup found the Discogs answer");
    let IdentifyState::Found { ledger, .. } = &state else {
        panic!("expected Found, got {state:?}");
    };
    let Some(crate::view::CatalogStepView::Numbers { rows, .. }) =
        ledger.as_ref().map(|ledger| &ledger.catalog)
    else {
        panic!("the ledger lays the numbers out: {ledger:?}");
    };
    assert_eq!(
        rows.iter().map(|row| row.value.as_str()).collect::<Vec<_>>(),
        vec!["AB 12345-2"],
        "the number in effect is a row with its search"
    );
}

/// The folder's name states one number a found release carries, and only
/// the artwork prints another — a booklet's page or another record's number
/// that some found release happens to carry. A number comes into effect as
/// the folder states it, by its highest-standing text stating any listed
/// number: the name's number is searched, the artwork's is not.
#[test]
fn a_number_only_the_artwork_prints_is_not_confirmed_over_the_folder_names() {
    let state = disc_run_of_text(
        &[
            ("Label AB-100", crate::signals::TextOrigin::FolderName),
            ("XY-250", crate::signals::TextOrigin::Artwork),
        ],
        &["AB-100", "XY-250"],
    );
    let (state, effects) = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                labelled(MB, "mb-1", "Label", "AB-100"),
                labelled(MB, "mb-2", "Other", "XY-250"),
            ])),
        },
    );
    let IdentifyState::Triangulating { context, .. } = &state else {
        panic!("the run goes round: {state:?}");
    };
    assert_eq!(context.catalog.confirmed, vec!["AB-100".to_string()]);
    assert!(
        effects.contains(&catalog_lookup(MB, "AB-100"))
            && effects.contains(&catalog_lookup(DG, "AB-100")),
        "the folder name's number is searched: {effects:?}"
    );
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            Effect::LookupCatalog { catalog, .. } if catalog == "XY-250"
        )),
        "the artwork's number is not searched: {effects:?}"
    );
}

/// A picked number the person struck out is not in effect: nothing searches
/// it.
#[test]
fn a_struck_out_picked_number_is_not_searched() {
    let (_, effects) = started_with_choices(
        vec![MB, DG],
        LookupChoices {
            discounted_catalogs: vec!["AB-1".to_string()],
            ..choosing(&["AB-1", "CD-2"])
        },
    );
    assert_eq!(
        effects,
        vec![catalog_lookup_under(MB, "CD-2", None), catalog_lookup_under(DG, "CD-2", None)]
    );
}

/// The disc ID names three albums carrying the number the folder states,
/// two of them credited to one artist however it is cased and the third to
/// another, all offered: the number comes into effect once, is searched on
/// MusicBrainz once as it stands, and on Discogs once under each offered
/// row's artist.
#[test]
fn a_number_is_searched_on_discogs_under_each_offered_rows_artist() {
    let state = disc_run_of_folder("Label AB-100", &["AB-100"]);
    // Another album each: rows on one album's card that show the same
    // thing are offered once.
    let titled = |result: MetadataResult, title: &str| MetadataResult {
        title: title.to_string(),
        source_group_id: Some(title.to_string()),
        ..result
    };
    let (state, effects) = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                labelled(MB, "mb-1", "Label", "AB-100"),
                titled(credited(MB, "mb-2", "ARTIST NAME", "Label", "AB 100"), "Album Two"),
                titled(credited(MB, "mb-3", "Other Artist", "Label", "AB-100"), "Album Three"),
            ])),
        },
    );
    let IdentifyState::Triangulating { catalog, context, .. } = &state else {
        panic!("the run goes round: {state:?}");
    };
    assert_eq!(crate::state::rounds::offered_rows(context).len(), 3, "every album is offered");
    let searches: Vec<&Effect> =
        effects.iter().filter(|effect| matches!(effect, Effect::LookupCatalog { .. })).collect();
    assert_eq!(
        searches,
        vec![
            &catalog_lookup(MB, "AB-100"),
            &catalog_lookup(DG, "AB-100"),
            &catalog_lookup_under(DG, "AB-100", Some("Other Artist")),
        ]
    );
    assert_eq!(context.catalog.confirmed, vec!["AB-100".to_string()]);
    assert_eq!(catalog.lookups().len(), 1, "one number, searched thrice");
}

/// The number's searches turn up releases of another artist that reuse it,
/// which the run does not offer: they carry the number, but Discogs is not
/// searched again under their artist — the album's artist is the offered
/// row's.
#[test]
fn a_release_of_another_artist_carrying_the_number_adds_no_search() {
    let state = disc_run_of_folder("Label AB-100", &["AB-100"]);
    let (state, effects) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![labelled(MB, "mb-1", "Label", "AB-100")])),
        },
    );
    assert!(effects.contains(&catalog_lookup(DG, "AB-100")), "{effects:?}");
    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "AB-100".to_string(),
            artist: None,
            outcome: Ok(vec![credited(MB, "mb-9", "Other Artist", "Other Label", "AB-100")]),
        },
    );
    let (state, effects) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: DG,
            for_catalog: "AB-100".to_string(),
            artist: Some("Artist Name".to_string()),
            outcome: Ok(vec![credited(DG, "dg-1", "Third Artist", "Label", "AB-100")]),
        },
    );
    assert!(
        !effects.iter().any(|effect| matches!(effect, Effect::LookupCatalog { .. })),
        "no search under another artist: {effects:?}"
    );
    let IdentifyState::Found { findings, .. } = &state else {
        panic!("the run settles: {state:?}");
    };
    assert!(
        findings.narrowed_out.matches.iter().any(|result| result.release_id == "mb-9"),
        "the other artist's release is found and not offered: {findings:?}"
    );
    let Some(crate::view::CatalogStepView::Numbers { rows, .. }) =
        state_ledger(&state).map(|ledger| ledger.catalog)
    else {
        panic!("the ledger lays the numbers out: {state:?}");
    };
    assert_eq!(rows.len(), 1, "one row for the number: {rows:?}");
}

/// The ledger a settled run recorded.
fn state_ledger(state: &IdentifyState) -> Option<crate::IdentifyRunView> {
    match state {
        IdentifyState::Found { ledger, .. }
        | IdentifyState::NotFoundAnywhere { ledger, .. }
        | IdentifyState::Failed { ledger, .. } => ledger.clone(),
        other => panic!("expected a settled run, got {other:?}"),
    }
}

/// A number the person picked is searched as it stands from the start, and a
/// release found carrying it under an artist adds no search of it: the one
/// it had covers every artist.
#[test]
fn a_picked_number_is_searched_as_it_stands_and_not_again_under_an_artist() {
    let (state, effects) = started_with_choices(vec![MB, DG], choosing(&["AB-100"]));
    assert_eq!(
        effects,
        vec![catalog_lookup_under(MB, "AB-100", None), catalog_lookup_under(DG, "AB-100", None),]
    );
    let mut signals = disc_only(&["AB-100"]);
    signals.text_pool = vec![crate::signals::TextLine {
        text: "Label AB-100".to_string(),
        origin: crate::signals::TextOrigin::FolderName,
    }];
    let (state, mut asked) = update(state, signals);
    let (state, more) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![labelled(MB, "mb-1", "Label", "AB-100")])),
        },
    );
    asked.extend(more);
    let mut state = state;
    for source in [MB, DG] {
        let (next, more) = step(
            state,
            IdentifyEvent::CatalogLookupAnswered {
                source,
                for_catalog: "AB-100".to_string(),
                artist: None,
                outcome: Ok(Vec::new()),
            },
        );
        state = next;
        asked.extend(more);
    }
    assert!(
        !asked.iter().any(|effect| matches!(effect, Effect::LookupCatalog { .. })),
        "nothing searches the picked number again: {asked:?}"
    );
    assert!(matches!(state, IdentifyState::Found { .. }), "the run settles: {state:?}");
}

/// An offered row whose number is in effect is not looked up again by that
/// number on a catalog the number was searched on, as it stands or under the
/// artist the row's record credits: the search already holds every release
/// the lookup would return.
#[test]
fn a_rows_number_searched_under_its_artist_is_not_looked_up_again() {
    let state = disc_run_of_folder("Label AB-100", &["AB-100"]);
    let answered = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![labelled(MB, "mb-1", "Label", "AB-100")])),
        },
    );
    let (mut state, effects, mut asked) = answer_rounds(
        answered,
        |_| ReleaseDocument {
            labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label"), Some("AB-100"))],
            ..plain_document(&[])
        },
        |_, _| Vec::new(),
    );
    assert!(effects.contains(&catalog_lookup(DG, "AB-100")), "{effects:?}");
    for (source, artist) in [(MB, None), (DG, Some("Artist Name"))] {
        let answered = crate::state::step(
            state,
            IdentifyEvent::CatalogLookupAnswered {
                source,
                for_catalog: "AB-100".to_string(),
                artist: artist.map(str::to_string),
                outcome: Ok(Vec::new()),
            },
        );
        let (next, _, more) = answer_rounds(answered, |_| plain_document(&[]), |_, _| Vec::new());
        state = next;
        asked.pressings.extend(more.pressings);
    }
    assert_eq!(asked.pressings, Vec::new(), "no pressing lookup is asked");
    assert!(matches!(state, IdentifyState::Found { .. }), "the run settles: {state:?}");
}

/// A Discogs row is looked up on MusicBrainz by its number under its label
/// alone: MusicBrainz answers a catalog-number search in one page, so naming
/// the artist there saves no request.
#[test]
fn a_rows_number_is_looked_up_on_musicbrainz_under_no_artist() {
    let row = vec![credited(DG, "dg-1", "Artist Name", "Label", "AB-100")];
    assert_eq!(
        crate::state::pressings::keys_of(&row, &[MB, DG]),
        vec![(
            MB,
            PressingKey::CatalogNumber {
                number: "AB-100".to_string(),
                label: "Label".to_string(),
                artist: None,
            }
        )]
    );
}

/// The offered album credits various artists: its number names no artist
/// on Discogs either, and is searched alone, once, on both catalogs.
#[test]
fn a_compilations_number_is_searched_alone_on_discogs_too() {
    let state = disc_run_of_folder("Label AB-100", &["AB-100"]);
    let (_, effects) = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![credited(
                MB,
                "mb-1",
                "Various Artists",
                "Label",
                "AB-100",
            )])),
        },
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::LookupCatalog { .. }))
            .collect::<Vec<_>>(),
        vec![&catalog_lookup_under(MB, "AB-100", None), &catalog_lookup_under(DG, "AB-100", None),]
    );
}

/// Once the offered row's document rules it out, no row is offered and
/// nothing names the album's artist: the number Discogs was already asked
/// about under the artist is not asked again alone, and the run settles.
#[test]
fn a_number_is_not_searched_again_once_no_row_is_offered() {
    let state = disc_run_of_folder("Label AB-100", &["AB-100"]);
    let answered = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![labelled(MB, "mb-1", "Label", "AB-100")])),
        },
    );
    let other_tracks = |_: &LookupRef| ReleaseDocument {
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label"), Some("AB-100"))],
        source_tracks: bae_catalogs::search::SourceTracks::Listed { count: 9 },
        ..plain_document(&[])
    };
    let (mut state, effects, _) = answer_rounds(answered, other_tracks, |_, _| Vec::new());
    assert!(effects.contains(&catalog_lookup(DG, "AB-100")), "{effects:?}");
    let mut asked = Vec::new();
    for (source, artist) in [(MB, None), (DG, Some("Artist Name"))] {
        let answered = crate::state::step(
            state,
            IdentifyEvent::CatalogLookupAnswered {
                source,
                for_catalog: "AB-100".to_string(),
                artist: artist.map(str::to_string),
                outcome: Ok(Vec::new()),
            },
        );
        let (next, more, _) = answer_rounds(answered, other_tracks, |_, _| Vec::new());
        state = next;
        asked.extend(more);
    }
    assert!(
        !asked.iter().any(|effect| matches!(effect, Effect::LookupCatalog { .. })),
        "nothing searches the number again: {asked:?}"
    );
    assert!(state.is_terminal(), "the run settles: {state:?}");
}

/// Two searches of one number name no artist, each on its own catalog: an
/// answer lands on the search still waiting on the catalog that answered.
#[test]
fn an_answer_lands_on_the_search_waiting_on_its_catalog() {
    let waiting = |source: LookupCatalog| crate::state::CatalogSearch {
        artist: None,
        providers: vec![crate::state::ProviderLookup { source, state: LookupState::LookingUp }],
    };
    let mut lookup = crate::state::CatalogLookup {
        value: "AB-100".to_string(),
        searches: vec![waiting(MB), waiting(DG)],
    };
    lookup.answer(DG, &None, Ok(Vec::new()));
    assert_eq!(lookup.searches[0], waiting(MB), "MusicBrainz's still waits");
    assert_eq!(lookup.searches[1].providers[0].state, LookupState::Done { results: Vec::new() });
}

/// A catalog number nobody chose narrows nothing.
#[test]
fn an_unchosen_catalog_number_narrows_nothing() {
    let (state, _) = update(started(), disc_only(&["LBL 001"]));
    let mut r_a = mk_result("rel-a", Some("g-x"));
    r_a.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("LBL-001"))];
    let mut r_b = mk_result("rel-b", Some("g-y"));
    r_b.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("LBL-002"))];
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![r_a, r_b])),
        },
    );
    match state {
        IdentifyState::Found { findings: Findings { matches, .. }, .. } => {
            assert_eq!(matches.len(), 2);
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

// A number the person picked, as the ledger shows its search.

/// A run whose disc ID found two releases of one group, settled `Found` with
/// "LBL 001" offered but not chosen.
fn state_with_catalog_offered(providers: Vec<LookupCatalog>) -> IdentifyState {
    let (state, _) = update(
        started_with(providers),
        signals(disc("disc-hash"), BarcodeSignal::Absent, &["LBL 001"]),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "disc-hash".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                pair("rel-a", Some("g-x")),
                pair("rel-b", Some("g-x")),
            ])),
        },
    );
    state
}

/// The same run started with "LBL 001" chosen, with the effects it dispatched.
fn run_with_catalog_chosen(providers: Vec<LookupCatalog>) -> (IdentifyState, Vec<Effect>) {
    let (state, chosen_effects) = started_with_choices(providers, choosing(&["LBL 001"]));
    let mut folder = signals(disc("disc-hash"), BarcodeSignal::Absent, &["LBL 001"]);
    folder.text_pool = vec![crate::signals::TextLine {
        text: "Album [LBL 001]".to_string(),
        origin: crate::signals::TextOrigin::FolderName,
    }];
    let (state, _) = update(state, folder);
    // rel-b carries the chosen number, which the folder's text prints.
    let mut numbered = pair("rel-b", Some("g-x"));
    numbered.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("LBL 001"))];
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "disc-hash".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![pair("rel-a", Some("g-x")), numbered])),
        },
    );
    (state, chosen_effects)
}

/// The ledger row of the number `value`.
fn number_row(state: &IdentifyState, value: &str) -> crate::SignalValueRow {
    catalog_rows(state)
        .0
        .into_iter()
        .find(|row| row.value == value)
        .unwrap_or_else(|| panic!("the ledger lays out {value}"))
}

/// A chosen number is looked up from the start, and the release carrying
/// it, which the text prints, leads.
#[test]
fn a_chosen_catalog_number_is_looked_up_from_the_start() {
    let (state, effects) = run_with_catalog_chosen(vec![MB]);
    assert_eq!(
        effects,
        vec![Effect::LookupCatalog { source: MB, catalog: "LBL 001".to_string(), artist: None }]
    );
    assert_eq!(cell(&number_row(&state, "LBL 001"), MB), &crate::LookupView::LookingUp);

    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "LBL 001".to_string(),
            artist: None,
            outcome: Ok(vec![pair("rel-b", Some("g-x"))]),
        },
    );
    match state {
        IdentifyState::Found { findings: Findings { ref matches, .. }, ref ledger, .. } => {
            assert_eq!(
                matches.iter().map(|m| m.release_id.as_str()).collect::<Vec<_>>(),
                vec!["rel-b"]
            );
            assert!(found_by_disc_id(ledger.as_ref(), &matches[0]));
        }
        ref other => panic!("expected Found, got {other:?}"),
    }
    let row = number_row(&state, "LBL 001");
    assert!(!row.excluded);
    assert!(matches!(cell(&row, MB), crate::LookupView::Found { count: 1, .. }));
}

/// With no number chosen, none is asked about and the catalog takes no part.
#[test]
fn a_run_with_no_chosen_number_asks_about_none_of_them() {
    let state = state_with_catalog_offered(vec![MB]);
    assert!(matches!(state, IdentifyState::Found { .. }));
    let (numbers, offered) = catalog_rows(&state);
    assert!(numbers.is_empty(), "no number is in effect: {numbers:?}");
    assert_eq!(offered, ["LBL 001"]);
}

/// A chosen number the settled snapshot no longer offers leaves the run.
#[test]
fn a_chosen_number_the_snapshot_does_not_offer_leaves_the_run() {
    let (state, effects) = started_with_choices(vec![MB], choosing(&["GONE 001"]));
    assert_eq!(
        effects,
        vec![Effect::LookupCatalog { source: MB, catalog: "GONE 001".to_string(), artist: None }]
    );
    let (state, _) =
        update(state, signals(DiscIdSignal::Absent, BarcodeSignal::Absent, &["LBL 001"]));
    // Nothing is left in flight to wait on.
    assert!(!matches!(state, IdentifyState::Triangulating { .. }), "got {state:?}");
    let (numbers, offered) = catalog_rows(&state);
    assert!(numbers.is_empty(), "no number is in effect: {numbers:?}");
    assert_eq!(offered, ["LBL 001"]);
}

/// A chosen number is not dropped against a snapshot still being read.
#[test]
fn a_chosen_number_survives_a_snapshot_still_being_read() {
    let (state, _) = started_with_choices(vec![MB], choosing(&["LBL 001"]));
    let (state, _) = step(
        state,
        IdentifyEvent::SignalsUpdated {
            signals: Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: DiscIdSignal::Absent,
                barcode: BarcodeSignal::Scanning { codes: Vec::new() },
                text: crate::signals::TextSignal::Scanning {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            audio: crate::signals::AudioFacts::default(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    assert_eq!(
        cell(&number_row(&state, "LBL 001"), MB),
        &crate::LookupView::LookingUp,
        "the chosen number's lookup is still out"
    );
}

/// One provider failing a catalog lookup leaves the other's results standing.
#[test]
fn a_catalog_lookup_keeps_one_provider_s_answer_beside_the_other_s_failure() {
    let (state, effects) = run_with_catalog_chosen(vec![MB, DG]);
    assert_eq!(
        effects,
        vec![
            Effect::LookupCatalog { source: MB, catalog: "LBL 001".to_string(), artist: None },
            Effect::LookupCatalog { source: DG, catalog: "LBL 001".to_string(), artist: None },
        ]
    );

    let timeout = bae_catalogs::LookupFailure::Timeout;
    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: DG,
            for_catalog: "LBL 001".to_string(),
            artist: None,
            outcome: Err(timeout.clone()),
        },
    );
    // MusicBrainz is still out, so nothing settles.
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    let row = number_row(&state, "LBL 001");
    assert_eq!(cell(&row, DG), &crate::LookupView::Failed { failure: timeout.clone() });
    assert_eq!(cell(&row, MB), &crate::LookupView::LookingUp);

    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "LBL 001".to_string(),
            artist: None,
            outcome: Ok(vec![pair("rel-b", Some("g-x"))]),
        },
    );
    match &state {
        IdentifyState::Failed { failures, findings: Findings { matches, .. }, .. } => {
            assert_eq!(
                failures,
                &vec![crate::IdentifyFailure::Catalog(SourceFailure {
                    source: DG.catalog(),
                    failure: timeout,
                })]
            );
            assert_eq!(
                matches.iter().map(|m| m.release_id.as_str()).collect::<Vec<_>>(),
                vec!["rel-b"]
            );
        }
        other => panic!("expected Failed with the surviving intersection, got {other:?}"),
    }
    assert!(matches!(
        cell(&number_row(&state, "LBL 001"), MB),
        crate::LookupView::Found { count: 1, .. }
    ));
}
