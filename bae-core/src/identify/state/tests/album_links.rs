// Included by `tests.rs`; shares the helpers of `signals_and_conflicts.rs`.

use crate::identify::documents::{ReleaseDocument, ReleaseReading, Twin, TwinToRead};
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
/// through its link to the Discogs release `twin`.
fn through(release: &str, twin: &str, master: &str) -> AlbumLink {
    AlbumLink {
        album: MetadataRef::new(DG, master),
        stated: AlbumStatement::Release {
            musicbrainz_release: release.to_string(),
            twin: MetadataRef::new(DG, twin),
        },
    }
}

/// A MusicBrainz release's document naming the Discogs release `twin` as
/// itself, filed under `master`.
fn linking_document(release: &str, twin: &str, master: &str) -> ReleaseDocument {
    ReleaseDocument {
        links: vec![MetadataRef::new(DG, twin)],
        album_links: AlbumLinks::Read(vec![through(release, twin, master)]),
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

/// Answer every read of documents `step` asks for: each record with what
/// `document` says of it, each twin with what `twin` makes of it. Returns
/// where the run lands, the effects it asks for past the reads, and each
/// read's records and twins in turn.
#[allow(clippy::type_complexity)]
fn answer_reads(
    (mut state, mut effects): (IdentifyState, Vec<Effect>),
    document: impl Fn(&MetadataRef) -> ReleaseDocument,
    twin: impl Fn(&TwinToRead) -> MetadataResult,
) -> (
    IdentifyState,
    Vec<Effect>,
    Vec<(Vec<MetadataRef>, Vec<TwinToRead>)>,
) {
    let mut asked = Vec::new();
    while let Some(at) = effects
        .iter()
        .position(|effect| matches!(effect, Effect::ReadReleases { .. }))
    {
        let Effect::ReadReleases {
            releases, twins, ..
        } = effects.remove(at)
        else {
            unreachable!("the position is of a read");
        };
        asked.push((releases.clone(), twins.clone()));
        let read = releases
            .iter()
            .chain(twins.iter().map(|to_read| &to_read.release))
            .map(|release| ReleaseReading {
                release: release.clone(),
                document: Ok(document(release)),
            })
            .collect();
        let twins = twins
            .iter()
            .map(|to_read| Twin {
                result: twin(to_read),
                named_by: to_read.named_by.clone(),
                status: LibraryStatus::absent(&to_read.release.key),
            })
            .collect();
        let (next, more) = super::step(state, IdentifyEvent::ReleasesRead { read, twins });
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

/// Each MusicBrainz release a disc ID names links its own Discogs release, no
/// lookup returned either, and each goes on the list as its namer's twin: two
/// rows, each carrying both catalogs' records. The twins are read once the
/// releases that name them are, and the album their documents state is kept.
#[test]
fn a_disc_id_s_releases_each_carry_the_twin_they_link() {
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
    let (state, effects, asked) = answer_reads(
        reading,
        |release| match release.key.as_str() {
            "mb-1" => linking_document("mb-1", "dg-1", "7"),
            "mb-2" => linking_document("mb-2", "dg-2", "7"),
            _ => plain_document(&[]),
        },
        |to_read| match to_read.release.key.as_str() {
            "dg-1" => packed_in(DG, "dg-1", "7", JewelCase),
            _ => packed_in(DG, "dg-2", "7", Digipak),
        },
    );

    let (first, first_twins) = &asked[0];
    assert_eq!(
        first,
        &vec![MetadataRef::new(MB, "mb-1"), MetadataRef::new(MB, "mb-2")],
        "the offered records are read first"
    );
    assert!(first_twins.is_empty(), "nothing names a twin before it is read");
    assert_eq!(
        asked[1],
        (
            Vec::new(),
            vec![
                TwinToRead {
                    release: MetadataRef::new(DG, "dg-1"),
                    named_by: MetadataRef::new(MB, "mb-1"),
                },
                TwinToRead {
                    release: MetadataRef::new(DG, "dg-2"),
                    named_by: MetadataRef::new(MB, "mb-2"),
                },
            ]
        ),
        "then the twins their documents name"
    );
    assert_eq!(asked.len(), 2, "and nothing more");

    let rows = rows_of(&state);
    let of = |release_id: &str| {
        rows.iter()
            .find(|(result, _, _)| result.release_id == release_id)
            .cloned()
            .unwrap_or_else(|| panic!("{release_id} is on the list: {rows:?}"))
    };
    for (release, twin) in [("mb-1", "dg-1"), ("mb-2", "dg-2")] {
        let (named, named_lookup, named_row) = of(release);
        let (_, twin_lookup, twin_row) = of(twin);
        assert_eq!(twin_row, named_row, "{twin} shares {release}'s row");
        assert!(named_row < 100, "{release}'s row is offered");
        assert_eq!(twin_lookup.named_by, Some(MetadataRef::new(MB, release)));
        assert!(!twin_lookup.by_disc_id, "no lookup returned {twin}");
        assert!(named_lookup.by_disc_id && named_lookup.named_by.is_none());
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

/// Two rows tied on everything the folder states, but for what a twin's own
/// document writes: the folder naming the plant only that twin's notes name
/// puts its row first.
#[test]
fn a_twin_s_note_word_breaks_the_tie() {
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
    let (state, _, _) = answer_reads(
        reading,
        |release| match release.key.as_str() {
            "mb-1" => linking_document("mb-1", "dg-1", "7"),
            "mb-2" => linking_document("mb-2", "dg-2", "7"),
            "dg-2" => plain_document(&["Pressed by Plantname"]),
            _ => plain_document(&["Pressed elsewhere"]),
        },
        |to_read| match to_read.release.key.as_str() {
            "dg-1" => packed_in(DG, "dg-1", "7", JewelCase),
            _ => packed_in(DG, "dg-2", "7", Digipak),
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
        "the named twin's row leads: {lead:?}"
    );
}

/// A run that does not join records across catalogs reads no twin and joins
/// no album, whatever its documents state.
#[test]
fn a_run_that_does_not_follow_catalog_links_reads_no_twin() {
    let state = started_without(
        vec![MB, DG],
        crate::config::IdentificationStep::FollowCatalogLinks,
    );
    let (state, _) = update(
        state,
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A"]),
            },
            &[],
        ),
    );
    let (state, _) = super::step(
        state,
        barcode_matched(MB, "A", vec![pair("mb-1", Some("g-linked"))]),
    );
    let reading = super::step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-1", Some("7"))]),
    );
    let (state, effects, asked) = answer_reads(
        reading,
        |release| match release.key.as_str() {
            "mb-1" => linking_document("mb-1", "dg-twin", "7"),
            _ => plain_document(&[]),
        },
        |_| panic!("no twin is read"),
    );
    assert!(asked.iter().all(|(_, twins)| twins.is_empty()));
    assert!(effects.is_empty(), "nothing is kept: {effects:?}");
    let mb = rows_of(&state)
        .into_iter()
        .find(|(result, _, _)| result.release_id == "mb-1")
        .expect("mb-1 is on the list")
        .0;
    assert_eq!(mb.album_links, AlbumLinks::NotAsked);
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
    let (state, effects, _) = answer_reads(
        reading,
        |release| ReleaseDocument {
            labels: vec![crate::pressing::ReleaseLabel::of(Some("Imprint"), Some("LB-100"))],
            album_links: match release.catalog {
                Catalog::MusicBrainz => AlbumLinks::Read(Vec::new()),
                _ => AlbumLinks::NotAsked,
            },
            ..plain_document(&[])
        },
        |_| panic!("no document names a twin"),
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
    let (state, effects, _) = answer_reads(
        reading,
        |_| ReleaseDocument {
            album_links: AlbumLinks::Read(Vec::new()),
            ..plain_document(&[])
        },
        |_| panic!("no document names a twin"),
    );
    assert!(effects.is_empty(), "nothing is kept: {effects:?}");
    assert!(matches!(state, IdentifyState::Found { .. }));
}
