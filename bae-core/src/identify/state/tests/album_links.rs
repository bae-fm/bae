// Included by `tests.rs`; shares the helpers of `signals_and_conflicts.rs`.

/// Once every lookup is in and what they found holds both catalogs'
/// releases, the run reads its MusicBrainz groups' album links before it
/// settles, and the matches carry what was read — read or not.
#[test]
fn a_run_holding_both_catalogs_reads_album_links_before_it_settles() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A"]),
            },
            &[],
        ),
    );
    let (state, effects) = step(
        state,
        barcode_matched(
            MB,
            "A",
            vec![pair("mb-1", Some("g-linked")), pair("mb-2", Some("g-gone"))],
        ),
    );
    assert!(effects.is_empty(), "one catalog alone joins nothing");
    let (state, effects) = step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-1", Some("7"))]),
    );
    let [Effect::ReadAlbumLinks { to_read }] = effects.as_slice() else {
        panic!("expected one album links read, got {effects:?}");
    };
    assert_eq!(
        to_read.group_ids().collect::<Vec<_>>(),
        vec!["g-linked", "g-gone"]
    );
    assert_eq!(
        to_read.on_list,
        vec![(crate::import::MetadataRef::new(DG, "dg-1"), Some("7".to_string()))]
    );
    assert!(matches!(state, IdentifyState::Triangulating { .. }));

    let linked = AlbumLinks::Read(vec![crate::import::album_links::AlbumLink {
        album: crate::import::MetadataRef::new(DG, "7"),
        stated: crate::import::album_links::AlbumStatement::Page,
    }]);
    let (state, effects) = step(
        state,
        IdentifyEvent::AlbumLinksRead {
            read: vec![
                GroupReading::of_links("g-linked", linked.clone()),
                GroupReading::of_links("g-gone", AlbumLinks::Unread),
            ],
        },
    );
    assert!(effects.is_empty());
    let IdentifyState::Found { matches, .. } = state else {
        panic!("expected Found");
    };
    let links_of = |release_id: &str| {
        matches
            .iter()
            .find(|m| m.release_id == release_id)
            .map(|m| m.album_links.clone())
    };
    assert_eq!(links_of("mb-1"), Some(linked));
    assert_eq!(links_of("mb-2"), Some(AlbumLinks::Unread));
    assert_eq!(links_of("dg-1"), Some(AlbumLinks::NotAsked));
}


/// A Discogs release the reading read through a MusicBrainz release's link
/// goes on the list beside that release: one row with both catalogs' records,
/// the twin's provenance naming the release that named it and no lookup, and
/// the two albums on one card because the reading states they are one.
#[test]
fn a_twin_joins_the_row_of_the_release_that_names_it() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A"]),
            },
            &[],
        ),
    );
    let mut named = pair("mb-1", Some("g-1"));
    named.0.links = vec![crate::import::MetadataRef::new(DG, "dg-twin")];
    let (state, _) = step(state, barcode_matched(MB, "A", vec![named]));
    let (state, effects) = step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-other", Some("7"))]),
    );
    let [Effect::ReadAlbumLinks { to_read }] = effects.as_slice() else {
        panic!("expected one album links read, got {effects:?}");
    };
    assert_eq!(
        to_read.groups[0].releases,
        vec![(
            "mb-1".to_string(),
            vec![crate::import::MetadataRef::new(DG, "dg-twin")]
        )]
    );

    let twin = mk_result_from(DG, "dg-twin", Some("7"));
    let reading = GroupReading {
        group: "g-1".to_string(),
        links: AlbumLinks::Read(vec![crate::import::album_links::AlbumLink {
            album: crate::import::MetadataRef::new(DG, "7"),
            stated: crate::import::album_links::AlbumStatement::Release {
                musicbrainz_release: "mb-1".to_string(),
                twin: crate::import::MetadataRef::new(DG, "dg-twin"),
            },
        }]),
        release_links: Vec::new(),
        twin: Some(crate::import::album_links::Twin {
            result: twin,
            named_by: crate::import::MetadataRef::new(MB, "mb-1"),
            status: LibraryStatus::absent("dg-twin"),
        }),
    };
    let (state, _) = step(state, IdentifyEvent::AlbumLinksRead { read: vec![reading] });
    let IdentifyState::Found {
        matches,
        provenance,
        pressings,
        narrowed_out,
        ..
    } = state
    else {
        panic!("expected Found");
    };
    let all: Vec<(&MetadataResult, &LookupProvenance, u32)> = matches
        .iter()
        .zip(&provenance)
        .zip(&pressings)
        .map(|((result, lookup), row)| (result, lookup, *row))
        .chain(
            narrowed_out
                .matches
                .iter()
                .zip(&narrowed_out.provenance)
                .zip(&narrowed_out.pressings)
                .map(|((result, lookup), row)| (result, lookup, *row + 100)),
        )
        .collect();
    let of = |release_id: &str| {
        all.iter()
            .find(|(result, _, _)| result.release_id == release_id)
            .copied()
            .unwrap_or_else(|| panic!("{release_id} is on the list"))
    };
    let (_, twin_lookup, twin_row) = of("dg-twin");
    let (_, named_lookup, named_row) = of("mb-1");
    assert_eq!(twin_row, named_row, "the twin shares its namer's row");
    assert_eq!(
        twin_lookup.named_by,
        Some(crate::import::MetadataRef::new(MB, "mb-1"))
    );
    assert!(!twin_lookup.by_barcode, "no lookup returned the twin");
    assert!(named_lookup.by_barcode && named_lookup.named_by.is_none());
}
