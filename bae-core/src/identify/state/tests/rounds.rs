// Included by `tests.rs`; shares the helpers of `signals_and_conflicts.rs`
// and `album_links.rs`.

/// A release of `source` by "Artist Name" printing `number` under `label`.
fn labelled(source: Catalog, release_id: &str, label: &str, number: &str) -> MetadataResult {
    credited(source, release_id, "Artist Name", label, number)
}

/// A release of `source` credited to `artist`, printing `number` under
/// `label`.
fn credited(
    source: Catalog,
    release_id: &str,
    artist: &str,
    label: &str,
    number: &str,
) -> MetadataResult {
    MetadataResult {
        artist: Some(artist.to_string()),
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some(label), Some(number))],
        ..mk_result_from(source, release_id, Some("g"))
    }
}

/// A run on both catalogs given a disc ID alone, its folder named `folder`.
fn disc_run_of_folder(folder: &str, catalogs: &[&str]) -> IdentifyState {
    disc_run_of_text(
        &[(folder, crate::signals::TextOrigin::FolderName)],
        catalogs,
    )
}

/// A run on both catalogs given a disc ID alone, its text the given lines.
fn disc_run_of_text(
    lines: &[(&str, crate::signals::TextOrigin)],
    catalogs: &[&str],
) -> IdentifyState {
    let mut disc_signals = disc_only(catalogs);
    disc_signals.text_pool = lines
        .iter()
        .map(|(text, origin)| crate::signals::TextLine {
            text: text.to_string(),
            origin: *origin,
        })
        .collect();
    let (state, effects) = super::step(
        started_with(vec![MB, DG]),
        IdentifyEvent::SignalsUpdated {
            signals: disc_signals,
            audio: five_tracks(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    assert_eq!(effects, vec![Effect::LookupDiscid { disc_id: "d".into() }]);
    state
}

/// The search of `catalog` on `source` a round asks for a release by
/// "Artist Name", the artist [`labelled`] credits: as it stands on
/// MusicBrainz, under the artist on Discogs.
fn catalog_lookup(source: Catalog, catalog: &str) -> Effect {
    let artist = (source == DG).then_some("Artist Name");
    catalog_lookup_under(source, catalog, artist)
}

fn catalog_lookup_under(source: Catalog, catalog: &str, artist: Option<&str>) -> Effect {
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
    let (state, effects) = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                (
                    labelled(MB, "mb-1", "Label", "AB 12345-2"),
                    LibraryStatus::absent("mb-1"),
                ),
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

    let (state, _) = super::step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "AB 12345-2".to_string(),
            artist: None,
            outcome: Ok(vec![(
                labelled(MB, "mb-1", "Label", "AB 12345-2"),
                LibraryStatus::absent("mb-1"),
            )]),
        },
    );
    let reading = super::step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: DG,
            for_catalog: "AB 12345-2".to_string(),
            artist: Some("Artist Name".to_string()),
            outcome: Ok(vec![(
                labelled(DG, "dg-1", "Label", "AB 12345-2"),
                LibraryStatus::absent("dg-1"),
            )]),
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
    assert!(discogs.1.by_catalog && !discogs.1.by_disc_id);
    let IdentifyState::Found { ledger, .. } = &state else {
        panic!("expected Found, got {state:?}");
    };
    let Some(crate::identify::view::CatalogStepView::Numbers { rows, .. }) =
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
    let (state, effects) = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                (
                    labelled(MB, "mb-1", "Label", "AB-100"),
                    LibraryStatus::absent("mb-1"),
                ),
                (
                    labelled(MB, "mb-2", "Other", "XY-250"),
                    LibraryStatus::absent("mb-2"),
                ),
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
        vec![
            catalog_lookup_under(MB, "CD-2", None),
            catalog_lookup_under(DG, "CD-2", None)
        ]
    );
}

/// An offered row whose page names no Discogs release and which prints no
/// barcode is looked up on Discogs by its catalog number, under its label
/// and the artist its record credits, and only a release printing that
/// number under the row's label answers. The run then goes round until a
/// round brings nothing new, asking each key once.
#[test]
fn a_row_with_no_link_and_no_barcode_is_looked_up_by_its_number_under_its_label() {
    let state = disc_run_of_folder("Some Album", &[]);
    let record = MetadataResult {
        artist: Some("Artist Name".to_string()),
        ..mk_result("mb-1", Some("g"))
    };
    let reading = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![(record, LibraryStatus::absent("mb-1"))])),
        },
    );
    let (state, effects, asked) = answer_rounds(
        reading,
        |release| ReleaseDocument {
            labels: vec![bae_types::pressing::ReleaseLabel::of(
                Some("Label One"),
                Some("XY-100"),
            )],
            album_links: match release.catalog {
                Catalog::MusicBrainz => AlbumLinks::Read(Vec::new()),
                _ => AlbumLinks::NotAsked,
            },
            ..plain_document(&[])
        },
        |source, _| match source {
            Catalog::Discogs => vec![
                labelled(DG, "dg-1", "Label One Records", "XY-100"),
                labelled(DG, "dg-2", "Other Label", "XY-100"),
            ],
            _ => Vec::new(),
        },
    );
    let number = PressingKey::CatalogNumber {
        number: "XY-100".to_string(),
        label: "Label One".to_string(),
        artist: Some("Artist Name".to_string()),
    };
    assert_eq!(asked.pressings.first(), Some(&(DG, number)));
    let mut keys = asked.pressings.clone();
    keys.dedup();
    assert_eq!(keys, asked.pressings, "each key is asked once");
    assert!(
        effects
            .iter()
            .all(|effect| matches!(effect, Effect::KeepAlbumLinks { .. })),
        "the last round asks nothing: {effects:?}"
    );
    let rows = rows_of(&state);
    let found = |id: &str| rows.iter().find(|(result, _, _)| result.release_id == id);
    let (_, lookup, _) = found("dg-1").expect("the label's release is pooled");
    assert!(lookup.by_pressing);
    assert!(found("dg-2").is_none(), "another label's number is not it");
}

/// A pressing lookup that fails leaves the run failed, naming the catalog.
#[test]
fn a_failed_pressing_lookup_fails_the_run() {
    let state = disc_run_of_folder("Some Album", &[]);
    let mut record = mk_result("mb-1", Some("g"));
    record.barcodes = vec!["5012345678900".to_string()];
    let (mut state, mut effects) = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![(record, LibraryStatus::absent("mb-1"))])),
        },
    );
    while let Some(effect) = effects.pop() {
        let event = match effect {
            Effect::ReadReleases { releases, .. } => IdentifyEvent::ReleasesRead {
                read: releases
                    .into_iter()
                    .map(|release| ReleaseReading {
                        release: release.release().clone(),
                        document: Err(LookupFailure::Network),
                    })
                    .collect(),
            },
            Effect::LookupPressing { source, key } => {
                assert_eq!(
                    (source, &key),
                    (
                        DG,
                        &PressingKey::Barcode {
                            barcode: "5012345678900".to_string()
                        }
                    )
                );
                IdentifyEvent::PressingLookupAnswered {
                    source,
                    key,
                    outcome: Err(LookupFailure::Timeout),
                }
            }
            other => panic!("unexpected {other:?}"),
        };
        let (next, more) = super::step(state, event);
        state = next;
        effects.extend(more);
    }
    let IdentifyState::Failed { failures, .. } = &state else {
        panic!("expected Failed, got {state:?}");
    };
    assert_eq!(
        failures,
        &vec![IdentifyFailure::Pressing(SourceFailure {
            source: DG,
            failure: LookupFailure::Timeout,
        })]
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
    let (state, effects) = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                (
                    labelled(MB, "mb-1", "Label", "AB-100"),
                    LibraryStatus::absent("mb-1"),
                ),
                (
                    titled(
                        credited(MB, "mb-2", "ARTIST NAME", "Label", "AB 100"),
                        "Album Two",
                    ),
                    LibraryStatus::absent("mb-2"),
                ),
                (
                    titled(
                        credited(MB, "mb-3", "Other Artist", "Label", "AB-100"),
                        "Album Three",
                    ),
                    LibraryStatus::absent("mb-3"),
                ),
            ])),
        },
    );
    let IdentifyState::Triangulating {
        catalog, context, ..
    } = &state
    else {
        panic!("the run goes round: {state:?}");
    };
    assert_eq!(
        crate::identify::state::rounds::offered_rows(context).len(),
        3,
        "every album is offered"
    );
    let searches: Vec<&Effect> = effects
        .iter()
        .filter(|effect| matches!(effect, Effect::LookupCatalog { .. }))
        .collect();
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
            outcome: Ok(DiscIdAnswer::for_test(vec![(
                labelled(MB, "mb-1", "Label", "AB-100"),
                LibraryStatus::absent("mb-1"),
            )])),
        },
    );
    assert!(effects.contains(&catalog_lookup(DG, "AB-100")), "{effects:?}");
    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "AB-100".to_string(),
            artist: None,
            outcome: Ok(vec![(
                credited(MB, "mb-9", "Other Artist", "Other Label", "AB-100"),
                LibraryStatus::absent("mb-9"),
            )]),
        },
    );
    let (state, effects) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: DG,
            for_catalog: "AB-100".to_string(),
            artist: Some("Artist Name".to_string()),
            outcome: Ok(vec![(
                credited(DG, "dg-1", "Third Artist", "Label", "AB-100"),
                LibraryStatus::absent("dg-1"),
            )]),
        },
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LookupCatalog { .. })),
        "no search under another artist: {effects:?}"
    );
    let IdentifyState::Found { findings, .. } = &state else {
        panic!("the run settles: {state:?}");
    };
    assert!(
        findings
            .narrowed_out
            .matches
            .iter()
            .any(|result| result.release_id == "mb-9"),
        "the other artist's release is found and not offered: {findings:?}"
    );
    let Some(crate::identify::view::CatalogStepView::Numbers { rows, .. }) =
        state_ledger(&state).map(|ledger| ledger.catalog)
    else {
        panic!("the ledger lays the numbers out: {state:?}");
    };
    assert_eq!(rows.len(), 1, "one row for the number: {rows:?}");
}

/// The ledger a settled run recorded.
fn state_ledger(state: &IdentifyState) -> Option<crate::identify::IdentifyRunView> {
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
        vec![
            catalog_lookup_under(MB, "AB-100", None),
            catalog_lookup_under(DG, "AB-100", None),
        ]
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
            outcome: Ok(DiscIdAnswer::for_test(vec![(
                labelled(MB, "mb-1", "Label", "AB-100"),
                LibraryStatus::absent("mb-1"),
            )])),
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
        !asked
            .iter()
            .any(|effect| matches!(effect, Effect::LookupCatalog { .. })),
        "nothing searches the picked number again: {asked:?}"
    );
    assert!(
        matches!(state, IdentifyState::Found { .. }),
        "the run settles: {state:?}"
    );
}

/// An offered row whose number is in effect is not looked up again by that
/// number on a catalog the number was searched on, as it stands or under the
/// artist the row's record credits: the search already holds every release
/// the lookup would return.
#[test]
fn a_rows_number_searched_under_its_artist_is_not_looked_up_again() {
    let state = disc_run_of_folder("Label AB-100", &["AB-100"]);
    let answered = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![(
                labelled(MB, "mb-1", "Label", "AB-100"),
                LibraryStatus::absent("mb-1"),
            )])),
        },
    );
    let (mut state, effects, mut asked) = answer_rounds(
        answered,
        |_| ReleaseDocument {
            labels: vec![bae_types::pressing::ReleaseLabel::of(
                Some("Label"),
                Some("AB-100"),
            )],
            ..plain_document(&[])
        },
        |_, _| Vec::new(),
    );
    assert!(effects.contains(&catalog_lookup(DG, "AB-100")), "{effects:?}");
    for (source, artist) in [(MB, None), (DG, Some("Artist Name"))] {
        let answered = super::step(
            state,
            IdentifyEvent::CatalogLookupAnswered {
                source,
                for_catalog: "AB-100".to_string(),
                artist: artist.map(str::to_string),
                outcome: Ok(Vec::new()),
            },
        );
        let (next, _, more) = answer_rounds(
            answered,
            |_| plain_document(&[]),
            |_, _| Vec::new(),
        );
        state = next;
        asked.pressings.extend(more.pressings);
    }
    assert_eq!(asked.pressings, Vec::new(), "no pressing lookup is asked");
    assert!(
        matches!(state, IdentifyState::Found { .. }),
        "the run settles: {state:?}"
    );
}

/// A Discogs row is looked up on MusicBrainz by its number under its label
/// alone: MusicBrainz answers a catalog-number search in one page, so naming
/// the artist there saves no request.
#[test]
fn a_rows_number_is_looked_up_on_musicbrainz_under_no_artist() {
    let row = vec![credited(DG, "dg-1", "Artist Name", "Label", "AB-100")];
    assert_eq!(
        super::pressings::keys_of(&row, &[MB, DG]),
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
    let (_, effects) = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![(
                credited(MB, "mb-1", "Various Artists", "Label", "AB-100"),
                LibraryStatus::absent("mb-1"),
            )])),
        },
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::LookupCatalog { .. }))
            .collect::<Vec<_>>(),
        vec![
            &catalog_lookup_under(MB, "AB-100", None),
            &catalog_lookup_under(DG, "AB-100", None),
        ]
    );
}

/// Once the offered row's document rules it out, no row is offered and
/// nothing names the album's artist: the number Discogs was already asked
/// about under the artist is not asked again alone, and the run settles.
#[test]
fn a_number_is_not_searched_again_once_no_row_is_offered() {
    let state = disc_run_of_folder("Label AB-100", &["AB-100"]);
    let answered = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![(
                labelled(MB, "mb-1", "Label", "AB-100"),
                LibraryStatus::absent("mb-1"),
            )])),
        },
    );
    let other_tracks = |_: &MetadataRef| ReleaseDocument {
        labels: vec![bae_types::pressing::ReleaseLabel::of(
            Some("Label"),
            Some("AB-100"),
        )],
        source_tracks: crate::import::search::SourceTracks::Listed { count: 9 },
        ..plain_document(&[])
    };
    let (mut state, effects, _) = answer_rounds(answered, other_tracks, |_, _| Vec::new());
    assert!(effects.contains(&catalog_lookup(DG, "AB-100")), "{effects:?}");
    let mut asked = Vec::new();
    for (source, artist) in [(MB, None), (DG, Some("Artist Name"))] {
        let answered = super::step(
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
        !asked
            .iter()
            .any(|effect| matches!(effect, Effect::LookupCatalog { .. })),
        "nothing searches the number again: {asked:?}"
    );
    assert!(
        state.is_terminal(),
        "the run settles: {state:?}"
    );
}

/// Two searches of one number name no artist, each on its own catalog: an
/// answer lands on the search still waiting on the catalog that answered.
#[test]
fn an_answer_lands_on_the_search_waiting_on_its_catalog() {
    let waiting = |source: Catalog| crate::identify::state::CatalogSearch {
        artist: None,
        providers: vec![crate::identify::state::ProviderLookup {
            source,
            state: LookupState::LookingUp,
        }],
    };
    let mut lookup = crate::identify::state::CatalogLookup {
        value: "AB-100".to_string(),
        searches: vec![waiting(MB), waiting(DG)],
    };
    lookup.answer(DG, &None, Ok(Vec::new()));
    assert_eq!(lookup.searches[0], waiting(MB), "MusicBrainz's still waits");
    assert_eq!(
        lookup.searches[1].providers[0].state,
        LookupState::Done {
            results: Vec::new()
        }
    );
}

/// An offered release the Disc ID lookup answered with its document is read
/// from that document; one the answer carries no document for is read with
/// its release lookup.
#[test]
fn a_release_the_disc_id_answered_is_read_from_its_answer() {
    let state = disc_run_of_folder("Some Album", &[]);
    let answered = crate::import::payloads::DiscIdRelease::new(
        "mb-answered".to_string(),
        r#"{"id":"mb-answered"}"#.to_string(),
    );
    // Two pressings, told apart by where each was released.
    let released_in = |release_id: &str, country: &str| {
        let (mut result, status) = pair(release_id, Some("g"));
        result.area = Some(bae_types::pressing::area(country));
        (result, status)
    };
    let (_, effects) = super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer {
                results: vec![released_in("mb-answered", "DE"), released_in("mb-other", "GB")],
                documents: vec![answered.clone()],
            }),
        },
    );
    let [Effect::ReadReleases { releases, .. }] = effects.as_slice() else {
        panic!("the run reads its offered records' documents, got {effects:?}");
    };
    let mut releases = releases.clone();
    releases.sort_by_key(|release| release.release().key.clone());
    assert_eq!(
        releases,
        vec![
            crate::identify::documents::ReleaseToRead::DiscIdAnswer(answered),
            crate::identify::documents::ReleaseToRead::ReleaseLookup(MetadataRef::new(
                MB,
                "mb-other"
            )),
        ]
    );
}
