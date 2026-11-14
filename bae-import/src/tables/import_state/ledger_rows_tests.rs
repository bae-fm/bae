//! A run's ledger as stored and read back.

use super::*;
use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use bae_catalogs::catalog::LookupCatalog;
use bae_identify::{Findings, NarrowedOut, TerminalVerdict};

/// `release` as one catalog's record of its own.
fn release_on(source: bae_catalogs::catalog::LookupCatalog, release_id: &str) -> MetadataResult {
    MetadataResult {
        source,
        release_id: release_id.to_string(),
        source_group_id: Some(format!("group-{release_id}")),
        ..sample_match()
    }
}

/// Every cell of the band reads back as the run left it: found, no match,
/// failed and not asked, each clue's rows in order with the values left out,
/// the catalog numbers offered and the ISRCs. A found cell lists every
/// release it found — one the run offers, one agreement set aside, and one
/// listed in neither, which only its cell shows and counts.
#[tokio::test]
async fn the_ledger_round_trips_every_cell_and_every_release_it_found() {
    use bae_catalogs::LookupFailure;
    use bae_identify::{
        BarcodeStepView, CatalogCandidateView, CatalogStepView, DiscIdRow, IsrcStepView,
        NotAskedReason, ProviderCell, SearchStepView, SignalValueRow,
    };

    let offered = release_on(LookupCatalog::MusicBrainz, "rel-offered");
    let catalogued = release_on(LookupCatalog::Discogs, "42");
    let set_aside = release_on(LookupCatalog::MusicBrainz, "rel-set-aside");
    let ruled_out = MetadataResult {
        source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 9 }),
        ..release_on(LookupCatalog::MusicBrainz, "rel-ruled-out")
    };
    let found = |releases: &[&MetadataResult]| {
        LookupView::answered(&releases.iter().map(|release| (*release).clone()).collect())
    };
    let left_out = LookupView::NotAsked { reason: NotAskedReason::LeftOut };
    let cell = |source, lookup| ProviderCell { source, lookup };
    let ledger = IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz, LookupCatalog::Discogs],
        disc_id: DiscIdStepView::Read {
            discs: vec![
                DiscIdRow { disc_id: "disc-1".to_string(), lookup: found(&[&offered, &set_aside]) },
                DiscIdRow {
                    disc_id: "disc-2".to_string(),
                    lookup: LookupView::Failed { failure: LookupFailure::Timeout },
                },
            ],
        },
        barcode: BarcodeStepView::Rows {
            scanning: false,
            rows: vec![
                SignalValueRow {
                    value: "012345678905".to_string(),
                    excluded: false,
                    cells: vec![
                        cell(LookupCatalog::MusicBrainz, found(&[&ruled_out, &offered])),
                        cell(
                            LookupCatalog::Discogs,
                            LookupView::Failed {
                                failure: LookupFailure::Provider { status: Some(503) },
                            },
                        ),
                    ],
                },
                SignalValueRow {
                    value: "5051961234567".to_string(),
                    excluded: true,
                    cells: vec![
                        cell(LookupCatalog::MusicBrainz, left_out.clone()),
                        cell(LookupCatalog::Discogs, left_out.clone()),
                    ],
                },
            ],
        },
        catalog: CatalogStepView::Numbers {
            scanning: false,
            rows: vec![
                SignalValueRow {
                    value: "CAT-1".to_string(),
                    excluded: false,
                    cells: vec![
                        cell(LookupCatalog::MusicBrainz, LookupView::NoMatch),
                        cell(LookupCatalog::Discogs, found(&[&catalogued])),
                    ],
                },
                SignalValueRow {
                    value: "CAT-2".to_string(),
                    excluded: true,
                    cells: vec![
                        cell(LookupCatalog::MusicBrainz, left_out.clone()),
                        cell(LookupCatalog::Discogs, left_out),
                    ],
                },
            ],
            candidates: vec![CatalogCandidateView { value: "CAT-9".to_string() }],
        },
        isrc: IsrcStepView::Read {
            isrcs: vec!["USAAA0000001".to_string(), "USAAA0000002".to_string()],
            lookup: LookupView::NotAsked { reason: NotAskedReason::NoCatalog },
        },
        search: SearchStepView::NotNeeded,
    };
    let verdict = TerminalVerdict::Failed {
        failures: vec![bae_identify::IdentifyFailure::DiscId(LookupFailure::Timeout)],
        findings: Findings {
            matches: vec![offered.clone(), catalogued.clone()],
            pressings: vec![0, 1],
            narrowed_out: NarrowedOut { matches: vec![set_aside.clone()], pressings: vec![0] },
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: 11,
        ledger: Some(ledger),
    };

    let (read, listings) = stored_and_read(&verdict).await;

    assert_eq!(read, verdict);
    // The Disc ID agreement is read from the stored ledger: the releases a
    // disc ID's lookup found agree on it, and no other.
    let bae_identify::IdentifyStateView::Failed { agreements, .. } =
        bae_identify::IdentifyStateView::from(read.clone().resume_state(
            None,
            &bae_identify::LookupChoices::default(),
            bae_identify::signals::AudioFacts::default(),
        ))
    else {
        panic!("a failed verdict resumes as Failed");
    };
    let by_disc_id = |id: &str| {
        agreements
            .iter()
            .find(|(release, _)| release == id)
            .unwrap_or_else(|| panic!("{id} has badges: {agreements:?}"))
            .1
            .fields
            .disc_id
    };
    assert!(by_disc_id("rel-offered"));
    assert!(by_disc_id("rel-set-aside"));
    assert!(!by_disc_id("42"), "only the catalog number found it");
    assert_eq!(
        listings,
        vec![
            "rel-offered offered",
            "42 offered",
            "rel-set-aside narrowed_out",
            "rel-ruled-out unlisted",
        ],
        "each release is stored once, the one no list holds after the listed ones"
    );
    let BarcodeStepView::Rows { rows, .. } = &read.ledger().expect("a ledger").barcode else {
        panic!("the barcode step read rows");
    };
    let LookupView::Found { count, .. } = &rows[0].cells[0].lookup else {
        panic!("the barcode's MusicBrainz lookup found releases");
    };
    assert_eq!(*count, 2, "the release no list holds still counts in its cell");
    assert!(
        rows[0].cells[0]
            .lookup
            .releases()
            .iter()
            .any(|release| release.release_id == "rel-ruled-out"),
        "and its cell lists it"
    );
    assert!(
        !read.named_releases().iter().any(|release| release.release_id == "rel-ruled-out"),
        "but the verdict names it nowhere else"
    );
}

/// How each clue stood reads back for every state but rows of lookups:
/// a sheet no CD plays, a barcode source with no code, no catalog number, no
/// ISRC and no title; and a title search reads back its words and its cells.
#[tokio::test]
async fn the_ledger_round_trips_how_each_clue_stood() {
    use bae_identify::{
        BarcodeStepView, CatalogStepView, IsrcStepView, ProviderCell, SearchStepView,
    };

    let quiet = IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz],
        disc_id: DiscIdStepView::NotCdAudio,
        barcode: BarcodeStepView::NoCodes,
        catalog: CatalogStepView::NoneFound,
        isrc: IsrcStepView::Absent,
        search: SearchStepView::NoTitle,
    };
    let verdict = TerminalVerdict::ManualOnly { track_count: 11, ledger: Some(quiet.clone()) };
    assert_eq!(stored_and_read(&verdict).await.0, verdict);

    let searched = release_on(LookupCatalog::Discogs, "77");
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![searched.clone()],
            pressings: vec![0],
            ..Findings::default()
        },
        track_count: 11,
        ledger: Some(IdentifyRunView {
            providers: vec![LookupCatalog::MusicBrainz, LookupCatalog::Discogs],
            disc_id: DiscIdStepView::Absent,
            barcode: BarcodeStepView::Absent,
            search: SearchStepView::Searched {
                album: "Album".to_string(),
                artist: String::new(),
                cells: vec![
                    ProviderCell {
                        source: LookupCatalog::MusicBrainz,
                        lookup: LookupView::NoMatch,
                    },
                    ProviderCell {
                        source: LookupCatalog::Discogs,
                        lookup: LookupView::answered(&vec![searched]),
                    },
                ],
            },
            ..quiet
        }),
    };
    let (read, listings) = stored_and_read(&verdict).await;
    assert_eq!(read, verdict);
    assert_eq!(listings, vec!["77 offered"]);
}

/// A ledger a settled run cannot have recorded, a lookup still under way, is
/// a bug of bae's own: its write fails with the bug the store reports.
#[tokio::test]
async fn a_settled_ledger_with_a_lookup_under_way_is_a_bug() {
    let (db, _tmp, heard) =
        bae_store::test_fixtures::temp_db_hearing_bugs(crate::TEST_SCHEMA).await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut ledger = sample_ledger();
    ledger.disc_id = DiscIdStepView::Read {
        discs: vec![bae_identify::DiscIdRow {
            disc_id: "disc-1".to_string(),
            lookup: LookupView::LookingUp,
        }],
    };
    let verdict = TerminalVerdict::NotFoundAnywhere { ledger: Some(ledger) };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;

    let failed =
        crate::tables::ImportTables::new(db.clone()).store_verdict(&row).await.unwrap_err();

    let bug = the_reported_bug(&heard, &failed);
    assert!(bug.contains("a lookup under way"), "{bug}");
}

/// A cell's list is laid out with the ledger's write: the MusicBrainz and the
/// Discogs release a lookup found that print one barcode are one row under
/// one card, the MusicBrainz record leading, the card names its title and
/// label once, and the row lists what its records state.
#[tokio::test]
async fn ledger_cell_cards_are_stored_with_the_ledger() {
    use bae_identify::{
        BarcodeStepView, CatalogStepView, IsrcStepView, ProviderCell, SearchStepView,
        SignalValueRow,
    };

    let printed = |source, id: &str, group: &str, number: &str| MetadataResult {
        barcodes: vec!["012345678905".to_string()],
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Imprint"), Some(number))],
        source_group_id: Some(group.to_string()),
        ..release_on(source, id)
    };
    let musicbrainz = printed(LookupCatalog::MusicBrainz, "mb-1", "group-x", "CAT 1");
    let discogs = printed(LookupCatalog::Discogs, "dg-1", "master-7", "CAT 2");
    let elsewhere = MetadataResult {
        barcodes: vec!["5051961234567".to_string()],
        ..release_on(LookupCatalog::MusicBrainz, "mb-2")
    };
    let found =
        LookupView::answered(&vec![discogs.clone(), elsewhere.clone(), musicbrainz.clone()]);
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![musicbrainz.clone(), discogs.clone()],
            pressings: vec![0, 0],
            ..Findings::default()
        },
        track_count: 11,
        ledger: Some(IdentifyRunView {
            providers: vec![LookupCatalog::MusicBrainz],
            disc_id: DiscIdStepView::Absent,
            barcode: BarcodeStepView::Rows {
                scanning: false,
                rows: vec![SignalValueRow {
                    value: "012345678905".to_string(),
                    excluded: false,
                    cells: vec![ProviderCell { source: LookupCatalog::MusicBrainz, lookup: found }],
                }],
            },
            catalog: CatalogStepView::NoneFound,
            isrc: IsrcStepView::Absent,
            search: SearchStepView::NoTitle,
        }),
    };

    let (db, hash, _tmp) = stored_verdict(&verdict).await;
    let (placed, text, statements): (Vec<String>, Vec<String>, Vec<String>) = db
        .read(move |sql| {
            let query = |statement: &str| {
                sql.query(statement, [hash.as_str()], |row| row.get::<_, String>(0))
            };
            Ok((
                query(
                    "SELECT found.release_id || ' ' || lookup.pressing || ' ' || lookup.card \
                            || ' ' || lookup.record_order \
                     FROM import_candidate_lookup_release AS lookup \
                     JOIN import_candidate_match AS found \
                       ON found.content_hash = lookup.content_hash \
                      AND found.position = lookup.match_position \
                     WHERE lookup.content_hash = ? ORDER BY lookup.ordinal",
                )?,
                query(
                    "SELECT card || ' ' || kind || ' ' || ordinal || ' ' || text \
                     FROM import_candidate_card_text WHERE content_hash = ? \
                     ORDER BY card, kind, ordinal",
                )?,
                query(
                    "SELECT pressing || ' ' || fact || ' ' || line || ' ' || ordinal || ' ' \
                            || value || ' ' || catalogs \
                     FROM import_candidate_row_statement WHERE content_hash = ? \
                     ORDER BY pressing, fact, line, ordinal",
                )?,
            ))
        })
        .await
        .unwrap();

    // The cell lists the cards the rules made of what it found, card by card.
    assert_eq!(placed, ["mb-1 0 0 0", "dg-1 0 0 1", "mb-2 1 2 0"]);
    assert!(text.contains(&"0 label 0 Imprint".to_string()), "{text:?}");
    assert_eq!(text.len(), 6, "each of the two cards names its title, artist and label");
    assert!(
        statements.contains(&"0 catalog_number 0 0 CAT 1 [\"musicbrainz\"]".to_string())
            && statements.contains(&"0 catalog_number 0 1 CAT 2 [\"discogs\"]".to_string()),
        "{statements:?}"
    );
}
