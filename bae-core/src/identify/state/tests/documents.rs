/// A Discogs search result on the master `m-1` stating only its first label.
fn first_label_only(release_id: &str) -> (MetadataResult, LibraryStatus) {
    let (mut result, status) = discogs_pair(release_id, Some("m-1"));
    result.labels = vec![crate::pressing::ReleaseLabel::of(
        Some("Label One"),
        Some("AB-100"),
    )];
    (result, status)
}

/// `row` in `packaging`, so it shows other than a look-alike beside it.
fn packed(
    (mut result, status): (MetadataResult, LibraryStatus),
    packaging: crate::pressing::Packaging,
) -> (MetadataResult, LibraryStatus) {
    result.packaging = Some(packaging);
    (result, status)
}

/// A run whose barcode found `results` on Discogs, holding at the read of
/// its offered records' documents, with the records it asks for.
fn reading_documents(
    results: Vec<(MetadataResult, LibraryStatus)>,
) -> (IdentifyState, Vec<crate::import::MetadataRef>) {
    let (state, _) = update(
        started_with(vec![DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A"]),
            },
            &[],
        ),
    );
    let (state, effects) = super::step(state, barcode_matched(DG, "A", results));
    let [Effect::ReadReleases { releases, .. }] = effects.as_slice() else {
        panic!("the run reads its offered records' documents, got {effects:?}");
    };
    (state, releases.clone())
}

fn document(labels: &[(&str, &str)], tracks: u32) -> crate::identify::documents::ReleaseDocument {
    crate::identify::documents::ReleaseDocument {
        labels: labels
            .iter()
            .map(|(name, number)| crate::pressing::ReleaseLabel::of(Some(name), Some(number)))
            .collect(),
        barcode: None,
        source_tracks: crate::import::search::SourceTracks::Listed { count: tracks },
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
        links: Vec::new(),
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
    }
}

fn read(
    release: &crate::import::MetadataRef,
    document: Result<crate::identify::documents::ReleaseDocument, LookupFailure>,
) -> crate::identify::documents::ReleaseReading {
    crate::identify::documents::ReleaseReading {
        release: release.clone(),
        document,
    }
}

/// Two tied search-result rows are both read in full before the run settles,
/// and each then states every label its document lists.
#[test]
fn every_offered_row_is_read_in_full_before_the_run_settles() {
    let (state, releases) =
        reading_documents(vec![
            first_label_only("dg-1"),
            packed(first_label_only("dg-2"), crate::pressing::Packaging::Digipak),
        ]);
    assert_eq!(releases.len(), 2, "both tied rows are read");
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    let full = document(&[("Label One", "AB-100"), ("Label One", "CD-200")], 5);
    let (state, _) = super::step(
        state,
        IdentifyEvent::ReleasesRead {
            read: releases
                .iter()
                .map(|release| read(release, Ok(full.clone())))
                .collect(),
        },
    );
    let IdentifyState::Found { findings, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    assert_eq!(findings.matches.len(), 2);
    for result in &findings.matches {
        assert_eq!(result.labels, full.labels, "{}", result.release_id);
        assert_eq!(result.source_tracks, Some(full.source_tracks.clone()));
    }
}

/// Of two rows the lookups tie, the one whose tracklist holds as many tracks
/// as the folder is offered. The other is no match: it is not set aside
/// under "N more releases" either.
#[test]
fn the_row_whose_tracklist_fits_the_folder_ranks_first() {
    let (state, releases) =
        reading_documents(vec![
            first_label_only("dg-long"),
            packed(first_label_only("dg-fits"), crate::pressing::Packaging::Digipak),
        ]);
    let (state, _) = super::step(
        state,
        IdentifyEvent::ReleasesRead {
            read: releases
                .iter()
                .map(|release| {
                    let tracks = if release.key == "dg-fits" { 5 } else { 6 };
                    read(release, Ok(document(&[("Label One", "AB-100")], tracks)))
                })
                .collect(),
        },
    );
    let IdentifyState::Found { findings, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    let ids = |results: &[MetadataResult]| {
        results
            .iter()
            .map(|result| result.release_id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&findings.matches), vec!["dg-fits"]);
    assert!(ids(&findings.narrowed_out.matches).is_empty());
}

/// A row whose document cannot be read keeps what its search result said and
/// says why; the run settles on the documents it did read, with no failure of
/// its own.
#[test]
fn a_row_whose_document_cannot_be_read_keeps_its_search_facts() {
    let (state, releases) =
        reading_documents(vec![
            first_label_only("dg-read"),
            packed(first_label_only("dg-unread"), crate::pressing::Packaging::Digipak),
        ]);
    let full = document(&[("Label One", "AB-100"), ("Label One", "CD-200")], 5);
    let (state, _) = super::step(
        state,
        IdentifyEvent::ReleasesRead {
            read: releases
                .iter()
                .map(|release| {
                    let document = if release.key == "dg-read" {
                        Ok(full.clone())
                    } else {
                        Err(LookupFailure::Network)
                    };
                    read(release, document)
                })
                .collect(),
        },
    );
    let IdentifyState::Found { findings, .. } = state else {
        panic!("the run settles on what it read, got {state:?}");
    };
    let row = |id: &str| {
        findings
            .matches
            .iter()
            .chain(&findings.narrowed_out.matches)
            .find(|result| result.release_id == id)
            .unwrap_or_else(|| panic!("{id} is on the list"))
    };
    let complete = row("dg-read");
    assert_eq!(complete.labels, full.labels);
    assert_eq!(complete.document_failure, None);
    let unread = row("dg-unread");
    assert_eq!(unread.labels, first_label_only("dg-unread").0.labels);
    assert_eq!(unread.source_tracks, None);
    assert_eq!(unread.document_failure, Some(LookupFailure::Network));
    assert_eq!(
        findings.matches.len(),
        2,
        "an unread tracklist says nothing against the row, so it does not lose to one read"
    );
}

/// A folder whose name writes the album as "Artist - 1990-2000 Album".
fn titled_folder() -> Signals {
    let mut folder = signals(DiscIdSignal::Absent, BarcodeSignal::Absent, &[]);
    folder.text_pool = vec![crate::signals::TextLine {
        text: "Artist - 1990-2000 Album".to_string(),
        origin: crate::signals::TextOrigin::FolderName,
    }];
    folder
}

/// A Discogs pressing of the master `m-1` titled `title`.
fn titled(release_id: &str, title: &str) -> (MetadataResult, LibraryStatus) {
    let (mut result, status) = discogs_pair(release_id, Some("m-1"));
    result.title = title.to_string();
    result.artist = Some("Artist".to_string());
    (result, status)
}

/// Answer a read of `releases` with a document listing `tracks(id)` tracks
/// for each.
fn read_with_tracks(
    releases: &[crate::import::MetadataRef],
    tracks: impl Fn(&str) -> u32,
) -> IdentifyEvent {
    IdentifyEvent::ReleasesRead {
        read: releases
            .iter()
            .map(|release| read(release, Ok(document(&[], tracks(&release.key)))))
            .collect(),
    }
}

fn offered_ids(state: &IdentifyState) -> Vec<String> {
    let IdentifyState::Found { findings, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    let mut ids: Vec<String> = findings
        .matches
        .iter()
        .map(|result| result.release_id.clone())
        .collect();
    ids.sort();
    ids
}

/// The title search returns pressings of one album whose records write its
/// title in two word orders, one of them the folder's. The order names the
/// album, not the pressing, so the rows tie, every one is read in full, and
/// those listing as many tracks as the folder holds are offered together —
/// not the one pressing whose record happens to spell the title alike.
#[test]
fn every_pressing_of_the_named_album_is_read_before_its_tracks_rank_it() {
    let state = started_searching(vec![DG], "1990-2000 Album", "Artist");
    let (state, effects) = update(state, titled_folder());
    assert_eq!(effects, vec![search_title(DG, "1990-2000 Album", "Artist")]);
    let (state, effects) = super::step(
        state,
        search_answered(
            DG,
            vec![
                titled("dg-cassette", "1990-2000 Album"),
                packed(
                    titled("dg-cd-1", "Album 1990-2000"),
                    crate::pressing::Packaging::JewelCase,
                ),
                packed(
                    titled("dg-cd-2", "Album 1990-2000"),
                    crate::pressing::Packaging::Digipak,
                ),
                packed(
                    titled("dg-lp", "Album 1990-2000"),
                    crate::pressing::Packaging::GatefoldCover,
                ),
            ],
        ),
    );
    let [Effect::ReadReleases { releases, .. }] = effects.as_slice() else {
        panic!("the run reads its offered records' documents, got {effects:?}");
    };
    let mut asked: Vec<&str> = releases.iter().map(|release| release.key.as_str()).collect();
    asked.sort();
    assert_eq!(asked, vec!["dg-cassette", "dg-cd-1", "dg-cd-2", "dg-lp"]);
    let (state, effects) = super::step(
        state,
        read_with_tracks(releases, |id| if id == "dg-lp" { 7 } else { 5 }),
    );
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(offered_ids(&state), vec!["dg-cassette", "dg-cd-1", "dg-cd-2"]);
}

/// A row whose document lists other tracks than the folder drops out, and
/// the rows ranked below it move up; those are read in turn before the run
/// settles, so no row is offered on a tracklist nobody read.
#[test]
fn rows_the_documents_raise_are_read_before_the_run_settles() {
    let state = started_searching(vec![DG], "1990-2000 Album", "Artist");
    let mut folder = titled_folder();
    folder.text_pool[0].text = "Artist - 1990-2000 Album (US)".to_string();
    let (state, _) = update(state, folder);
    let in_country = |release_id: &str| {
        let (mut result, status) = titled(release_id, "1990-2000 Album");
        result.area = Some(crate::pressing::area("US"));
        (result, status)
    };
    let (state, effects) = super::step(
        state,
        search_answered(
            DG,
            vec![
                in_country("dg-us"),
                titled("dg-elsewhere-1", "1990-2000 Album"),
                packed(
                    titled("dg-elsewhere-2", "1990-2000 Album"),
                    crate::pressing::Packaging::Digipak,
                ),
            ],
        ),
    );
    let [Effect::ReadReleases { releases, .. }] = effects.as_slice() else {
        panic!("the run reads its offered records' documents, got {effects:?}");
    };
    assert_eq!(
        releases.iter().map(|release| release.key.as_str()).collect::<Vec<_>>(),
        vec!["dg-us"]
    );
    let (state, effects) = super::step(state, read_with_tracks(releases, |_| 7));
    let [Effect::ReadReleases { releases, .. }] = effects.as_slice() else {
        panic!("the rows the country held back are read next, got {effects:?}");
    };
    let mut asked: Vec<&str> = releases.iter().map(|release| release.key.as_str()).collect();
    asked.sort();
    assert_eq!(asked, vec!["dg-elsewhere-1", "dg-elsewhere-2"]);
    let (state, _) = super::step(
        state,
        read_with_tracks(releases, |id| if id == "dg-elsewhere-1" { 5 } else { 6 }),
    );
    assert_eq!(offered_ids(&state), vec!["dg-elsewhere-1"]);
}

/// A document's barcode joins the ones its result lists: a code the result
/// left out is added, and one it lists in another spelling is not listed twice.
#[test]
fn a_document_s_barcode_joins_its_result_s() {
    let with_barcode = |barcode: &str| {
        let mut document = document(&[("Label One", "AB-100")], 5);
        document.barcode = Some(barcode.to_string());
        document
    };
    let (mut result, _) = first_label_only("dg-1");
    result.barcodes = vec!["0 12345 67890 5".to_string()];
    let release = crate::import::MetadataRef::new(DG, "dg-1");
    for (barcode, expected) in [
        ("012345678905", vec!["0 12345 67890 5"]),
        ("5051961234567", vec!["0 12345 67890 5", "5051961234567"]),
    ] {
        let mut read_back = result.clone();
        crate::identify::documents::DocumentReading::Read(vec![read(
            &release,
            Ok(with_barcode(barcode)),
        )])
        .apply(&mut read_back);
        assert_eq!(read_back.barcodes, expected, "{barcode}");
    }
}

/// A row whose record arrives already stating its tracklist ranks on what
/// this run reads of it, like the rows tied beside it: all three are read, and
/// the one whose document lists other tracks than the folder holds drops out,
/// whatever it stated before.
#[test]
fn tied_rows_rank_on_the_documents_this_run_reads() {
    let mut known = first_label_only("dg-known");
    known.0.source_tracks = Some(crate::import::search::SourceTracks::Listed { count: 5 });
    let (state, releases) = reading_documents(vec![
        known,
        packed(first_label_only("dg-other-1"), crate::pressing::Packaging::Digipak),
        packed(
            first_label_only("dg-other-2"),
            crate::pressing::Packaging::GatefoldCover,
        ),
    ]);
    let mut asked: Vec<&str> = releases.iter().map(|release| release.key.as_str()).collect();
    asked.sort();
    assert_eq!(asked, vec!["dg-known", "dg-other-1", "dg-other-2"]);
    let (state, _) = super::step(
        state,
        read_with_tracks(&releases, |id| if id == "dg-known" { 7 } else { 5 }),
    );
    assert_eq!(offered_ids(&state), vec!["dg-other-1", "dg-other-2"]);
}

/// However many rows tie at the top, every one is read before any is
/// offered, and those that fit stay tied for the person to pick.
#[test]
fn every_row_of_a_wide_tie_is_read_before_it_is_offered() {
    let tied: Vec<(MetadataResult, LibraryStatus)> = (1..=8)
        .map(|at| {
            let (mut result, status) = first_label_only(&format!("dg-{at}"));
            result.year = Some(2000 + at);
            (result, status)
        })
        .collect();
    let (state, releases) = reading_documents(tied);
    assert_eq!(releases.len(), 8, "every tied row is read");
    let (state, effects) = super::step(state, read_with_tracks(&releases, |_| 5));
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(offered_ids(&state).len(), 8);
    let verdict = crate::identify::TerminalVerdict::try_from(state).expect("the run settled");
    assert!(
        !crate::identify::VerdictSummary::of(&verdict, false).judgement().0,
        "tied rows are the person's to pick"
    );
}

/// Rows that tie and look alike are listed once, and only the one kept is
/// read.
#[test]
fn look_alike_rows_listed_once_are_read() {
    let tied: Vec<(MetadataResult, LibraryStatus)> =
        (1..=6).map(|at| first_label_only(&format!("dg-{at}"))).collect();
    let (_, releases) = reading_documents(tied);
    assert_eq!(
        releases,
        vec![crate::import::MetadataRef::new(DG, "dg-1")],
        "the look-alike kept, the lowest id, is read"
    );
}

/// Every row read lists other tracks than the folder: nothing is left, and
/// the folder is not found.
#[test]
fn a_run_whose_every_row_lists_other_tracks_finds_nothing() {
    let (state, releases) = reading_documents(vec![
        first_label_only("dg-long"),
        packed(first_label_only("dg-short"), crate::pressing::Packaging::Digipak),
    ]);
    let (state, effects) = super::step(
        state,
        read_with_tracks(&releases, |id| if id == "dg-long" { 6 } else { 4 }),
    );
    assert!(effects.is_empty(), "{effects:?}");
    assert!(
        matches!(state, IdentifyState::NotFoundAnywhere { .. }),
        "expected NotFoundAnywhere, got {state:?}"
    );
}

/// A row read with another count than the folder's drops out and the row
/// ranked below it is read next: here a row whose title the folder states
/// but whose tracks number other, then a row the folder names less of whose
/// tracks fit. The one left is offered alone and picked.
#[test]
fn a_row_that_lists_other_tracks_lets_the_next_best_row_be_read() {
    let state = started_searching(vec![DG], "1990-2000 Album", "Artist");
    let (state, _) = update(state, titled_folder());
    let (state, effects) = super::step(
        state,
        search_answered(
            DG,
            vec![
                titled("dg-named", "1990-2000 Album"),
                titled("dg-other", "Another Title"),
            ],
        ),
    );
    let [Effect::ReadReleases { releases, .. }] = effects.as_slice() else {
        panic!("the run reads its offered records' documents, got {effects:?}");
    };
    assert_eq!(
        releases.iter().map(|release| release.key.as_str()).collect::<Vec<_>>(),
        vec!["dg-named"]
    );
    let (state, effects) = super::step(state, read_with_tracks(releases, |_| 3));
    let [Effect::ReadReleases { releases, .. }] = effects.as_slice() else {
        panic!("the row below is read next, got {effects:?}");
    };
    assert_eq!(
        releases.iter().map(|release| release.key.as_str()).collect::<Vec<_>>(),
        vec!["dg-other"]
    );
    let (state, effects) = super::step(state, read_with_tracks(releases, |_| 5));
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(offered_ids(&state), vec!["dg-other"]);
    let IdentifyState::Found { findings, .. } = &state else {
        unreachable!("offered_ids checked it");
    };
    assert!(findings.narrowed_out.is_empty(), "{:?}", findings.narrowed_out);
    let verdict = crate::identify::TerminalVerdict::try_from(state).expect("the run settled");
    assert!(
        crate::identify::VerdictSummary::of(&verdict, false).judgement().0,
        "the one row left fits, and is picked"
    );
}
