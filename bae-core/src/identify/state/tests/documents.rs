/// A Discogs search result on the master `m-1` stating only its first label.
fn first_label_only(release_id: &str) -> (MetadataResult, LibraryStatus) {
    let (mut result, status) = discogs_pair(release_id, Some("m-1"));
    result.labels = vec![crate::pressing::ReleaseLabel::of(
        Some("Label One"),
        Some("AB-100"),
    )];
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
        reading_documents(vec![first_label_only("dg-1"), first_label_only("dg-2")]);
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
/// as the folder is offered, and the other is set aside.
#[test]
fn the_row_whose_tracklist_fits_the_folder_ranks_first() {
    let (state, releases) =
        reading_documents(vec![first_label_only("dg-long"), first_label_only("dg-fits")]);
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
    assert_eq!(ids(&findings.narrowed_out.matches), vec!["dg-long"]);
}

/// A row whose document cannot be read keeps what its search result said and
/// says why; the run settles on the documents it did read, with no failure of
/// its own.
#[test]
fn a_row_whose_document_cannot_be_read_keeps_its_search_facts() {
    let (state, releases) =
        reading_documents(vec![first_label_only("dg-read"), first_label_only("dg-unread")]);
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
