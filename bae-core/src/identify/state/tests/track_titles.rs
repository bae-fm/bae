// The order of the folder's track titles against what each read document
// lists.

const FOLDER_TITLES: [&str; 5] = [
    "Song One",
    "Song Two",
    "Song Three",
    "Song Four",
    "Song Five",
];

/// A run like `reading_documents`, over a folder whose tracks carry
/// `FOLDER_TITLES`.
fn reading_titled_documents(
    results: Vec<(MetadataResult, LibraryStatus)>,
) -> (IdentifyState, Vec<crate::import::MetadataRef>) {
    let (state, _) = update(
        started_with(vec![DG]),
        Signals {
            track_titles: FOLDER_TITLES.iter().map(|title| title.to_string()).collect(),
            ..signals(
                DiscIdSignal::Absent,
                BarcodeSignal::Settled {
                    codes: artwork_codes(&["A"]),
                },
                &[],
            )
        },
    );
    let (state, effects) = super::step(state, barcode_matched(DG, "A", results));
    let [Effect::ReadReleases { releases, .. }] = effects.as_slice() else {
        panic!("the run reads its offered records' documents, got {effects:?}");
    };
    (state, releases.clone())
}

/// Of three rows the lookups tie, all listing the folder's five tracks, the
/// one whose document lists them in the folder's order is offered — written
/// with a version in brackets and in another case — and the one whose titles
/// are spelled apart and the one listing them in another order are set
/// aside.
#[test]
fn the_row_listing_the_folder_s_titles_in_order_ranks_first() {
    let (state, releases) = reading_titled_documents(vec![
        first_label_only("dg-reordered"),
        first_label_only("dg-spelled"),
        first_label_only("dg-in-order"),
    ]);
    let titled = |titles: &[&str]| crate::identify::documents::ReleaseDocument {
        track_titles: titles.iter().map(|title| title.to_string()).collect(),
        ..document(&[("Label One", "AB-100")], 5)
    };
    let (state, _) = super::step(
        state,
        IdentifyEvent::ReleasesRead {
            read: releases
                .iter()
                .map(|release| {
                    let document = match release.key.as_str() {
                        "dg-in-order" => titled(&[
                            "song one (Remastered)",
                            "Song Two",
                            "Song Three [Live]",
                            "Song Four",
                            "Song Five",
                        ]),
                        "dg-spelled" => titled(&[
                            "Chanson Un",
                            "Song Two",
                            "Song Three",
                            "Song Four",
                            "Song Five",
                        ]),
                        _ => titled(&[
                            "Song Two",
                            "Song One",
                            "Song Three",
                            "Song Four",
                            "Song Five",
                        ]),
                    };
                    read(release, Ok(document))
                })
                .collect(),
            twins: Vec::new(),
        },
    );
    let IdentifyState::Found { findings, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    assert_eq!(findings.matches.len(), 1);
    assert_eq!(findings.matches[0].release_id, "dg-in-order");
    assert_eq!(
        findings.matches[0].track_titles[0],
        "song one (Remastered)",
        "the document's titles are kept with its record"
    );
}

/// Of the rows the lookups tie, a run reads only some documents. While one
/// of them is left unread, a row whose titles agree is not lifted above it,
/// nor above one read with its titles spelled apart; only a row listing the
/// folder's titles in another order is set aside.
#[test]
fn agreeing_lifts_no_row_above_one_left_unread() {
    let (state, releases) = reading_titled_documents(vec![
        first_label_only("dg-in-order"),
        packed(first_label_only("dg-spelled"), crate::pressing::Packaging::Digipak),
        packed(
            first_label_only("dg-reordered"),
            crate::pressing::Packaging::GatefoldCover,
        ),
        packed(
            first_label_only("dg-unread"),
            crate::pressing::Packaging::CardboardSleeve,
        ),
    ]);
    let titled = |titles: &[&str]| crate::identify::documents::ReleaseDocument {
        track_titles: titles.iter().map(|title| title.to_string()).collect(),
        ..document(&[("Label One", "AB-100")], 5)
    };
    let (state, _) = super::step(
        state,
        IdentifyEvent::ReleasesRead {
            read: releases
                .iter()
                .map(|release| match release.key.as_str() {
                    "dg-in-order" => read(release, Ok(titled(&FOLDER_TITLES))),
                    "dg-spelled" => read(
                        release,
                        Ok(titled(&[
                            "Chanson Un",
                            "Song Two",
                            "Song Three",
                            "Song Four",
                            "Song Five",
                        ])),
                    ),
                    "dg-reordered" => read(
                        release,
                        Ok(titled(&[
                            "Song Two",
                            "Song One",
                            "Song Three",
                            "Song Four",
                            "Song Five",
                        ])),
                    ),
                    _ => read(release, Err(LookupFailure::Timeout)),
                })
                .collect(),
            twins: Vec::new(),
        },
    );
    let IdentifyState::Found { findings, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    let mut offered: Vec<&str> = findings
        .matches
        .iter()
        .map(|result| result.release_id.as_str())
        .collect();
    offered.sort();
    assert_eq!(offered, vec!["dg-in-order", "dg-spelled", "dg-unread"]);
}
