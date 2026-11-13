//! How an offered row's pressing is looked up on the catalog it has no
//! record of.

use super::*;
use crate::documents::ReleaseDocument;
use crate::state::tests::*;
use crate::state::*;
use crate::IdentifyFailure;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::records::LookupRef;
/// An offered row whose page names no Discogs release and which prints no
/// barcode is looked up on Discogs by its catalog number, under its label
/// and the artist its record credits, and only a release printing that
/// number under the row's label answers. The run then goes round until a
/// round brings nothing new, asking each key once.
#[test]
fn a_row_with_no_link_and_no_barcode_is_looked_up_by_its_number_under_its_label() {
    let state = disc_run_of_folder("Some Album", &[]);
    let record = MetadataResult {
        artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        ..mk_result("mb-1", Some("g"))
    };
    let reading = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![record])),
        },
    );
    let (state, effects, asked) = answer_rounds(
        reading,
        |release| ReleaseDocument {
            labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label One"), Some("XY-100"))],
            album_links: match release.catalog {
                LookupCatalog::MusicBrainz => AlbumLinks::Read(Vec::new()),
                _ => AlbumLinks::NotAsked,
            },
            ..plain_document(&[])
        },
        |source, _| match source {
            LookupCatalog::Discogs => vec![
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
        effects.iter().all(|effect| matches!(effect, Effect::KeepAlbumLinks { .. })),
        "the last round asks nothing: {effects:?}"
    );
    let rows = rows_of(&state);
    let found = |id: &str| rows.iter().find(|(result, _, _)| result.release_id == id);
    found("dg-1").expect("the label's release is pooled");
    assert!(found("dg-2").is_none(), "another label's number is not it");
}

/// An offered row whose page names no Discogs release is looked up on
/// Discogs by every barcode it prints and every catalog number it prints
/// under a label, each its own key: a row printing two of each asks four.
#[test]
fn a_row_with_no_link_is_looked_up_by_every_barcode_and_every_number() {
    let state = disc_run_of_folder("Some Album", &[]);
    let record = MetadataResult {
        artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        ..mk_result("mb-1", Some("g"))
    };
    let reading = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![record])),
        },
    );
    let (_, _, asked) = answer_rounds(
        reading,
        |release| ReleaseDocument {
            barcodes: vec!["5012345678900".to_string(), "5012345678917".to_string()],
            labels: vec![
                bae_types::pressing::ReleaseLabel::of(Some("Label One"), Some("XY-100")),
                bae_types::pressing::ReleaseLabel::of(Some("Label Two"), Some("AB-200")),
            ],
            album_links: match release.catalog {
                LookupCatalog::MusicBrainz => AlbumLinks::Read(Vec::new()),
                _ => AlbumLinks::NotAsked,
            },
            ..plain_document(&[])
        },
        |_, _| Vec::new(),
    );
    let number = |number: &str, label: &str| PressingKey::CatalogNumber {
        number: number.to_string(),
        label: label.to_string(),
        artist: Some("Artist Name".to_string()),
    };
    assert_eq!(
        asked.pressings,
        vec![
            (DG, PressingKey::Barcode { barcode: "5012345678900".to_string() }),
            (DG, PressingKey::Barcode { barcode: "5012345678917".to_string() }),
            (DG, number("XY-100", "Label One")),
            (DG, number("AB-200", "Label Two")),
        ]
    );
}

/// A pressing lookup that fails leaves the run failed, naming the catalog.
#[test]
fn a_failed_pressing_lookup_fails_the_run() {
    let state = disc_run_of_folder("Some Album", &[]);
    let mut record = mk_result("mb-1", Some("g"));
    record.barcodes = vec!["5012345678900".to_string()];
    let (mut state, mut effects) = crate::state::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![record])),
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
                    (DG, &PressingKey::Barcode { barcode: "5012345678900".to_string() })
                );
                IdentifyEvent::PressingLookupAnswered {
                    source,
                    key,
                    outcome: Err(LookupFailure::Timeout),
                }
            }
            other => panic!("unexpected {other:?}"),
        };
        let (next, more) = crate::state::step(state, event);
        state = next;
        effects.extend(more);
    }
    let IdentifyState::Failed { failures, .. } = &state else {
        panic!("expected Failed, got {state:?}");
    };
    assert_eq!(
        failures,
        &vec![IdentifyFailure::Pressing(SourceFailure {
            source: DG.catalog(),
            failure: LookupFailure::Timeout,
        })]
    );
}

/// Each MusicBrainz release a disc ID names links its own Discogs release,
/// which no lookup returned. Once the releases are read, each row is looked
/// up on Discogs by the release its page names, and each goes on the list
/// beside its row: two rows, each carrying both catalogs' records, whose
/// Discogs records are read in turn. The album their documents state is kept.
#[test]
fn a_disc_id_s_releases_are_looked_up_on_discogs_by_their_links() {
    use bae_types::pressing::Packaging::{Digipak, JewelCase};
    let reading = disc_run_reading(
        vec![packed_in(MB, "mb-1", "g-1", JewelCase), packed_in(MB, "mb-2", "g-1", Digipak)],
        Vec::new(),
    );
    let (state, effects, asked) = answer_rounds(
        reading,
        |release| match release.key.as_str() {
            "mb-1" => linking_document("dg-1", "7"),
            "mb-2" => linking_document("dg-2", "7"),
            _ => plain_document(&[]),
        },
        |_, key| match key {
            PressingKey::Link { release } if release.key == "dg-1" => {
                vec![packed_in(DG, "dg-1", "7", JewelCase)]
            }
            PressingKey::Link { .. } => vec![packed_in(DG, "dg-2", "7", Digipak)],
            other => panic!("a row whose page names a link is asked by it: {other:?}"),
        },
    );

    assert_eq!(
        asked.reads[0],
        vec![LookupRef::new(MB, "mb-1"), LookupRef::new(MB, "mb-2")],
        "the offered records are read first"
    );
    assert_eq!(
        asked.pressings,
        vec![
            (DG, PressingKey::Link { release: LookupRef::new(DG, "dg-1") }),
            (DG, PressingKey::Link { release: LookupRef::new(DG, "dg-2") }),
        ],
        "then each row by the release its page names"
    );
    assert_eq!(
        asked.reads[1..],
        [vec![LookupRef::new(DG, "dg-1"), LookupRef::new(DG, "dg-2")]],
        "then the releases found, now on the offered rows"
    );

    let rows = rows_of(&state);
    let of = |release_id: &str| {
        rows.iter()
            .find(|(result, _, _)| result.release_id == release_id)
            .cloned()
            .unwrap_or_else(|| panic!("{release_id} is on the list: {rows:?}"))
    };
    for (release, linked) in [("mb-1", "dg-1"), ("mb-2", "dg-2")] {
        let (named, named_by_disc_id, named_row) = of(release);
        let (_, linked_by_disc_id, linked_row) = of(linked);
        assert_eq!(linked_row, named_row, "{linked} shares {release}'s row");
        assert!(named_row < 100, "{release}'s row is offered");
        assert!(named_by_disc_id && !linked_by_disc_id);
        assert_eq!(
            named.album_links,
            AlbumLinks::Read(vec![MetadataRef::new(DG.catalog(), "7")]),
            "the group is every album its read releases' documents name, each \
             once"
        );
    }
    assert_ne!(of("mb-1").2, of("mb-2").2, "the two pressings are two rows");
    assert_eq!(
        effects,
        vec![Effect::KeepAlbumLinks {
            kept: vec![("g-1".to_string(), vec![MetadataRef::new(DG.catalog(), "7")])],
        }]
    );
}
