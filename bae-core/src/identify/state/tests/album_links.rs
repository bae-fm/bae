// Included by `tests.rs`; shares the helpers of `signals_and_conflicts.rs`.

use crate::identify::documents::{ReleaseDocument, ReleaseReading};
use crate::identify::state::PressingKey;
use crate::import::album_links::{AlbumLink, AlbumStatement};
use crate::import::MetadataRef;

/// A document stating five tracks, `notes`, and nothing else.
fn plain_document(notes: &[&str]) -> ReleaseDocument {
    ReleaseDocument {
        labels: Vec::new(),
        barcode: None,
        album_first_year: None,
        source_tracks: crate::import::search::SourceTracks::Listed { count: 5 },
        track_titles: Vec::new(),
        notes: notes.iter().map(|note| note.to_string()).collect(),
        links: Vec::new(),
        album_links: AlbumLinks::NotAsked,
    }
}

/// The Discogs master `master` a MusicBrainz release's documents state
/// through its link to the Discogs release `linked`.
fn through(release: &str, linked: &str, master: &str) -> AlbumLink {
    AlbumLink {
        album: MetadataRef::new(DG, master),
        stated: AlbumStatement::Release {
            musicbrainz_release: release.to_string(),
            twin: MetadataRef::new(DG, linked),
        },
    }
}

/// A MusicBrainz release's document naming the Discogs release `linked` as
/// itself, filed under `master`.
fn linking_document(release: &str, linked: &str, master: &str) -> ReleaseDocument {
    ReleaseDocument {
        links: vec![MetadataRef::new(DG, linked)],
        album_links: AlbumLinks::Read(vec![through(release, linked, master)]),
        ..plain_document(&[])
    }
}

/// A release on `group` in `packaging`.
fn packed_in(
    source: Catalog,
    release_id: &str,
    group: &str,
    packaging: crate::pressing::Packaging,
) -> MetadataResult {
    MetadataResult {
        packaging: Some(packaging),
        ..mk_result_from(source, release_id, Some(group))
    }
}

/// What a run asked in its rounds past the folder's own keys: each read's
/// records, and each lookup of an offered row's pressing.
#[derive(Debug, Default)]
struct Asked {
    reads: Vec<Vec<MetadataRef>>,
    pressings: Vec<(Catalog, PressingKey)>,
}

/// Answer every read of documents and every pressing lookup `step` asks for:
/// each record with what `document` says of it, each pressing lookup with
/// what `pressing` finds. Returns where the run lands, the effects it asks
/// for past these, and what it asked.
fn answer_rounds(
    (mut state, mut effects): (IdentifyState, Vec<Effect>),
    document: impl Fn(&MetadataRef) -> ReleaseDocument,
    pressing: impl Fn(Catalog, &PressingKey) -> Vec<MetadataResult>,
) -> (IdentifyState, Vec<Effect>, Asked) {
    let mut asked = Asked::default();
    while let Some(at) = effects.iter().position(|effect| {
        matches!(
            effect,
            Effect::ReadReleases { .. } | Effect::LookupPressing { .. }
        )
    }) {
        let answer = match effects.remove(at) {
            Effect::ReadReleases { releases, .. } => {
                asked.reads.push(releases.clone());
                IdentifyEvent::ReleasesRead {
                    read: releases
                        .iter()
                        .map(|release| ReleaseReading {
                            release: release.clone(),
                            document: Ok(document(release)),
                        })
                        .collect(),
                }
            }
            Effect::LookupPressing { source, key } => {
                asked.pressings.push((source, key.clone()));
                let outcome = Ok(pressing(source, &key)
                    .into_iter()
                    .map(|result| {
                        let status = LibraryStatus::absent(&result.release_id);
                        (result, status)
                    })
                    .collect());
                IdentifyEvent::PressingLookupAnswered {
                    source,
                    key,
                    outcome,
                }
            }
            _ => unreachable!("the position is of a read or a pressing lookup"),
        };
        let (next, more) = super::step(state, answer);
        state = next;
        effects.extend(more);
    }
    (state, effects, asked)
}

/// Every release a settled state names with its row and provenance, offered
/// or set aside.
fn rows_of(state: &IdentifyState) -> Vec<(MetadataResult, LookupProvenance, u32)> {
    let IdentifyState::Found { findings, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    findings
        .matches
        .iter()
        .zip(&findings.provenance)
        .zip(&findings.pressings)
        .map(|((result, lookup), row)| (result.clone(), lookup.clone(), *row))
        .chain(
            findings
                .narrowed_out
                .matches
                .iter()
                .zip(&findings.narrowed_out.provenance)
                .zip(&findings.narrowed_out.pressings)
                .map(|((result, lookup), row)| (result.clone(), lookup.clone(), *row + 100)),
        )
        .collect()
}

/// A disc ID run on both catalogs whose lookup named `results`, holding at
/// the read of its offered records' documents.
fn disc_run_reading(
    results: Vec<(MetadataResult, LibraryStatus)>,
    text_pool: Vec<crate::signals::TextLine>,
) -> (IdentifyState, Vec<Effect>) {
    let mut disc_signals = disc_only(&[]);
    disc_signals.text_pool = text_pool;
    let (state, effects) = super::step(
        started_with(vec![MB, DG]),
        IdentifyEvent::SignalsUpdated {
            signals: disc_signals,
            audio: five_tracks(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    assert!(
        matches!(effects.as_slice(), [Effect::LookupDiscid { .. }]),
        "the disc ID is looked up: {effects:?}"
    );
    super::step(state, IdentifyEvent::DiscidLookupCompleted { results })
}

/// Each MusicBrainz release a disc ID names links its own Discogs release,
/// which no lookup returned. Once the releases are read, each row is looked
/// up on Discogs by the release its page names, and each goes on the list
/// beside its row: two rows, each carrying both catalogs' records, whose
/// Discogs records are read in turn. The album their documents state is kept.
#[test]
fn a_disc_id_s_releases_are_looked_up_on_discogs_by_their_links() {
    use crate::pressing::Packaging::{Digipak, JewelCase};
    let reading = disc_run_reading(
        vec![
            (
                packed_in(MB, "mb-1", "g-1", JewelCase),
                LibraryStatus::absent("mb-1"),
            ),
            (
                packed_in(MB, "mb-2", "g-1", Digipak),
                LibraryStatus::absent("mb-2"),
            ),
        ],
        Vec::new(),
    );
    let (state, effects, asked) = answer_rounds(
        reading,
        |release| match release.key.as_str() {
            "mb-1" => linking_document("mb-1", "dg-1", "7"),
            "mb-2" => linking_document("mb-2", "dg-2", "7"),
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
        vec![MetadataRef::new(MB, "mb-1"), MetadataRef::new(MB, "mb-2")],
        "the offered records are read first"
    );
    assert_eq!(
        asked.pressings,
        vec![
            (
                DG,
                PressingKey::Link {
                    release: MetadataRef::new(DG, "dg-1")
                }
            ),
            (
                DG,
                PressingKey::Link {
                    release: MetadataRef::new(DG, "dg-2")
                }
            ),
        ],
        "then each row by the release its page names"
    );
    assert_eq!(
        asked.reads[1..],
        [vec![MetadataRef::new(DG, "dg-1"), MetadataRef::new(DG, "dg-2")]],
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
        let (named, named_lookup, named_row) = of(release);
        let (_, linked_lookup, linked_row) = of(linked);
        assert_eq!(linked_row, named_row, "{linked} shares {release}'s row");
        assert!(named_row < 100, "{release}'s row is offered");
        assert!(linked_lookup.by_pressing && !linked_lookup.by_disc_id);
        assert!(named_lookup.by_disc_id && !named_lookup.by_pressing);
        assert_eq!(
            named.album_links,
            AlbumLinks::Read(vec![through("mb-1", "dg-1", "7")]),
            "the group is every album its read releases' documents name, each \
             once, by the first statement naming it"
        );
    }
    assert_ne!(of("mb-1").2, of("mb-2").2, "the two pressings are two rows");
    assert_eq!(
        effects,
        vec![Effect::KeepAlbumLinks {
            kept: vec![("g-1".to_string(), vec![through("mb-1", "dg-1", "7")])],
        }]
    );
}

/// Two rows tied on everything the folder states, but for what one linked
/// Discogs release's own document writes: the folder naming the plant only
/// that release's notes name puts its row first.
#[test]
fn a_linked_release_s_note_word_breaks_the_tie() {
    use crate::pressing::Packaging::{Digipak, JewelCase};
    let reading = disc_run_reading(
        vec![
            (
                packed_in(MB, "mb-1", "g-1", JewelCase),
                LibraryStatus::absent("mb-1"),
            ),
            (
                packed_in(MB, "mb-2", "g-1", Digipak),
                LibraryStatus::absent("mb-2"),
            ),
        ],
        vec![crate::signals::TextLine {
            text: "Album Title (Pressed by Plantname)".to_string(),
            origin: crate::signals::TextOrigin::FolderName,
        }],
    );
    let (state, _, _) = answer_rounds(
        reading,
        |release| match release.key.as_str() {
            "mb-1" => linking_document("mb-1", "dg-1", "7"),
            "mb-2" => linking_document("mb-2", "dg-2", "7"),
            "dg-2" => plain_document(&["Pressed by Plantname"]),
            _ => plain_document(&["Pressed elsewhere"]),
        },
        |_, key| match key {
            PressingKey::Link { release } if release.key == "dg-1" => {
                vec![packed_in(DG, "dg-1", "7", JewelCase)]
            }
            _ => vec![packed_in(DG, "dg-2", "7", Digipak)],
        },
    );
    let IdentifyState::Found { findings, .. } = &state else {
        panic!("expected Found, got {state:?}");
    };
    assert_eq!(
        findings.named_notes,
        vec![crate::identify::NamedNote {
            release: MetadataRef::new(DG, "dg-2"),
            note: "Pressed by Plantname".to_string(),
        }]
    );
    let lead_row = findings.pressings[0];
    let lead: Vec<&str> = findings
        .matches
        .iter()
        .zip(&findings.pressings)
        .filter(|(_, row)| **row == lead_row)
        .map(|(result, _)| result.release_id.as_str())
        .collect();
    assert!(
        lead.contains(&"mb-2") && lead.contains(&"dg-2"),
        "the named release's row leads: {lead:?}"
    );
}

/// A catalog number only the fetched documents state joins the albums no
/// document links, and what the group was then read to be is kept.
#[test]
fn a_catalog_number_only_a_document_states_joins_the_albums() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A"]),
            },
            &[],
        ),
    );
    let (state, _) = super::step(state, barcode_matched(MB, "A", vec![pair("mb-1", Some("g-1"))]));
    let reading = super::step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-1", Some("7"))]),
    );
    let (state, effects, _) = answer_rounds(
        reading,
        |release| ReleaseDocument {
            labels: vec![crate::pressing::ReleaseLabel::of(Some("Imprint"), Some("LB-100"))],
            album_links: match release.catalog {
                Catalog::MusicBrainz => AlbumLinks::Read(Vec::new()),
                _ => AlbumLinks::NotAsked,
            },
            ..plain_document(&[])
        },
        |_, _| Vec::new(),
    );
    let joined = AlbumLink {
        album: MetadataRef::new(DG, "7"),
        stated: AlbumStatement::CatalogNumber {
            musicbrainz_release: "mb-1".to_string(),
            release: MetadataRef::new(DG, "dg-1"),
        },
    };
    assert_eq!(
        effects,
        vec![Effect::KeepAlbumLinks {
            kept: vec![("g-1".to_string(), vec![joined.clone()])],
        }]
    );
    let mb = rows_of(&state)
        .into_iter()
        .find(|(result, _, _)| result.release_id == "mb-1")
        .expect("mb-1 is on the list")
        .0;
    assert_eq!(mb.album_links, AlbumLinks::Read(vec![joined]));
}

/// A disc ID alone names only MusicBrainz releases: where their documents
/// name no album there is no other catalog's album to compare them against,
/// so nothing is kept, and what an earlier run kept for the group stands.
#[test]
fn a_disc_id_alone_whose_documents_name_nothing_keeps_nothing() {
    let reading = disc_run_reading(vec![pair("mb-1", Some("g-1"))], Vec::new());
    let (state, effects, _) = answer_rounds(
        reading,
        |_| ReleaseDocument {
            album_links: AlbumLinks::Read(Vec::new()),
            ..plain_document(&[])
        },
        |_, _| Vec::new(),
    );
    assert!(effects.is_empty(), "nothing is kept: {effects:?}");
    assert!(matches!(state, IdentifyState::Found { .. }));
}
